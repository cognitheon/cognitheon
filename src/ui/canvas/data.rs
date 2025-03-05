use egui::Pos2;
use petgraph::graph::NodeIndex;

use crate::{
    input::{input_state::InputState, state_manager::InputStateManager},
    resource::{CanvasStateResource, GraphResource, ParticleSystemResource},
    ui::temp_edge::TempEdge,
};

#[derive(Debug)]
pub struct CanvasWidget {
    pub input_manager: InputStateManager,
    pub input_state: InputState,
    pub temp_edge: Option<TempEdge>,
    pub graph_resource: GraphResource,
    pub canvas_state_resource: CanvasStateResource,
    // pub particle_system_resource: ParticleSystemResource,
    // pub input_state: InputState,
    pub input_busy: bool,
    pub drag_select_range: Option<[Pos2; 2]>,
}

impl CanvasWidget {
    pub fn new(
        graph_resource: GraphResource,
        canvas_state_resource: CanvasStateResource,
        // particle_system_resource: ParticleSystemResource,
    ) -> Self {
        Self {
            input_manager: InputStateManager::new(
                graph_resource.clone(),
                canvas_state_resource.clone(),
            ),
            input_state: InputState::Idle,
            temp_edge: None,
            graph_resource,
            canvas_state_resource,
            // particle_system_resource,
            input_busy: false,
            drag_select_range: None,
        }
    }

    pub fn update_selected_nodes(&mut self) {
        if let Some(range) = self.drag_select_range {
            let start_canvas = self
                .canvas_state_resource
                .read_resource(|canvas_state| canvas_state.to_canvas(range[0]));
            let end_canvas = self
                .canvas_state_resource
                .read_resource(|canvas_state| canvas_state.to_canvas(range[1]));

            let min_canvas_pos = Pos2::new(
                start_canvas.x.min(end_canvas.x),
                start_canvas.y.min(end_canvas.y),
            );
            let max_canvas_pos = Pos2::new(
                start_canvas.x.max(end_canvas.x),
                start_canvas.y.max(end_canvas.y),
            );

            let node_indices = self.graph_resource.read_resource(|graph| {
                graph
                    .graph
                    .node_indices()
                    .collect::<Vec<NodeIndex>>()
                    .iter()
                    .filter(|&node_index| {
                        let node = graph.get_node(*node_index).unwrap();
                        let node_pos = node.position;
                        node_pos.x >= min_canvas_pos.x
                            && node_pos.x <= max_canvas_pos.x
                            && node_pos.y >= min_canvas_pos.y
                            && node_pos.y <= max_canvas_pos.y
                    })
                    .copied()
                    .collect::<Vec<NodeIndex>>()
            });

            // println!("node_indices: {:?}", node_indices);
            self.graph_resource.with_resource(|graph| {
                graph.selected.clear();
                graph.select_nodes(node_indices);
            });
        }
    }
}
