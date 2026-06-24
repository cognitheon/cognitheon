use std::sync::Arc;

use crate::graph::node_observer::NodeObserver;
use crate::graph::render_info::NodeRenderInfo;
use crate::graph::selection::GraphSelection;
use crate::resource::{CanvasStateResource, GraphResource};
use crate::wikilink;
use egui::{Id, Sense, Stroke, Widget};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use petgraph::graph::NodeIndex;

use crate::colors::{node_background, node_border, node_border_selected};

thread_local! {
    /// 复用的 Markdown 渲染缓存（egui 单线程；避免每帧每节点重建）。
    static MARKDOWN_CACHE: std::cell::RefCell<CommonMarkCache> =
        std::cell::RefCell::new(CommonMarkCache::default());
}

pub struct NodeWidget {
    pub node_index: NodeIndex,
    pub graph_resource: GraphResource,
    pub canvas_state_resource: CanvasStateResource,
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
    ) -> Self {
        Self {
            node_index,
            graph_resource,
            canvas_state_resource,
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
            self.graph_resource.with_resource(|graph| {
                if let GraphSelection::Node(selected_nodes) = &graph.selected {
                    if selected_nodes.contains(&self.node_index) && graph.editing_node.is_none() {
                        log::debug!("node deleted: {:?}", self.node_index);
                        graph.remove_node(self.node_index);
                    }
                }
            });
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

        let screen_pos = self
            .canvas_state_resource
            .read_resource(|c| c.to_screen(position));

        let theme = ui.ctx().theme();
        let border = if selected {
            node_border_selected(theme)
        } else {
            node_border(theme)
        };

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
                .stroke(Stroke::new(if selected { 2.0 } else { 1.0 }, border))
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
                                    MARKDOWN_CACHE.with_borrow_mut(|cache| {
                                        CommonMarkViewer::new().show(ui, cache, &body);
                                    });
                                });
                        }
                    }
                })
                .response
                .rect
        });

        let rect = card.inner;

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

        // Ctrl/Cmd + Enter：退出编辑，并把正文里的 [[双链]] 落到图上
        if ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command) {
            self.graph_resource.with_resource(|g| {
                g.set_editing_node(None);
                wikilink::resolve_links(g, &self.canvas_state_resource, self.node_index);
            });
        }
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
