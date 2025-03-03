use std::sync::atomic::Ordering;

use egui::{Id, PointerButton};

use crate::{
    graph::{
        anchor::{BezierAnchor, LineAnchor},
        edge::Edge,
        node::Node,
        render_info::NodeRenderInfo,
    },
    ui::{bezier::BezierEdge, line_edge::LineEdge, temp_edge::TempEdgeTarget},
};

use super::data::CanvasWidget;

impl CanvasWidget {
    pub fn pre_render_actions(&mut self, ui: &mut egui::Ui) {
        // make_input_idle(ui);

        // self.handle_scale(ui);
        // self.handle_pan(ui);
    }

    pub fn post_render_actions(&mut self, ui: &mut egui::Ui, canvas_response: &egui::Response) {
        // self.input_manager.handle_input(ui, canvas_response);
        // self.handle_drag_select(ui, canvas_response);
        // self.handle_escape(ui, canvas_response);

        // // 处理双击
        // if canvas_response.hovered() {
        //     if ui.input(|i| {
        //         i.pointer
        //             .button_double_clicked(egui::PointerButton::Primary)
        //     }) {
        //         if let Some(screen_pos) = ui.input(|i| i.pointer.hover_pos()) {
        //             let node = self
        //                 .canvas_state_resource
        //                 .read_canvas_state(|canvas_state| {
        //                     // 将屏幕坐标转换为画布坐标
        //                     let canvas_pos = canvas_state.to_canvas(screen_pos);
        //                     let new_node_id =
        //                         canvas_state.global_node_id.fetch_add(1, Ordering::Relaxed);
        //                     Node {
        //                         id: new_node_id,
        //                         position: canvas_pos,
        //                         text: String::new(),
        //                         note: String::new(),
        //                         // render_info: None,
        //                     }
        //                 });

        //             self.graph_resource.with_graph(|graph| {
        //                 let node_index = graph.add_node(node);
        //                 graph.select_node(node_index);
        //                 graph.set_editing_node(Some(node_index));
        //             });
        //         }
        //         // self.editing_text = Some((canvas_pos, String::new()));
        //         println!("double clicked");
        //     }
        // }

        // // 拖拽创建边
        // // 检测右键按下
        // let right_mouse_down =
        //     ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary));
        // if right_mouse_down {
        //     if let Some(mouse_screen_pos) = ui.input(|i| i.pointer.hover_pos()) {
        //         // println!("mouse_screen_pos: {:?}", mouse_screen_pos);
        //         // Check if we clicked on a node
        //         if let Some(node_index) = self.hit_test_node(ui, mouse_screen_pos) {
        //             // println!("node_index: {:?}", node_index);
        //             // Mark ourselves as "dragging from this node"
        //             self.temp_edge = self.make_temp_edge(ui, node_index);
        //         }
        //     }
        // }
        // // println!("self.temp_edge: {:?}", self.temp_edge);
        // let right_mouse_pressed = ui.input(|i| i.pointer.button_down(PointerButton::Secondary));
        // if right_mouse_pressed {
        //     // println!("right_mouse_pressed: {:?}", right_mouse_pressed);
        //     if let Some(temp_edge) = self.temp_edge.as_mut() {
        //         // println!("temp_edge: {:?}", temp_edge);
        //         let source_index = temp_edge.source;
        //         if let Some(mouse_screen_pos) = ui.input(|i| i.pointer.hover_pos()) {
        //             // println!("mouse_screen_pos: {:?}", mouse_screen_pos);

        //             // Update target anchor
        //             let mouse_canvas_pos = self
        //                 .canvas_state_resource
        //                 .read_canvas_state(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));

        //             let node_render_info: Option<NodeRenderInfo> = ui
        //                 .ctx()
        //                 .data(|d| d.get_temp(Id::new(source_index.index().to_string())));
        //             if let Some(node_render_info) = node_render_info {
        //                 let source_canvas_center = node_render_info.canvas_center();
        //                 // println!("source_canvas_center: {:?}", source_canvas_center);

        //                 let control_anchors = temp_edge.bezier_edge.control_anchors.clone();

        //                 temp_edge.target = TempEdgeTarget::Point(mouse_canvas_pos);
        //                 temp_edge.bezier_edge = BezierEdge::new(
        //                     BezierAnchor::new_smooth(source_canvas_center),
        //                     BezierAnchor::new_smooth(mouse_canvas_pos),
        //                 )
        //                 .with_control_anchors(control_anchors);

        //                 temp_edge.line_edge = LineEdge {
        //                     source: LineAnchor::new(source_canvas_center),
        //                     target: LineAnchor::new(mouse_canvas_pos),
        //                 };
        //             }
        //         }
        //     }
        // }

        // // 3) On right button release, finalize or discard
        // let right_released = ui.input(|i| i.pointer.button_released(PointerButton::Secondary));
        // if right_released && self.temp_edge.is_some() {
        //     // Check if we released on another node
        //     if let Some(mouse_screen_pos) = ui.input(|i| i.pointer.hover_pos()) {
        //         if let Some(target_node_index) = self.hit_test_node(ui, mouse_screen_pos) {
        //             // If you want to finalize an edge from self.right_drag_node to target_index
        //             println!(
        //                 "Released on a node: {:?}. Connect source -> target?",
        //                 target_node_index
        //             );
        //             let source_node_index = self.temp_edge.as_ref().unwrap().source;

        //             let edge_exists = self.graph_resource.read_graph(|graph| {
        //                 graph.edge_exists(source_node_index, target_node_index)
        //             });
        //             if !edge_exists && source_node_index != target_node_index {
        //                 let (source_canvas_pos, target_canvas_pos) = ui.ctx().data(|d| {
        //                     let source_node_render_info: NodeRenderInfo = d
        //                         .get_temp(Id::new(source_node_index.index().to_string()))
        //                         .unwrap();
        //                     let target_node_render_info: NodeRenderInfo = d
        //                         .get_temp(Id::new(target_node_index.index().to_string()))
        //                         .unwrap();
        //                     (
        //                         source_node_render_info.canvas_center(),
        //                         target_node_render_info.canvas_center(),
        //                     )
        //                 });

        //                 let edge = Edge::new(
        //                     source_node_index,
        //                     target_node_index,
        //                     source_canvas_pos,
        //                     target_canvas_pos,
        //                     self.canvas_state_resource.clone(),
        //                 );

        //                 self.graph_resource.with_graph(|graph| {
        //                     graph.add_edge(edge);
        //                 });
        //             }
        //         } else {
        //             // Released on empty space, create a node or do nothing
        //             println!("Released on empty space. Maybe create a new node?");
        //             let source_node_index = self.temp_edge.as_ref().unwrap().source;
        //             let mouse_canvas_pos = self
        //                 .canvas_state_resource
        //                 .read_canvas_state(|canvas_state| canvas_state.to_canvas(mouse_screen_pos));
        //             let node = Node {
        //                 id: self
        //                     .canvas_state_resource
        //                     .read_canvas_state(|canvas_state| {
        //                         canvas_state.global_node_id.fetch_add(1, Ordering::Relaxed)
        //                     }),
        //                 position: mouse_canvas_pos,
        //                 text: String::new(),
        //                 note: String::new(),
        //             };

        //             self.graph_resource.with_graph(|graph| {
        //                 graph.add_node_with_edge(
        //                     node,
        //                     source_node_index,
        //                     self.canvas_state_resource.clone(),
        //                 );
        //             });
        //         }
        //     }

        //     // Clear your local dragging state
        //     self.temp_edge = None;
        // }
    }
}

pub fn make_input_busy(ui: &mut egui::Ui) {
    ui.ctx()
        .data_mut(|d| d.insert_temp(Id::new("input_busy"), true));
}

pub fn make_input_idle(ui: &mut egui::Ui) {
    ui.ctx()
        .data_mut(|d| d.insert_temp(Id::new("input_busy"), false));
}

pub fn is_input_busy(ui: &mut egui::Ui) -> bool {
    ui.ctx()
        .data(|d| d.get_temp(Id::new("input_busy")))
        .unwrap_or(false)
}
