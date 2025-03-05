use crate::{
    canvas::CanvasState, gpu_render::particle::particle_system::ParticleSystem,
    graph::graph_impl::Graph,
};

mod resource_impl;

pub type GraphResource = resource_impl::Resource<Graph>;
pub type CanvasStateResource = resource_impl::Resource<CanvasState>;
pub type ParticleSystemResource = resource_impl::Resource<ParticleSystem>;
