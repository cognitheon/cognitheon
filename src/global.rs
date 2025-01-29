use std::sync::{Arc, RwLock};

use crate::{canvas::CanvasState, graph::Graph};

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct GraphResource(pub Arc<RwLock<Graph>>);

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct CanvasStateResource(pub Arc<RwLock<CanvasState>>);
