// Presents the 720x400 VGA framebuffer at a 4:3 aspect ratio.
// Scanlines, bloom, curvature, and vignette arrive in Task 15.

struct Uniforms {
    // Clip-space scale that letterboxes the 4:3 image inside the surface.
    scale: vec2<f32>,
    time: f32,
    // 0.0 = plain, 1.0 = CRT effects.
    effects: f32,
};

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> u: Uniforms;

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    // A triangle strip covering the quad, scaled to preserve 4:3.
    var positions = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0,  1.0),
    );
    var uvs = array<vec2<f32>, 4>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
    );

    var out: VertexOutput;
    out.clip = vec4<f32>(positions[index] * u.scale, 0.0, 1.0);
    out.uv = uvs[index];
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, in.uv);
}
