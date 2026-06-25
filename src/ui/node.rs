use std::sync::Arc;

use crate::graph::node_observer::NodeObserver;
use crate::graph::render_info::NodeRenderInfo;
use crate::graph::selection::GraphSelection;
use crate::history::History;
use crate::resource::{CanvasStateResource, GraphResource};
use crate::wikilink;
use egui::{Id, Sense, Stroke, Widget};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use petgraph::graph::NodeIndex;

use crate::colors::{node_background, node_border, node_border_hit, node_border_selected};

thread_local! {
    /// 复用的 Markdown 渲染缓存（egui 单线程；避免每帧每节点重建）。
    static MARKDOWN_CACHE: std::cell::RefCell<CommonMarkCache> =
        std::cell::RefCell::new(CommonMarkCache::default());
}

/// `[[` 自动补全 popup 打开时，于 TextEdit 渲染前拦截到的键盘动作。
///
/// 经 egui temp data 从 [`NodeWidget::show_editor`] 顶部的拦截点传递到
/// [`NodeWidget::wikilink_autocomplete`]——之所以要先拦截再传递，是因为 multiline
/// `TextEdit` 会在自己的渲染里把 Enter / Tab 当文本吃掉，必须赶在它之前 `consume_key`。
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum AcKey {
    #[default]
    None,
    Up,
    Down,
    Confirm,
    Close,
}

pub struct NodeWidget {
    pub node_index: NodeIndex,
    pub graph_resource: GraphResource,
    pub canvas_state_resource: CanvasStateResource,
    /// 撤销/重做历史（构造函数注入）：本 widget 内会改图数据的写入（删点）经它打快照。
    pub history: History,
    pub observers: Vec<Arc<dyn NodeObserver>>,
    // pub graph: &'a mut Graph,
    // pub canvas_state: &'a mut CanvasState,
}

impl NodeWidget {
    pub fn add_observer(&mut self, observer: Arc<dyn NodeObserver>) {
        self.observers.push(observer);
    }

    pub fn remove_observer(&mut self, observer_id: usize) {
        self.observers.remove(observer_id);
    }
}

impl NodeWidget {
    pub fn new(
        node_index: NodeIndex,
        graph_resource: GraphResource,
        canvas_state_resource: CanvasStateResource,
        history: History,
    ) -> Self {
        Self {
            node_index,
            graph_resource,
            canvas_state_resource,
            history,
            observers: vec![],
        }
    }

