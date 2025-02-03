use std::sync::Arc;

use eframe::{
    egui_wgpu::{CallbackResources, CallbackTrait, ScreenDescriptor},
    wgpu::{self, CommandBuffer, CommandEncoder, RenderPass},
};
use egui::PaintCallbackInfo;

use super::particle_system::ParticleSystem;

pub struct ParticleCallback {
    pub mouse_pos: Option<[f32; 2]>,
    pub dt: f32,
    pub rect: egui::Rect,
}

impl ParticleCallback {
    pub fn new(mouse_pos: [f32; 2], dt: f32, rect: egui::Rect) -> Self {
        Self {
            mouse_pos: Some(mouse_pos),
            dt,
            rect,
        }
    }
}

impl CallbackTrait for ParticleCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &ScreenDescriptor,
        _encoder: &mut CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<CommandBuffer> {
        // 拿到我们之前插入的 particle_system
        let particle_system: &mut ParticleSystem = resources.get_mut().unwrap();

        // 1. 在 CPU 端更新粒子
        particle_system.update_particles(self.dt, self.mouse_pos, self.rect);

        // 2. 把更新后的粒子数据上传到 GPU
        particle_system.upload_to_gpu(queue);

        // 3. 更新 uniform（可选，这里用 time 做点动画效果）
        // 这里用 time = 0.0 假设一下，你也可以通过别的方法把 time 传进来
        // 如果需要全局time，可将其存到 particle_system 内部或 callback 里
        let time = 0.0;
        particle_system.update_uniform(queue, time, self.rect);

        // 可以返回额外的 command buffers（可选）
        Vec::new()
    }

    fn paint(
        &self,
        _info: PaintCallbackInfo,
        render_pass: &mut RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        let particle_system: &ParticleSystem = resources.get().unwrap();

        // 这里我们直接画全部的粒子
        let particle_count = particle_system.particles.len() as u32;
        particle_system.paint(render_pass, particle_count);
    }
}
