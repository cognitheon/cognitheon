/// 代表一个粒子
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub struct Particle {
    // 粒子在屏幕或逻辑坐标中的位置
    pub pos: [f32; 2],
    // 粒子的速度
    pub vel: [f32; 2],
    // 粒子的剩余生命（秒）
    pub life: f32,
    // 预留填充，保证 16 字节对齐（或者也可以塞进其他信息）
    _pad: f32,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            pos: [0.0, 0.0],
            vel: [3.0, 3.0],
            life: 3.0,
            _pad: 0.0,
        }
    }
}

impl Particle {
    pub fn with_max_life(max_life: f32) -> Self {
        Self {
            pos: [0.0, 0.0],
            vel: [3.0, 3.0],
            life: max_life,
            _pad: 0.0,
        }
    }

    pub fn random_vel(mut self, max_vel: f32) -> Self {
        self.vel = [
            rand::random::<f32>() * max_vel,
            rand::random::<f32>() * max_vel,
        ];
        self
    }
}