    pub fn setup_actions(&mut self, response: &egui::Response, ui: &mut egui::Ui) {
        let input_busy = ui.ctx().data(|d| d.get_temp(Id::new("input_busy")));
        if input_busy.is_some() && input_busy.unwrap() {
            return;
        }
        // self.handle_secondary_drag(ui, response);

        // self.handle_secondary_drag(ui, response);

        if response.double_clicked() {
            log::debug!("node double clicked: {:?}", self.node_index);
            self.graph_resource.with_resource(|graph| {
                graph.set_editing_node(Some(self.node_index));
            });
        }

        // 检测单击事件
        if response.clicked()
            && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary))
        {
            log::debug!("node clicked: {:?}", self.node_index);
            self.graph_resource.with_resource(|graph| {
                if graph.get_editing_node() != Some(self.node_index) {
                    graph.set_editing_node(None);
                }
                graph.selected.clear();
                graph.select_node(self.node_index);
            });
        }

        // 处理键盘按键
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.graph_resource.with_resource(|graph| {
                graph.selected.clear();
                graph.set_editing_node(None);
            });
        }

        if ui.input(|i| i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete)) {
            // 先在写闭包外判定是否真要删，避免无条件打快照（§3.1：克隆在写闭包外单独 read）。
            let should_delete = self.graph_resource.read_resource(|graph| {
                matches!(&graph.selected, GraphSelection::Node(ns) if ns.contains(&self.node_index))
                    && graph.editing_node.is_none()
                    && graph.get_node(self.node_index).is_some()
            });
            if should_delete {
                log::debug!("node deleted: {:?}", self.node_index);
                // 删点（连同其边）经 history 打一次快照，撤销可让节点连边复活（§3.3 索引稳定）。
                self.history.mutate(&self.graph_resource, |graph| {
                    graph.remove_node(self.node_index);
                });
            }
        }

        // Ctrl + Enter
        if ui.input(|i| {
            i.key_pressed(egui::Key::Enter) && i.modifiers.contains(egui::Modifiers::CTRL)
        }) && self
            .graph_resource
            .read_resource(|graph| graph.get_editing_node())
            == Some(self.node_index)
        {
            log::debug!("node enter: {:?}", self.node_index);
            self.graph_resource.with_resource(|graph| {
                graph.set_editing_node(None);
            });
        }

        // 处理拖动事件
        // if response.dragged_by(egui::PointerButton::Primary)
        //     && self
        //         .graph_resource
        //         .read_resource(|graph| graph.get_editing_node())
        //         != Some(self.node_index)
        // {
        //     self.graph_resource.with_resource(|graph| {
        //         graph.set_editing_node(None);
        //     });
        //     // println!("node dragged: {:?}", self.node_index);
        //     let drag_delta = response.drag_delta()
        //         / (self
        //             .canvas_state_resource
        //             .read_resource(|canvas_state| canvas_state.transform.scaling));
        //     self.graph_resource.with_resource(|graph| {
        //         let node = graph.get_node_mut(self.node_index).unwrap();
        //         node.position += drag_delta;
        //         // let canvas_rect = self
        //         //     .canvas_state_resource
        //         //     .read_resource(|canvas_state| canvas_state.to_canvas_rect(response.rect));
        //         // let render_info = NodeRenderInfo { canvas_rect };
        //         // node.render_info = Some(render_info);
        //     });
        // }
    }

    // fn handle_secondary_drag(&mut self, ui: &mut egui::Ui, response: &egui::Response) {
    //     // 处理右键拖动
    //     // 处理右键拖动
    //     if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary))
    //         && response.hovered()
    //     {
    //         println!("right button drag started");
    //         let mouse_screen_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
    //         let mouse_canvas_pos = self
    //             .canvas_state_resource
    //             .read_resource(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));
    //         let node_render_info: NodeRenderInfo = ui
    //             .ctx()
    //             .data(|d| d.get_temp(Id::new(self.node_index.index().to_string())))
    //             .unwrap();

    //         let node_canvas_center = node_render_info.canvas_center();
    //         // let canvas_pos = canvas_state_resource
    //         //     .read_resource(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));

    //         // if let Some(node_index) = self.hit_test(ui, mouse_screen_pos) {
    //         // 创建临时边
    //         let temp_edge = TempEdge {
    //             edge_type: EdgeType::Bezier(BezierEdge {
    //                 source_anchor: Anchor::new_smooth(node_canvas_center),
    //                 target_anchor: Anchor::new_smooth(mouse_canvas_pos),
    //                 control_anchors: vec![],
    //             }),
    //             source: self.node_index,
    //             target: TempEdgeTarget::Point(mouse_canvas_pos),
    //         };
    //         self.graph_resource.with_resource(|graph| {
    //             graph.set_temp_edge(None);
    //             graph.set_temp_edge(Some(temp_edge));
    //         });
    //         // }
    //         // });
    //     }

    //     if ui.input(|i| i.pointer.button_down(egui::PointerButton::Secondary)) {
    //         // if response.dragged_by(egui::PointerButton::Secondary) {
    //         println!("right button dragging");
    //         let mouse_screen_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
    //         println!("mouse_screen_pos: {:?}", mouse_screen_pos);
    //         let node_render_info: NodeRenderInfo = ui
    //             .ctx()
    //             .data(|d| d.get_temp(Id::new(self.node_index.index().to_string())))
    //             .unwrap();

    //         let node_canvas_center = node_render_info.canvas_center();
    //         self.graph_resource.with_resource(|graph| {
    //             let temp_edge = graph.get_temp_edge();
    //             println!("====temp_edge: {:?}====", temp_edge);
    //             let mouse_canvas_pos = self
    //                 .canvas_state_resource
    //                 .read_resource(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));
    //             println!("====mouse_canvas_pos: {:?}====", mouse_canvas_pos);
    //             if let Some(temp_edge_clone) = temp_edge.clone() {
    //                 let control_anchors =
    //                     if let EdgeType::Bezier(bezier_edge) = temp_edge_clone.edge_type {
    //                         bezier_edge.control_anchors
    //                     } else {
    //                         vec![]
    //                     };
    //                 // 更新临时边目标坐标
    //                 let new_temp_edge = TempEdge {
    //                     source: temp_edge_clone.source,
    //                     target: TempEdgeTarget::Point(mouse_canvas_pos),
    //                     edge_type: EdgeType::Bezier(BezierEdge {
    //                         source_anchor: Anchor::new_smooth(node_canvas_center),
    //                         target_anchor: Anchor::new_smooth(mouse_canvas_pos),
    //                         control_anchors,
    //                     }),
    //                 };
    //                 graph.set_temp_edge(Some(new_temp_edge));
    //             }
    //         });
    //         // println!("mouse_screen_pos: {:?}", mouse_screen_pos);
    //     }
    //     // println!(
    //     //     "NodeWidget::setup_actions: {:?}",
    //     //     ui.input(|i| i.pointer.hover_pos())
    //     // );

    //     if ui.input(|i| i.pointer.button_released(egui::PointerButton::Secondary)) {
    //         // if response.drag_stopped_by(egui::PointerButton::Secondary) {
    //         let mouse_screen_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
    //         let mouse_canvas_pos = self
    //             .canvas_state_resource
    //             .read_resource(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));
    //         println!("right button drag stopped");
    //         // 创建新的节点，并创建边
    //         let node_render_info: NodeRenderInfo = ui
    //             .ctx()
    //             .data(|d| d.get_temp(Id::new(self.node_index.index().to_string())))
    //             .unwrap();
    //         let node_canvas_center = node_render_info.canvas_center();

    //         let new_node_id = self
    //             .canvas_state_resource
    //             .read_resource(|canvas_state| canvas_state.new_node_id());
    //         let new_node = Node {
    //             id: new_node_id,
    //             text: String::new(),
    //             note: String::new(),
    //             position: mouse_canvas_pos,
    //             render_info: None,
    //         };
    //         let new_edge_id = self
    //             .canvas_state_resource
    //             .read_resource(|canvas_state| canvas_state.new_edge_id());
    //         let new_edge = Edge {
    //             id: new_edge_id,
    //             source: self.node_index,
    //             target: NodeIndex::new(new_node_id.try_into().unwrap()),
    //             text: None,
    //             edge_type: EdgeType::Bezier(BezierEdge {
    //                 source_anchor: Anchor::new_smooth(node_canvas_center),
    //                 target_anchor: Anchor::new_smooth(mouse_canvas_pos),
    //                 control_anchors: vec![],
    //             }),
    //         };
    //         self.graph_resource.with_resource(|graph| {
    //             let node_index = graph.add_node(new_node);
    //             graph.set_selected_node(Some(node_index));
    //             graph.set_editing_node(Some(node_index));
    //             graph.add_edge(new_edge);
    //             graph.set_temp_edge(None);
    //         });

    //         // graph_resource.with_resource(|graph| {
    //         //     graph.set_temp_edge(None);
    //         // });
    //     }
    // }
}

