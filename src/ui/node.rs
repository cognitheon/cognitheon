use crate::global::{CanvasStateResource, GraphResource};
use crate::graph::edge::{TempEdge, TempEdgeTarget};
use crate::graph::node::NodeRenderInfo;
use egui::{Id, Sense, Stroke, Widget};
use petgraph::graph::NodeIndex;

use crate::colors::{node_background, node_border, node_border_selected};

pub struct NodeWidget {
    pub node_index: NodeIndex,
    // pub graph: &'a mut Graph,
    // pub canvas_state: &'a mut CanvasState,
}

impl NodeWidget {
    pub fn setup_actions(&mut self, response: &egui::Response, ui: &mut egui::Ui) {
        let graph_resource: GraphResource = ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();
        let canvas_state_resource: CanvasStateResource =
            ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();

        // 处理右键拖动
        if response.drag_started_by(egui::PointerButton::Secondary) {
            println!("right button drag started");
            let mouse_screen_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
            // let canvas_pos = canvas_state_resource
            //     .read_canvas_state(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));

            // if let Some(node_index) = self.hit_test(ui, mouse_screen_pos) {
            // 创建临时边
            let temp_edge = TempEdge {
                source: self.node_index,
                target: TempEdgeTarget::Point(mouse_screen_pos),
            };
            graph_resource.with_graph(|graph| {
                graph.set_temp_edge(None);
                graph.set_temp_edge(Some(temp_edge));
            });
            // }
            // });
        }

        if ui.input(|i| i.pointer.button_down(egui::PointerButton::Secondary)) {
            println!("right button dragging");
            let mouse_screen_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
            graph_resource.with_graph(|graph| {
                let temp_edge = graph.get_temp_edge();
                println!("====temp_edge: {:?}====", temp_edge);
                let mouse_canvas_pos = canvas_state_resource
                    .read_canvas_state(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));
                if let Some(temp_edge_clone) = temp_edge.clone() {
                    // 更新临时边目标坐标
                    let mut new_temp_edge = temp_edge_clone;
                    new_temp_edge.target = TempEdgeTarget::Point(mouse_canvas_pos);
                    graph.set_temp_edge(Some(new_temp_edge));
                }
            });
            // println!("mouse_screen_pos: {:?}", mouse_screen_pos);
        }
        println!(
            "NodeWidget::setup_actions: {:?}",
            ui.input(|i| i.pointer.hover_pos())
        );

        if ui.input(|i| i.pointer.button_released(egui::PointerButton::Secondary)) {
            println!("right button drag stopped");
            // graph_resource.with_graph(|graph| {
            //     graph.set_temp_edge(None);
            // });
        }

        if response.double_clicked() {
            println!("node double clicked: {:?}", self.node_index);
            graph_resource.with_graph(|graph| {
                graph.set_editing_node(Some(self.node_index));
            });
        }

        // 检测单击事件
        if response.clicked() {
            if ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary)) {
                println!("node clicked: {:?}", self.node_index);
                graph_resource.with_graph(|graph| {
                    if graph.get_editing_node() != Some(self.node_index) {
                        graph.set_editing_node(None);
                    }
                    graph.set_selected_node(Some(self.node_index));
                });
            }
        }

        // 处理键盘按键
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            graph_resource.with_graph(|graph| {
                graph.set_selected_node(None);
                graph.set_editing_node(None);
            });
        }

        if ui.input(|i| i.key_pressed(egui::Key::Backspace)) {
            graph_resource.with_graph(|graph| {
                if graph.get_selected_node() == Some(self.node_index)
                    && !graph.get_editing_node().is_some()
                {
                    println!("node deleted: {:?}", self.node_index);
                    graph.remove_node(self.node_index);
                }
            });
        }

        // Ctrl + Enter
        if ui.input(|i| {
            i.key_pressed(egui::Key::Enter) && i.modifiers.contains(egui::Modifiers::CTRL)
        }) && graph_resource.read_graph(|graph| graph.get_editing_node())
            == Some(self.node_index)
        {
            println!("node enter: {:?}", self.node_index);
            graph_resource.with_graph(|graph| {
                graph.set_editing_node(None);
            });
        }

        if response.dragged_by(egui::PointerButton::Primary)
            && graph_resource.read_graph(|graph| graph.get_editing_node()) != Some(self.node_index)
        {
            graph_resource.with_graph(|graph| {
                graph.set_editing_node(None);
            });
            println!("node dragged: {:?}", self.node_index);
            let drag_delta = response.drag_delta()
                / (canvas_state_resource.read_canvas_state(|canvas_state| canvas_state.scale));
            graph_resource.with_graph(|graph| {
                let node = graph.get_node_mut(self.node_index).unwrap();
                node.position += drag_delta;
                let render_info = NodeRenderInfo {
                    screen_rect: response.rect,
                    screen_center: response.rect.center(),
                };
                node.render_info = Some(render_info);
            });
        }
    }
}

