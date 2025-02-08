use std::sync::Arc;

use crate::globals::{canvas_state_resource::CanvasStateResource, graph_resource::GraphResource};
use crate::graph::node_observer::NodeObserver;
use crate::graph::render_info::NodeRenderInfo;
use egui::{Id, Sense, Stroke, Widget};
use petgraph::graph::NodeIndex;

use crate::colors::{node_background, node_border, node_border_selected};

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
            println!("node double clicked: {:?}", self.node_index);
            self.graph_resource.with_graph(|graph| {
                graph.set_editing_node(Some(self.node_index));
            });
        }

        // 检测单击事件
        if response.clicked()
            && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary))
        {
            println!("node clicked: {:?}", self.node_index);
            self.graph_resource.with_graph(|graph| {
                if graph.get_editing_node() != Some(self.node_index) {
                    graph.set_editing_node(None);
                }
                graph.selected_nodes.clear();
                graph.select_node(self.node_index);
            });
        }

        // 处理键盘按键
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.graph_resource.with_graph(|graph| {
                graph.selected_nodes.clear();
                graph.set_editing_node(None);
            });
        }

        if ui.input(|i| i.key_pressed(egui::Key::Backspace)) {
            self.graph_resource.with_graph(|graph| {
                if graph.selected_nodes.contains(&self.node_index) && graph.editing_node.is_none() {
                    println!("node deleted: {:?}", self.node_index);
                    graph.remove_node(self.node_index);
                }
            });
        }

        // Ctrl + Enter
        if ui.input(|i| {
            i.key_pressed(egui::Key::Enter) && i.modifiers.contains(egui::Modifiers::CTRL)
        }) && self
            .graph_resource
            .read_graph(|graph| graph.get_editing_node())
            == Some(self.node_index)
        {
            println!("node enter: {:?}", self.node_index);
            self.graph_resource.with_graph(|graph| {
                graph.set_editing_node(None);
            });
        }

        // 处理拖动事件
        if response.dragged_by(egui::PointerButton::Primary)
            && self
                .graph_resource
                .read_graph(|graph| graph.get_editing_node())
                != Some(self.node_index)
        {
            self.graph_resource.with_graph(|graph| {
                graph.set_editing_node(None);
            });
            println!("node dragged: {:?}", self.node_index);
            let drag_delta = response.drag_delta()
                / (self
                    .canvas_state_resource
                    .read_canvas_state(|canvas_state| canvas_state.transform.scaling));
            self.graph_resource.with_graph(|graph| {
                let node = graph.get_node_mut(self.node_index).unwrap();
                node.position += drag_delta;
                // let canvas_rect = self
                //     .canvas_state_resource
                //     .read_canvas_state(|canvas_state| canvas_state.to_canvas_rect(response.rect));
                // let render_info = NodeRenderInfo { canvas_rect };
                // node.render_info = Some(render_info);
            });
        }
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
    //             .read_canvas_state(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));
    //         let node_render_info: NodeRenderInfo = ui
    //             .ctx()
    //             .data(|d| d.get_temp(Id::new(self.node_index.index().to_string())))
    //             .unwrap();

    //         let node_canvas_center = node_render_info.canvas_center();
    //         // let canvas_pos = canvas_state_resource
    //         //     .read_canvas_state(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));

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
    //         self.graph_resource.with_graph(|graph| {
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
    //         self.graph_resource.with_graph(|graph| {
    //             let temp_edge = graph.get_temp_edge();
    //             println!("====temp_edge: {:?}====", temp_edge);
    //             let mouse_canvas_pos = self
    //                 .canvas_state_resource
    //                 .read_canvas_state(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));
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
    //             .read_canvas_state(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));
    //         println!("right button drag stopped");
    //         // 创建新的节点，并创建边
    //         let node_render_info: NodeRenderInfo = ui
    //             .ctx()
    //             .data(|d| d.get_temp(Id::new(self.node_index.index().to_string())))
    //             .unwrap();
    //         let node_canvas_center = node_render_info.canvas_center();

    //         let new_node_id = self
    //             .canvas_state_resource
    //             .read_canvas_state(|canvas_state| canvas_state.new_node_id());
    //         let new_node = Node {
    //             id: new_node_id,
    //             text: String::new(),
    //             note: String::new(),
    //             position: mouse_canvas_pos,
    //             render_info: None,
    //         };
    //         let new_edge_id = self
    //             .canvas_state_resource
    //             .read_canvas_state(|canvas_state| canvas_state.new_edge_id());
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
    //         self.graph_resource.with_graph(|graph| {
    //             let node_index = graph.add_node(new_node);
    //             graph.set_selected_node(Some(node_index));
    //             graph.set_editing_node(Some(node_index));
    //             graph.add_edge(new_edge);
    //             graph.set_temp_edge(None);
    //         });

    //         // graph_resource.with_graph(|graph| {
    //         //     graph.set_temp_edge(None);
    //         // });
    //     }
    // }
}

impl Widget for NodeWidget {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        // 对缩放比例进行区间化
        // 对缩放比例进行区间化
        // let scale_level = (self
        //     .canvas_state_resource
        //     .read_canvas_state(|canvas_state| canvas_state.transform.scaling)
        //     * 10.0)
        //     .ceil()
        //     / 10.0;

        let scale_level = self
            .canvas_state_resource
            .read_canvas_state(|canvas_state| canvas_state.transform.scaling);
        // let node = self.graph.get_node_mut(self.node_id).unwrap();

