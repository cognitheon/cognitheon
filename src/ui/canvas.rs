use std::sync::atomic::Ordering;

use eframe::egui_wgpu;
use egui::{emath::TSTransform, Id, PointerButton, Widget};
use petgraph::graph::NodeIndex;

use crate::{
    geometry::widget_screen_pos,
    globals::{canvas_state_resource::CanvasStateResource, graph_resource::GraphResource},
    graph::{
        edge::Edge,
        node::{Node, NodeRenderInfo},
    },
    particle::particle_callback::ParticleCallback,
};

use super::{
    bezier::{Anchor, BezierEdge},
    helpers::draw_grid,
    line_edge::LineEdge,
    temp_edge::{TempEdge, TempEdgeTarget, TempEdgeWidget},
};

#[derive(Debug)]
pub struct CanvasWidget {
    temp_edge: Option<TempEdge>,
    graph_resource: GraphResource,
    canvas_state_resource: CanvasStateResource,
}

impl CanvasWidget {
    pub fn new(graph_resource: GraphResource, canvas_state_resource: CanvasStateResource) -> Self {
        Self {
            temp_edge: None,
            graph_resource,
            canvas_state_resource,
        }
    }

    pub fn hit_test_node(&self, ui: &mut egui::Ui, screen_pos: egui::Pos2) -> Option<NodeIndex> {
        self.graph_resource.read_graph(|graph| {
            graph.graph.node_indices().find(|&node_index| {
                let node_render_info: Option<NodeRenderInfo> = ui
                    .ctx()
                    .data(|d| d.get_temp(Id::new(node_index.index().to_string())));
                // println!("node_render_info: {:?}", node_render_info);
                if let Some(node_render_info) = node_render_info {
                    let node_screen_rect =
                        self.canvas_state_resource
                            .read_canvas_state(|canvas_state| {
                                canvas_state.to_screen_rect(node_render_info.canvas_rect)
                            });
                    if node_screen_rect.contains(screen_pos) {
                        return true;
                    }
                }
                false
            })
        })
    }

    fn make_temp_edge(&self, ui: &mut egui::Ui, node_index: NodeIndex) -> Option<TempEdge> {
        let node_render_info: Option<NodeRenderInfo> = ui
            .ctx()
            .data(|d| d.get_temp(Id::new(node_index.index().to_string())));
        // println!("node_render_info: {:?}", node_render_info);

        node_render_info.as_ref()?;

        let mouse_screen_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
        let node_canvas_center = node_render_info.unwrap().canvas_center();

        let mouse_canvas_pos = self
            .canvas_state_resource
            .read_canvas_state(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));

