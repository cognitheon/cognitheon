use petgraph::graph::NodeIndex;

use crate::ui::bezier::BezierEdge;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Edge {
    pub id: u64,
    pub source: NodeIndex,
    pub target: NodeIndex,
    pub text: Option<String>,
    pub edge_type: EdgeType,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub enum EdgeType {
    Line,
    Bezier(BezierEdge),
}
