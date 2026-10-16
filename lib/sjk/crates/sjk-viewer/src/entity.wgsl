struct Camera {
    view_projection: mat4x4<f32>,
    position: vec3<f32>,
    _padding: f32,
}

@group(0) @binding(0)
var<uniform> camera: Camera;
@group(1) @binding(0)
var effect_atlas: texture_2d<f32>;
@group(1) @binding(1)
var effect_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) texture_coordinates: vec2<f32>,
    @location(2) particle: f32,
    @location(3) effect_local_uv: vec2<f32>,
    @location(4) effect_uv_rect: vec4<f32>,
    @location(5) effect_uv_transform: vec4<f32>,
}

const CUBE: array<vec3<f32>, 36> = array<vec3<f32>, 36>(
    vec3(-1.0, -1.0, -1.0), vec3( 1.0, -1.0, -1.0), vec3( 1.0,  1.0, -1.0),
    vec3(-1.0, -1.0, -1.0), vec3( 1.0,  1.0, -1.0), vec3(-1.0,  1.0, -1.0),
    vec3(-1.0, -1.0,  1.0), vec3( 1.0,  1.0,  1.0), vec3( 1.0, -1.0,  1.0),
    vec3(-1.0, -1.0,  1.0), vec3(-1.0,  1.0,  1.0), vec3( 1.0,  1.0,  1.0),
    vec3(-1.0, -1.0, -1.0), vec3(-1.0,  1.0, -1.0), vec3(-1.0,  1.0,  1.0),
    vec3(-1.0, -1.0, -1.0), vec3(-1.0,  1.0,  1.0), vec3(-1.0, -1.0,  1.0),
    vec3( 1.0, -1.0, -1.0), vec3( 1.0,  1.0,  1.0), vec3( 1.0,  1.0, -1.0),
    vec3( 1.0, -1.0, -1.0), vec3( 1.0, -1.0,  1.0), vec3( 1.0,  1.0,  1.0),
    vec3(-1.0, -1.0, -1.0), vec3(-1.0, -1.0,  1.0), vec3( 1.0, -1.0,  1.0),
    vec3(-1.0, -1.0, -1.0), vec3( 1.0, -1.0,  1.0), vec3( 1.0, -1.0, -1.0),
    vec3(-1.0,  1.0, -1.0), vec3( 1.0,  1.0, -1.0), vec3( 1.0,  1.0,  1.0),
    vec3(-1.0,  1.0, -1.0), vec3( 1.0,  1.0,  1.0), vec3(-1.0,  1.0,  1.0),
);

const QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
    vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0),
);

