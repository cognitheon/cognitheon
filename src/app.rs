use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Color32, ComboBox, FontId, Id, Layout, RichText};
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::{Builder, Runtime};

use crate::history::History;
use crate::resource::{CanvasStateResource, GraphResource, ParticleSystemResource};
// use crate::globals::{CanvasStateResource, GraphResource};
use crate::gpu_render::particle::particle_system::ParticleSystem;
use crate::graph::edge::EdgeType;
use crate::graph::layout::{self, LayoutParams};
use crate::input::state_manager::InputStateManager;
use crate::ui::canvas::data::CanvasWidget;
use crate::wikilink;

/// 快捷键 / 手势帮助文案的**单一来源**（避免文案与实现漂移）。
///
/// 结构：`&[(分组标题, &[(按键/手势, 说明)])]`。`show_keymap_help` 据此渲染分组表格。
/// 每条都对照真实绑定核对过：
/// - 画布导航：`state_manager` 的 `handle_space_*`（Space 平移）、`handle_scroll`（滚轮平移）、
///   `handle_zoom`（`zoom_delta`：Ctrl+滚轮 / 触控板捏合缩放）、右键 press/release（连边 / 建点+连边）。
/// - 节点 / 选择：`handle_double_click`、`handle_primary_button_press`（含 Shift 加选）、
///   `handle_delete_key`（Delete/Backspace）、`Selecting` 框选。
/// - 编辑：`node.rs::show_editor` 的 Ctrl/Cmd+Enter 退出 + `wikilink_autocomplete`（`[[` 触发，
///   Tab/Enter 确认、↑↓ 选择、Esc 关闭）。
/// - 历史 / 搜索：`app.rs::ui` 顶部的 Ctrl/Cmd+Z、Ctrl/Cmd+Y、Ctrl/Cmd+Shift+Z、Ctrl/Cmd+P，
///   以及 `handle_search_hits_nav`（F3 / Shift+F3 巡览搜索命中、Esc 清除画布命中高亮）。
type KeymapSection = (&'static str, &'static [(&'static str, &'static str)]);
const KEYMAP_HELP: &[KeymapSection] = &[
    (
        "画布导航",
        &[
            ("Space + 拖动", "平移画布"),
            ("滚轮", "平移画布"),
            ("Ctrl + 滚轮 / 触控板捏合", "缩放画布（以指针为中心）"),
            ("右键拖动 节点→节点", "连边"),
            ("右键拖动 节点→空白", "新建节点并连边"),
        ],
    ),
    (
        "右键菜单",
        &[
            ("右键单击 节点", "编辑标题 / 删除节点 / 删除选中"),
            ("右键单击 边", "删除此边 / 跳转到源 / 跳转到目标"),
            ("右键单击 空白", "在此新建节点 / 全选 / 整理布局"),
        ],
    ),
    (
        "节点",
        &[
            ("双击空白", "新建节点并进入编辑"),
            ("双击节点", "进入编辑"),
            ("单击节点", "选中"),
            ("拖动节点", "移动（含已选中的多个）"),
            ("Delete / Backspace", "删除选中节点（连同其边）"),
        ],
    ),
    (
        "选择",
        &[
            ("左键拖框", "框选"),
            ("Shift + 单击节点", "加入 / 切换选择"),
            ("Shift + 拖框", "在已有选择上追加框选"),
        ],
    ),
    (
        "编辑",
        &[
            ("Ctrl / Cmd + Enter", "退出编辑并解析正文 [[双链]]"),
            ("[[", "触发标题自动补全"),
            ("Tab / Enter", "确认补全候选"),
            ("↑ / ↓", "在补全候选间移动"),
            ("Esc", "关闭补全 / 退出编辑"),
        ],
    ),
    (
        "历史",
        &[
            ("Ctrl / Cmd + Z", "撤销"),
            ("Ctrl / Cmd + Y", "重做"),
            ("Ctrl / Cmd + Shift + Z", "重做"),
        ],
    ),
    (
        "搜索 / 导航",
        &[
            (
                "Ctrl / Cmd + P",
                "命令面板（搜索；↑↓ 移动、Enter 跳转、Esc 关闭）",
            ),
            ("F3", "跳到下一个搜索命中并居中（循环）"),
            ("Shift + F3", "跳到上一个搜索命中并居中（循环）"),
            ("Esc", "清除画布上的搜索命中高亮"),
        ],
    ),
    (
        "布局",
        &[("菜单「整理布局」", "力导向自动布局 + 缩放至全部可见")],
    ),
    ("帮助", &[("? / F1", "打开 / 关闭本帮助")]),
];

/// 搜索命中巡览游标的总线 key（隐式状态总线约定，纯 UI、不进序列化）。
///
/// 与命中集 [`crate::ui::node::SEARCH_HITS_KEY`]（`Vec<NodeIndex>`）配套：本 key 存
/// `Option<usize>`，= 命中集中"当前已聚焦项"的下标，`None` 表示"尚未巡览"（命令面板刚写入命中集时）。
/// `F3` / `Shift+F3` 在 [`eframe::App::ui`] 顶部据它对命中集做循环巡览并 `focus_node`（选中 + 居中）：
/// `None` 时首个 `F3` 落到第 0 项、`Shift+F3` 落到末项；其后循环 ±1。`Esc` 清空命中集时连同它一并移除。
const SEARCH_HITS_CURSOR_KEY: &str = "search_hits_cursor";

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct CognitheonApp {
    // Example stuff:
    label: String,

    #[serde(skip)] // This how you opt-out of serialization of a field
    value: f32,
    // edge_type: EdgeType,
    canvas_resource: CanvasStateResource,
    graph_resource: GraphResource,
    /// 撤销/重做历史栈：运行态，**不持久化**（`.cnt` / storage 格式零变更，AGENTS.md §7）。
    /// 同一个句柄经构造函数注入到 `canvas_widget` 的输入状态机与节点 widget，写同一真源（§3.1）。
    #[serde(skip)]
    history: History,
    #[serde(skip)]
    canvas_widget: CanvasWidget,
    #[serde(skip)]
    particle_system: Option<ParticleSystemResource>,
    #[cfg(not(target_arch = "wasm32"))]
    #[serde(skip)]
    runtime: Runtime,
}

// impl Debug for CognitheonApp {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{:?}", self.graph_resource)?;
//         write!(f, "{:?}", self.canvas_resource)
//     }
// }

impl Default for CognitheonApp {
    fn default() -> Self {
        let graph_resource = GraphResource::default();
        let canvas_resource = CanvasStateResource::default();
        let history = History::default();
        Self {
            // Example stuff:
            label: "Hello World!".to_owned(),
            value: 2.7,
            // edge_type: EdgeType::Line,
            canvas_resource: canvas_resource.clone(),
            graph_resource: graph_resource.clone(),
            history: history.clone(),
            canvas_widget: CanvasWidget::new(
                graph_resource.clone(),
                canvas_resource.clone(),
                history.clone(),
            ),
            particle_system: None,
            #[cfg(not(target_arch = "wasm32"))]
            runtime: Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        }
    }
}

impl CognitheonApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        // if let Some(storage) = cc.storage {
        //     return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        // }
        setup_font(&cc.egui_ctx);

        let mut app = if let Some(storage) = cc.storage {
            log::info!("load");
            let mut app: CognitheonApp =
                eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
            // history 不持久化：反序列化后是一个全新的空 History。重建 canvas_widget 时必须用
            // 同一个 app.history 句柄（clone Arc，§3.1），否则状态机/节点 widget 与菜单/快捷键
            // 会写到不同的历史栈。
            app.canvas_widget = CanvasWidget::new(
                app.graph_resource.clone(),
                app.canvas_resource.clone(),
                app.history.clone(),
            );
            // println!("app: {:?}", app);
            app
        } else {
            Default::default()
        };
        // let mut app: CognitheonApp = Default::default();

        let wgpu_render_state = cc.wgpu_render_state.as_ref();
        if let Some(rs) = wgpu_render_state {
            let device = &rs.device;

            // 构造我们的粒子系统
            let particle_system = ParticleSystem::new(
                device,
                rs.target_format,
                2000, // 最大粒子数
                10,   // 每帧生成多少粒子
                2.0,  // 粒子最大生命（秒）
                10.0, // 粒子最大速度
            );

            let particle_system_resource = ParticleSystemResource::new(particle_system);
            // println!("particle_system: {:?}", particle_system);

            // 注册到资源里，这样在回调里可以获取到
            rs.renderer
                .write()
                .callback_resources
                .insert::<ParticleSystemResource>(particle_system_resource.clone());

            app.particle_system = Some(particle_system_resource.clone());
        }

        app
    }

    // pub fn get_graph(ctx: &egui::Context) -> &Graph {
    //     ctx.data(|data| {
    //         let app = data
    //             .get_persisted::<CognitheonApp>(eframe::APP_KEY.into())
    //             .unwrap();
    //         &app.graph
    //     })
    // }
}

