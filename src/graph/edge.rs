use petgraph::graph::NodeIndex;

use crate::{
    global::{CanvasStateResource, GraphResource},
    ui::{
        bezier::{Anchor, BezierEdge},
        line_edge::LineEdge,
    },
};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Edge {
    pub id: u64,
    pub source: NodeIndex,
    pub target: NodeIndex,
    pub text: Option<String>,
    pub bezier_edge: BezierEdge,
    pub line_edge: LineEdge,
}

impl Edge {
    pub fn new(
        source: NodeIndex,
        target: NodeIndex,
        source_canvas_pos: egui::Pos2,
        target_canvas_pos: egui::Pos2,
        canvas_state_resource: CanvasStateResource,
    ) -> Self {
        let edge_id =
            canvas_state_resource.read_canvas_state(|canvas_state| canvas_state.new_edge_id());
        Self {
            id: edge_id,
            source,
            target,
            text: None,
            bezier_edge: BezierEdge::new(
                Anchor::new_smooth(source_canvas_pos),
                Anchor::new_smooth(target_canvas_pos),
            ),
            line_edge: LineEdge::new(
                Anchor::new_smooth(source_canvas_pos),
                Anchor::new_smooth(target_canvas_pos),
            ),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub enum EdgeType {
    Line,
    Bezier,
}
