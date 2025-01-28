pub mod node;

use crate::graph::node::Node;
use petgraph::graph::NodeIndex;

use crate::canvas::CanvasState;

use crate::ui::node::NodeWidget;

// #[typetag::serde(tag = "type")]
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Graph {
    pub graph: petgraph::stable_graph::StableGraph<Node, ()>,
    pub selected_node: Option<NodeIndex>,
    pub editing_node: Option<NodeIndex>,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            graph: petgraph::stable_graph::StableGraph::new(),
            selected_node: None,
            editing_node: None,
        }
    }
}

impl Graph {
    pub fn add_node(&mut self, node: Node) -> NodeIndex {
        let idx = self.graph.add_node(node);
        idx
    }

    pub fn get_node(&self, node_index: NodeIndex) -> Option<&Node> {
        self.graph.node_weight(node_index)
    }

    pub fn get_node_mut(&mut self, node_index: NodeIndex) -> Option<&mut Node> {
        self.graph.node_weight_mut(node_index)
    }

    pub fn get_selected_node(&self) -> Option<NodeIndex> {
        self.selected_node
    }

    pub fn set_selected_node(&mut self, node_index: Option<NodeIndex>) {
        self.selected_node = node_index;
    }

    pub fn get_editing_node(&self) -> Option<NodeIndex> {
        self.editing_node
    }

    pub fn set_editing_node(&mut self, node_index: Option<NodeIndex>) {
        self.editing_node = node_index;
    }

    pub fn remove_node(&mut self, node_index: NodeIndex) {
        let result = self.graph.remove_node(node_index);
        println!("result: {:?}", result);
        self.set_selected_node(None);
        self.set_editing_node(None);
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Edge {
    pub id: u64,
    pub source: u64,
    pub target: u64,
}

pub fn render_graph(
    graph: &mut Graph,
    ui: &mut egui::Ui,
    canvas_state: &mut CanvasState,
) {
    let node_indices = graph
        .graph
        .node_indices()
        .map(|idx| idx)
        .collect::<Vec<NodeIndex>>();

    // println!("node_ids: {:?}", node_ids.len());

    for node_index in node_indices {
        // println!("node: {}", node.id);
        // Put the node id into the ui

        // 在屏幕上指定位置放置label控件

        ui.add(NodeWidget {
            node_index,
            graph,
            canvas_state,
        });
    }
}