impl CognitheonApp {
    /// 右侧链接面板：选中节点的出链 + 反向引用（含上下文原话），点击跳转聚焦。
    fn show_links_panel(&self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.heading("链接");
        ui.separator();

        // 选中边时：展示边标签编辑（恰好一条）或「选中 N 条边」提示（多条）。优先于节点出链/反链分支。
        let selected_edges = self
            .graph_resource
            .read_resource(|g| g.get_selected_edges());
        if !selected_edges.is_empty() {
            self.show_edge_label_editor(ui, &selected_edges);
            return;
        }

        let selected = self
            .graph_resource
            .read_resource(|g| g.get_selected_nodes().first().copied());
        let Some(idx) = selected else {
            ui.weak("选中一个节点，查看它的出链与反向引用。");
            self.show_orphan_nodes(ui);
            return;
        };

        let (title, outlinks) = self
            .graph_resource
            .read_resource(|g| match g.get_node(idx) {
                Some(n) => (n.text.clone(), wikilink::parse_links(&n.note)),
                None => (String::new(), Vec::new()),
            });
        let title_disp = if title.is_empty() {
            "（无标题）"
        } else {
            title.as_str()
        };
        ui.label(RichText::new(title_disp).strong());

        // 出链
        ui.add_space(6.0);
        ui.label(RichText::new("出链").weak().small());
        if outlinks.is_empty() {
            ui.weak("（在正文里用 [[标题]] 建立）");
        }
        for lt in &outlinks {
            let target = self
                .graph_resource
                .read_resource(|g| wikilink::find_by_title(g, lt));
            let text = if target.is_some() {
                format!("→ {lt}")
            } else {
                format!("→ {lt}（未创建）")
            };
            if ui
                .add_enabled(target.is_some(), egui::Button::new(text).frame(false))
                .clicked()
            {
                if let Some(t) = target {
                    self.focus_node(ui.ctx(), t);
                }
            }
        }

        // 反向链接 + 上下文原话
        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new("反向链接").weak().small());
        let backlinks = self
            .graph_resource
            .read_resource(|g| wikilink::backlinks_with_context(g, idx));
        if backlinks.is_empty() {
            ui.weak("（还没有笔记用 [[…]] 提到它）");
        }
        for bl in &backlinks {
            if ui
                .add(
                    egui::Button::new(RichText::new(format!("↩ {}", bl.title)).strong())
                        .frame(false),
                )
                .clicked()
            {
                self.focus_node(ui.ctx(), bl.source);
            }
            for c in &bl.contexts {
                ui.label(RichText::new(c.as_str()).weak().small());
            }
            ui.add_space(4.0);
        }
    }

    /// 选中边时的右侧面板分支：恰好一条边 → 标签编辑框；多条边 → 「选中 N 条边」提示。
    ///
    /// 编辑写回经 `self.history`：进入编辑（TextEdit 获焦）打一次预快照 `stage`，编辑期逐次
    /// `with_resource` 写 `Edge.text`（不每键打快照），失焦时 `commit_staged_if_changed` 收口成
    /// **一个可撤销单元**（与节点正文编辑同构，见 node.rs / state_manager.rs）。§3.3：取 `Edge`
    /// 用 Option 容错——选区里的 `EdgeIndex` 可能已失效（被删），失效则跳过、不 panic。
    fn show_edge_label_editor(
        &self,
        ui: &mut egui::Ui,
        selected_edges: &[petgraph::graph::EdgeIndex],
    ) {
        if selected_edges.len() > 1 {
            ui.label(
                RichText::new(format!("选中 {} 条边", selected_edges.len()))
                    .weak()
                    .small(),
            );
            ui.weak("（仅选中单条边可编辑标签）");
            return;
        }

        let edge_index = selected_edges[0];
        // §3.3 失效容错：边可能已被删除——取不到就提示后返回，不 panic。
        let Some(current) = self.graph_resource.read_resource(|g| {
            g.get_edge(edge_index)
                .map(|e| e.text.clone().unwrap_or_default())
        }) else {
            ui.weak("（该边已不存在）");
            return;
        };

        ui.label(RichText::new("边标签").weak().small());

        let mut buf = current;
        let resp = ui.add(
            egui::TextEdit::singleline(&mut buf)
                .hint_text("边标签")
                .desired_width(f32::INFINITY),
        );

        // 进入编辑：获焦那一帧打预快照（写闭包外单独 read 克隆 before，§3.1），把整段编辑
        // 合并为一个撤销单元。
        if resp.gained_focus() {
            let before = self.graph_resource.read_resource(|g| g.clone());
            self.history.stage(before);
        }

        // 编辑期写回：内容真的变了才写 Edge.text（空串归一为 None，与初始 None 语义一致）。
        // 不在此处各自打快照——已由 stage/commit 合并。
        if resp.changed() {
            let new_text = if buf.trim().is_empty() {
                None
            } else {
                Some(buf.clone())
            };
            self.graph_resource
                .with_resource(|g| g.update_edge_text(edge_index, new_text));
        }

        // 退出编辑（失焦）：把暂存快照按"图数据是否真的变了"提交或丢弃，收口成一个撤销单元。
        if resp.lost_focus() {
            let after = self.graph_resource.read_resource(|g| g.clone());
            self.history.commit_staged_if_changed(&after);
        }
    }

    /// 未选中节点时的全局视图：列出图中的「孤立节点」（无任何边连接），点击跳转聚焦。
    ///
    /// 大图卫生工具——把无出入边、未被 `[[…]]` 引用也未手画连接的节点集中暴露出来，便于发现盲点。
    /// 数据只读经 [`crate::wikilink::orphan_nodes`]（`read_resource`，§3.1），跳转复用 [`Self::focus_node`]
    /// （选中 + 居中，§3.3 用 `NodeIndex` 句柄）。放在「未选中」分支，不喧宾夺主、不与选中态的出链/反链争位。
    fn show_orphan_nodes(&self, ui: &mut egui::Ui) {
        let orphans = self.graph_resource.read_resource(wikilink::orphan_nodes);

        ui.add_space(8.0);
        ui.separator();
        ui.label(
            RichText::new(format!("孤立节点（{}）", orphans.len()))
                .weak()
                .small(),
        );
        if orphans.is_empty() {
            ui.weak("（没有孤立节点，连接很健康）");
            return;
        }
        for &idx in &orphans {
            let title = self
                .graph_resource
                .read_resource(|g| g.get_node(idx).map(|n| n.text.clone()));
            let Some(title) = title else { continue };
            let label = if title.is_empty() {
                "（无标题）".to_owned()
            } else {
                title
            };
            if ui
                .add(egui::Button::new(RichText::new(format!("• {label}"))).frame(false))
                .clicked()
            {
                self.focus_node(ui.ctx(), idx);
            }
        }
    }

    /// 快捷键 / 手势帮助浮层（按 `?` / `F1` 开关，菜单「帮助」亦可开）。
    ///
    /// 纯 UI、零数据层：开关状态存 egui temp data `keymap_help_open` (`bool`)，不进序列化
    /// （隐式状态总线约定）。文案来自单一来源 [`KEYMAP_HELP`] 常量表，与真实绑定对齐。
    ///
    /// 范式复刻 [`Self::show_command_palette`]：`egui::Area`(Order::Foreground) 居中 +
    /// `Frame::popup`。开关与 Esc 关闭都在 [`eframe::App::ui`] 顶部、画布 `state_manager`
    /// 渲染之前完成（§3.4 输入只在 app.rs 顶部 consume），故画布状态机当帧看不到这些键。
    fn show_keymap_help(&self, ctx: &egui::Context) {
        let open_id = Id::new("keymap_help_open");
        if !ctx.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false) {
            return;
        }

        egui::Area::new(Id::new("keymap_help_area"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .movable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(420.0);

                        ui.horizontal(|ui| {
                            ui.heading("快捷键 / 手势");
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.weak("? / F1 / Esc 关闭");
                            });
                        });
                        ui.separator();

                        egui::ScrollArea::vertical()
                            .max_height(440.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                for (section, rows) in KEYMAP_HELP {
                                    ui.add_space(4.0);
                                    ui.label(RichText::new(*section).strong());
                                    egui::Grid::new(("keymap_help_grid", *section))
                                        .num_columns(2)
                                        .spacing([16.0, 4.0])
                                        .striped(true)
                                        .show(ui, |ui| {
                                            for (keys, desc) in *rows {
                                                ui.label(RichText::new(*keys).monospace());
                                                ui.label(*desc);
                                                ui.end_row();
                                            }
                                        });
                                    ui.add_space(2.0);
                                }
                            });
                    });
            });
    }

    /// 关键词过滤控件（顶栏）：输入框 + Dim/Hide 模式切换 + 清空按钮（大图降噪）。
    ///
    /// 输入关键词 → 不匹配的节点（及其相连边）淡出（Dim）或隐藏（Hide）；清空恢复全部可见。
    /// 状态全在 egui temp data（[`crate::graph::filter`]：query / 模式），纯 UI、**不进序列化、
    /// 不进 history、不碰 SSOT / 状态机**——可见度判定集中在 `render_graph` 入口算一次、各 widget
    /// 反读（§3.3 / §3.5）。匹配集复用现成 `wikilink::search`（标题 + 别名 + 正文，已有测试）。
    ///
    /// `&self` 无关（只读写 temp data），用关联函数避免无谓借用；与命令面板 / 边类型切换并列于顶栏。
    fn show_filter_controls(ui: &mut egui::Ui) {
        use crate::graph::filter::{current_mode, current_query, set_mode, set_query, FilterMode};

        let ctx = ui.ctx().clone();
        ui.label("过滤");

        // 输入框：改动即写回 temp data（下一帧 render_graph 入口据此重算可见集）。
        let mut query = current_query(&ctx);
        let resp = ui.add(
            egui::TextEdit::singleline(&mut query)
                .hint_text("关键词…")
                .desired_width(140.0),
        );
        if resp.changed() {
            set_query(&ctx, query.clone());
        }

        // Dim / Hide 模式切换（仿 EdgeType 的 selectable_value 风格）。
        let mut mode = current_mode(&ctx);
        let before = mode;
        ComboBox::from_id_salt("filter_mode")
            .selected_text(match mode {
                FilterMode::Dim => "淡出",
                FilterMode::Hide => "隐藏",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut mode, FilterMode::Dim, "淡出 (Dim)");
                ui.selectable_value(&mut mode, FilterMode::Hide, "隐藏 (Hide)");
            });
        if mode != before {
            set_mode(&ctx, mode);
        }

        // 清空：恢复全部可见（query 置空 → render_graph 入口判定 active=false）。
        if ui.button("✖").on_hover_text("清空过滤").clicked() {
            set_query(&ctx, String::new());
        }

        ui.add_space(8.0);

        // 邻居聚焦开关（默认关）：ON 且恰好选中一个节点时，该节点 + 直接邻居（1 跳）+ 连接它们的边
        // 保持高亮、其余淡出（复用过滤的 Dim 渲染管线，在 render_graph 入口与过滤合成、过滤优先）。
        // 纯 UI / temp data，不进 history / 序列化（仿 filter 开关风格）。
        let mut focus = crate::graph::filter::focus_enabled(&ctx);
        if ui
            .checkbox(&mut focus, "邻域聚焦")
            .on_hover_text("选中单个节点时高亮其邻居、淡出其余（取消选中恢复全图）")
            .changed()
        {
            crate::graph::filter::set_focus_enabled(&ctx, focus);
        }

        ui.add_space(8.0);

        // 结构着色开关（默认关，保持现有干净外观）：ON 时按节点度数给外框上色/调宽——低度冷色细框、
        // 高度暖色粗框、孤立（度数 0）弱化虚线框。归一化基准 max_degree 在 render_graph 入口算一次
        // 经 temp data 下发，NodeWidget 反读自身度数渲染。纯 UI / temp data，不进 history / 序列化。
        let mut structure = crate::graph::filter::structure_coloring_enabled(&ctx);
        if ui
            .checkbox(&mut structure, "结构着色")
            .on_hover_text("按节点度数给外框上色/调宽：低度冷细、高度暖粗、孤立弱化虚线")
            .changed()
        {
            crate::graph::filter::set_structure_coloring_enabled(&ctx, structure);
        }
    }

    /// 命令面板（全文搜索 / 快速跳转）。
    ///
    /// 状态全部存在 egui temp data（隐式状态总线约定），不进序列化：
    /// - `command_palette_open` (`bool`)：开关
    /// - `command_palette_query` (`String`)：搜索框文本
    /// - `command_palette_sel` (`usize`)：高亮索引，随结果数量钳制
    /// - `command_palette_just_opened` (`bool`)：仅"刚打开那帧"请求聚焦的一次性标记
    ///
    /// `Ctrl+P` 的截获与导航键（↑/↓/Enter/Esc）的消费都在 [`eframe::App::ui`] 顶部、
    /// CentralPanel（画布 `state_manager`）渲染之前完成，故画布状态机当帧看不到这些键，
    /// 不会与命令面板打架（尤其 Esc 不会既关面板又被状态机当成回 Idle/清选中双重触发）。
    fn show_command_palette(&self, ctx: &egui::Context) {
        let open_id = Id::new("command_palette_open");
        let query_id = Id::new("command_palette_query");
        let sel_id = Id::new("command_palette_sel");
        let just_opened_id = Id::new("command_palette_just_opened");

        if !ctx.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false) {
            return;
        }

        let mut query: String = ctx
            .data(|d| d.get_temp::<String>(query_id))
            .unwrap_or_default();

        // 搜索结果：复用 wikilink::search（标题+正文、大小写不敏感）。
        // 空 query 时展示全部节点（按索引顺序），作为"快速跳转"列表。
        let results: Vec<(petgraph::graph::NodeIndex, String, String)> =
            self.graph_resource.read_resource(|g| {
                let indices = if query.trim().is_empty() {
                    g.graph.node_indices().collect::<Vec<_>>()
                } else {
                    wikilink::search(g, query.trim())
                };
                indices
                    .into_iter()
                    .filter_map(|i| g.get_node(i).map(|n| (i, n.text.clone(), n.note.clone())))
                    .take(50)
                    .collect()
            });

        // 高亮索引：持久化 + 按结果数量钳制（结果变化时不越界）。
        let mut selected = ctx
            .data(|d| d.get_temp::<usize>(sel_id))
            .unwrap_or(0)
            .min(results.len().saturating_sub(1));

        // 导航键在面板打开时由它消费（赶在画布状态机前），避免泄漏到画布。
        // Esc 关闭面板（不传给状态机）；↑/↓ 移动高亮；Enter 跳转高亮项。
        let mut close = false;
        let mut jump: Option<petgraph::graph::NodeIndex> = None;
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                close = true;
            }
            if !results.is_empty() {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    selected = (selected + 1) % results.len();
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    selected = (selected + results.len() - 1) % results.len();
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                    jump = Some(results[selected].0);
                }
            }
        });

        let just_opened = ctx
            .data_mut(|d| d.remove_temp::<bool>(just_opened_id))
            .unwrap_or(false);

        let mut query_changed = false;
        egui::Area::new(Id::new("command_palette_area"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 80.0))
            .movable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(480.0);

                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut query)
                                .hint_text("搜索节点（标题 / 正文）…")
                                .desired_width(f32::INFINITY),
                        );
                        // 仅"刚打开那帧"请求一次焦点，避免每帧抢焦点。
                        if just_opened {
                            edit.request_focus();
                        }
                        if edit.changed() {
                            query_changed = true;
                        }

                        ui.separator();

                        if results.is_empty() {
                            ui.weak("无匹配节点");
                        } else {
                            // 命中计数（N = 结果数；空 query 时即全部节点）。
                            ui.weak(format!("{} 个匹配", results.len()));
                            ui.add_space(2.0);

                            // 着色取色：命中段用超链接强调色，普通段用 base 文本色。
                            // 提前 copy 出（Color32: Copy），避免后续 &mut ui 借用冲突。
                            let strong_col = ui.visuals().strong_text_color();
                            let weak_col = ui.visuals().weak_text_color();
                            let hit_col = ui.visuals().hyperlink_color;
                            let body_h = ui.text_style_height(&egui::TextStyle::Body);
                            let small_h = ui.text_style_height(&egui::TextStyle::Small);
                            let q = query.trim();

                            egui::ScrollArea::vertical()
                                .max_height(360.0)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    for (row, (idx, title, note)) in results.iter().enumerate() {
                                        let title_disp = if title.is_empty() {
                                            "（无标题）"
                                        } else {
                                            title.as_str()
                                        };
                                        // 标题着色：命中子串高亮，其余用 strong 文本色。
                                        let title_job = highlight_job(
                                            title_disp,
                                            q,
                                            strong_col,
                                            hit_col,
                                            FontId::proportional(body_h),
                                        );
                                        // 正文一行摘要：命中行优先、回退首个非空行，char 截断。
                                        let snippet = wikilink::snippet_around(note, q);
                                        let resp = ui
                                            .selectable_label(row == selected, title_job)
                                            .on_hover_text(snippet.as_str());
                                        if !snippet.is_empty() {
                                            ui.indent(("cp_snip", row), |ui| {
                                                let snip_job = highlight_job(
                                                    &snippet,
                                                    q,
                                                    weak_col,
                                                    hit_col,
                                                    FontId::proportional(small_h),
                                                );
                                                ui.label(snip_job);
                                            });
                                        }
                                        if resp.clicked() {
                                            jump = Some(*idx);
                                        }
                                        ui.add_space(2.0);
                                    }
                                });
                        }
                    });
            });

        if query_changed {
            // query 变化：写回并把面板高亮复位到第一项。
            ctx.data_mut(|d| {
                d.insert_temp(query_id, query.clone());
                d.insert_temp(sel_id, 0usize);
            });
            // 同步刷新「画布命中高亮集」（实时高亮 + F3 巡览的数据源）：
            // - 非空 query：把当前命中节点写入命中集、巡览游标复位 0（NodeWidget 据此发金色描边）；
            // - 空 query（=全部节点的快速跳转列表）：清空命中集，避免"全图高亮"的视觉噪声。
            // 命中集 / 游标都是纯 UI temp data、不进序列化；关闭面板后仍保留供 F3 巡览，直到 Esc 清除。
            let hits: Vec<petgraph::graph::NodeIndex> = if query.trim().is_empty() {
                Vec::new()
            } else {
                results.iter().map(|(i, _, _)| *i).collect()
            };
            ctx.data_mut(|d| {
                if hits.is_empty() {
                    d.remove::<Vec<petgraph::graph::NodeIndex>>(Id::new(
                        crate::ui::node::SEARCH_HITS_KEY,
                    ));
                    d.remove::<Option<usize>>(Id::new(SEARCH_HITS_CURSOR_KEY));
                } else {
                    d.insert_temp(Id::new(crate::ui::node::SEARCH_HITS_KEY), hits);
                    // 游标置 None：尚未巡览，首个 F3 落到第 0 项。
                    d.insert_temp(Id::new(SEARCH_HITS_CURSOR_KEY), Option::<usize>::None);
                }
            });
        } else {
            ctx.data_mut(|d| d.insert_temp(sel_id, selected));
        }

        if let Some(idx) = jump {
            self.focus_node(ctx, idx);
            close = true;
        }

        if close {
            ctx.data_mut(|d| {
                d.remove::<bool>(open_id);
                d.remove::<String>(query_id);
                d.remove::<usize>(sel_id);
                d.remove::<bool>(just_opened_id);
            });
        }
    }

    /// 缩放/平移画布到能看见全部节点（zoom-to-fit）。
    ///
    /// 计算所有节点位置（画布坐标，AGENTS.md §3.2）的包围盒，求出让它带边距铺满视口的
    /// 缩放与平移，写回 `CanvasState.transform`（坐标变换的唯一权威）。缩放钳到 §3.2 的 `[0.1, 100]`。
    /// 空图 / 无节点时不动。
    fn zoom_to_fit(&self, ctx: &egui::Context) {
        // 包围盒计算与 minimap 共享同一纯函数（直接读 Node.position，画布坐标 §3.2）。
        let bbox = self.graph_resource.read_resource(wikilink::minimap_bbox);
        let Some(bbox) = bbox else {
            return;
        };

        let view = ctx.content_rect();
        if view.width() <= 0.0 || view.height() <= 0.0 {
            return;
        }

        // 给包围盒留出边距，并兜底一个最小尺寸（单节点 / 共线时 width/height 可能为 0）。
        let content_w = bbox.width().max(1.0) + 320.0;
        let content_h = bbox.height().max(1.0) + 320.0;
        let margin = 0.9; // 视口利用率，留白
        let scale =
            ((view.width() / content_w).min(view.height() / content_h) * margin).clamp(0.1, 100.0);

        // 让包围盒中心落到视口中心：screen = scale * canvas + translation。
        let translation = view.center().to_vec2() - scale * bbox.center().to_vec2();
        self.canvas_resource.with_resource(|cs| {
            cs.transform = egui::emath::TSTransform::new(translation, scale);
        });
    }

    /// 用一份序列化字节替换整图（Save/Load 的 **native 与 wasm 共享路径**，避免两套漂移）。
    ///
    /// 解析复用 [`crate::persistence::load`]（格式分派 + 旧 `.cnt` 兼容，AGENTS.md §7），失败仅记日志、
    /// 不动当前图。替换语义与原 File 菜单 Load 完全一致（撤销 Load 让旧图整体复活，视图/缩放与 id
    /// 计数器不回滚）：
    /// 1. `discard_staged`：作废任何在途暂存快照（如正编辑/拖拽中触发导入），它对新图无意义；
    /// 2. 在替换资源 `Arc` **之前**、写闭包之外克隆"载入前"的图 `record` 进 undo（§3.1）；
    /// 3. 把新 `Graph` / `CanvasState` 装进**新** `Resource`（替换 Arc，非原地写）；
    /// 4. 用**同一个** `self.history` 句柄重建 `canvas_widget`（§3.1 共享同一真源——务必传 `clone`，
    ///    否则导入后菜单/快捷键与状态机会写到不同历史栈）。
    ///
    /// 调用时机（§3.3）：native 在 File 菜单 Load 点击处同帧调用；wasm 经 [`crate::io::take_loaded_bytes`]
    /// 在 `ui` 顶部、画布 CentralPanel 渲染**之前**调用——替换发生在本帧几何读取之前，无悬空索引窗口。
    fn replace_document(&mut self, bytes: &[u8]) {
        // 兼容旧 .cnt：persistence::load 会回退解析无版本号的旧格式。
        match crate::persistence::load(bytes) {
            Ok(doc) => {
                let (graph, canvas) = doc.into_parts();
                // 整图被替换：作废任何在途暂存快照（同 New）。
                self.history.discard_staged();
                // 作为一个可撤销单元：先把"载入前"的图压入 undo（在替换资源 Arc 之前、写闭包外
                // 克隆，§3.1）。撤销会让旧图整体复活（视图/缩放与 id 计数器不回滚，符合 spec）。
                let before = self.graph_resource.read_resource(|g| g.clone());
                self.history.record(before);
                self.graph_resource = GraphResource::new(graph);
                self.canvas_resource = CanvasStateResource::new(canvas);
                // 复用同一个 history 句柄（§3.1：共享同一真源）——务必传 clone，否则替换后
                // 菜单/快捷键与状态机会写到不同历史栈。
                self.canvas_widget = CanvasWidget::new(
                    self.graph_resource.clone(),
                    self.canvas_resource.clone(),
                    self.history.clone(),
                );
                log::info!("document replaced ({} bytes)", bytes.len());
            }
            Err(e) => log::error!("load failed: {e}"),
        }
    }

    /// **单节点导出**：把某节点导成一个 `标题.md`（frontmatter + note 原样）触发下载 / 落盘。
    ///
    /// 只读图（[`read_resource`]，§3.1）、不改图、不进 history（导出非图变更）。文件名经
    /// [`crate::markdown::sanitize_filename`]（空标题回退 `node-{id}`）。两 target 都走 io 单文件网关
    /// [`crate::io::download::trigger_file_download`]（native 弹保存框落盘 / wasm 浏览器下载 Blob）。
    /// 目标节点失效（跨帧右键菜单期间被删）则仅记日志、不导出（§3.3 容错）。
    fn export_node_markdown(&self, node_index: petgraph::graph::NodeIndex) {
        let exported = self.graph_resource.read_resource(|g| {
            g.get_node(node_index).map(|node| {
                let base = crate::markdown::sanitize_filename(&node.text, node.id);
                let filename = format!("{base}.md");
                // 单节点导出无去重上下文，stem 即 sanitize 后的 base；若它 ≠ 原标题
                // （标题含非法字符被替换），node_to_markdown 会把原标题注入 aliases 保住链接句柄。
                let bytes = crate::markdown::node_to_markdown(node, &base).into_bytes();
                (filename, bytes)
            })
        });
        let Some((filename, bytes)) = exported else {
            log::warn!("export node markdown: node {node_index:?} not found");
            return;
        };
        #[cfg(not(target_arch = "wasm32"))]
        crate::io::download::trigger_file_download(
            &self.runtime,
            &filename,
            &bytes,
            "text/markdown",
        );
        #[cfg(target_arch = "wasm32")]
        crate::io::download::trigger_file_download(&filename, &bytes, "text/markdown");
        log::info!("exported node markdown: {filename}");
    }

    /// **vault 导出**：全图导成多个 `标题.md` + （存在手画边时）手画边旁路 JSON。
    ///
    /// 只读图（§3.1）算出文件清单（[`crate::markdown::vault_files`]，纯函数：sanitize + 去重 + 旁路），
    /// 不改图、不进 history。两 target **行为不对称**（AGENTS.md §2 各自门控，属预期）：
    /// - **native**：弹目录选择框，`std::fs` 逐文件写盘（[`crate::io::download::trigger_vault_dir_export`]）。
    /// - **wasm**：手写 store-only ZIP 打包全部文件（[`crate::markdown::vault_zip`]）→ 浏览器下载
    ///   `cognitheon-vault.zip`（store-zip 纯 Rust、无新依赖、wasm 安全，规避 zip/flate2 的 wasm 兼容风险）。
    ///
    /// 空图（无节点、无手画边）则不导出、仅记日志。
    fn export_vault(&self) {
        let files = self
            .graph_resource
            .read_resource(crate::markdown::vault_files);
        if files.is_empty() {
            log::info!("vault export: nothing to export (empty graph)");
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        crate::io::download::trigger_vault_dir_export(&self.runtime, &files);
        #[cfg(target_arch = "wasm32")]
        {
            let zip = crate::markdown::vault_zip(&files);
            crate::io::download::trigger_file_download(
                "cognitheon-vault.zip",
                &zip,
                "application/zip",
            );
        }
        log::info!("vault export triggered: {} files", files.len());
    }

    /// 选中并把画布聚焦（居中）到某节点。
    fn focus_node(&self, ctx: &egui::Context, idx: petgraph::graph::NodeIndex) {
        let pos = self.graph_resource.with_resource(|g| {
            g.selected.clear();
            g.select_node(idx);
            g.get_node(idx).map(|n| n.position)
        });
        if let Some(pos) = pos {
            let center = ctx.content_rect().center();
            self.canvas_resource.with_resource(|cs| {
                let s = cs.transform.scaling;
                cs.transform.translation = center.to_vec2() - s * pos.to_vec2();
            });
        }
    }

    /// 搜索命中巡览的键盘处理：`F3` 下一个命中、`Shift+F3` 上一个（循环、复用 [`Self::focus_node`]
    /// 选中 + 居中）；`Esc` 清除画布命中高亮（**仅在命中集非空时**才 consume，避免误吞本该给
    /// 状态机 / 编辑态 / 浮层的 Esc）。
    ///
    /// 数据源是隐式状态总线（纯 UI temp data、不进序列化）：命中集 [`crate::ui::node::SEARCH_HITS_KEY`]
    /// （`Vec<NodeIndex>`，命令面板搜索写入）+ 巡览游标 [`SEARCH_HITS_CURSOR_KEY`]（`usize`）。
    ///
    /// 调用时机（§3.4）：在 [`eframe::App::ui`] 顶部、画布 `state_manager` 渲染之前 consume，
    /// 故按键不泄漏给状态机。Esc 协调顺序：帮助浮层 / 命令面板的 Esc 已在更靠前处 consume
    /// （浮层/面板开着时它们先吃 Esc），到这里只有"无浮层、有命中高亮"时 Esc 才被本方法消费。
    ///
    /// 容错（§3.3）：命中集里的 `NodeIndex` 可能在搜索后被删；`focus_node` 内 `get_node` 已 `Option`
    /// 容错（失效则不居中、仅清选区），不 panic。
    fn handle_search_hits_nav(&self, ctx: &egui::Context, text_focus: bool) {
        let hits_id = Id::new(crate::ui::node::SEARCH_HITS_KEY);
        let cursor_id = Id::new(SEARCH_HITS_CURSOR_KEY);

        let hits: Vec<petgraph::graph::NodeIndex> = ctx
            .data(|d| d.get_temp::<Vec<petgraph::graph::NodeIndex>>(hits_id))
            .unwrap_or_default();

        // Esc 仅在有命中高亮时消费并清除——空集时放行给后续（状态机 / 编辑态 / 其它浮层）。
        if !hits.is_empty()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            ctx.data_mut(|d| {
                d.remove::<Vec<petgraph::graph::NodeIndex>>(hits_id);
                d.remove::<Option<usize>>(cursor_id);
            });
            log::debug!("search hits highlight cleared (Esc)");
            return;
        }

        // F3 / Shift+F3 巡览：F3 不是文本字符，但编辑态 / 文本框持焦时不抢（与 ? 门控一致）。
        if hits.is_empty() || text_focus {
            return;
        }

        // 先判 Shift+F3（上一个），再判 F3（下一个）——consume_key 走 matches_logically
        // 忽略多余修饰键，故 F3 须放在带 SHIFT 的判定之后，避免 Shift+F3 被无修饰 F3 误吞。
        let prev = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F3));
        let next = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F3));
        if !prev && !next {
            return;
        }

        let cursor = ctx
            .data(|d| d.get_temp::<Option<usize>>(cursor_id))
            .flatten();
        let len = hits.len();
        // 循环巡览：未巡览（None）时首个 F3 → 第 0 项、Shift+F3 → 末项；其后循环 ±1。
        let new_cursor = match (cursor, next) {
            (None, true) => 0,
            (None, false) => len - 1,
            (Some(c), true) => (c + 1) % len,
            (Some(c), false) => (c + len - 1) % len,
        };
        ctx.data_mut(|d| d.insert_temp(cursor_id, Some(new_cursor)));
        let target = hits[new_cursor];
        log::debug!("search hits nav -> {new_cursor}/{len} = {target:?}");
        self.focus_node(ctx, target);
    }

    /// 右键上下文菜单（节点 / 边 / 空白）+ 删除可视 UI。
    ///
    /// 自管弹出（`egui::Popup::new(id, ctx, anchor, layer_id)`）而非 egui 原生 `context_menu`，依据见
    /// [`crate::ui::context_menu`] 模块文档（右键被状态机独占于连边手势，原生 context_menu 会争
    /// secondary）。请求由状态机判定"右键单击"时写入 temp data，本方法在画布渲染后消费。
    ///
    /// 生命周期：用一个本地 `open: bool`（初值 `true`，因为 temp data 里有请求才会进到这里）经
    /// `open_bool` 交给 Popup；`CloseOnClickOutside` 会在点击菜单外 / Esc 时把 `open` 置 `false`，
    /// 点击菜单项后我们主动置 `false`。`open` 为 `false` 时清掉 temp data 请求，菜单不再渲染。
    ///
    /// 跨帧失效容错（§3.3）：菜单可能跨多帧打开，期间目标 `NodeIndex` / `EdgeIndex` 可能已被删除。
    /// 回调里所有 `get_node` / `get_edge` / `edge_endpoints` 均 `Option` 容错，目标失效则对应项不显示
    /// 或菜单整体关闭，**绝不 `.unwrap()`**。
    fn show_context_menu(&self, ctx: &egui::Context) {
        use crate::ui::context_menu::{
            ContextMenuRequest, ContextMenuTarget, CONTEXT_MENU_REQUEST_KEY,
        };

        let req_id = Id::new(CONTEXT_MENU_REQUEST_KEY);
        let Some(request) = ctx.data(|d| d.get_temp::<ContextMenuRequest>(req_id)) else {
            return;
        };

        let popup_id = Id::new("context_menu_popup");
        let layer_id = egui::LayerId::new(egui::Order::Foreground, popup_id);

        let mut open = true;
        // 菜单内某项被点击后置 true：执行完动作后统一关闭菜单（避免在闭包里多处改 open 借用纠缠）。
        let mut action_taken = false;

        egui::Popup::new(popup_id, ctx.clone(), request.screen_pos, layer_id)
            .open_bool(&mut open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .layout(Layout::top_down_justified(Align::Min))
            .show(|ui| {
                ui.set_min_width(160.0);
                match request.target {
                    ContextMenuTarget::Node(node_index) => {
                        action_taken |= self.context_menu_node(ui, ctx, node_index);
                    }
                    ContextMenuTarget::Edge(edge_index) => {
                        action_taken |= self.context_menu_edge(ui, ctx, edge_index);
                    }
                    ContextMenuTarget::Canvas => {
                        action_taken |= self.context_menu_canvas(ui, ctx, request.screen_pos);
                    }
                }
            });

        if action_taken {
            open = false;
        }
        if !open {
            // 菜单关闭：清掉请求，下一帧不再渲染。
            ctx.data_mut(|d| d.remove::<ContextMenuRequest>(req_id));
        }
    }

    /// 节点右键菜单项。返回是否有项被点击（用于关闭菜单）。
    ///
    /// 目标节点可能已失效（跨帧打开期间被删）：`get_node` 容错——失效时只显示一条灰色提示、不提供
    /// 任何操作（§3.3）。
    fn context_menu_node(
        &self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        node_index: petgraph::graph::NodeIndex,
    ) -> bool {
        // 容错读节点是否仍存在（§3.3：跨帧打开期间可能已被删除）。
        let exists = self
            .graph_resource
            .read_resource(|g| g.get_node(node_index).is_some());
        if !exists {
            ui.weak("（节点已不存在）");
            return false;
        }

        // 当前选中节点数（多选时提供"删除选中 N 个"）。
        let selected_count = self
            .graph_resource
            .read_resource(|g| g.get_selected_nodes().len());

        let mut acted = false;

        if ui.button("编辑标题").clicked() {
            // 进入编辑态**不在此处**直接 set_editing_node：那样 editing_node 不被输入状态机持有，
            // 下一帧 Idle 分支会无条件清掉（编辑框永不出现），且从未 stage、编辑不可撤销。
            // 改为写 EditNodeRequest，由 state_manager.update() 消费时走与双击同款三件套
            // （stage_edit_snapshot + select + set_editing_node + transition_to(EditingNode)），
            // 编辑态由状态机持有、退出经 resolve_on_exit_edit 提交为可撤销单元（详见 context_menu 模块文档）。
            crate::ui::context_menu::request_edit_node(
                ctx,
                crate::ui::context_menu::EditNodeRequest { node: node_index },
            );
            acted = true;
        }

        if ui.button("删除节点").clicked() {
            // 删除单个节点（连同邻接边），经 history 打一次快照可撤销（§3.3 索引稳定，Ctrl+Z 整组复活）。
            self.history.mutate(&self.graph_resource, |g| {
                g.remove_node(node_index);
                // 删除后清选区，避免悬空索引（§3.3）。
                g.selected.clear();
            });
            acted = true;
        }

        // 多选时：删除选中的 N 个节点（仅当当前是节点选区且包含 >1 个，避免与"删除节点"重复）。
        if selected_count > 1 && ui.button(format!("删除选中 {selected_count} 个")).clicked() {
            self.history.mutate(&self.graph_resource, |g| {
                g.remove_selected();
            });
            acted = true;
        }

        ui.separator();

        // 导出此节点为 .md（单节点 Markdown 导出）：frontmatter + note 原样，Obsidian 兼容。
        // 只读图、不进 history（导出非图变更，见 export_node_markdown 文档）。
        if ui
            .button("导出此节点为 .md")
            .on_hover_text("导出为 Obsidian 兼容的 Markdown（[[链接]] 原样保留）")
            .clicked()
        {
            self.export_node_markdown(node_index);
            acted = true;
        }

        acted
    }

    /// 边右键菜单项。返回是否有项被点击。
    ///
    /// 目标边可能已失效；端点经 `edge_endpoints` 容错读取（§3.3）。"跳转到源/目标"对**所有边**
    /// （wiki 边与手画边一视同仁）提供——跳转复用 `focus_node`（选中 + 居中），对两类边都安全；
    /// 端点节点失效时对应项灰显（`add_enabled(false, …)`）。
    fn context_menu_edge(
        &self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        edge_index: petgraph::graph::EdgeIndex,
    ) -> bool {
        // 容错读端点（§3.3）：跨帧打开期间边可能已被删 → endpoints 为 None。
        let endpoints = self
            .graph_resource
            .read_resource(|g| g.graph.edge_endpoints(edge_index));
        let Some((source, target)) = endpoints else {
            ui.weak("（边已不存在）");
            return false;
        };

        let mut acted = false;

        if ui.button("删除此边").clicked() {
            // 只删边、不动端点节点，经 history 可撤销。
            self.history.mutate(&self.graph_resource, |g| {
                g.remove_edge(edge_index);
                g.selected.clear();
            });
            acted = true;
        }

        ui.separator();

        // 跳转到源 / 目标：复用 focus_node（选中 + 居中）。端点节点存在才可点（§3.3 容错）。
        let source_ok = self
            .graph_resource
            .read_resource(|g| g.get_node(source).is_some());
        let target_ok = self
            .graph_resource
            .read_resource(|g| g.get_node(target).is_some());

        if ui
            .add_enabled(source_ok, egui::Button::new("跳转到源节点"))
            .clicked()
        {
            self.focus_node(ctx, source);
            acted = true;
        }
        if ui
            .add_enabled(target_ok, egui::Button::new("跳转到目标节点"))
            .clicked()
        {
            self.focus_node(ctx, target);
            acted = true;
        }

        acted
    }

    /// 空白画布右键菜单项。返回是否有项被点击。
    fn context_menu_canvas(
        &self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        screen_pos: egui::Pos2,
    ) -> bool {
        let mut acted = false;

        if ui.button("在此新建节点").clicked() {
            // 右键单击处屏幕坐标 → 画布坐标（§3.2）。**不在此处** add_node + set_editing_node：
            // 直接设 editing_node 会被下一帧 Idle 分支清掉（编辑框永不出现）。改为写 CreateNodeRequest，
            // 由 state_manager.update() 消费时走与双击空白建点同款链路（stage_edit_snapshot + add_node
            // + select + set_editing_node + transition_to(EditingNode)），create+edit 为**单一撤销单元**，
            // 且编辑态由状态机持有、新建节点真能进入编辑框（详见 context_menu 模块文档）。
            let canvas_pos = self
                .canvas_resource
                .read_resource(|cs| cs.to_canvas(screen_pos));
            crate::ui::context_menu::request_create_node(
                ctx,
                crate::ui::context_menu::CreateNodeRequest { canvas_pos },
            );
            acted = true;
        }

        if ui.button("全选").clicked() {
            // 全选是运行态（选区，#[serde(skip)]），非图数据变更——不经 history。
            self.graph_resource.with_resource(|g| g.select_all_nodes());
            acted = true;
        }

        if ui.button("整理布局").clicked() {
            // 与菜单栏「整理布局」同一入口：力导向布局批量改 position，经 history 可撤销，随后 zoom-to-fit。
            let affected = self.history.mutate(&self.graph_resource, |g| {
                layout::layout_graph(g, LayoutParams::default())
            });
            log::info!("force-directed layout applied to {affected} nodes (context menu)");
            self.zoom_to_fit(ctx);
            acted = true;
        }

        acted
    }
}