@vertex
fn vertex_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) entity_position: vec3<f32>,
    @location(1) kind: u32,
    @location(2) effect_size: f32,
    @location(3) effect_alpha: f32,
    @location(4) effect_uv_rect: vec4<f32>,
    @location(5) effect_color: vec4<f32>,
    @location(6) effect_direction: vec3<f32>,
    @location(7) effect_rotation: f32,
    @location(8) effect_uv_transform: vec4<f32>,
) -> VertexOutput {
    var output: VertexOutput;
    output.texture_coordinates = vec2(0.5);
    output.particle = 0.0;
    output.effect_local_uv = vec2(0.0);
    output.effect_uv_rect = vec4(0.0);
    output.effect_uv_transform = vec4(1.0, 1.0, 0.0, 0.0);
    if kind == 3u || kind == 4u || kind == 5u || kind == 6u || kind == 7u {
        if vertex_index >= 6u {
            output.clip_position = vec4(2.0, 2.0, 2.0, 1.0);
            output.color = vec4(0.0);
            return output;
        }
        let source_local = QUAD[vertex_index];
        let radians = radians(effect_rotation);
        let local = vec2(
            source_local.x * cos(radians) - source_local.y * sin(radians),
            source_local.x * sin(radians) + source_local.y * cos(radians),
        );
        let view_direction = normalize(camera.position - entity_position);
        var world: vec3<f32>;
        if kind == 6u {
            output.clip_position = vec4(
                entity_position.xy + local * effect_direction.xy,
                0.0,
                1.0,
            );
            output.color = vec4(effect_color.rgb, effect_alpha);
            output.texture_coordinates = source_local * 0.5 + vec2(0.5);
            output.effect_local_uv = output.texture_coordinates;
            output.effect_uv_rect = effect_uv_rect;
            output.effect_uv_transform = effect_uv_transform;
            output.particle = 2.0;
            return output;
        } else if kind == 5u && dot(effect_direction, effect_direction) > 0.0001 {
            let normal = normalize(effect_direction);
            let reference = select(vec3(0.0, 0.0, 1.0), vec3(0.0, 1.0, 0.0), abs(normal.z) > 0.95);
            let right = normalize(cross(reference, normal));
            let up = normalize(cross(normal, right));
            world = entity_position + normal * 0.04 + (right * local.x + up * local.y) * max(effect_size, 0.01);
        } else if kind == 4u && dot(effect_direction, effect_direction) > 0.0001 {
            let axis = normalize(effect_direction);
            var right = cross(axis, view_direction);
            if dot(right, right) < 0.0001 {
                right = cross(axis, vec3(0.0, 0.0, 1.0));
            }
            right = normalize(right);
            let along = (local.y + 1.0) * 0.5;
            world = entity_position + effect_direction * along + right * local.x * max(effect_size, 0.01);
        } else {
            let reference = select(vec3(0.0, 0.0, 1.0), vec3(0.0, 1.0, 0.0), abs(view_direction.z) > 0.95);
            let right = normalize(cross(reference, view_direction));
            let up = normalize(cross(view_direction, right));
            world = entity_position + (right * local.x + up * local.y) * max(effect_size, 0.01);
        }
        output.clip_position = camera.view_projection * vec4(world, 1.0);
        output.color = vec4(effect_color.rgb, effect_alpha);
        output.texture_coordinates = source_local * 0.5 + vec2(0.5);
        output.effect_local_uv = output.texture_coordinates;
        output.effect_uv_rect = effect_uv_rect;
        output.effect_uv_transform = effect_uv_transform;
        // World icons retain depth testing but skip the soft-particle intersection fade.
        output.particle = select(1.0, 2.0, kind == 7u);
        return output;
    }

    var scale = vec3(15.0, 15.0, 28.0);
    var color = vec3(0.25, 0.75, 1.0);
    if kind == 1u {
        scale = vec3(9.0);
        color = vec3(1.0, 0.65, 0.2);
    } else if kind == 2u {
        scale = vec3(4.0);
        color = vec3(1.0, 0.2, 0.08);
    }
    output.clip_position = camera.view_projection * vec4(entity_position + CUBE[vertex_index] * scale, 1.0);
    output.color = vec4(color, 1.0);
    return output;
}

fn particle_color(input: VertexOutput) -> vec4<f32> {
    // Mip level from the unwrapped coordinates, taken before any branch.
    let unwrapped = input.effect_local_uv * input.effect_uv_transform.xy
        + input.effect_uv_transform.zw;
    let span = input.effect_uv_rect.zw - input.effect_uv_rect.xy;
    let gradient_x = dpdx(unwrapped) * span;
    let gradient_y = dpdy(unwrapped) * span;
    if input.particle > 0.5 {
        let transformed_uv = fract(unwrapped);
        let effect_uv = mix(input.effect_uv_rect.xy, input.effect_uv_rect.zw, transformed_uv);
        let texel = textureSampleGrad(effect_atlas, effect_sampler, effect_uv,
            gradient_x, gradient_y);
        let coverage = texel.a;
        if coverage < 0.01 {
            discard;
        }
        return vec4(texel.rgb * input.color.rgb, coverage * input.color.a);
    }
    return input.color;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return particle_color(input);
}
