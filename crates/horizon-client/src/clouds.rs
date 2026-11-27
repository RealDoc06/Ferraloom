#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Uniform {
    offset: [f32; 4],
    colors: [[f32; 4]; 4],
}
