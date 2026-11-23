// PictoDesk canvas shader: one fullscreen triangle; the fragment shader
// maps screen pixels -> document pixels through the viewport, samples
// the engine's flattened render, and draws a checkerboard behind
// transparency + a dark surround outside the document.

struct Uniforms {
    offset: vec2<f32>,
    scale: f32,
    _pad: f32,
    doc_size: vec2<f32>,
    win_size: vec2<f32>,
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> u: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32> {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(pos[idx], 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let px = frag.xy;
    let doc = (px - u.offset) / u.scale;
    if (doc.x < 0.0 || doc.y < 0.0 || doc.x >= u.doc_size.x || doc.y >= u.doc_size.y) {
        return vec4<f32>(0.16, 0.16, 0.17, 1.0);
    }
    let uv = doc / u.doc_size;
    let c = textureSample(tex, samp, uv);
    let cell = floor(doc.x / 8.0) + floor(doc.y / 8.0);
    let checker = select(0.72, 0.88, (i32(cell) % 2) == 0);
    let rgb = mix(vec3<f32>(checker), c.rgb, c.a);
    return vec4<f32>(rgb, 1.0);
}