/// 把 `text` 布局成 [`LayoutJob`]，其中（大小写不敏感地）匹配 `query` 的子串用 `hit` 色标出，
/// 其余用 `base` 色——命令面板里直观展示"为何/在哪命中"。
///
/// 不变量（同 [`crate::ui::md_highlight`]）：输出的 `LayoutJob.text` 必须**逐字节覆盖** `text`，
/// 否则 egui galley 文本与源不一致会 panic。下方"先冲刷未着色普通段、再追加命中段"严格保证这点。
///
/// 多字节安全：命中区间的端点 `hs`/`he` **只取自 `text.char_indices()` 产出的字节偏移**（始终落在
/// char 边界），绝不裸 byte slice。匹配按 char 序列大小写不敏感比较（`q` = `query` 的小写 char 向量），
/// 不依赖 `to_lowercase()` 的字节长度稳定性。`query` 为空时整段按 base 着色、不查找。
fn highlight_job(text: &str, query: &str, base: Color32, hit: Color32, font: FontId) -> LayoutJob {
    let mut job = LayoutJob::default();
    let base_fmt = TextFormat {
        font_id: font.clone(),
        color: base,
        ..Default::default()
    };
    // 小写化的 query 字符序列（按 char 比较，避免 to_lowercase 改变字节长度带来的偏移错位）。
    let q: Vec<char> = query.trim().to_lowercase().chars().collect();
    if q.is_empty() || text.is_empty() {
        job.append(text, 0.0, base_fmt);
        return job;
    }
    let hit_fmt = TextFormat {
        font_id: font,
        color: hit,
        ..Default::default()
    };

    // (字节偏移, 该位置起的小写 char 流) —— 命中端点只能取自这些 char 边界。
    let starts: Vec<(usize, char)> = text
        .char_indices()
        .flat_map(|(b, ch)| ch.to_lowercase().map(move |lc| (b, lc)))
        .collect();
    // 注意：一个原文 char 小写化可能展开成多个 char（如 'İ'），它们共享同一字节偏移 `b`，
    // 故命中端点回到原文字节偏移时天然对齐 char 边界。

    let mut plain_start = 0usize; // 原文中尚未冲刷的普通段起点
    let mut i = 0usize; // starts 上的扫描游标
    while i + q.len() <= starts.len() {
        let matched = (0..q.len()).all(|k| starts[i + k].1 == q[k]);
        if matched {
            let hs = starts[i].0; // 命中首字符的原文字节偏移
            let hi_idx = i + q.len();
            let he = starts.get(hi_idx).map(|s| s.0).unwrap_or(text.len()); // 命中后首字符偏移 / 末尾
            if hs > plain_start {
                job.append(&text[plain_start..hs], 0.0, base_fmt.clone());
            }
            job.append(&text[hs..he], 0.0, hit_fmt.clone());
            plain_start = he;
            i = hi_idx; // 不重叠匹配
        } else {
            i += 1;
        }
    }
    if text.len() > plain_start {
        job.append(&text[plain_start..], 0.0, base_fmt);
    }
    job
}

