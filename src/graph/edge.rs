use petgraph::graph::NodeIndex;

pub struct Edge {
    pub id: u64,
    pub source: NodeIndex,
    pub target: NodeIndex,
    
}
