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
}

impl CanvasWidget {
    pub fn new(graph_resource: GraphResource, canvas_state_resource: CanvasStateResource) -> Self {
        Self {
            temp_edge: None,
            graph_resource,
            canvas_state_resource,
            input_state: InputState::default(),
            input_busy: false,
        }
    }
}
