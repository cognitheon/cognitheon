use std::sync::{Arc, RwLock};

use crate::canvas::CanvasState;

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

impl Default for CanvasStateResource {
    fn default() -> Self {
        Self(Arc::new(RwLock::new(CanvasState::default())))
    }
}
