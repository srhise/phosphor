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

// Source framebuffer is 720x400; scanlines run at that row frequency.
const SOURCE_HEIGHT: f32 = 400.0;
const CURVATURE: f32 = 0.018;
const SCANLINE_DEPTH: f32 = 0.18;
const BLOOM_RADIUS: f32 = 0.0016;
const BLOOM_STRENGTH: f32 = 0.38;
const VIGNETTE_STRENGTH: f32 = 0.28;

// Pull the corners in slightly, as a curved tube does.
fn barrel(uv: vec2<f32>) -> vec2<f32> {
    let centered = uv * 2.0 - 1.0;
    let r2 = dot(centered, centered);
    return (centered * (1.0 + CURVATURE * r2)) * 0.5 + 0.5;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (u.effects < 0.5) {
        return textureSample(tex, samp, in.uv);
    }

    let uv = barrel(in.uv);
    // Past the edge of the tube there is no picture.
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    var color = textureSample(tex, samp, uv).rgb;

    // Phosphor bloom: a small cross-shaped tap set, so bright text glows
    // into the dark around it the way it did on a real tube.
    var glow = vec3<f32>(0.0);
    glow = glow + textureSample(tex, samp, uv + vec2<f32>( BLOOM_RADIUS, 0.0)).rgb;
    glow = glow + textureSample(tex, samp, uv + vec2<f32>(-BLOOM_RADIUS, 0.0)).rgb;
    glow = glow + textureSample(tex, samp, uv + vec2<f32>(0.0,  BLOOM_RADIUS)).rgb;
    glow = glow + textureSample(tex, samp, uv + vec2<f32>(0.0, -BLOOM_RADIUS)).rgb;
    glow = glow + textureSample(tex, samp, uv + vec2<f32>( BLOOM_RADIUS,  BLOOM_RADIUS)).rgb;
    glow = glow + textureSample(tex, samp, uv + vec2<f32>(-BLOOM_RADIUS, -BLOOM_RADIUS)).rgb;
    color = color + glow * (BLOOM_STRENGTH / 6.0);

    // Scanlines at the source row frequency.
    let scan = 1.0 - SCANLINE_DEPTH * pow(sin(uv.y * SOURCE_HEIGHT * 3.14159265), 2.0);
    color = color * scan;

    // A very slight mains-frequency wobble in brightness.
    color = color * (1.0 + 0.012 * sin(u.time * 6.0));

    // Vignette.
    let d = distance(uv, vec2<f32>(0.5, 0.5));
    color = color * (1.0 - VIGNETTE_STRENGTH * d * d);

    return vec4<f32>(color, 1.0);
}