        Some(TempEdge {
            source: node_index,
            target: TempEdgeTarget::Point(mouse_canvas_pos),
            bezier_edge: BezierEdge {
                source_anchor: Anchor::new_smooth(node_canvas_center),
                target_anchor: Anchor::new_smooth(mouse_canvas_pos),
                control_anchors: vec![],
            },
            line_edge: LineEdge {
                source: Anchor::new_smooth(node_canvas_center),
                target: Anchor::new_smooth(mouse_canvas_pos),
            },
        })
    }

    pub fn setup_actions(&mut self, ui: &mut egui::Ui, canvas_response: &egui::Response) {
        if canvas_response.hovered() {
            let pointer_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
            let rect = canvas_response.rect;
            let canvas_widge_pos = widget_screen_pos(pointer_pos, rect);
            // println!("canvas rect: {:?}", rect);
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                rect,
                ParticleCallback::new(
                    canvas_widge_pos.into(),
                    ui.ctx().input(|i| i.stable_dt),
                    rect,
                ),
            ));
        }
        // println!("CanvasWidget::setup_actions");
        // 检测右键按下
        let right_mouse_down =
            ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary));
        if right_mouse_down {
            if let Some(mouse_screen_pos) = ui.input(|i| i.pointer.hover_pos()) {
                // println!("mouse_screen_pos: {:?}", mouse_screen_pos);
                // Check if we clicked on a node
                if let Some(node_index) = self.hit_test_node(ui, mouse_screen_pos) {
                    // println!("node_index: {:?}", node_index);
                    // Mark ourselves as "dragging from this node"
                    self.temp_edge = self.make_temp_edge(ui, node_index);
                }
            }
        }
        // println!("self.temp_edge: {:?}", self.temp_edge);
        let right_mouse_pressed = ui.input(|i| i.pointer.button_down(PointerButton::Secondary));
        if right_mouse_pressed {
            // println!("right_mouse_pressed: {:?}", right_mouse_pressed);
            if let Some(temp_edge) = self.temp_edge.as_mut() {
                // println!("temp_edge: {:?}", temp_edge);
                let source_index = temp_edge.source;
                if let Some(mouse_screen_pos) = ui.input(|i| i.pointer.hover_pos()) {
                    // println!("mouse_screen_pos: {:?}", mouse_screen_pos);

                    // Update target anchor
                    let mouse_canvas_pos = self
                        .canvas_state_resource
                        .read_canvas_state(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));

                    let node_render_info: Option<NodeRenderInfo> = ui
                        .ctx()
                        .data(|d| d.get_temp(Id::new(source_index.index().to_string())));
                    if let Some(node_render_info) = node_render_info {
                        let source_canvas_center = node_render_info.canvas_center();
                        // println!("source_canvas_center: {:?}", source_canvas_center);

                        let control_anchors = temp_edge.bezier_edge.control_anchors.clone();

                        temp_edge.target = TempEdgeTarget::Point(mouse_canvas_pos);
                        temp_edge.bezier_edge = BezierEdge {
                            source_anchor: Anchor::new_smooth(source_canvas_center),
                            target_anchor: Anchor::new_smooth(mouse_canvas_pos),
                            control_anchors,
                        };
                        temp_edge.line_edge = LineEdge {
                            source: Anchor::new_smooth(source_canvas_center),
                            target: Anchor::new_smooth(mouse_canvas_pos),
                        };
                    }
                }
            }
        }

        // 3) On right button release, finalize or discard
        let right_released = ui.input(|i| i.pointer.button_released(PointerButton::Secondary));
        if right_released && self.temp_edge.is_some() {
            // Check if we released on another node
            if let Some(mouse_screen_pos) = ui.input(|i| i.pointer.hover_pos()) {
                if let Some(target_node_index) = self.hit_test_node(ui, mouse_screen_pos) {
                    // If you want to finalize an edge from self.right_drag_node to target_index
                    println!(
                        "Released on a node: {:?}. Connect source -> target?",
                        target_node_index
                    );
                    let source_node_index = self.temp_edge.as_ref().unwrap().source;

                    let edge_exists = self.graph_resource.read_graph(|graph| {
                        graph.edge_exists(source_node_index, target_node_index)
                    });
                    if !edge_exists {
                        let (source_canvas_pos, target_canvas_pos) = ui.ctx().data(|d| {
                            let source_node_render_info: NodeRenderInfo = d
                                .get_temp(Id::new(source_node_index.index().to_string()))
                                .unwrap();
                            let target_node_render_info: NodeRenderInfo = d
                                .get_temp(Id::new(target_node_index.index().to_string()))
                                .unwrap();
                            (
                                source_node_render_info.canvas_center(),
                                target_node_render_info.canvas_center(),
                            )
                        });

                        let edge = Edge::new(
                            source_node_index,
                            target_node_index,
                            source_canvas_pos,
                            target_canvas_pos,
                            self.canvas_state_resource.clone(),
                        );

                        self.graph_resource.with_graph(|graph| {
                            graph.add_edge(edge);
                        });
                    } else {
                        println!("edge exists");
                    }
                } else {
                    // Released on empty space, create a node or do nothing
                    println!("Released on empty space. Maybe create a new node?");
                }
            }

            // Clear your local dragging state
            self.temp_edge = None;
        }

        // =====================
        // 1. 处理缩放 (鼠标滚轮)
        // =====================
        // if canvas_response.hovered() {
        let zoom_delta = ui.input(|i| i.zoom_delta());
        if zoom_delta != 1.0 {
            // 计算鼠标指针相对于画布原点的偏移
            self.canvas_state_resource
                .with_canvas_state(|canvas_state| {
                    let mouse_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
                    // let mouse_canvas_pos = (mouse_pos - canvas_state.offset) / canvas_state.scale;
                    // // 保存旧的缩放值
                    // // let old_scale = self.canvas_state.scale;

                    // // 更新缩放值
                    // let mut scale = canvas_state.scale;
                    // scale *= zoom_delta;
                    // scale = scale.clamp(0.1, 100.0);
                    // canvas_state.scale = scale;

                    // // 计算新的偏移量，保持鼠标位置不变
                    // // let mut offset = canvas_state.offset;
                    // let offset = mouse_pos - (mouse_canvas_pos * scale);
                    // canvas_state.offset = offset;

                    let pointer_in_layer = canvas_state.transform.inverse() * mouse_pos;
                    let zoom_delta = ui.ctx().input(|i| i.zoom_delta());
                    let pan_delta = ui.ctx().input(|i| i.smooth_scroll_delta);

                    // Zoom in on pointer:
                    canvas_state.transform = canvas_state.transform
                        * TSTransform::from_translation(pointer_in_layer.to_vec2())
                        * TSTransform::from_scaling(zoom_delta)
                        * TSTransform::from_translation(-pointer_in_layer.to_vec2());

                    // Pan:
                    canvas_state.transform =
                        TSTransform::from_translation(pan_delta) * canvas_state.transform;
                });
        }
        // }

        // =====================
        // 2. 处理平移 (拖拽)
        // =====================
        // if canvas_response.dragged() {
        // 只有按住空格键且用鼠标左键时，才允许拖拽
        if ui.input(|i| i.key_down(egui::Key::Space)) {
            // 设置鼠标指针为手型
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            if ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary)) {
                self.canvas_state_resource
                    .with_canvas_state(|canvas_state| {
                        // drag_delta() 表示本次帧被拖拽的增量
                        let drag_delta = canvas_response.drag_delta();
                        // let mut offset = canvas_state.offset;
                        // offset += drag_delta;
                        // canvas_state.offset = offset;

                        canvas_state.transform.translation += drag_delta;
                    });
            }
        }
        // }

        // if canvas_response.hovered() {
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
        if scroll_delta != egui::Vec2::ZERO {
            self.canvas_state_resource
                .with_canvas_state(|canvas_state| {
                    // let mut offset = canvas_state.offset;
                    // offset += scroll_delta;
                    // canvas_state.offset = offset;

                    canvas_state.transform.translation += scroll_delta;
                });
        }
        // }

        // 处理双击
        if canvas_response.hovered() {
            if ui.input(|i: &egui::InputState| {
                i.key_pressed(egui::Key::Escape)
                    || i.pointer.button_clicked(egui::PointerButton::Primary)
            }) {
                self.graph_resource.with_graph(|graph| {
                    graph.set_selected_node(None);
                    graph.set_editing_node(None);
                });
            }

            if ui.input(|i| {
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
            }) {
                if let Some(screen_pos) = ui.input(|i| i.pointer.hover_pos()) {
                    let node = self
                        .canvas_state_resource
                        .read_canvas_state(|canvas_state| {
                            // 将屏幕坐标转换为画布坐标
                            let canvas_pos = canvas_state.to_canvas(screen_pos);
                            let new_node_id =
                                canvas_state.global_node_id.fetch_add(1, Ordering::Relaxed);
                            Node {
                                id: new_node_id,
                                position: canvas_pos,
                                text: String::new(),
                                note: String::new(),
                                render_info: None,
                            }
                        });

                    self.graph_resource.with_graph(|graph| {
                        let node_index = graph.add_node(node);
                        graph.set_selected_node(Some(node_index));
                        graph.set_editing_node(Some(node_index));
                    });
                }
                // self.editing_text = Some((canvas_pos, String::new()));
                println!("double clicked");
            }
        }
    }
}

impl Widget for &mut CanvasWidget {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let desired_size = ui.available_size();
        let (screen_rect, canvas_response) =
            ui.allocate_exact_size(desired_size, egui::Sense::drag());

        // println!("desired_size: {:?}", desired_size);

        self.canvas_state_resource
            .read_canvas_state(|canvas_state| {
                draw_grid(ui, canvas_state, screen_rect);
            });

        if let Some(edge) = self.temp_edge.as_ref() {
            // println!("temp_edge target: {:?}", edge.target);
            ui.add(TempEdgeWidget {
                temp_edge: edge,
                graph_resource: self.graph_resource.clone(),
                canvas_state_resource: self.canvas_state_resource.clone(),
            });
        }

        // graph_resource.read_graph(|graph| {
        crate::graph::render_graph(
            ui,
            self.graph_resource.clone(),
            self.canvas_state_resource.clone(),
        );
        // });

        self.setup_actions(ui, &canvas_response);

        canvas_response
    }
}