        let text = {
            self.graph_resource.with_graph(|graph| {
                let node = graph.get_node(self.node_index).unwrap();
                node.text.to_string()
            })
        };
        let font_size = 20.0 * scale_level; // 你可以调整这个数值
                                            // let font_size = 20.0;
        let font = egui::FontId::new(font_size, egui::FontFamily::Proportional);

        let galley = ui
            .painter()
            .layout_no_wrap(text.clone(), font.clone(), egui::Color32::RED);
        let text_size = galley.size();
        // let text_size = egui::Vec2::new(100.0, 100.0);

        let min_width = 60.0 * scale_level;
        // let min_height = 40.0 * self.canvas_state.scale;

        let desired_size = egui::vec2(
            (text_size.x + 20.0 * scale_level).max(min_width),
            text_size.y + 10.0 * scale_level,
        );

        let screen_pos = {
            self.graph_resource.with_graph(|graph| {
                let node = graph.get_node(self.node_index).unwrap();
                self.canvas_state_resource
                    .read_canvas_state(|canvas_state| canvas_state.to_screen(node.position))
            })
        };

        let rect = egui::Rect::from_min_size(screen_pos, desired_size);

        let response = ui.allocate_rect(rect, Sense::click_and_drag());

        self.setup_actions(&response, ui);

        let selected_rect = rect.expand(5.0 * scale_level);
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();

            // 绘制包围矩形
            painter.rect(
                rect,
                0.0,
                egui::Color32::TRANSPARENT,
                Stroke::new(1.0, egui::Color32::ORANGE),
            );

            // let (response, painter) = ui.allocate_painter(desired_size, Sense::click_and_drag());
            // println!("screen_pos: {:?}", screen_pos);
            // 根据文本大小创建矩形区域

            // let stroke_width = 3.0 * canvas_state.scale;
            let stroke_width = 1.0;

            if self
                .graph_resource
                .read_graph(|graph| graph.selected_nodes.contains(&self.node_index))
            {
                painter.rect(
                    selected_rect,
                    egui::Rounding::same(5.0),
                    egui::Color32::TRANSPARENT,
                    egui::Stroke::new(2.0, node_border_selected(ui.ctx().theme())), // 将线宽从20.0改为1.0
                );
            }
            // 绘制边框
            painter.rect(
                rect,
                egui::Rounding::same(5.0),
                node_background(ui.ctx().theme()),
                egui::Stroke::new(stroke_width, node_border(ui.ctx().theme())), // 将线宽从20.0改为1.0
            );

            // 根据rect计算文本位置，使得文本居中
            let text_pos = rect.center();

            // 当前节点正在编辑
            if self
                .graph_resource
                .read_graph(|graph| graph.get_editing_node())
                == Some(self.node_index)
            {
                let mut text = {
                    self.graph_resource.with_graph(|graph| {
                        let node = graph.get_node(self.node_index).unwrap();
                        node.text.to_string()
                    })
                };
                // let mut response = ui.text_edit_singleline(&mut text);
                let edit_response = ui.put(
                    rect,
                    egui::TextEdit::multiline(&mut text)
                        // .min_size(egui::vec2(min_width, min_height))
                        .desired_rows(1)
                        // .min_size(egui::vec2(min_width, 2.0))
                        .font(font)
                        .text_color(egui::Color32::RED)
                        .background_color(node_background(ui.ctx().theme()))
                        // .margin(
                        //     egui::vec2(10.0, 0.0)
                        //         * canvas_state_resource
                        //             .read_canvas_state(|canvas_state| canvas_state.scale),
                        // )
                        .horizontal_align(egui::Align::Center)
                        .vertical_align(egui::Align::Center),
                );

                // if edit_response.lost_focus() {
                //     graph_resource.with_graph(|graph| {
                //         graph.set_editing_node(None);
                //     });
                // }

                edit_response.request_focus();
                self.graph_resource.with_graph(|graph| {
                    let node = graph.get_node_mut(self.node_index).unwrap();
                    node.text = text;
                });
            } else {
                // 绘制文本
                painter.text(
                    text_pos,
                    egui::Align2::CENTER_CENTER,
                    text,
                    font.clone(),
                    egui::Color32::RED,
                );
            }

            // 在右上角绘制节点ID
            self.draw_node_id(ui, &response);

            let canvas_rect = self
                .canvas_state_resource
                .read_canvas_state(|canvas_state| canvas_state.to_canvas_rect(rect));
            let render_info = NodeRenderInfo { canvas_rect };

            self.observers.iter().for_each(|observer| {
                observer.on_node_changed(self.node_index, render_info);
            });
            // ui.ctx().data_mut(|d| {
            //     d.insert_temp(Id::new(self.node_index.index().to_string()), render_info)
            // });
            // self.graph_resource.with_graph(|graph| {
            //     let node = graph.get_node_mut(self.node_index).unwrap();
            //     node.render_info = Some(render_info);
            // });
        };

        response

        // let response = ui.label("text");
        // response
    }
}

impl NodeWidget {
    fn draw_node_id(&self, ui: &mut egui::Ui, node_response: &egui::Response) {
        let scale_level = (self
            .canvas_state_resource
            .read_canvas_state(|canvas_state| canvas_state.transform.scaling)
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
            egui::Rounding::ZERO,
            egui::Color32::from_rgba_premultiplied(0, 0, 240, 200),
            egui::Stroke::new(1.0, egui::Color32::RED),
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