/// 卡片固定像素宽（v1：内容不随画布缩放缩放，保证 Markdown / 中文始终可读）。
const CARD_WIDTH: f32 = 240.0;
/// 正文区最大高度，超出滚动。
const BODY_MAX_HEIGHT: f32 = 160.0;

/// 读模式正文里 `[[标题]]` 渲染为可点链接时使用的自定义 URL scheme 前缀。
///
/// 形如 `wikilink:0` / `wikilink:1`——纯 ASCII 数字下标，避开标题里中文 / 空格 /
/// markdown 特殊字符的 URL 编码问题；真正的标题与目标 [`NodeIndex`] 经一张当帧重建的
/// 旁表传递（见 [`NodeWidget::show_body_markdown`]），不进 URL。
const WIKILINK_SCHEME: &str = "wikilink:";

/// 跨层"focus 请求"总线 key（隐式状态总线约定）：读模式正文里点击 `[[已存在标题]]` 时，
/// [`NodeWidget`] 往 egui temp data 写入目标 [`NodeIndex`]，由 [`crate::app::CognitheonApp::ui`]
/// 读取并调用既有 `focus_node`（选中 + 居中）后清除——复用单一 focus 实现、零跨层耦合。
pub const FOCUS_REQUEST_KEY: &str = "focus_request_node";

