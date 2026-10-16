struct HudState {
    crosshair_color: vec4<f32>,
    health_ratio: f32,
    armor_ratio: f32,
    force_ratio: f32,
    menu_open: f32,
    inverse_width: f32,
    inverse_height: f32,
    menu_row: f32,
    menu_row_count: f32,
    crosshair: f32,
    hud_visible: f32,
    status_visible: f32,
    menu_kind: f32,
    menu_phase: f32,
    damage_x: f32,
    damage_y: f32,
    damage_alpha: f32,
    damage_strength: f32,
    health_bar: vec4<f32>,
    armor_bar: vec4<f32>,
    force_bar: vec4<f32>,
    crosshair_parameters: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> hud: HudState;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vertex_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    let positions = array(
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0),
    );
    let uvs = array(
        vec2(0.0, 0.0),
        vec2(2.0, 0.0),
        vec2(0.0, 2.0),
    );
    var output: VertexOutput;
    output.clip_position = vec4(positions[index], 0.0, 1.0);
    output.uv = uvs[index];
    return output;
}

fn inside(point: vec2<f32>, minimum: vec2<f32>, maximum: vec2<f32>) -> bool {
    return all(point >= minimum) && all(point <= maximum);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let uv = input.uv;
    // The retained console owns its bounded menu fade. Do not draw the
    // obsolete opaque console panel (or its full-screen dimmer) beneath it.
    if hud.menu_open > 3.5 {
        discard;
    }
    if hud.menu_open > 2.5 {
        let edge = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
        let vignette = 1.0 - smoothstep(0.0, 0.32, edge);
        let glow = max(0.0, 1.0 - distance(uv, vec2(0.13, 0.82)) * 1.25);
        var color = vec4(0.006 + glow * 0.020, 0.012 + glow * 0.052, 0.021 + glow * 0.080, 0.76 + vignette * 0.15);
        if hud.menu_kind > 0.5 && hud.menu_kind < 1.5 {
            let aspect = hud.inverse_height / hud.inverse_width;
            let selected_x = select(0.102, 0.137, aspect > 2.1);
            let selected_y = 0.594 - hud.menu_row * 0.082;
            let pulse = 0.82 + 0.10 * sin(hud.menu_phase * 2.2);
            if inside(uv, vec2(selected_x, selected_y - 0.034), vec2(selected_x + 0.383, selected_y + 0.034)) {
                color = vec4(0.025, 0.15, 0.22, 0.84 * pulse);
            }
            if inside(uv, vec2(selected_x, selected_y - 0.034), vec2(selected_x + 0.004, selected_y + 0.034)) {
                color = vec4(0.20, 0.82, 1.0, 0.96);
            }
        }
        return color;
    }
    if hud.menu_open > 0.5 {
        if inside(uv, vec2(0.18, 0.10), vec2(0.82, 0.90)) {
            if inside(uv, vec2(0.18, 0.895), vec2(0.82, 0.90)) {
                return vec4(0.18, 0.76, 0.95, 0.96);
            }
            let selected_y = 0.665 - hud.menu_row * 0.052;
            if inside(uv, vec2(0.215, selected_y - 0.024), vec2(0.535, selected_y + 0.024)) {
                return vec4(0.06, 0.38, 0.53, 0.94);
            }
            if inside(uv, vec2(0.56, 0.17), vec2(0.565, 0.70)) {
                return vec4(0.16, 0.25, 0.32, 0.82);
            }
            return vec4(0.018, 0.028, 0.040, 0.965);
        }
        return vec4(0.003, 0.006, 0.011, 0.72);
    }
    let center = (uv - vec2(0.5) - hud.crosshair_parameters.xy)
        * hud.crosshair_parameters.zw / max(hud.crosshair, 0.001);
    let horizontal_outline = abs(center.y) <= 2.0 / 26.0
        && abs(center.x) >= 4.0 / 26.0
        && abs(center.x) <= 14.0 / 26.0;
    let vertical_outline = abs(center.x) <= 2.0 / 26.0
        && abs(center.y) >= 4.0 / 26.0
        && abs(center.y) <= 14.0 / 26.0;
    let horizontal_crosshair = abs(center.y) <= 1.0 / 26.0
        && abs(center.x) >= 5.0 / 26.0
        && abs(center.x) <= 13.0 / 26.0;
    let vertical_crosshair = abs(center.x) <= 1.0 / 26.0
        && abs(center.y) >= 5.0 / 26.0
        && abs(center.y) <= 13.0 / 26.0;
    if hud.hud_visible > 0.5 && hud.crosshair > 0.0 {
        if horizontal_crosshair || vertical_crosshair {
            return hud.crosshair_color;
        }
        if horizontal_outline || vertical_outline {
            return vec4(0.0, 0.0, 0.0, 0.82 * hud.crosshair_color.a);
        }
    }

    if hud.hud_visible > 0.5 && hud.damage_alpha > 0.0 {
        let raw_direction = vec2(hud.damage_x, -hud.damage_y);
        let directional = length(raw_direction) > 0.001;
        let direction = select(vec2(1.0, 0.0), normalize(raw_direction), directional);
        let aspect = hud.inverse_height / hud.inverse_width;
        let delta = vec2((uv.x - 0.5) * aspect, uv.y - 0.5);
        let radial = length(delta);
        let facing = select(1.0, dot(normalize(delta + vec2(0.0001)), direction), directional);
        let inner = 0.080 - 0.004 * hud.damage_strength;
        let outer = 0.092 + 0.004 * hud.damage_strength;
        if radial >= inner && radial <= outer && facing > 0.88 {
            return vec4(1.0, 0.11, 0.045, hud.damage_alpha * (0.55 + 0.40 * hud.damage_strength));
        }
    }

    let pixel = vec2(uv.x / hud.inverse_width, (1.0 - uv.y) / hud.inverse_height);
    let health_min = hud.health_bar.xy;
    let health_max = hud.health_bar.zw;
    let armor_min = hud.armor_bar.xy;
    let armor_max = hud.armor_bar.zw;
    let force_min = hud.force_bar.xy;
    let force_max = hud.force_bar.zw;
    if hud.hud_visible > 0.5 && hud.status_visible > 0.5 && inside(pixel, health_min - vec2(3.0), health_max + vec2(3.0)) {
        if pixel.x <= mix(health_min.x, health_max.x, hud.health_ratio) {
            return vec4(0.82, 0.12, 0.09, 0.92);
        }
        return vec4(0.05, 0.02, 0.02, 0.72);
    }
    if hud.hud_visible > 0.5 && hud.status_visible > 0.5 && inside(pixel, armor_min - vec2(3.0), armor_max + vec2(3.0)) {
        if pixel.x <= mix(armor_min.x, armor_max.x, hud.armor_ratio) {
            return vec4(0.12, 0.48, 0.95, 0.92);
        }
        return vec4(0.02, 0.035, 0.06, 0.72);
    }
    if hud.hud_visible > 0.5 && hud.status_visible > 0.5 && inside(pixel, force_min - vec2(3.0), force_max + vec2(3.0)) {
        if pixel.x >= mix(force_max.x, force_min.x, hud.force_ratio) {
            return vec4(0.25, 0.55, 1.0, 0.92);
        }
        return vec4(0.02, 0.035, 0.06, 0.72);
    }
    discard;
}
