//particle_shader.wgsl

struct Particle {
    pos : vec2 < f32>,
    vel : vec2 < f32>,
    life : f32,
    _pad : f32,
};

@group(0) @binding(0)
var<storage, read> particles : array<Particle>;

@group(0) @binding(1)
var<uniform> u_data : vec4 < f32>; //x: time, y,z,w: 保留

struct VSOutput {
    @builtin(position) pos : vec4 < f32>,
    @location(0) color : vec4 < f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index : u32) -> VSOutput {
    let p = particles[vertex_index];
    //这里假设粒子坐标就是屏幕空间 [-width/2, width/2] 之类，或你自己管理变换。
    //如果你想用 NDC(标准化设备坐标)，需要自行做投影变换。

    var out : VSOutput;
    var canvas_width = u_data.y;
    var canvas_height = u_data.z;
    //转换到 canvas 坐标系
    var x = (p.pos[0] / canvas_width) * 2.0 - 1.0;
    var y = (p.pos[1] / canvas_height) * 2.0 - 1.0;
    out.pos = vec4 < f32 > (x, -y, 0.0, 1.0);

    var alpha = p.life / u_data.w;
    //颜色可以根据 life 的剩余值或者 time 做一些变化，这里仅做简单的绿色
    out.color = vec4 < f32 > (0.0, 1.0, 0.0, alpha);
    return out;
}

@fragment
fn fs_main(in : VSOutput) -> @location(0) vec4 < f32> {
    //简单返回传入的颜色
    return in.color;
}