impl Widget for NodeWidget {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        let graph_resource: GraphResource = ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();
        let canvas_state_resource: CanvasStateResource =
            ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();

        // 对缩放比例进行区间化
        let scale_level =
            (canvas_state_resource.read_canvas_state(|canvas_state| canvas_state.scale) * 10.0)
                .ceil()
                / 10.0;
        // let node = self.graph.get_node_mut(self.node_id).unwrap();

        let text = {
            graph_resource.with_graph(|graph| {
                let node = graph.get_node(self.node_index).unwrap();
                format!("{}", node.text)
            })
        };
        let font_size = 20.0 * scale_level as f32; // 你可以调整这个数值
                                                   // let font_size = 20.0;
        let font = egui::FontId::new(font_size, egui::FontFamily::Proportional);

        let galley = ui
            .painter()
            .layout_no_wrap(text.clone(), font.clone(), egui::Color32::RED);
        let text_size = galley.size();
        // let text_size = egui::Vec2::new(100.0, 100.0);

        let min_width =
            60.0 * canvas_state_resource.read_canvas_state(|canvas_state| canvas_state.scale);
        // let min_height = 40.0 * self.canvas_state.scale;

        let desired_size = canvas_state_resource.read_canvas_state(|canvas_state| {
            egui::vec2(
                (text_size.x + 20.0 * canvas_state.scale).max(min_width),
                text_size.y + 10.0 * canvas_state.scale,
            )
        });

        let screen_pos = {
            graph_resource.with_graph(|graph| {
                let node = graph.get_node(self.node_index).unwrap();
                canvas_state_resource
                    .read_canvas_state(|canvas_state| canvas_state.to_screen(node.position))
            })
        };

        let rect = egui::Rect::from_min_size(screen_pos, desired_size);

        let response = ui.allocate_rect(rect, Sense::click_and_drag());

        self.setup_actions(&response, ui);

        let selected_rect = rect.expand(
            5.0 * canvas_state_resource.read_canvas_state(|canvas_state| canvas_state.scale),
        );
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

            if graph_resource.read_graph(|graph| graph.get_selected_node()) == Some(self.node_index)
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

            // 在右上角绘制节点ID
            let font_size = 10.0 * scale_level as f32;
            let node_id_font = egui::FontId::new(font_size, egui::FontFamily::Proportional);
            let node_id_rect = egui::Rect::from_min_size(
                rect.min + egui::vec2(response.rect.width() - 10.0, 10.0),
                egui::vec2(10.0, 10.0),
            );
            painter.rect(
                node_id_rect,
                egui::Rounding::ZERO,
                egui::Color32::from_rgba_premultiplied(0, 0, 240, 200),
                egui::Stroke::new(1.0, egui::Color32::RED),
            );
            painter.text(
                rect.min + egui::vec2(response.rect.width() - 10.0, 10.0),
                egui::Align2::RIGHT_TOP,
                format!("{:?}", self.node_index),
                node_id_font.clone(),
                egui::Color32::RED,
            );

            if graph_resource.read_graph(|graph| graph.get_editing_node()) == Some(self.node_index)
            {
                let mut text = {
                    graph_resource.with_graph(|graph| {
                        let node = graph.get_node(self.node_index).unwrap();
                        format!("{}", node.text)
                    })
                };
                // let mut response = ui.text_edit_singleline(&mut text);
                let edit_response = ui.put(
                    rect,
                    egui::TextEdit::multiline(&mut text)
                        // .min_size(egui::vec2(min_width, min_height))
                        .desired_rows(1)
                        .font(font)
                        .text_color(egui::Color32::RED)
                        .background_color(node_background(ui.ctx().theme()))
                        .margin(
                            egui::vec2(10.0, 5.0)
                                * canvas_state_resource
                                    .read_canvas_state(|canvas_state| canvas_state.scale),
                        )
                        .horizontal_align(egui::Align::Center),
                );

                // if edit_response.lost_focus() {
                //     graph_resource.with_graph(|graph| {
                //         graph.set_editing_node(None);
                //     });
                // }

                edit_response.request_focus();
                graph_resource.with_graph(|graph| {
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

            let render_info = NodeRenderInfo {
                screen_rect: rect,
                screen_center: rect.center(),
            };
            graph_resource.with_graph(|graph| {
                let node = graph.get_node_mut(self.node_index).unwrap();
                node.render_info = Some(render_info);
            });
        };

        response

        // let response = ui.label("text");
        // response
    }
}
