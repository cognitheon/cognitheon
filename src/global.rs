use std::sync::{Arc, RwLock};

use crate::{canvas::CanvasState, graph::Graph};

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub struct GraphResource(pub Arc<RwLock<Graph>>);

impl GraphResource {
    pub fn with_graph<T>(&self, f: impl FnOnce(&mut Graph) -> T) -> T {
        let mut graph = self.0.write().unwrap();
        f(&mut graph)
    }

    pub fn read_graph<T>(&self, f: impl FnOnce(&Graph) -> T) -> T {
        let graph = self.0.read().unwrap();
        f(&graph)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub struct CanvasStateResource(pub Arc<RwLock<CanvasState>>);

impl CanvasStateResource {
    pub fn with_canvas_state<T>(&self, f: impl FnOnce(&mut CanvasState) -> T) -> T {
        let mut canvas_state = self.0.write().unwrap();
        f(&mut canvas_state)
    }

    pub fn read_canvas_state<T>(&self, f: impl FnOnce(&CanvasState) -> T) -> T {
        let canvas_state = self.0.read().unwrap();
        f(&canvas_state)
    }
}