/// 搜索命中高亮集总线 key（隐式状态总线约定，纯 UI、不进序列化）：命令面板搜索时把当前
/// 命中的 [`NodeIndex`] 列表写入 egui temp data（`Vec<NodeIndex>`），[`NodeWidget`] 渲染末尾
/// 读它，若本节点在集内则补画一圈命中色描边（[`crate::colors::node_border_hit`]）。
/// F3 / Shift+F3 巡览靠配套的游标 key（见 [`crate::app`]），Esc 清空本集即清除高亮。
pub const SEARCH_HITS_KEY: &str = "search_hits";

/// 转义双链标题作为 markdown 链接显示文本 `[…]`：反斜杠转义会破坏链接文本闭合的字符
/// （`\`、`[`、`]`），使含 markdown 特殊字符的标题（如 `a[b]`）能完整、安全地显示。
fn escape_md_link_text(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for ch in title.chars() {
        if matches!(ch, '\\' | '[' | ']') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

impl Widget for NodeWidget {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        // 读取节点数据与状态
        let (title, body, position) = self.graph_resource.read_resource(|graph| {
            let n = graph.get_node(self.node_index).unwrap();
            (n.text.clone(), n.note.clone(), n.position)
        });
        let editing =
            self.graph_resource.read_resource(|g| g.get_editing_node()) == Some(self.node_index);
        let selected = self.graph_resource.read_resource(
            |g| matches!(&g.selected, GraphSelection::Node(ns) if ns.contains(&self.node_index)),
        );

        // 搜索命中高亮：是否在当前命中集内（隐式状态总线，纯 UI、§3.3 只依赖本 widget 自身
        // rect，不读跨组件 observer 几何）。命中集由命令面板搜索写入（见 [`crate::app`]），
        // Esc 清空即不再高亮。失效/不存在时 `get_temp` 返回 None → `is_hit = false`，安全早退。
        let is_hit = ui.ctx().data(|d| {
            d.get_temp::<Vec<NodeIndex>>(Id::new(SEARCH_HITS_KEY))
                .is_some_and(|hits| hits.contains(&self.node_index))
        });

        let screen_pos = self
            .canvas_state_resource
            .read_resource(|c| c.to_screen(position));

        let theme = ui.ctx().theme();

        // 卡片边框：Frame 的 stroke 固定为常量 1.0（color = 未选中态边框色），使内容区
        // geometry 在选中/未选中时一致——egui Frame 的描边是 StrokeKind::Inside 且 widget_rect
        // 随 stroke.width 增长，若选中时加粗会挤占内容、引发布局颤动。选中态改为在卡片 rect
        // 外侧另画一圈更粗的边框（见下方 rect_stroke + StrokeKind::Outside），不侵占内容。
        let base_border = node_border(theme);

        // 在节点屏幕位置分配一个子 UI 渲染卡片（内容决定高度）
        let builder = egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_size(
                screen_pos,
                egui::vec2(CARD_WIDTH, 100_000.0),
            ))
            .layout(egui::Layout::top_down(egui::Align::Min));
        let card = ui.scope_builder(builder, |ui| {
            egui::Frame::default()
                .fill(node_background(theme))
                .stroke(Stroke::new(1.0, base_border))
                .corner_radius(6)
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(CARD_WIDTH - 16.0);
                    if editing {
                        self.show_editor(ui, &title, &body);
                    } else {
                        if title.is_empty() {
                            ui.weak("（无标题）");
                        } else {
                            ui.label(egui::RichText::new(&title).strong().size(16.0));
                        }
                        if !body.is_empty() {
                            ui.separator();
                            egui::ScrollArea::vertical()
                                .max_height(BODY_MAX_HEIGHT)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    self.show_body_markdown(ui, &body);
                                });
                        }
                    }
                })
                .response
                .rect
        });

        let rect = card.inner;

        // 搜索命中态：在卡片 rect **更外侧**补画一圈琥珀/金色描边（命中色，区别于选中红 / 边 hover 蓝）。
        // 画在选中环之外（先画、半径更大、外扩 3px），故"选中 + 命中"时两环并存且不互相遮挡：
        // 内红外金，选中语义仍占优（红更贴近卡片）。描边像素与卡片本身一致——卡片是固定像素尺寸
        // （CARD_WIDTH，刻意不随画布缩放，见上方 Frame 注释），故描边宽 / 外扩量也用固定像素、
        // 不 ×scaling（§3.2 的 ×scaling 只适用于随画布缩放的画布空间元素；此卡片不缩放）。
        if is_hit {
            let hit_rect = rect.expand(3.0);
            ui.painter().rect_stroke(
                hit_rect,
                9.0, // = Frame 圆角 6 + 外扩 3，保持圆角同心
                Stroke::new(2.5, node_border_hit(theme)),
                egui::StrokeKind::Outside,
            );
        }

        // 选中态：在卡片 rect 外侧补画一圈更粗的边框（StrokeKind::Outside，向外扩展，
        // 不侵占内容、不改变布局 geometry → 无颤动）。corner_radius 与 Frame 的 6 对齐。
        if selected {
            ui.painter().rect_stroke(
                rect,
                6.0,
                Stroke::new(2.0, node_border_selected(theme)),
                egui::StrokeKind::Outside,
            );
        }

        // 读模式才在整张卡片上接管点击/拖拽（编辑模式让内部 TextEdit 处理输入）
        let response = if editing {
            card.response
        } else {
            let r = ui.interact(
                rect,
                ui.id().with(("node", self.node_index)),
                Sense::click_and_drag(),
            );
            self.setup_actions(&r, ui);
            r
        };

        // 发布几何（供边渲染 / 命中测试读取）
        let canvas_rect = self
            .canvas_state_resource
            .read_resource(|c| c.to_canvas_rect(rect));
        let render_info = NodeRenderInfo { canvas_rect };
        self.observers
            .iter()
            .for_each(|o| o.on_node_changed(self.node_index, render_info));

        response
    }
}

