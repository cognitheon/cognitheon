use std::fmt::Display;

use petgraph::graph::NodeIndex;

use crate::{
    resource::CanvasStateResource,
    ui::{bezier::BezierEdge, line_edge::LineEdge},
};

use super::anchor::{BezierAnchor, LineAnchor};

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
            canvas_state_resource.read_resource(|canvas_state| canvas_state.new_edge_id());
        Self {
            id: edge_id,
            source,
            target,
            text: None,
            bezier_edge: BezierEdge::new(
                BezierAnchor::new_smooth(source_canvas_pos),
                BezierAnchor::new_smooth(target_canvas_pos),
            ),

            line_edge: LineEdge::new(
                LineAnchor::new(source_canvas_pos),
                LineAnchor::new(target_canvas_pos),
            ),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub enum EdgeType {
    Line,
    Bezier,
}

impl Display for EdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let edge_type_str = match self {
            EdgeType::Line => "Line",
            EdgeType::Bezier => "Bezier",
        };
        write!(f, "{}", edge_type_str)
    }
}