impl eframe::App for CognitheonApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // println!("save");
        // println!("self: {:?}", self);
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let last_offset: f32 = ctx
            .data(|m| m.get_temp(Id::new("animation_offset")))
            .unwrap_or(0.0);

        let delta_time = ctx.input(|i| i.stable_dt).min(0.1); // 稳定的一帧时间
        let speed = 20.0; // 像素/秒

        // 每帧更新 offset
        let new_offset = last_offset - speed * delta_time;
        ctx.data_mut(|m| m.insert_temp(Id::new("animation_offset"), new_offset));

        // 导入（Load）异步读出的文件字节：每帧在所有面板（尤其画布 CentralPanel）渲染**之前**
        // 一次性消费（io 总线，native 同帧 / wasm FileReader 跨帧写入；take 取出即清，一份字节只
        // 加载一次）。在画布渲染前替换整图 → 本帧几何按新图重写，无悬空索引窗口（§3.3，与
        // replace_document 文档一致）。
        if let Some(bytes) = crate::io::take_loaded_bytes(&ctx) {
            self.replace_document(&bytes);
        }
        // println!(
        //     "update: {:?}",
        //     self.graph_resource.0.read().unwrap().graph.node_count()
        // );

        // if let Some(particle_system_resource) = &self.particle_system {
        //     particle_system_resource.read_particle_system(|particle_system| {
        //         println!(
        //             "particle_system: {:?}",
        //             particle_system
        //                 .particles
        //                 .iter()
        //                 .filter(|p| p.life > 0.0)
        //                 .count()
        //         );
        //     });
        // }
        // 快捷键帮助浮层：? / F1 开关，Esc 关闭。在任何面板（尤其画布 state_manager）渲染前
        // 截获并 consume，与 Ctrl+P 同范式（§3.4 输入只在 app.rs 顶部 consume）。
        //
        // `?` 的消费依据（egui 0.34）：egui 把逻辑字符 '?' 映射为 `Key::Questionmark`
        // （见 `Key::from_name`：`"?" => Questionmark`）；而 `consume_key` 走 `matches_logically`，
        // 文档明确「忽略多余的 Shift/Alt」——故 `consume_key(NONE, Questionmark)` 能匹配
        // Shift+/ 产出的 '?'，不必显式带 SHIFT。为兼容个别把 '?' 报成 `Slash`+Shift 的布局，
        // 再额外消费一次 `SHIFT + Slash` 兜底。F1 直接消费。
        //
        // 门控：`?` 是文本字符，编辑态 / 任意文本框（命令面板搜索框等）持焦时必须放行给
        // TextEdit，否则每打一次 '?' 都会被这里 consume 并翻转浮层（字符仍经独立 Event::Text
        // 插入，但浮层乱闪）。`editing` 走 graph 读锁（闭包结束即释放，不与后续取锁嵌套，§3.1，
        // 复用 undo 路径同款读法）；`egui_wants_keyboard_input()` 覆盖「任意 TextEdit 持焦」
        // （egui 0.34 该方法即 `memory.focused().is_some()`；旧名 `wants_keyboard_input` 已 deprecated）。
        // F1 不是文本字符、无冲突，故不门控——编辑态也能呼出帮助。
        let editing = self
            .graph_resource
            .read_resource(|g| g.get_editing_node().is_some());
        let text_focus = editing || ctx.egui_wants_keyboard_input();
        let toggle_help = ctx.input_mut(|i| {
            let question = !text_focus
                && (i.consume_key(egui::Modifiers::NONE, egui::Key::Questionmark)
                    | i.consume_key(egui::Modifiers::SHIFT, egui::Key::Slash));
            let f1 = i.consume_key(egui::Modifiers::NONE, egui::Key::F1);
            question | f1
        });
        let help_open_id = Id::new("keymap_help_open");
        let palette_open_id = Id::new("command_palette_open");
        // 帮助浮层打开时，Esc 优先关它（consume 掉，不冒泡给命令面板/画布状态机）。
        let help_is_open = ctx
            .data(|d| d.get_temp::<bool>(help_open_id))
            .unwrap_or(false);
        let help_esc = help_is_open
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if toggle_help || help_esc {
            // toggle：? / F1 翻转；Esc 仅在开着时关闭。
            let now_open = if help_esc { false } else { !help_is_open };
            ctx.data_mut(|d| d.insert_temp(help_open_id, now_open));
            // 浮层互斥：打开帮助时若命令面板开着则关掉它（清掉其全部 temp 状态）。
            if now_open {
                ctx.data_mut(|d| {
                    d.remove::<bool>(palette_open_id);
                    d.remove::<String>(Id::new("command_palette_query"));
                    d.remove::<usize>(Id::new("command_palette_sel"));
                    d.remove::<bool>(Id::new("command_palette_just_opened"));
                });
            }
        }

        // 命令面板：Ctrl+P 开关。在任何面板（尤其画布 state_manager）渲染前截获并 consume，
        // 避免快捷键泄漏到菜单栏 / 画布状态机。再次 Ctrl+P 关闭（Esc 关闭在面板内处理）。
        let toggle_palette = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::COMMAND, egui::Key::P)
                || i.consume_key(egui::Modifiers::CTRL, egui::Key::P)
        });
        if toggle_palette {
            let open_id = Id::new("command_palette_open");
            let now_open = !ctx.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
            ctx.data_mut(|d| {
                d.insert_temp(open_id, now_open);
                if now_open {
                    // 仅在刚打开时标记请求聚焦，并复位 query / 高亮。
                    d.insert_temp(Id::new("command_palette_just_opened"), true);
                    d.insert_temp(Id::new("command_palette_query"), String::new());
                    d.insert_temp(Id::new("command_palette_sel"), 0usize);
                    // 浮层互斥：打开命令面板时若帮助浮层开着则关掉它。
                    d.remove::<bool>(help_open_id);
                } else {
                    d.remove::<String>(Id::new("command_palette_query"));
                    d.remove::<usize>(Id::new("command_palette_sel"));
                    d.remove::<bool>(Id::new("command_palette_just_opened"));
                }
            });
        }
        // 面板打开时，导航键（↑↓/Enter/Esc）在此 consume，赶在画布 state_manager 之前。
        self.show_command_palette(&ctx);
        // 帮助浮层（静态文本，无导航键；Esc/开关已在上方 consume）。与命令面板互斥、并列渲染。
        self.show_keymap_help(&ctx);

        // 撤销/重做快捷键：与 Ctrl+P 同范式，在画布 state_manager 渲染前截获并 consume，
        // 赶在状态机之前（避免 Delete/移动等被状态机当帧再处理）。
        // 编辑态（EditingNode）内**不拦截**：让 multiline TextEdit 的自带文本撤销（Ctrl+Z）
        // / 重做（Ctrl+Y / Ctrl+Shift+Z）生效——整图撤销只在非编辑态接管。
        let editing = self
            .graph_resource
            .read_resource(|g| g.get_editing_node().is_some());
        if !editing {
            // Ctrl/Cmd+Z = 撤销；Ctrl/Cmd+Y 或 Ctrl/Cmd+Shift+Z = 重做。
            // 注意：先判 redo（含 Shift+Z），再判 undo（不含 Shift），避免 Shift+Z 被 undo 误吞。
            let do_redo = ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
                    || i.consume_key(egui::Modifiers::CTRL, egui::Key::Y)
                    || i.consume_key(
                        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                        egui::Key::Z,
                    )
                    || i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z)
            });
            let do_undo = ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)
                    || i.consume_key(egui::Modifiers::CTRL, egui::Key::Z)
            });
            if do_redo {
                if self.history.redo(&self.graph_resource) {
                    log::debug!("redo");
                }
            } else if do_undo && self.history.undo(&self.graph_resource) {
                log::debug!("undo");
            }
        }

        // 搜索命中巡览：F3 跳下一个命中并居中、Shift+F3 上一个、Esc 清除画布命中高亮。
        // 与 Ctrl+P / 撤销同范式，在画布 state_manager 渲染前截获并 consume（§3.4：输入只在
        // app.rs 顶部 consume）。命中集 / 游标读自隐式状态总线（命令面板搜索写入）。
        self.handle_search_hits_nav(&ctx, text_focus);

        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::MenuBar::new().ui(ui, |ui| {
                // File 菜单：Save/Load **双 target 都显示**。内部按 cfg 分派——native 走
                // rfd + tokio（io::download / io::request_open_file 内部门控），wasm 走 web-sys
                // 浏览器下载 / FileReader（同上）。统一 IO 网关见 crate::io，避免两套逻辑漂移。
                ui.menu_button("File", |ui| {
                    if ui.button("New").clicked() {
                        log::info!("new");
                        // 整图被 New 替换：作废任何在途的暂存快照（如正编辑/拖拽中点 New），
                        // 它对新图无意义，留着会在退出编辑那帧产生多余撤销项。
                        self.history.discard_staged();
                        // New = 清空整图，作为一个可撤销单元（撤销后旧图整体复活）。
                        // history.mutate 在写闭包外先克隆 before 压栈（§3.1），再 reset。
                        self.history
                            .mutate(&self.graph_resource, |graph| graph.reset());
                    }

                    if ui.button("Save").clicked() {
                        ui.close();
                        // 读出图 + 画布，序列化为带 schema 版本号的开放 JSON 文档。
                        match self.graph_resource.read_resource(|graph| {
                            self.canvas_resource.read_resource(|canvas| {
                                crate::persistence::save_string(graph, Some(canvas))
                            })
                        }) {
                            // native：trigger_download 需 tokio runtime 句柄落盘；
                            // wasm：trigger_download 同步触发浏览器下载（无 runtime 参数）。
                            #[cfg(not(target_arch = "wasm32"))]
                            Ok(data) => crate::io::download::trigger_download(
                                &self.runtime,
                                "cognitheon.cnt",
                                data.as_bytes(),
                                "application/json",
                            ),
                            #[cfg(target_arch = "wasm32")]
                            Ok(data) => crate::io::download::trigger_download(
                                "cognitheon.cnt",
                                data.as_bytes(),
                                "application/json",
                            ),
                            Err(e) => log::error!("serialize failed: {e}"),
                        }
                    }

                    if ui.button("Load").clicked() {
                        ui.close();
                        // 导入天生异步：弹选择器、读字节后写入 io 总线，下一帧由 take_loaded_bytes
                        // 消费 → replace_document（native 同帧 block_on 读出后写总线；wasm 经 FileReader
                        // 异步回调跨帧写总线）。native 需 runtime 句柄，wasm 无。
                        #[cfg(not(target_arch = "wasm32"))]
                        crate::io::request_open_file(&ctx, &self.runtime);
                        #[cfg(target_arch = "wasm32")]
                        crate::io::request_open_file(&ctx);
                    }

                    ui.separator();

                    // 导出为 Markdown（vault）：每节点一个 .md（Obsidian 兼容，[[链接]] 原样）+
                    // 手画边旁路。native 弹目录选择写盘 / wasm 打包 store-zip 下载（行为不对称、属预期）。
                    // 只读图、不进 history（导出非图变更，见 export_vault 文档）。
                    if ui
                        .button("导出为 Markdown（vault）")
                        .on_hover_text(
                            "每个节点导成 .md（Obsidian 兼容，[[链接]] 原样）。\
                             native 选目录写文件；wasm 下载 zip。",
                        )
                        .clicked()
                    {
                        ui.close();
                        self.export_vault();
                    }

                    // Quit 仅 native 有意义（关闭桌面窗口）；wasm 是浏览器标签页，无此动作。
                    #[cfg(not(target_arch = "wasm32"))]
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.add_space(16.0);

                // Edit 菜单：撤销 / 重做（双 target 都可用）。按 can_undo/can_redo 灰显，
                // 走与快捷键同一入口（self.history），保证两条路径语义一致。
                //
                // 编辑态守卫：与快捷键路径『编辑态不接管整图 Ctrl+Z』语义对称——编辑态下整图
                // 撤销/重做会替换整图并清 editing_node，但 staged 编辑快照仍滞留，下一帧
                // resolve_on_exit_edit 会把过期 staged 错序 commit 成脏撤销项。故编辑态直接禁用。
                // §3.1：editing 的读锁在 with_resource/mutate 之外单独取（read_resource 闭包
                // 作用域 = 锁作用域，求值后锁即释放），不与 history 内部锁/图写锁嵌套重入。
                let editing = self
                    .graph_resource
                    .read_resource(|g| g.get_editing_node().is_some());
                let can_undo = self.history.can_undo();
                let can_redo = self.history.can_redo();
                ui.menu_button("Edit", |ui| {
                    if ui
                        .add_enabled(
                            can_undo && !editing,
                            egui::Button::new("Undo").shortcut_text("Ctrl+Z"),
                        )
                        .clicked()
                    {
                        ui.close();
                        self.history.undo(&self.graph_resource);
                    }
                    if ui
                        .add_enabled(
                            can_redo && !editing,
                            egui::Button::new("Redo").shortcut_text("Ctrl+Y"),
                        )
                        .clicked()
                    {
                        ui.close();
                        self.history.redo(&self.graph_resource);
                    }

                    ui.separator();

                    // 删除选中：按当前选区类型显示「删除 N 项」，无选区灰显。删除经 remove_selected
                    // （纯图层）+ history.mutate 包裹可撤销；删点连带删邻接边、删后清选区（§3.3）。
                    // 编辑态禁用（与 Undo/Redo 对称——编辑期不应整图删除）。
                    use crate::graph::selection::GraphSelection;
                    let sel_count = self.graph_resource.read_resource(|g| match &g.selected {
                        GraphSelection::Node(ns) => ns.len(),
                        GraphSelection::Edge(es) => es.len(),
                        GraphSelection::None => 0,
                    });
                    let del_label = if sel_count > 0 {
                        format!("删除选中（{sel_count} 项）")
                    } else {
                        "删除选中".to_owned()
                    };
                    if ui
                        .add_enabled(
                            sel_count > 0 && !editing,
                            egui::Button::new(del_label).shortcut_text("Delete"),
                        )
                        .clicked()
                    {
                        ui.close();
                        self.history.mutate(&self.graph_resource, |g| {
                            g.remove_selected();
                        });
                    }
                });

                ui.add_space(16.0);

                egui::widgets::global_theme_preference_buttons(ui);
                // 获取全局主题
                // let theme = ui.ctx().theme();
                // println!("theme: {:?}", theme);

                if ui.button("test").clicked() {
                    log::debug!("test");
                    // egui::Window::new("test").show(ctx, |ui| {
                    //     ui.label("test");
                    // });
                }

                // 一键力导向布局：按连接关系把图自然铺开，随后缩放/平移到能看见全部节点。
                if ui
                    .button("整理布局")
                    .on_hover_text("力导向自动布局：相连节点靠近、不相连分散")
                    .clicked()
                {
                    // 整理布局批量改 Node.position，作为一个可撤销单元：history.mutate 在写闭包外
                    // 先克隆 before 压栈（§3.1），再跑力导向。撤销可让所有节点回到布局前位置。
                    let affected = self.history.mutate(&self.graph_resource, |graph| {
                        layout::layout_graph(graph, LayoutParams::default())
                    });
                    log::info!("force-directed layout applied to {affected} nodes");
                    // 加分项：布局后缩放/平移到能看见全部节点（zoom-to-fit）。
                    self.zoom_to_fit(&ctx);
                }

                // EdgeType 是 Graph 的序列化字段（参与 history 的 graph_data_differs 比较），
                // 故切换必须经 history.mutate 成为一个独立可撤销单元（写闭包外先克隆 before
                // 压 undo + 清 redo，§3.1 锁安全由 mutate 封装保证），不再直接 with_resource 绕过。
                // 编辑态禁用该 ComboBox：避免其变更与正在暂存的 edit 快照交叠造成语义错乱
                // （复用上面已读出的 editing —— 同一帧、读锁早已释放，无重入）。
                let current_edge_type = self
                    .graph_resource
                    .read_resource(|graph| graph.edge_type.clone());
                let mut edge_type = current_edge_type.clone();
                ui.add_enabled_ui(!editing, |ui| {
                    ComboBox::from_label("Edge Type")
                        .selected_text(format!("{:?}", edge_type))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut edge_type, EdgeType::Bezier, "Bezier");
                            ui.selectable_value(&mut edge_type, EdgeType::Line, "Line");
                        });
                });
                // 仅当新值与当前不同才 mutate：选回同值不产生空撤销项。
                if edge_type != current_edge_type {
                    self.history
                        .mutate(&self.graph_resource, |graph| graph.edge_type = edge_type);
                }

                ui.add_space(16.0);

                // 关键词过滤（大图降噪）：输入框 + Dim/Hide 模式切换。纯 UI / temp data，不进 history。
                Self::show_filter_controls(ui);

                ui.add_space(16.0);

                // 帮助入口：与 ? / F1 共用同一 `keymap_help_open` 标志（纯 UI、不进序列化），
                // 点击翻转开关并维持与命令面板的互斥（打开帮助时关掉命令面板）。
                ui.menu_button("帮助", |ui| {
                    if ui
                        .add(egui::Button::new("快捷键 / 手势").shortcut_text("? / F1"))
                        .clicked()
                    {
                        ui.close();
                        let help_open_id = Id::new("keymap_help_open");
                        let now_open = !ctx
                            .data(|d| d.get_temp::<bool>(help_open_id))
                            .unwrap_or(false);
                        ctx.data_mut(|d| {
                            d.insert_temp(help_open_id, now_open);
                            if now_open {
                                // 浮层互斥：打开帮助时关掉命令面板（清其全部 temp 状态）。
                                d.remove::<bool>(Id::new("command_palette_open"));
                                d.remove::<String>(Id::new("command_palette_query"));
                                d.remove::<usize>(Id::new("command_palette_sel"));
                                d.remove::<bool>(Id::new("command_palette_just_opened"));
                            }
                        });
                    }
                });
            });
        });

        egui::Panel::bottom("bottom_panel").show_inside(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
                    current_zoom(ui, &self.canvas_resource);
                    current_offset(ui, &self.canvas_resource);
                    current_input_state(ui, &self.canvas_widget.input_manager);
                    current_fps(ui, &self.canvas_widget.input_manager);
                });
                ui.end_row();
                ui.with_layout(
                    egui::Layout::left_to_right(egui::Align::LEFT)
                        .with_main_justify(true)
                        .with_cross_justify(true),
                    |ui| {
                        // 两端对齐
                        powered_by_egui_and_eframe(ui);
                        // ui.add_space();
                        // ui.label("test");
                        ui.with_layout(Layout::right_to_left(Align::RIGHT), |ui| {
                            let label = ui.label(
                                RichText::new("⚠ Debug build ⚠")
                                    .small()
                                    .color(ui.visuals().warn_fg_color),
                            );
                            label.on_hover_text("egui was compiled with debug assertions enabled.");
                        });
                    },
                );
            });
        });

        // 右侧链接面板：选中节点的出链 + 反向引用（含上下文原话），点击跳转聚焦
        egui::Panel::right("links_panel")
            .default_size(260.0)
            .show_inside(ui, |ui| {
                self.show_links_panel(ui);
            });

        egui::CentralPanel::default()
            // .frame(egui::Frame::default().outer_margin(egui::Margin::same(3.0)))
            .show_inside(ui, |ui| {
                // 画布主渲染（内部依次 draw_grid → state_manager → render_graph → 粒子）。
                // 取回 Response.rect = 画布实际屏幕区域，供 minimap 锚定右下角 + 反算视口框。
                let canvas_rect = ui.add(&mut self.canvas_widget).rect;

                // 小地图 / 鸟瞰图：右下角常驻只读投影 + 点击平移。在画布（含 render_graph）渲染
                // 之后调用，z-order 在节点之上（§3.5）；空图自动隐藏。它用独立的 Area 消费自身
                // 指针交互、不穿透画布状态机；只读 Node.position 投影、点击平移仅改
                // transform.translation（scaling 不变，§3.2）。
                crate::ui::minimap::show_minimap(
                    &ctx,
                    canvas_rect,
                    &self.graph_resource,
                    &self.canvas_resource,
                );

                // egui::Window::new("test")
                //     .default_size(Vec2::new(800.0, 600.0))
                //     .show(ctx, |ui| {
                //         ui.label("test");
                //     });
            });

        // 读模式正文里点击 [[已存在标题]] 的跨层 focus 请求（隐式状态总线）：NodeWidget 渲染时
        // 写入目标 NodeIndex，这里读取后复用既有 focus_node（选中 + 居中），随即清除——
        // 单一 focus 实现、note 原文不被改写。放在画布渲染之后，确保拿得到本帧写入的请求。
        if let Some(target) = ctx.data_mut(|d| {
            d.remove_temp::<petgraph::graph::NodeIndex>(Id::new(crate::ui::node::FOCUS_REQUEST_KEY))
        }) {
            self.focus_node(&ctx, target);
        }

        // 右键上下文菜单（隐式状态总线）：状态机判为"右键单击"时把请求写入 temp data，这里在画布
        // 渲染之后读取并自管弹出（egui::Popup::new(id, ctx, anchor, layer_id)）。放在画布渲染之后，
        // 确保拿得到本帧请求；菜单关闭时清掉请求。
        self.show_context_menu(&ctx);

        // ctx.show_viewport_deferred(
        //     ViewportId::from_hash_of("test"),
        //     ViewportBuilder::default().with_title("testwindow"),
        //     |ctx, _viewport_class| {},
        // );
        // if let Some(rs) = ctx..as_ref() {
        //     rs.renderer.write().callback_resources.clear();
        // }
        ctx.request_repaint();
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}

