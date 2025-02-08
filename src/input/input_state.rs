use egui::{Pos2, Vec2};
use petgraph::graph::{EdgeIndex, NodeIndex};

#[derive(Debug, Clone)]
pub enum InputState {
    Idle,
    /// a
    Dragging(DragType),
    /// Selecting
    Selecting(SelectType),
    /// Cutting
    Cutting([Pos2; 2]),
    /// Click to create a node
    ClickCreateNode(Pos2),
    /// Drag to create a node
    DragCreateNode([Pos2; 2]),
    /// Drag to create an edge
    DragCreateEdge([Pos2; 2]),
}

impl Default for InputState {
    fn default() -> Self {
        InputState::Idle
    }
}

/// Drag Type
#[derive(Debug, Clone)]
pub enum DragType {
    /// Dragging on the canvas
    Canvas,
    /// Dragging on a node
    Node(DragNode),
    /// Dragging on an edge
    Edge(DragEdge),
    /// Dragging to create an edge
    TempEdge([Pos2; 2]),
}

#[derive(Debug, Clone)]
pub enum SelectType {
    Single(NodeIndex),
    Range,
}

#[derive(Debug, Clone)]
pub struct DragNode {
    pub node_index: NodeIndex,
    pub delta: Vec2,
}

#[derive(Debug, Clone)]
pub struct DragEdge {
    pub edge_index: EdgeIndex,
    pub delta: Vec2,
    pub part: EdgePart,
}

#[derive(Debug, Clone)]
pub enum EdgePart {
    Anchor(usize),
    // Handle(usize),
    Line,
}
