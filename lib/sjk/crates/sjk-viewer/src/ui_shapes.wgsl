struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) local: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) start_color: vec4<f32>,
    @location(4) end_color: vec4<f32>,
    @location(5) parameters: vec2<f32>,
    @location(6) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) start_color: vec4<f32>,
    @location(3) end_color: vec4<f32>,
    @location(4) parameters: vec2<f32>,
    @location(5) uv: vec2<f32>,
}

@group(0) @binding(0) var icon_texture: texture_2d<f32>;
@group(0) @binding(1) var icon_sampler: sampler;

@vertex
fn vertex_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4(input.position, 0.0, 1.0);
    output.local = input.local;
    output.size = input.size;
    output.start_color = input.start_color;
    output.end_color = input.end_color;
    output.parameters = input.parameters;
    output.uv = input.uv;
    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.parameters.y > 3.5 {
        // Straight stroke: the quad runs along the line with one pixel of margin on
        // each side; coverage fades across one pixel at the stroke's edges.
        // end_color.x = stroke width in pixels.
        let across = abs(input.local.y - 0.5) * input.size.y - input.end_color.x * 0.5;
        let coverage = clamp(0.5 - across, 0.0, 1.0);
        if coverage <= 0.0 {
            discard;
        }
        return vec4(input.start_color.rgb, input.start_color.a * coverage);
    }
    if input.parameters.y > 2.5 {
        // Arc stroke with round caps: distance to the nearest point of the centre
        // line, which is the circle's arc clamped to its angular range.
        // end_color = (radius, width, start angle, sweep) in pixels and radians. The
        // knockout stripe, where the stroke is left out, is parameters.x (left edge), uv.x
        // (right edge) and uv.y (half height), in pixels from the centre; a zero height
        // is no stripe.
        let radius = input.end_color.x;
        let half_width = input.end_color.y * 0.5;
        let sweep = input.end_color.w;
        let pixel = input.local * input.size - input.size * 0.5;
        let middle = input.end_color.z + sweep * 0.5;
        var delta = atan2(pixel.y, pixel.x) - middle;
        // Wrap to (-pi, pi] so the arc may cross the +/-pi seam.
        delta = delta - 6.2831855 * floor((delta + 3.1415927) / 6.2831855);
        let limit = abs(sweep) * 0.5;
        let angle = middle + clamp(delta, -limit, limit);
        let nearest = radius * vec2(cos(angle), sin(angle));
        let distance = length(pixel - nearest) - half_width;
        var coverage = clamp(0.5 - distance, 0.0, 1.0);
        if input.uv.y > 0.0 {
            // The same rounded-box distance as the panels, of an unrounded stripe
            // (`sjk_ui::knockout_coverage`). The panel there has this stroke's colour and
            // alpha, so only what it leaves is added (`sjk_ui::knockout_remainder`).
            let q = abs(vec2(pixel.x - (input.parameters.x + input.uv.x) * 0.5, pixel.y))
                - vec2((input.uv.x - input.parameters.x) * 0.5, input.uv.y);
            let outside = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0);
            let knock = clamp(0.5 - outside, 0.0, 1.0);
            coverage = clamp(
                (coverage - knock) / max(1.0 - input.start_color.a * knock, 1.0e-4),
                0.0,
                1.0,
            );
        }
        if coverage <= 0.0 {
            discard;
        }
        return vec4(input.start_color.rgb, input.start_color.a * coverage);
    }
    if input.parameters.y > 1.5 {
        return textureSample(icon_texture, icon_sampler, input.uv) * input.start_color;
    }
    let radius = input.parameters.x;
    // A negative parameters.y is a rounded border: its magnitude is the ring width.
    let ring = -input.parameters.y;
    var coverage = 1.0;
    if radius > 0.0 || ring > 0.0 {
        // Signed distance to the rounded outline in pixels; the edge is
        // anti-aliased by fading alpha across one pixel of it (and across
        // the inner edge of a ring) instead of a hard discard.
        let pixel = input.local * input.size;
        let half_size = input.size * 0.5;
        let q = abs(pixel - half_size) - (half_size - vec2(radius));
        let distance = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
        coverage = clamp(0.5 - distance, 0.0, 1.0);
        if ring > 0.0 {
            coverage *= clamp(distance + ring + 0.5, 0.0, 1.0);
        }
        if coverage <= 0.0 {
            discard;
        }
    }
    let amount = select(input.local.x, input.local.y, input.parameters.y > 0.5);
    let color = mix(input.start_color, input.end_color, amount);
    return vec4(color.rgb, color.a * coverage);
}