fn current_zoom(ui: &mut egui::Ui, canvas_state_resource: &CanvasStateResource) {
    // 获取当前缩放
    canvas_state_resource.read_resource(|canvas_state| {
        ui.label(format!("zoom: {:.2}", canvas_state.transform.scaling));
    });
    // let zoom = ui.input(|i| i.zoom_delta());
    // ui.label(format!("zoom: {}", canvas_state.scale));
}

fn current_offset(ui: &mut egui::Ui, canvas_state_resource: &CanvasStateResource) {
    canvas_state_resource.read_resource(|canvas_state| {
        ui.label(format!("offset: {:?}", canvas_state.transform.translation));
    });
}

fn current_input_state(ui: &mut egui::Ui, input_state_manager: &InputStateManager) {
    let input_state = &input_state_manager.current_state;
    ui.label(format!("input_state: {:?}", input_state));
}

fn current_fps(ui: &mut egui::Ui, _input_state_manager: &InputStateManager) {
    let dt = ui.ctx().input(|i| i.stable_dt);
    ui.label(format!("fps: {:?}", 1.0 / dt));
}

fn setup_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "source_hans_sans".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/SourceHanSansSC-Regular.otf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "source_hans_sans".to_owned());

    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "source_hans_sans".to_owned());

    // 在插入字体后添加调试输出
    log::debug!(
        "Font data size: {:?} bytes",
        fonts.font_data["source_hans_sans"].font.len()
    );

    // Tell egui to use these fonts:
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job_text(text: &str, query: &str) -> LayoutJob {
        highlight_job(
            text,
            query,
            Color32::WHITE,
            Color32::RED,
            FontId::proportional(14.0),
        )
    }

    /// 着色后的 LayoutJob.text 必须逐字节覆盖源文本，否则 egui galley 会 panic。
    #[test]
    fn highlight_covers_source_byte_for_byte() {
        for (text, query) in [
            ("", "x"),
            ("纯中文标题", ""),
            ("Hello World", "world"),
            ("知识图谱与第二大脑", "图谱"),
            ("AaAaA", "a"),
            ("重复重复重复", "重复"),
            ("末尾命中关键词", "关键词"),
            ("关键词在开头", "关键词"),
            ("no match here", "zzz"),
            ("混合 Mixed 大小写 CASE", "case"),
        ] {
            let job = job_text(text, query);
            assert_eq!(job.text, text, "源={text:?} query={query:?} 必须逐字节覆盖");
        }
    }

    /// 命中段着 hit 色、其余 base 色；区间端点落在 char 边界（中文不 panic）。
    #[test]
    fn highlight_marks_matched_segments() {
        let job = job_text("前缀关键词后缀", "关键词");
        // 段切分：前缀 / 关键词 / 后缀
        assert_eq!(job.sections.len(), 3);
        let colors: Vec<Color32> = job.sections.iter().map(|s| s.format.color).collect();
        assert_eq!(colors, vec![Color32::WHITE, Color32::RED, Color32::WHITE]);
    }

    /// query 为空或无命中：整段单一 base 段，零着色。
    #[test]
    fn highlight_no_query_is_all_base() {
        let job = job_text("任意文本", "");
        assert_eq!(job.sections.len(), 1);
        assert_eq!(job.sections[0].format.color, Color32::WHITE);
    }
}
