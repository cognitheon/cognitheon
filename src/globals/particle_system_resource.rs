use crate::particle::particle_system::ParticleSystem;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct ParticleSystemResource(pub Arc<RwLock<ParticleSystem>>);

impl ParticleSystemResource {
    pub fn new(particle_system: ParticleSystem) -> Self {
        Self(Arc::new(RwLock::new(particle_system)))
    }

    pub fn with_particle_system<T>(&self, f: impl FnOnce(&mut ParticleSystem) -> T) -> T {
        let mut particle_system = self.0.write().unwrap();
        f(&mut particle_system)
    }

    pub fn read_particle_system<T>(&self, f: impl FnOnce(&ParticleSystem) -> T) -> T {
        let particle_system = self.0.read().unwrap();
        f(&particle_system)
    }
}
