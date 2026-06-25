use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Color32, ComboBox, FontId, Id, Layout, RichText};
#[cfg(not(target_arch = "wasm32"))]
use rfd::AsyncFileDialog;
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
/// - 历史 / 搜索：`app.rs::ui` 顶部的 Ctrl/Cmd+Z、Ctrl/Cmd+Y、Ctrl/Cmd+Shift+Z、Ctrl/Cmd+P。
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
        &[(
            "Ctrl / Cmd + P",
            "命令面板（搜索；↑↓ 移动、Enter 跳转、Esc 关闭）",
        )],
    ),
    (
        "布局",
        &[("菜单「整理布局」", "力导向自动布局 + 缩放至全部可见")],
    ),
    ("帮助", &[("? / F1", "打开 / 关闭本帮助")]),
];

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

        let selected = self
            .graph_resource
            .read_resource(|g| g.get_selected_nodes().first().copied());
        let Some(idx) = selected else {
            ui.weak("选中一个节点，查看它的出链与反向引用。");
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
            // query 变化：写回并把高亮复位到第一项。
            ctx.data_mut(|d| {
                d.insert_temp(query_id, query.clone());
                d.insert_temp(sel_id, 0usize);
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
        let bbox = self.graph_resource.read_resource(|g| {
            let mut it = g.graph.node_indices().map(|i| g.graph[i].position);
            let first = it.next()?;
            let mut rect = egui::Rect::from_min_max(first, first);
            for p in it {
                rect = rect.union(egui::Rect::from_min_max(p, p));
            }
            Some(rect)
        });
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

        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::MenuBar::new().ui(ui, |ui| {
                // NOTE: 文件 Save/Load 走 tokio + rfd，仅 native；web 上不显示 File 菜单。
                #[cfg(not(target_arch = "wasm32"))]
                {
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
                            // 读出图 + 画布，序列化为带 schema 版本号的开放 JSON 文档
                            match self.graph_resource.read_resource(|graph| {
                                self.canvas_resource.read_resource(|canvas| {
                                    crate::persistence::save_string(graph, Some(canvas))
                                })
                            }) {
                                Ok(data) => {
                                    let future = async move {
                                        if let Some(file) = AsyncFileDialog::new()
                                            .add_filter("Cognitheon", &["cnt"])
                                            .set_directory("~")
                                            .save_file()
                                            .await
                                        {
                                            match file.write(data.as_bytes()).await {
                                                Ok(_) => log::info!("save success"),
                                                Err(e) => log::error!("save failed: {e}"),
                                            }
                                        }
                                    };
                                    self.runtime.block_on(future);
                                }
                                Err(e) => log::error!("serialize failed: {e}"),
                            }
                        }

                        if ui.button("Load").clicked() {
                            ui.close();
                            let future = async {
                                match AsyncFileDialog::new()
                                    .add_filter("Cognitheon", &["cnt"])
                                    .set_directory("~")
                                    .pick_file()
                                    .await
                                {
                                    Some(file) => Some(file.read().await),
                                    None => None,
                                }
                            };
                            if let Some(data) = self.runtime.block_on(future) {
                                // 兼容旧 .cnt：persistence::load 会回退解析无版本号的旧格式
                                match crate::persistence::load(&data) {
                                    Ok(doc) => {
                                        let (graph, canvas) = doc.into_parts();
                                        // 整图被 Load 替换：作废任何在途暂存快照（同 New）。
                                        self.history.discard_staged();
                                        // Load = 替换整图，作为一个可撤销单元：先把"载入前"的图压入
                                        // undo（在替换资源 Arc 之前、写闭包外克隆，§3.1）。撤销 Load
                                        // 会让旧图整体复活（视图/缩放与 id 计数器不回滚，符合 spec）。
                                        let before =
                                            self.graph_resource.read_resource(|g| g.clone());
                                        self.history.record(before);
                                        self.graph_resource = GraphResource::new(graph);
                                        self.canvas_resource = CanvasStateResource::new(canvas);
                                        // 复用同一个 history 句柄（§3.1：共享同一真源）——务必传 clone，
                                        // 否则 Load 后菜单/快捷键与状态机会写到不同历史栈。
                                        self.canvas_widget = CanvasWidget::new(
                                            self.graph_resource.clone(),
                                            self.canvas_resource.clone(),
                                            self.history.clone(),
                                        );
                                    }
                                    Err(e) => log::error!("load failed: {e}"),
                                }
                            }
                        }

                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.add_space(16.0);
                }

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
                ui.add(&mut self.canvas_widget);

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