impl NodeWidget {
    /// 读模式正文渲染：把正文里的 `[[标题]]` 在**渲染时**转换为可点击的双链，点击后请求
    /// 跳转聚焦目标节点。**绝不改写 `Node.note` 原文**（SSOT）——转换只发生在这帧的临时字符串上。
    ///
    /// 实现要点：
    /// - **已存在**标题（`find_by_title` 命中）→ 替换为标准 markdown 链接 `[标题](wikilink:N)`，
    ///   并把 `wikilink:N → NodeIndex` 记进当帧旁表 + 经 `cache.add_link_hook` 注册为链接钩子；
    ///   egui_commonmark 见到已注册的 destination 会渲染成 `ui.link`（而非外部超链接），点击置位钩子。
    /// - **未创建**标题（无命中）→ **原样保留** `[[标题]]`，CommonMark 当作普通文本渲染，
    ///   天然与蓝色链接区分、不可点（符合"仅弱显不可跳转"的默认）。
    /// - destination 用纯数字下标（非标题本身），规避中文 / 空格 / markdown 特殊字符的 URL 编码坑；
    ///   普通 markdown 链接 `[x](http://…)` 因 destination 未注册为钩子，仍走原有外链逻辑，互不影响。
    fn show_body_markdown(&self, ui: &mut egui::Ui, body: &str) {
        // 收集正文里的 [[标题]]，解析出已存在的目标，构造"转换后的 markdown"+ destination→目标 旁表。
        let titles = wikilink::parse_links(body);
        let resolved: Vec<(String, NodeIndex)> = self.graph_resource.read_resource(|g| {
            titles
                .iter()
                .filter_map(|t| wikilink::find_by_title(g, t).map(|idx| (t.clone(), idx)))
                .collect()
        });

        MARKDOWN_CACHE.with_borrow_mut(|cache| {
            // 旁表每帧重建；先清掉上一帧（可能来自任意节点）残留的链接钩子，避免跨节点串味与无界增长。
            cache.link_hooks_clear();

            // 把已存在的 [[标题]] 整体替换为 [标题](wikilink:N)，并登记钩子。
            // 注意按"标题"做整体替换（含 [[ ]]）；同一标题多处出现共用同一 destination。
            let mut rendered = body.to_owned();
            for (i, (title, _idx)) in resolved.iter().enumerate() {
                let dest = format!("{WIKILINK_SCHEME}{i}");
                let needle = format!("[[{title}]]");
                let link = format!("[{}]({dest})", escape_md_link_text(title));
                rendered = rendered.replace(&needle, &link);
                cache.add_link_hook(dest);
            }

            CommonMarkViewer::new().show(ui, cache, &rendered);

            // 渲染后回读：哪个钩子被点了 → 写 focus 请求（跨层交给 app.rs 调 focus_node）。
            for (i, (_title, idx)) in resolved.iter().enumerate() {
                let dest = format!("{WIKILINK_SCHEME}{i}");
                if cache.get_link_hook(&dest) == Some(true) {
                    log::debug!("wikilink clicked in body -> focus {:?}", idx);
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(Id::new(FOCUS_REQUEST_KEY), *idx));
                }
            }
        });
    }

    /// 编辑态：标题单行 + 正文多行（Markdown 源）；Ctrl/Cmd+Enter 退出并解析 `[[双链]]`。
    fn show_editor(&self, ui: &mut egui::Ui, title: &str, body: &str) {
        let mut t = title.to_owned();
        let tr = ui.add(
            egui::TextEdit::singleline(&mut t)
                .hint_text("标题")
                .desired_width(f32::INFINITY),
        );
        if tr.changed() {
            self.graph_resource.with_resource(|g| {
                if let Some(n) = g.get_node_mut(self.node_index) {
                    n.text = t.clone();
                }
            });
        }

        ui.separator();

        let mut b = body.to_owned();

        // 补全 popup 的键盘拦截必须发生在 TextEdit 渲染之前——multiline TextEdit 会在自己的
        // `ui.add` 里把 Enter 当换行、Tab 当缩进吃掉。故：上一帧 popup 开着时，在此先把
        // ↑/↓/Tab/Enter/Esc 从输入队列 consume 掉（TextEdit 当帧便看不到），把动作暂存进
        // temp data，交由 `wikilink_autocomplete` 取用。key 命名遵循隐式状态总线风格。
        let body_id = ui.id().with(("node_body", self.node_index));
        let ac_open_last = ui
            .ctx()
            .data(|d| d.get_temp::<bool>(body_id.with("wikilink_ac_open")))
            .unwrap_or(false);
        if ac_open_last && ui.memory(|m| m.has_focus(body_id)) {
            let action = ui.input_mut(|i| {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
                    || i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                {
                    AcKey::Confirm
                } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    AcKey::Down
                } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    AcKey::Up
                } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                    AcKey::Close
                } else {
                    AcKey::None
                }
            });
            ui.ctx()
                .data_mut(|d| d.insert_temp(body_id.with("wikilink_ac_key"), action));
        }

        // Markdown 源码语法高亮（含 [[双链]] 高亮）
        let md_colors = crate::ui::md_highlight::MdColors::from_visuals(ui.visuals());
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let job = crate::ui::md_highlight::layout(buf.as_str(), 14.0, wrap_width, &md_colors);
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let br = egui::ScrollArea::vertical()
            .max_height(BODY_MAX_HEIGHT)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut b)
                        .id(body_id)
                        .hint_text("正文（Markdown；用 [[标题]] 建双链）")
                        .desired_width(f32::INFINITY)
                        .desired_rows(4)
                        .layouter(&mut layouter),
                )
            })
            .inner;
        if br.changed() {
            self.graph_resource.with_resource(|g| {
                if let Some(n) = g.get_node_mut(self.node_index) {
                    n.note = b.clone();
                }
            });
        }

        // [[ 自动补全：光标前是未闭合的 `[[query` 时，弹出匹配的已有标题；
        // 键盘 ↑/↓ 选择、Tab/Enter 确认、Esc 关闭，鼠标点击亦可确认。
        // popup 打开时无修饰键的 Enter 会被它消费，故下方 Ctrl+Enter 退出逻辑不会被误触。
        self.wikilink_autocomplete(ui, &br, &b);

        // Ctrl/Cmd + Enter：退出编辑，并把正文里的 [[双链]] 落到图上
        if ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command) {
            self.graph_resource.with_resource(|g| {
                g.set_editing_node(None);
                wikilink::resolve_links(g, &self.canvas_state_resource, self.node_index);
            });
        }
    }

    /// `[[` 自动补全：检测光标前未闭合的 `[[query`，弹出匹配的已有标题。
    ///
    /// 交互：↑/↓ 移动高亮、Tab/Enter 确认、Esc 关闭、鼠标点击亦可确认；确认后补全为
    /// `[[标题]]` 并把光标移到 `]]` 之后。返回值表示本帧 popup 是否处于打开态。
    ///
    /// 架构要点（修复焦点陷阱）：popup 用 [`egui::Popup::from_response`] 渲染在独立 popup 图层、
    /// `Sense::click()`，故点击候选项不依赖 `TextEdit` 当帧是否持焦，`clicked()` 能在释放帧捕获；
    /// 是否打开仅由"光标处于未闭合 `[[query` 上下文"决定（该上下文跨焦点转移帧依然成立），
    /// 不再以 `br.has_focus()` 当帧门控，从根上消除"点击转移焦点导致 popup 当帧消失"。
    /// 高亮索引持久化在 egui temp data（key = `br.id.with("wikilink_ac_sel")`，遵循隐式状态总线风格），
    /// query 变化时被钳制到合法区间。
    fn wikilink_autocomplete(&self, ui: &mut egui::Ui, br: &egui::Response, body: &str) -> bool {
        let open_id = br.id.with("wikilink_ac_open");
        let sel_id = br.id.with("wikilink_ac_sel");
        let key_id = br.id.with("wikilink_ac_key");

        // 取出本帧 TextEdit 渲染前拦截到的键盘动作（见 show_editor 顶部），随即清空。
        let pending = ui
            .ctx()
            .data_mut(|d| d.remove_temp::<AcKey>(key_id))
            .unwrap_or(AcKey::None);

        // 不开/早退时统一清理持久化状态的小工具。
        let clear = |ctx: &egui::Context| {
            ctx.data_mut(|d| {
                d.remove::<usize>(sel_id);
                d.remove::<bool>(open_id);
            });
        };

        // 光标字节位置（char 索引 → byte 索引）；无 TextEdit 状态（从未聚焦）则不弹。
        let Some(cursor_byte) = egui::text_edit::TextEditState::load(ui.ctx(), br.id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| r.primary.index)
            .map(|ci| {
                body.char_indices()
                    .nth(ci)
                    .map_or(body.len(), |(byte, _)| byte)
            })
        else {
            clear(ui.ctx());
            return false;
        };

        // 光标前最近的 `[[`，且其后还没闭合 `]]`、未跨行
        let before = &body[..cursor_byte];
        let Some(open) = before.rfind("[[") else {
            clear(ui.ctx());
            return false;
        };
        let frag = &before[open + 2..];
        if frag.contains("]]") || frag.contains('\n') {
            clear(ui.ctx());
            return false;
        }
        let query = frag.to_lowercase();

        // 匹配的已有标题（排除自身、去重、最多 8 条）
        let suggestions = self.graph_resource.read_resource(|g| {
            let mut seen = std::collections::BTreeSet::new();
            g.graph
                .node_indices()
                .filter(|&i| i != self.node_index)
                .filter_map(|i| {
                    let t = g.graph[i].text.clone();
                    (!t.is_empty() && t.to_lowercase().contains(&query) && seen.insert(t.clone()))
                        .then_some(t)
                })
                .take(8)
                .collect::<Vec<_>>()
        });
        if suggestions.is_empty() {
            clear(ui.ctx());
            return false;
        }

        // 高亮索引（持久化），按候选数量钳制——query 变化导致候选变少时不越界。
        let mut selected = ui
            .ctx()
            .data(|d| d.get_temp::<usize>(sel_id))
            .unwrap_or(0)
            .min(suggestions.len() - 1);

        // 应用 TextEdit 渲染前拦截到的键盘动作。
        let mut chosen: Option<String> = None;
        match pending {
            AcKey::Down => selected = (selected + 1) % suggestions.len(),
            AcKey::Up => selected = (selected + suggestions.len() - 1) % suggestions.len(),
            AcKey::Confirm => chosen = Some(suggestions[selected].clone()),
            AcKey::Close => {
                clear(ui.ctx());
                return false;
            }
            AcKey::None => {}
        }

        // 渲染：独立 popup 图层 + IgnoreClicks（点击候选项不致 popup 自关，确认逻辑由我们接管）。
        egui::Popup::from_response(br)
            .id(br.id.with("wikilink_ac"))
            .open(true)
            .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
            .gap(2.0)
            .show(|ui| {
                ui.set_max_width(CARD_WIDTH);
                for (idx, s) in suggestions.iter().enumerate() {
                    if ui.selectable_label(idx == selected, s.as_str()).clicked() {
                        chosen = Some(s.clone());
                    }
                }
            });

        if let Some(title) = chosen {
            // 改写正文：把 `[[frag` 替换为 `[[title]]`，并把光标落到新 `]]` 之后。
            let new_note = format!("{}{}]]{}", &body[..open + 2], title, &body[cursor_byte..]);
            let new_cursor_char = new_note[..open + 2 + title.len() + 2].chars().count();
            self.graph_resource.with_resource(|g| {
                if let Some(n) = g.get_node_mut(self.node_index) {
                    n.note = new_note;
                }
            });
            // 把光标移到补全后的 `]]` 之后，并让 TextEdit 重新持焦（点击候选项会把焦点移走）。
            if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), br.id) {
                let range =
                    egui::text::CCursorRange::one(egui::text::CCursor::new(new_cursor_char));
                state.cursor.set_char_range(Some(range));
                state.store(ui.ctx(), br.id);
            }
            ui.ctx().memory_mut(|m| m.request_focus(br.id));
            clear(ui.ctx());
            return false;
        }

        // 持久化高亮索引 + "popup 打开中"标志，供下一帧的键盘拦截使用。
        ui.ctx().data_mut(|d| {
            d.insert_temp(sel_id, selected);
            d.insert_temp(open_id, true);
        });
        true
    }
}

