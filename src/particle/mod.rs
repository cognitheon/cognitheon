use egui::Vec2;

pub struct Particle {
    pub position: Vec2,
    pub velocity: Vec2,
    pub acceleration: Vec2,
    pub lifetime: f32,
}
