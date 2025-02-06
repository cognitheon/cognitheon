use egui::{Id, Pos2};
use petgraph::stable_graph::NodeIndex;

use super::render_info::NodeRenderInfo;

pub fn get_node_render_info(node_index: NodeIndex, ui: &egui::Ui) -> Option<NodeRenderInfo> {
    let node_render_info: Option<NodeRenderInfo> = ui
        .ctx()
        .data(|reader| reader.get_temp(Id::new(node_index.index().to_string())));
    node_render_info
}

pub fn node_rect_center(node_index: NodeIndex, ui: &egui::Ui) -> Pos2 {
    let node_render_info: NodeRenderInfo = ui
        .ctx()
        .data(|reader| reader.get_temp(Id::new(node_index.index().to_string())))
        .unwrap();

    node_render_info.canvas_center()
}
