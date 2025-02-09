use petgraph::graph::{EdgeIndex, NodeIndex};

#[derive(serde::Serialize, serde::Deserialize, Default, Clone, Debug)]
pub enum GraphSelection {
    #[default]
    None,
    Node(Vec<NodeIndex>),
    Edge(Vec<EdgeIndex>),
}

impl GraphSelection {
    pub fn clear(&mut self) {
        *self = GraphSelection::None;
    }

    pub fn is_nodes(&self) -> bool {
        matches!(self, GraphSelection::Node(_))
    }

    pub fn is_edge(&self) -> bool {
        matches!(self, GraphSelection::Edge(_))
    }
}
