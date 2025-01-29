pub mod edge;
pub mod node;
pub mod node_render_info;
use std::sync::{Arc, RwLock};

use crate::global::GraphResource;
use crate::graph::node::Node;
use edge::TempEdge;
use egui::Id;
use petgraph::graph::NodeIndex;

use crate::ui::node::NodeWidget;

// #[typetag::serde(tag = "type")]
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Graph {
    pub graph: petgraph::stable_graph::StableGraph<Node, ()>,
    pub selected_node: Arc<RwLock<Option<NodeIndex>>>,
    pub editing_node: Arc<RwLock<Option<NodeIndex>>>,
    pub creating_edge: Arc<RwLock<Option<TempEdge>>>,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            graph: petgraph::stable_graph::StableGraph::new(),
            selected_node: Arc::new(RwLock::new(None)),
            editing_node: Arc::new(RwLock::new(None)),
            creating_edge: Arc::new(RwLock::new(None)),
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
        self.selected_node.read().unwrap().clone()
    }

    pub fn set_selected_node(&mut self, node_index: Option<NodeIndex>) {
        let mut selected_node = self.selected_node.write().unwrap();
        *selected_node = node_index;
        drop(selected_node);
    }

    pub fn get_editing_node(&self) -> Option<NodeIndex> {
        self.editing_node.read().unwrap().clone()
    }

    pub fn set_editing_node(&mut self, node_index: Option<NodeIndex>) {
        let mut editing_node = self.editing_node.write().unwrap();
        *editing_node = node_index;
        drop(editing_node);
    }

    pub fn remove_node(&mut self, node_index: NodeIndex) {
        let result = self.graph.remove_node(node_index);
        println!("result: {:?}", result);
        self.set_selected_node(None);
        self.set_editing_node(None);
    }
}

impl Graph {
    pub fn add_edge(&mut self, source: NodeIndex, target: NodeIndex) {
        self.graph.add_edge(source, target, ());
    }

    pub fn set_creating_edge(&mut self, target: Option<TempEdge>) {
        let mut creating_edge = self.creating_edge.write().unwrap();
        *creating_edge = target;
        drop(creating_edge);
    }

    pub fn get_creating_edge(&self) -> Option<TempEdge> {
        self.creating_edge.read().unwrap().clone()
    }
}

pub fn render_graph(ui: &mut egui::Ui) {
    // println!("render_graph");

    let graph_resource: GraphResource = ui.ctx().data(|d| d.get_temp(Id::NULL)).unwrap();

    let node_indices = graph_resource.read_graph(|graph| {
        graph
            .graph
            .node_indices()
            .map(|idx| idx)
            .collect::<Vec<NodeIndex>>()
    });

    // println!("node_indices: {:?}", node_indices.len());

    for node_index in node_indices {
        // println!("node: {}", node.id);
        // Put the node id into the ui

        // 在屏幕上指定位置放置label控件

        ui.add(NodeWidget {
            node_index,
            // graph,
            // canvas_state,
        });
    }
}
