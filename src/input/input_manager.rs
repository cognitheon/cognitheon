use egui::Id;
use petgraph::graph::NodeIndex;

use super::{
    detectors::*,
    input_state::{DragType, InputState, SelectType},
};

pub fn calc_input(ui: &mut egui::Ui) -> InputState {
    let last_input: Option<InputState> = ui.data(|d| d.get_temp(Id::NULL));
    let mut input_state = InputState::Idle;

    if detect_drag_canvas(ui) {
        input_state = InputState::Dragging(DragType::Canvas);
    } else if detect_select_node(ui) {
        input_state = InputState::Selecting(SelectType::Single(NodeIndex::new(0)));
    }

    input_state
}

pub fn hit_test_nodes(ui: &mut egui::Ui) -> Option<NodeIndex> {
    None
}
