use std::sync::Arc;

use crate::globals::{canvas_state_resource::CanvasStateResource, graph_resource::GraphResource};
use crate::graph::node::Node;
use crate::ui::bezier::BezierEdge;
use crate::ui::edge::EdgeWidget;
use crate::ui::line_edge::LineEdge;
use crate::ui::node_render_observer::NodeRenderObserver;
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::{EdgeRef, IntoEdgeReferences};

use crate::ui::node::NodeWidget;

use super::edge::{Edge, EdgeType};
use super::selection::GraphSelection;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Graph {
    pub edge_type: EdgeType,
    pub graph: petgraph::stable_graph::StableGraph<Node, Edge>,
    #[serde(skip)]
    pub selected: GraphSelection,
    #[serde(skip)]
    pub editing_node: Option<NodeIndex>,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            edge_type: EdgeType::Line,
            graph: petgraph::stable_graph::StableGraph::new(),
            selected: GraphSelection::None,
            editing_node: None,
        }
    }
}

impl Graph {
    pub fn add_node(&mut self, node: Node) -> NodeIndex {
        self.graph.add_node(node)
    }

    pub fn add_node_with_edge(
        &mut self,
        node: Node,
        src_node_index: NodeIndex,
        canvas_state_resource: CanvasStateResource,
    ) -> NodeIndex {
        let node = node.clone();
        let dst_node_index = self.add_node(node.clone());
        let src_node = self.get_node(src_node_index).unwrap();

        self.add_edge(Edge::new(
            src_node_index,
            dst_node_index,
            src_node.position,
            node.clone().position,
            canvas_state_resource,
        ));
        dst_node_index
    }

    pub fn get_node(&self, node_index: NodeIndex) -> Option<&Node> {
        self.graph.node_weight(node_index)
    }

    pub fn get_node_mut(&mut self, node_index: NodeIndex) -> Option<&mut Node> {
        self.graph.node_weight_mut(node_index)
    }

    pub fn get_selected_nodes(&self) -> Vec<NodeIndex> {
        match &self.selected {
            GraphSelection::Node(nodes) => nodes.clone(),
            _ => vec![],
        }
    }

    pub fn is_node_selected(&self, node_index: NodeIndex) -> bool {
        match &self.selected {
            GraphSelection::Node(nodes) => nodes.contains(&node_index),
            _ => false,
        }
    }

    pub fn select_node(&mut self, node_index: NodeIndex) {
        match &mut self.selected {
            GraphSelection::Node(nodes) => nodes.push(node_index),
            _ => self.selected = GraphSelection::Node(vec![node_index]),
        }
    }

    pub fn select_nodes(&mut self, nodes: Vec<NodeIndex>) {
        match &mut self.selected {
            GraphSelection::Node(selected_nodes) => selected_nodes.extend(nodes),
            _ => self.selected = GraphSelection::Node(nodes),
        }
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
        // self.selected_nodes.clear();
        self.editing_node = None;
    }
}

impl Graph {
    pub fn select_edge(&mut self, edge_index: EdgeIndex) {
        match &mut self.selected {
            GraphSelection::Edge(edges) => edges.push(edge_index),
            _ => self.selected = GraphSelection::Edge(vec![edge_index]),
        }
    }

    pub fn add_edge(&mut self, edge: Edge) {
        self.graph.add_edge(edge.source, edge.target, edge);
    }

    pub fn get_edge(&self, edge_index: EdgeIndex) -> Option<&Edge> {
        self.graph.edge_weight(edge_index)
    }

    pub fn remove_edge(&mut self, edge_index: EdgeIndex) {
        self.graph.remove_edge(edge_index);
    }

    pub fn update_bezier_edge(&mut self, edge_index: EdgeIndex, bezier_edge: BezierEdge) {
        let edge = self.graph.edge_weight_mut(edge_index).unwrap();
        edge.bezier_edge = bezier_edge;
    }

    pub fn update_line_edge(&mut self, edge_index: EdgeIndex, line_edge: LineEdge) {
        let edge = self.graph.edge_weight_mut(edge_index).unwrap();
        edge.line_edge = line_edge;
    }

    pub fn edge_exists(&self, src_node_index: NodeIndex, dst_node_index: NodeIndex) -> bool {
        self.graph.contains_edge(src_node_index, dst_node_index)
    }

    pub fn edge_count_undirected(&self, node1_index: NodeIndex, node2_index: NodeIndex) -> usize {
        self.graph
            .edge_references()
            .filter(|edge| {
                edge.source() == node1_index && edge.target() == node2_index
                    || edge.source() == node2_index && edge.target() == node1_index
            })
            .count()
    }
}

impl Graph {
    pub fn reset(&mut self) {
        self.graph = petgraph::stable_graph::StableGraph::new();
        self.selected = GraphSelection::None;
        self.editing_node = None;
    }
}

pub fn render_graph(
    ui: &mut egui::Ui,
    graph_resource: GraphResource,
    canvas_state_resource: CanvasStateResource,
) {
    let node_indices =
        graph_resource.read_graph(|graph| graph.graph.node_indices().collect::<Vec<NodeIndex>>());

    // println!("node_indices: {:?}", node_indices.len());

    let edge_indices =
        graph_resource.read_graph(|graph| graph.graph.edge_indices().collect::<Vec<EdgeIndex>>());

    for edge_index in edge_indices {
        ui.add(EdgeWidget {
            edge_index,
            graph_resource: graph_resource.clone(),
            canvas_state_resource: canvas_state_resource.clone(),
        });
    }

    for node_index in node_indices {
        // println!("node: {}", node_index.index());
        // Put the node id into the ui

        let mut node_widget = NodeWidget::new(
            node_index,
            graph_resource.clone(),
            canvas_state_resource.clone(),
        );
        node_widget.add_observer(Arc::new(NodeRenderObserver::new(ui.ctx().clone())));
        ui.add(node_widget);
    }
}
