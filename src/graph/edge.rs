use petgraph::graph::NodeIndex;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Edge {
    pub id: u64,
    pub source: NodeIndex,
    pub target: NodeIndex,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct TempEdge {
    pub source: NodeIndex,
    pub target: TempEdgeTarget,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub enum TempEdgeTarget {
    None,
    Node(NodeIndex),
    Point(egui::Pos2),
}