impl NodeWidget {
    /// 调试用：在节点右上角绘制其 NodeIndex（当前未接线，见 AGENTS.md §5 调试钩子）。
    #[allow(dead_code)]
    fn draw_node_id(&self, ui: &mut egui::Ui, node_response: &egui::Response) {
        let scale_level = (self
            .canvas_state_resource
            .read_resource(|canvas_state| canvas_state.transform.scaling)
            * 10.0)
            .ceil()
            / 10.0;
        let rect = node_response.rect;
        let text = format!("{:?}", self.node_index.index());
        let font_size = 9.0 * scale_level; // 你可以调整这个数值
                                           // let font_size = 20.0;
        let font = egui::FontId::new(font_size, egui::FontFamily::Proportional);

        let galley = ui
            .painter()
            .layout_no_wrap(text.clone(), font.clone(), egui::Color32::RED);
        let text_size = galley.size();
        // let text_size = egui::Vec2::new(100.0, 100.0);

        let min_width = 10.0 * scale_level;
        // let min_height = 40.0 * self.canvas_state.scale;

        let desired_size = egui::vec2(
            (text_size.x + 5.0 * scale_level).max(min_width),
            text_size.y + 2.0 * scale_level,
        );

        let node_id_rect_start = rect.min + egui::vec2(rect.width() - desired_size.x, 0.0);
        let node_id_rect = egui::Rect::from_min_size(node_id_rect_start, desired_size);
        ui.painter().rect(
            node_id_rect,
            egui::CornerRadius::ZERO,
            egui::Color32::from_rgba_premultiplied(0, 0, 240, 200),
            egui::Stroke::new(1.0, egui::Color32::RED),
            egui::StrokeKind::Outside,
        );
        ui.painter().text(
            node_id_rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            font.clone(),
            egui::Color32::RED,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::escape_md_link_text;

    #[test]
    fn escape_md_link_text_escapes_brackets_and_backslash() {
        // 含 markdown 链接文本特殊字符的标题被反斜杠转义，不会破坏 `[…]` 闭合。
        assert_eq!(escape_md_link_text("a[b]"), r"a\[b\]");
        assert_eq!(escape_md_link_text(r"x\y"), r"x\\y");
        // 普通标题（含中文 / 空格）原样保留。
        assert_eq!(escape_md_link_text("知识 图谱"), "知识 图谱");
        assert_eq!(escape_md_link_text("plain"), "plain");
    }
}
