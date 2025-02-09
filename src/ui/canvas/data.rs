use egui::{Pos2, Rect};
use petgraph::graph::NodeIndex;

use crate::{
    globals::{canvas_state_resource::CanvasStateResource, graph_resource::GraphResource},
    input::input_state::InputState,
    ui::temp_edge::TempEdge,
};

#[derive(Debug)]
pub struct CanvasWidget {
    pub temp_edge: Option<TempEdge>,
    pub graph_resource: GraphResource,
    pub canvas_state_resource: CanvasStateResource,
    pub input_state: InputState,
    pub input_busy: bool,
    pub drag_select_range: Option<[Pos2; 2]>,
}

impl CanvasWidget {
    pub fn new(graph_resource: GraphResource, canvas_state_resource: CanvasStateResource) -> Self {
        Self {
            temp_edge: None,
            graph_resource,
            canvas_state_resource,
            input_state: InputState::default(),
            input_busy: false,
            drag_select_range: None,
        }
    }

    pub fn update_selected_nodes(&mut self) {
        if self.drag_select_range.is_some() {
            let range = self.drag_select_range.unwrap();
            let start_canvas = self
                .canvas_state_resource
                .read_canvas_state(|canvas_state| canvas_state.to_canvas(range[0]));
            let end_canvas = self
                .canvas_state_resource
                .read_canvas_state(|canvas_state| canvas_state.to_canvas(range[1]));

            let min_canvas_pos = Pos2::new(
                start_canvas.x.min(end_canvas.x),
                start_canvas.y.min(end_canvas.y),
            );
            let max_canvas_pos = Pos2::new(
                start_canvas.x.max(end_canvas.x),
                start_canvas.y.max(end_canvas.y),
            );

            let node_indices = self.graph_resource.read_graph(|graph| {
                graph
                    .graph
                    .node_indices()
                    .collect::<Vec<NodeIndex>>()
                    .iter()
                    .filter(|&node_index| {
                        let node = graph.get_node(*node_index).unwrap();
                        let node_pos =
                            self.canvas_state_resource
                                .read_canvas_state(|canvas_state| {
                                    canvas_state.to_canvas(node.position)
                                });
                        node_pos.x >= min_canvas_pos.x
                            && node_pos.x <= max_canvas_pos.x
                            && node_pos.y >= min_canvas_pos.y
                            && node_pos.y <= max_canvas_pos.y
                    })
                    .copied()
                    .collect::<Vec<NodeIndex>>()
            });

            println!("node_indices: {:?}", node_indices);
            self.graph_resource.with_graph(|graph| {
                graph.selected_nodes.clear();
                graph.selected_nodes.extend(node_indices);
            });
        }
    }
}
