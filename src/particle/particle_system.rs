use eframe::egui_wgpu::wgpu;
use eframe::egui_wgpu::wgpu::util::DeviceExt;
use eframe::wgpu::PipelineCompilationOptions;
use rand::Rng;
use std::num::NonZeroU64;

use super::particle::Particle;

#[derive(Debug)]
/// 粒子系统
pub struct ParticleSystem {
    /// CPU端的粒子数组
    pub particles: Vec<Particle>,
    /// 每帧要新生成的粒子数量
    pub spawn_per_frame: u32,
    /// 粒子的最大存活时间
    pub max_life: f32,

    /// 下面是 GPU 相关的资源
    /// 用来存储粒子数据的缓冲（Storage Buffer）
    pub storage_buffer: wgpu::Buffer,
    /// 用来存储全局 uniform 的缓冲，例如屏幕尺寸、时间等
    pub uniform_buffer: wgpu::Buffer,
    /// bind group layout
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// 实际绑定粒子数据 & uniform 的 bind group
    pub bind_group: wgpu::BindGroup,
    /// 渲染管线
    pub render_pipeline: wgpu::RenderPipeline,
    // 粒子最大速度
    pub max_vel: f32,
}

impl ParticleSystem {
    /// 创建新的粒子系统。这里演示了带一些参数，以便你可以在不同组件中初始化。
    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        max_particles: usize, // 最大粒子数
        spawn_per_frame: u32, // 每帧生成多少粒子
        max_life: f32,        // 粒子最大生命
        max_vel: f32,         // 粒子最大速度
    ) -> Self {
        // 创建 CPU 端粒子存储（此处先初始化为空）
        let particles = vec![Particle::default(); max_particles];

        // 创建 GPU StorageBuffer，用来存储粒子数据
        let storage_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("particle_storage_buffer"),
            contents: bytemuck::cast_slice(&particles),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        // 可以存一些全局数据，例如屏幕尺寸、时间等，但这里暂时只放一个2D变换/时间等。
        // 我们示例只写16字节，以防止对齐问题
        let uniform_data = [0.0f32; 4];
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("particle_uniform_buffer"),
            contents: bytemuck::cast_slice(&uniform_data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // 创建 BindGroupLayout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle_bind_group_layout"),
            entries: &[
                // 绑定 storage_buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(
                            std::mem::size_of::<Particle>() as u64 * max_particles as u64,
                        ),
                    },
                    count: None,
                },
                // 绑定 uniform_buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        // 16字节
                        min_binding_size: NonZeroU64::new(16),
                    },
                    count: None,
                },
            ],
        });

        // 创建 BindGroup
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: storage_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        // 创建渲染管线
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle_shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "../../assets/particle.wgsl"
            ))),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // 这里为了演示，采用点列表(PrimitiveTopology::PointList)来绘制粒子
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particle_pipeline"),
            cache: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                compilation_options: PipelineCompilationOptions::default(),
                entry_point: Some("vs_main"), // 对应 WGSL 中的入口函数
                buffers: &[],                 // 我们用 StorageBuffer，而不是传统的 VertexBuffer
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                compilation_options: PipelineCompilationOptions::default(),
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::PointList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Self {
            particles,
            spawn_per_frame,
            max_life,
            storage_buffer,
            uniform_buffer,
            bind_group_layout,
            bind_group,
            render_pipeline,
            max_vel,
        }
    }

    /// 每帧更新逻辑（CPU 端）
    pub fn update_particles(&mut self, dt: f32, mouse_pos: Option<[f32; 2]>, _rect: egui::Rect) {
        // println!("dt: {:?}", dt);
        // 衰减所有已存在的粒子寿命
        for p in &mut self.particles {
            if p.life > 0.0 {
                p.random_vel(self.max_vel);
                p.life -= dt;
                // 如果粒子还活着，就更新位置
                if p.life > 0.0 {
                    p.pos[0] += p.vel[0] * dt;
                    p.pos[1] += p.vel[1] * dt;
                }
            }
        }

        // 如果检测到鼠标移动（或者直接按每帧都生成），我们就生成一些新的粒子
        if let Some(pos) = mouse_pos {
            // 随机生成 spawn_per_frame 个粒子
            let mut rng = rand::rng();
            for _ in 0..self.spawn_per_frame {
                // 找一个死亡的粒子 “复用”
                if let Some(slot) = self.particles.iter_mut().find(|p| p.life <= 0.0) {
                    slot.pos = pos;
                    slot.vel = [rng.random_range(-50.0..50.0), rng.random_range(-50.0..50.0)];
                    slot.life = self.max_life;
                }
            }
        }
    }

    /// 将 CPU 数据写回 GPU
    pub fn upload_to_gpu(&self, queue: &wgpu::Queue) {
        queue.write_buffer(
            &self.storage_buffer,
            0,
            bytemuck::cast_slice(&self.particles),
        );
    }

    /// 更新 uniform，比如时间、屏幕大小等
    /// 这里示例只传入一个 time 或者 scaleFactor
    pub fn update_uniform(&self, queue: &wgpu::Queue, time: f32, rect: egui::Rect) {
        let data = [time, rect.width(), rect.height(), self.max_life];
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&data));
    }

    /// 在渲染时调用
    pub fn paint(&self, render_pass: &mut wgpu::RenderPass<'static>, particle_count: u32) {
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        // 我们绘制的粒子数 = 还在“存活”中的粒子或者直接写最大值
        // 此处简单写成全部绘制 max_particles；也可以做一个统计
        render_pass.draw(0..particle_count, 0..1);
    }
}
