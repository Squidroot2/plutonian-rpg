import bevy_pbr::render::forward_io::VertexOutput;

@group(constants::MATERIAL_BIND_GROUP) @binding(0)
var<uniform> age: f32;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = (in.uv - 0.5) * 2.0;
    let radius = length(p);
    let t = clamp(age, 0.0, 1.0);
    let ease = 1.0 - (1.0 - t) * (1.0 - t);
    let fade = 1.0 - smoothstep(0.13, 1.0, t);

    let center = 1.0 - smoothstep(0.015, 0.27 - 0.11 * t, radius);
    let ring_radius = mix(0.30, 0.81, ease);
    let ring = 1.0 - smoothstep(0.015, 0.064, abs(radius - ring_radius));
    let angle = atan2(p.y, p.x + 0.00001);
    let rays = pow(abs(cos(3.0 * angle + 0.45)), 27.0);
    let spokes = rays * smoothstep(0.17, 0.30, radius)
        * (1.0 - smoothstep(ring_radius, min(0.99, ring_radius + 0.16), radius));

    let core = center * 0.9;
    let amber = ring * 0.73 + spokes * 0.45;
    let color = mix(vec3<f32>(1.7, 0.72, 0.24), vec3<f32>(2.75, 2.61, 2.35),
        core / max(0.001, core + amber));
    return vec4<f32>(color, clamp((core + amber) * fade, 0.0, 0.9));
}
