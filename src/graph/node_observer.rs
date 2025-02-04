use petgraph::graph::NodeIndex;

use super::node::NodeRenderInfo;

pub trait NodeObserver: Send + Sync {
    fn on_node_changed(&self, node_index: NodeIndex, render_info: NodeRenderInfo);
}
