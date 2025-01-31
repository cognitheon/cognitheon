pub mod edge;
pub mod node;
pub mod node_render_info;

use crate::global::{CanvasStateResource, GraphResource};
use crate::graph::node::Node;
use crate::ui::temp_edge::TempEdge;
use edge::Edge;
use egui::Id;
use petgraph::graph::{EdgeIndex, NodeIndex};

use crate::ui::node::NodeWidget;

// #[typetag::serde(tag = "type")]
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Graph {
    pub graph: petgraph::stable_graph::StableGraph<Node, Edge>,
    pub selected_node: Option<NodeIndex>,
    pub editing_node: Option<NodeIndex>,
    pub temp_edge: Option<TempEdge>,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            graph: petgraph::stable_graph::StableGraph::new(),
            selected_node: None,
            editing_node: None,
            temp_edge: None,
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

impl Graph {
    pub fn add_edge(&mut self, edge: Edge) {
        self.graph.add_edge(edge.source, edge.target, edge);
    }

    pub fn set_temp_edge(&mut self, temp_edge: Option<TempEdge>) {
        if let Some(temp_edge_clone) = temp_edge.clone() {
            println!("set_temp_edge: {:?}", temp_edge_clone.target);
        }
        self.temp_edge = temp_edge;
    }

    // 返回创建的临时边
    pub fn get_temp_edge(&self) -> Option<TempEdge> {
        if let Some(temp_edge) = self.temp_edge.clone() {
            println!("get_temp_edge: {:?}", temp_edge.target);
        }
        self.temp_edge.clone()
    }
}

pub fn render_graph(
    ui: &mut egui::Ui,
    graph_resource: GraphResource,
    canvas_state_resource: CanvasStateResource,
) {
    // println!("render_graph");

    let node_indices = graph_resource.read_graph(|graph| {
        graph
            .graph
            .node_indices()
            .map(|idx| idx)
            .collect::<Vec<NodeIndex>>()
    });

    let edge_indices = graph_resource.read_graph(|graph| {
        graph
            .graph
            .edge_indices()
            .map(|idx| idx)
            .collect::<Vec<EdgeIndex>>()
    });

    // println!("node_indices: {:?}", node_indices.len());

    for node_index in node_indices {
        // println!("node: {}", node.id);
        // Put the node id into the ui

        ui.add(NodeWidget {
            node_index,
            graph_resource: graph_resource.clone(),
            canvas_state_resource: canvas_state_resource.clone(),
        });
    }

    // for edge_index in edge_indices {
    //     ui.add(EdgeWidget {
    //         edge_index,
    //         // graph,
    //         // canvas_state,
    //     });
    // }
}
