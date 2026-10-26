import bevy_pbr::render::mesh_functions::mesh_position_local_to_clip;

struct CrowdFrame {
    params: vec4<f32>,
};
@group(3) @binding(0) var<uniform> frame: CrowdFrame;

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) positions: vec4<f32>,
    @location(4) appearance: vec4<f32>,
    @location(5) behavior: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) material_class: f32,
};

// Keep this analytic height function in sync with battle_core::terrain_height.
// It is evaluated per submitted vertex; the million-unit overview has one
// vertex per unit, while the detailed models have a strict population budget.
fn ground_height(p: vec2<f32>) -> f32 {
    if (frame.params.y < 0.5) {
        return sin(p.x * 0.065) * cos(p.y * 0.055) * 0.28;
    }
    if (frame.params.y < 1.5) {
        let valley = sin(p.y * 0.035);
        return valley * valley * 4.4 + sin(p.x * 0.04) * 0.25;
    }
    if (abs(p.x) <= 6.2 && abs(p.y) <= 5.5) { return 0.35; }
    if (abs(p.x) < 5.2) { return -1.0; }
    let bank = clamp((abs(p.x) - 5.2) / 3.0, 0.0, 1.0);
    return bank * 0.18 + sin(p.x * 0.05) * cos(p.y * 0.07) * 0.12;
}

@vertex
fn vertex(input: Vertex) -> VertexOutput {
    let p = mix(input.positions.zw, input.positions.xy, frame.params.x);
    var local = input.position;
    var normal = input.normal;
    let point_marker = input.uv.y > 90.0;
    let walk = input.behavior.y;
    let phase = frame.params.z * 8.5 + input.behavior.w;
    // A tiny procedural gait keeps the reference artwork readable without
    // allocating skeletons or animation players for each soldier.
    if (!point_marker) {
        if (abs(input.uv.y) <= 1.0) {
            if (abs(input.uv.y) > 0.5) {
                let side = input.uv.y;
                local.x += sin(phase) * 0.18 * walk * side;
                local.y += max(0.0, sin(phase) * side) * 0.10 * walk;
            } else {
                local.y += abs(sin(phase)) * 0.035 * walk;
            }
        }
        let c = cos(input.behavior.x);
        let s = sin(input.behavior.x);
        local = vec3<f32>(c * local.x - s * local.z, local.y, s * local.x + c * local.z);
        normal = vec3<f32>(c * normal.x - s * normal.z, normal.y, s * normal.x + c * normal.z);
    }
    let world = local * input.appearance.w + vec3<f32>(p.x, ground_height(p) + 0.015, p.y);
    let identity = mat4x4<f32>(
        vec4<f32>(1.0, 0.0, 0.0, 0.0), vec4<f32>(0.0, 1.0, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 1.0, 0.0), vec4<f32>(0.0, 0.0, 0.0, 1.0)
    );
    var output: VertexOutput;
    // World-space instance data: deliberately independent of Bevy mesh index 0.
    output.clip_position = mesh_position_local_to_clip(identity, vec4<f32>(world, 1.0));
    output.normal = normal;
    output.material_class = select(input.uv.x, -1.0, point_marker);
    output.color = vec4<f32>(mix(input.appearance.rgb, vec3<f32>(1.0, 0.75, 0.32), input.behavior.z * 0.55), 1.0);
    return output;
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    // A point is an intentionally cheap one-pixel tactical marker. Points stay
    // beneath close models so LOD changes never erase a unit's representation.
    if (input.material_class < -0.5) {
        return vec4<f32>(input.color.rgb * 1.3, 1.0);
    }
    let n = normalize(input.normal);
    let diffuse = max(dot(n, normalize(vec3<f32>(-0.45, 0.85, 0.30))), 0.0);
    let light = 0.38 + diffuse * 0.62;
    // Mesh UV0 encodes authored material classes (team, skin, metal, leather).
    var base = input.color.rgb;
    if (input.material_class > 0.5 && input.material_class < 1.5) { base = vec3<f32>(0.58, 0.37, 0.22); }
    if (input.material_class > 1.5 && input.material_class < 2.5) { base = vec3<f32>(0.31, 0.37, 0.39); }
    if (input.material_class > 2.5) { base = vec3<f32>(0.12, 0.08, 0.05); }
    return vec4<f32>(base * light, 1.0);
}
