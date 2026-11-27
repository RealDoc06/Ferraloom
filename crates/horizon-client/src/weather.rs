/// One unsplit camera-relative weather vertex. UV animation and atlas-local
/// repetition happen in the weather shader, rather than rebuilding tiled quads.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub rect: [f32; 4],
    pub animation: [f32; 4],
    pub bases: [u32; 4],
    pub color: u32,
    pub light: u32,
    pub snow: u32,
}

/// Current ticks are masked on the CPU before conversion to float, including
/// negative/large tick values. Do not use the renderer's unbounded game_time.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Time {
    pub values: [f32; 4],
}
