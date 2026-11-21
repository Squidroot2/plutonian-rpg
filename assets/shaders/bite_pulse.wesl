import bevy_ui_render::ui_vertex_output::UiVertexOutput;

@group(1) @binding(0)
var<uniform> age: f32;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let p = abs((in.uv - 0.5) * 2.0);
    let edge = pow(pow(p.x, 4.0) + pow(p.y, 4.0), 0.25);
    let vignette = smoothstep(0.66, 1.12, edge);
    let t = clamp(age, 0.0, 1.0);
    let fade = (1.0 - t) * (1.0 - t);
    return vec4<f32>(0.59, 0.061, 0.054, 0.25 * vignette * fade);
}
