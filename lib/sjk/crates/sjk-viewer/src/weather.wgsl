// Weather: rain streaks, sprite clouds (snow, dust, mist) and splashes, drawn into the
// display-space effect layer after the effects, as rd-vanilla draws its world effects
// last. Every particle is derived from its instance index and the flow the CPU
// integrates: no vertex or instance buffer exists.
//
// Instance numbers: bits 16.. are the cloud slot (0-4) or 7 for splashes, bit 15 is the
// far rain layer, bits 0-14 the particle.
struct Camera {
    view_projection: mat4x4<f32>,
    position: vec3<f32>,
    shader_time: f32,
    forward: vec3<f32>,
    view_flags: f32,
};

struct Cloud {
    // rgb display colour, a the reference's alpha target.
    color: vec4<f32>,
    // x half width, y streak length or sprite half height, z image layer, w spin.
    shape: vec4<f32>,
    // xyz box corner relative to the camera, w near-layer count.
    box_min: vec4<f32>,
    // xyz box size, w far-layer count.
    box_size: vec4<f32>,
    // x far layer's horizontal scale, y far layer's opacity, zw unused.
    layers: vec4<f32>,
    // Per mass bucket: xyz velocity.
    velocity: array<vec4<f32>, 8>,
    // Per mass bucket: xyz flow offset, wrapped to the far box (x, y) and the box (z).
    offset: array<vec4<f32>, 8>,
};

struct Weather {
    // First cell x, y of the cover window, and one past its last.
    window: vec4<f32>,
    // x cell size, y cover texture size, z cover in use, w weather time (seconds).
    cover: vec4<f32>,
    // xy viewport pixels, z global fog's 1 / depthForOpaque (0 none), w splash count.
    view: vec4<f32>,
    // rgb light tint, w the cloud whose rain splashes.
    light: vec4<f32>,
    // The main view's clip-to-world transform, for the fog pass.
    inverse_view_projection: mat4x4<f32>,
    // x rain haze per unit, y ground fog per unit at the floor, z the height it thins
    // over, w samples along each ray (0: no fog pass).
    haze: vec4<f32>,
    // rgb fog colour (display), w 1 once the noise volume is made.
    fog_color: vec4<f32>,
    // xyz the fog's drift with the wind, w the longest ray marched.
    fog_flow: vec4<f32>,
    // xy the far cover's first corner, z its column width, w 1 once it is surveyed.
    far: vec4<f32>,
    // x how wet the rain makes what it falls on (0 dry), y what it does there
    // (`r_weatherQuality`'s: 1 wet, 2 running water, 3 puddles), z running water's scroll
    // down the walls (wrapped to STREAK_LENGTH), w its pattern's change (wrapped to 1).
    wet: vec4<f32>,
    // rgb the overcast sky a wet surface mirrors, in the scene's light units.
    wet_sky: vec4<f32>,
    // xyz the direction the rain falls (unit), w unused.
    rain: vec4<f32>,
    clouds: array<Cloud, 5>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<uniform> weather: Weather;
@group(1) @binding(1) var cover_map: texture_2d<f32>;
@group(1) @binding(2) var images: texture_2d_array<f32>;
@group(1) @binding(3) var image_sampler: sampler;
@group(1) @binding(4) var noise: texture_3d<f32>;
@group(1) @binding(5) var noise_sampler: sampler;
@group(1) @binding(6) var far_cover: texture_2d<f32>;
@group(2) @binding(0) var scene_depth: texture_depth_2d;

// Additive (GL_ONE GL_ONE) or alpha-blended sprites.
override additive: bool = true;

const SPLASH: u32 = 1u;
const LIQUID: u32 = 2u;
const VOID: u32 = 4u;
const HIDDEN: vec4<f32> = vec4(2.0, 2.0, 2.0, 1.0);
const TAU: f32 = 6.2831853;

// PCG hash (Jarzynski and Olano, 2020).
fn pcg(value: u32) -> u32 {
    let state = value * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn hash3(value: u32) -> vec3<f32> {
    let a = pcg(value);
    let b = pcg(a);
    let c = pcg(b);
    return vec3(f32(a), f32(b), f32(c)) * (1.0 / 4294967296.0);
}

// Two triangles: corners (-,-) (+,-) (+,+) and (-,-) (+,+) (-,+).
fn corner(vertex: u32) -> vec2<f32> {
    let x = f32((0x16u >> vertex) & 1u);
    let y = f32((0x34u >> vertex) & 1u);
    return vec2(x, y) * 2.0 - 1.0;
}

// The weather span of the column under `p`: x bottom, y top, z flags. Columns outside
// the surveyed window hold no weather and read as void; without a cover everything is
// open.
fn cover_at(p: vec2<f32>) -> vec3<f32> {
    if weather.cover.z == 0.0 { return vec3(-3.0e38, 3.0e38, 0.0); }
    let cell = floor(p / weather.cover.x);
    if any(cell < weather.window.xy) || any(cell >= weather.window.zw) {
        return vec3(3.0e38, -3.0e38, f32(VOID));
    }
    let size = i32(weather.cover.y);
    let texel = ((vec2<i32>(cell) % size) + size) % size;
    return textureLoad(cover_map, texel, 0).xyz;
}

fn open_at(p: vec3<f32>) -> bool {
    let span = cover_at(p.xy);
    return p.z >= span.x && p.z <= span.y;
}

// The global fog between the eye and `p`, as the world's GL_EXP2 fog: 1 clear, 0 lost.
fn fog_clear(p: vec3<f32>) -> f32 {
    let ratio = dot(p - camera.position, normalize(camera.forward)) * weather.view.z;
    return exp(-5.541263545 * ratio * ratio);
}

fn wrap(value: vec3<f32>, size: vec3<f32>) -> vec3<f32> {
    return value - size * floor(value / size);
}

fn wrap2(value: vec2<f32>, size: vec2<f32>) -> vec2<f32> {
    return value - size * floor(value / size);
}

struct Particle {
    position: vec3<f32>,
    velocity: vec3<f32>,
    seed: vec3<f32>,
    // Distance and box-edge fades, not yet the cover.
    fade: f32,
};

// Where particle `index` of `cloud` is now: anchored in the world, carried by its mass
// bucket's flow, wrapped into the box around the camera.
fn particle(cloud: Cloud, slot: u32, index: u32, far: bool) -> Particle {
    var result: Particle;
    let salt = slot * 0x85EBCA77u + select(0u, 0xC2B2AE3Du, far);
    result.seed = hash3(index * 0x9E3779B1u + salt);
    let bucket = pcg(index ^ salt ^ 0x27D4EB2Fu) & 7u;
    let scale = select(vec3(1.0), vec3(cloud.layers.x, cloud.layers.x, 1.0), far);
    let size = cloud.box_size.xyz * scale;
    let low = cloud.box_min.xyz * scale;
    let anchor = result.seed * size + cloud.offset[bucket].xyz;
    let local = low + wrap(anchor - (camera.position + low), size);
    result.position = camera.position + local;
    result.velocity = cloud.velocity[bucket].xyz;
    let edge = abs(local - (low + 0.5 * size)) / (0.5 * size);
    let distance = length(local);
    var fade = 1.0 - smoothstep(0.8, 1.0, max(max(edge.x, edge.y), edge.z));
    fade *= smoothstep(16.0, 64.0, distance);
    if far {
        // The far layer starts where the near one thins out.
        let near = 0.5 * min(cloud.box_size.x, cloud.box_size.y);
        fade *= smoothstep(0.6 * near, near, length(local.xy)) * cloud.layers.y;
    }
    result.fade = fade * fog_clear(result.position);
    return result;
}

struct StreakOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    // x across the streak (-1..1), y along it (0 head, 1 tail).
    @location(1) uv: vec2<f32>,
    @location(2) color: vec3<f32>,
    @location(3) opacity: f32,
};

@vertex fn vertex_streak(@builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32) -> StreakOutput {
    var output: StreakOutput;
    output.position = HIDDEN;
    let slot = instance >> 16u;
    let far = ((instance >> 15u) & 1u) != 0u;
    let cloud = weather.clouds[slot];
    let p = particle(cloud, slot, instance & 0x7FFFu, far);
    if p.fade <= 0.002 { return output; }
    let speed = length(p.velocity);
    let direction = select(vec3(0.0, 0.0, -1.0), p.velocity / max(speed, 0.001), speed > 1.0);
    let length_ = cloud.shape.y * (0.8 + 0.4 * p.seed.z);
    let head = p.position;
    let tail = head - direction * length_;
    // Wholly under a roof or the ground, or above the sky: skip the quad.
    let head_span = cover_at(head.xy);
    let tail_span = cover_at(tail.xy);
    if (head.z < head_span.x && tail.z < tail_span.x) || (head.z > head_span.y && tail.z > tail_span.y) {
        return output;
    }
    let c = corner(vertex);
    let along = c.y * 0.5 + 0.5;
    let centre = mix(head, tail, along);
    var side = cross(direction, camera.position - centre);
    if dot(side, side) < 1e-6 { side = cross(direction, vec3(0.0, 0.0, 1.0)); }
    side = normalize(side);
    // Keep a streak at least about a pixel wide; a thinner one gets fainter instead
    // of flickering in and out.
    let a = camera.view_projection * vec4(centre, 1.0);
    let b = camera.view_projection * vec4(centre + side, 1.0);
    if a.w <= 1.0 || b.w <= 1.0 { return output; }
    let pixels = length((b.xy / b.w - a.xy / a.w) * weather.view.xy * 0.5);
    let half_width = max(cloud.shape.x, 0.6 / max(pixels, 0.0001));
    let world = centre + side * c.x * half_width;
    output.position = camera.view_projection * vec4(world, 1.0);
    output.world = world;
    output.uv = vec2(c.x, along);
    // Drops vary: some catch more light than others. x is the streak's opacity, so a
    // thinner or farther streak is fainter rather than whiter; yzw the cloud's colour.
    let catch_light = 0.55 + 0.45 * fract(p.seed.x * 7.31 + p.seed.y * 3.17);
    output.opacity = catch_light * cloud.color.a * p.fade * (cloud.shape.x / half_width);
    output.color = cloud.color.rgb;
    return output;
}

// A falling drop shows a blurred, tinted image of the bright sky around it more than a
// white line: rain is drawn over the scene as a faint streak of the light where the camera
// is, cooled toward the overcast blue-grey, brightest along its core.
const RAIN_TINT: vec3<f32> = vec3(0.80, 0.85, 0.92);
const RAIN_OPACITY: f32 = 0.42;

@fragment fn fragment_streak(input: StreakOutput) -> @location(0) vec4<f32> {
    if !open_at(input.world) { discard; }
    // Soft across, brightest just behind the head, fading along the tail.
    let across = 1.0 - input.uv.x * input.uv.x;
    let along = smoothstep(0.0, 0.08, input.uv.y) * pow(max(1.0 - input.uv.y, 0.0), 1.2);
    let alpha = clamp(input.opacity * across * along * RAIN_OPACITY, 0.0, 1.0);
    // The reference's grey rain (0.5) is neutral; acid rain keeps its green.
    let hue = input.color / max(max(input.color.r, max(input.color.g, input.color.b)), 0.001);
    let core = 1.0 + 0.25 * across * across;
    return vec4(min(weather.light.rgb * RAIN_TINT * hue * core, vec3(1.0)), alpha);
}

struct SpriteOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) layer: i32,
};

@vertex fn vertex_sprite(@builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32) -> SpriteOutput {
    var output: SpriteOutput;
    output.position = HIDDEN;
    let slot = instance >> 16u;
    let cloud = weather.clouds[slot];
    let p = particle(cloud, slot, instance & 0x7FFFu, false);
    if p.fade <= 0.002 || !open_at(p.position) { return output; }
    let forward = normalize(camera.forward);
    var right = cross(forward, vec3(0.0, 0.0, 1.0));
    if dot(right, right) < 1e-4 { right = vec3(0.0, 1.0, 0.0); }
    right = normalize(right);
    let up = cross(right, forward);
    // `mRotation`: turning quads, ±0.7 radians a second.
    let angle = p.seed.x * TAU + weather.cover.w * (p.seed.y - 0.5) * 1.4 * cloud.shape.w;
    let turn = vec2(cos(angle), sin(angle));
    let c = corner(vertex);
    let r = vec2(c.x * turn.x - c.y * turn.y, c.x * turn.y + c.y * turn.x);
    let world = p.position + right * r.x * cloud.shape.x + up * r.y * cloud.shape.y;
    output.position = camera.view_projection * vec4(world, 1.0);
    output.uv = c * 0.5 + 0.5;
    output.layer = i32(cloud.shape.z);
    output.color = vec4(cloud.color.rgb * weather.light.rgb, cloud.color.a * p.fade);
    return output;
}

// The scene's depth gap along the view, in world units (soft_particles.wgsl's).
fn depth_gap(fragment: vec4<f32>) -> f32 {
    let scene = textureLoad(scene_depth, vec2<i32>(fragment.xy), 0);
    if scene >= 1.0 { return 1.0e6; }
    let w = 1.0 / fragment.w;
    let origin = camera.view_projection * vec4(camera.position, 1.0);
    if abs(w - origin.w) < 0.000001 { return 1.0e6; }
    let slope = (fragment.z * w - origin.z) / (w - origin.w);
    let denominator = scene - slope;
    if abs(denominator) < 0.00000001 { return 1.0e6; }
    let behind = (origin.z - slope * origin.w) / denominator;
    let row = vec3(camera.view_projection[0].w, camera.view_projection[1].w,
        camera.view_projection[2].w);
    return (behind - w) / max(length(row), 0.000001);
}

@fragment fn fragment_sprite(input: SpriteOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(images, image_sampler, input.uv, input.layer);
    let soft = clamp(depth_gap(input.position) / 24.0, 0.0, 1.0);
    let alpha = input.color.a * soft;
    if additive {
        return vec4(input.color.rgb * alpha * texel.rgb, 0.0);
    }
    return vec4(input.color.rgb * texel.rgb, alpha * texel.a);
}

// A splash is real geometry, not a sprite: a ring flush with the ground, a crown of
// water as a curved wall of segments that flares out, rises and collapses, and drops
// thrown out on arcs, each part placed in the world so the splash keeps its shape and
// parallax from any angle. On water the ring becomes two ripples under a lower crown.
const SPLASH_PARTS: u32 = 15u;
const CROWN_SEGMENTS: u32 = 8u;
const DROPLETS: u32 = 6u;
const PI: f32 = 3.14159265;

struct SplashOutput {
    @builtin(position) position: vec4<f32>,
    // Ring: the ground plane (-1..1). Crown: across the segment and up (0..1). Drop:
    // across (-1..1) and along, head 0 to tail 1.
    @location(0) uv: vec2<f32>,
    @location(1) opacity: f32,
    // 0 ring on the ground, 1 ripples on water, 2 crown, 3 drop.
    @location(2) @interpolate(flat) kind: u32,
    // x age (0..1), y how edge-on a crown segment is seen, z a random phase.
    @location(3) @interpolate(flat) detail: vec3<f32>,
};

@vertex fn vertex_splash(@builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32) -> SplashOutput {
    var output: SplashOutput;
    output.position = HIDDEN;
    let rain = weather.clouds[u32(weather.light.w)];
    let index = instance & 0x7FFFu;
    let timing = hash3(index * 0x632BE5ABu + 0x1234567u);
    let life = 0.28 + 0.2 * timing.x;
    let clock = weather.cover.w / life + timing.y;
    let age = fract(clock);
    let cycle = u32(i32(floor(clock)));
    let seed = hash3(index * 0x9E3779B1u ^ (cycle * 0x85EBCA77u));
    let radius = 0.5 * min(rain.box_size.x, rain.box_size.y);
    // Anchored in the world as the streaks are: the spot repeats every box width, and
    // the copy inside the box around the camera is the one drawn, so a splash stays
    // where it struck while the camera moves and only wraps at the box's edge.
    let low = camera.position.xy - radius;
    let ground = low + wrap2(seed.xy * (2.0 * radius) - low, vec2(2.0 * radius));
    let span = cover_at(ground);
    let flags = u32(span.z);
    if (flags & SPLASH) == 0u || span.x > span.y || abs(span.x - camera.position.z) > 1200.0 {
        return output;
    }
    let base = vec3(ground, span.x);
    let distance = length(base - camera.position);
    let fade = (1.0 - smoothstep(0.7 * radius, radius, length(ground - camera.position.xy)))
        * smoothstep(24.0, 64.0, distance) * fog_clear(base);
    if fade <= 0.002 { return output; }
    let part = vertex / 6u;
    // Far away a crown is a few pixels and its drops less: only the ring and three drops
    // past 400 units, only the ring past 700.
    if (distance > 700.0 && part != 0u) || (distance > 400.0 && part > CROWN_SEGMENTS + 3u) {
        return output;
    }
    let c = corner(vertex % 6u);
    let water = (flags & LIQUID) != 0u;
    let shape = hash3(index * 0x27D4EB2Fu ^ (cycle * 0x165667B1u));
    let scale = 0.75 + 0.5 * seed.z;
    let spin = shape.x * TAU;
    output.detail = vec3(age, 0.0, shape.y);
    output.opacity = rain.color.a * fade;
    var world: vec3<f32>;
    if part == 0u {
        // Lifted a little more with distance, where depth is coarser.
        let size = select(9.0 * scale, (4.0 + 14.0 * age) * scale, water);
        world = base + vec3(c * size, 0.3 + distance * 0.0015);
        output.uv = c;
        output.kind = select(0u, 1u, water);
    } else if part <= CROWN_SEGMENTS {
        let segment = part - 1u;
        let jag = hash3(index ^ (segment * 0x9E3779B1u) ^ (cycle * 0x7FEB352Du)).x;
        // Out fast and slowing; up and back down before the end.
        let expand = 1.0 - (1.0 - age) * (1.0 - age);
        let reach = (0.8 + 4.5 * expand) * scale * select(1.0, 0.7, water);
        let rise = pow(max(sin(PI * min(age / 0.8, 1.0)), 0.0), 0.7);
        let height = 4.0 * scale * select(1.0, 0.6, water) * rise * (0.75 + 0.45 * jag);
        if height < 0.15 { return output; }
        let across = c.x * 0.5 + 0.5;
        let up = c.y * 0.5 + 0.5;
        let angle = spin + (f32(segment) + across) * TAU / f32(CROWN_SEGMENTS);
        // The wall leans outwards as it rises.
        let out = reach + up * height * 0.45;
        world = base + vec3(vec2(cos(angle), sin(angle)) * out, 0.15 + up * height);
        let middle = spin + (f32(segment) + 0.5) * TAU / f32(CROWN_SEGMENTS);
        let normal = vec3(cos(middle), sin(middle), 0.0);
        output.detail.y = 1.0 - abs(dot(normal, normalize(camera.position - base)));
        output.uv = vec2(across, up);
        output.kind = 2u;
    } else {
        let k = part - 1u - CROWN_SEGMENTS;
        let drop = hash3(index * 0xB5297A4Du ^ (k * 0x68E31DA4u) ^ (cycle * 0x1B56C4E9u));
        // Drops leave once the crown has formed.
        if age < 0.08 { return output; }
        let flight = clamp((age - 0.08) / 0.92, 0.0, 1.0);
        let angle = spin + (f32(k) + 0.6 * drop.x) * TAU / f32(DROPLETS);
        let outward = vec2(cos(angle), sin(angle));
        let travel = (5.0 + 9.0 * drop.y) * scale;
        let peak = (3.0 + 6.0 * drop.z) * scale * select(1.0, 1.4, water);
        let head = base + vec3(outward * (scale + travel * flight),
            0.3 + peak * 4.0 * flight * (1.0 - flight));
        let velocity = vec3(outward * travel, peak * 4.0 * (1.0 - 2.0 * flight));
        let direction = normalize(velocity);
        let along = c.y * 0.5 + 0.5;
        let centre = head - direction * 1.6 * scale * along;
        var side = cross(direction, camera.position - centre);
        if dot(side, side) < 1e-6 { side = vec3(0.0, 0.0, 1.0); }
        side = normalize(side);
        // At least about a pixel wide, fainter instead of thinner, as the rain is.
        let a = camera.view_projection * vec4(centre, 1.0);
        let b = camera.view_projection * vec4(centre + side, 1.0);
        if a.w <= 1.0 || b.w <= 1.0 { return output; }
        let pixels = length((b.xy / b.w - a.xy / a.w) * weather.view.xy * 0.5);
        let width = 0.35 * scale;
        let half_width = max(width, 0.6 / max(pixels, 0.0001));
        output.opacity *= width / half_width;
        world = centre + side * c.x * half_width;
        output.uv = vec2(c.x, along);
        output.kind = 3u;
    }
    output.position = camera.view_projection * vec4(world, 1.0);
    return output;
}

@fragment fn fragment_splash(input: SplashOutput) -> @location(0) vec4<f32> {
    let age = input.detail.x;
    let tint = min(weather.light.rgb * RAIN_TINT, vec3(1.0));
    var bright = 0.0;
    var dark = 0.0;
    switch input.kind {
        case 0u: {
            // The ring where the drop struck, a flash at its centre, and a darker wet spot
            // under it that dries more slowly than the ring fades.
            let d = length(input.uv);
            if d > 1.0 { discard; }
            let spread = 0.12 + 0.55 * (1.0 - (1.0 - age) * (1.0 - age));
            let off = (d - spread) / 0.05;
            let ring = exp(-off * off) * pow(1.0 - age, 1.5) * 0.9;
            let flash = exp(-d * d * 60.0) * pow(1.0 - age, 3.0) * 0.8;
            bright = ring + flash;
            dark = (1.0 - smoothstep(0.0, 0.55, d)) * 0.22 * (1.0 - 0.5 * age);
        }
        case 1u: {
            // Two ripples widening on the water, the second a beat behind.
            let d = length(input.uv);
            if d > 1.0 { discard; }
            let near = (d - (0.15 + 0.8 * age)) / 0.035;
            let far = (d - (0.05 + 0.5 * age)) / 0.035;
            let first = exp(-near * near);
            let second = exp(-far * far) * 0.6
                * smoothstep(0.1, 0.25, age);
            bright = (first + second) * pow(1.0 - age, 1.3) * 1.1;
        }
        case 2u: {
            // A thin sheet of water, thicker at the rim, which breaks into beads; it
            // catches more light seen edge-on, as a film does.
            let up = input.uv.y;
            let sheet = 0.18 + 0.5 * smoothstep(0.55, 1.0, up);
            let rim = smoothstep(0.8, 0.95, up) * (1.0 - smoothstep(0.95, 1.0, up));
            let beads = 0.6 + 0.4 * sin((input.uv.x + input.detail.z) * TAU * 2.0);
            let glint = 0.6 + 0.9 * input.detail.y * input.detail.y;
            bright = (sheet * (1.0 - 0.5 * smoothstep(0.92, 1.0, up)) + rim * beads * 0.8)
                * glint * pow(1.0 - age, 1.2) * 1.15;
        }
        default: {
            // A drop in flight: bright head, short tail.
            let across = 1.0 - input.uv.x * input.uv.x;
            bright = across * mix(1.0, 0.3, input.uv.y) * (1.0 - age * age * age) * 1.1;
        }
    }
    bright *= input.opacity;
    dark *= input.opacity;
    // Blended over the scene: the bright part adds the rain's light, the dark part only
    // darkens what is under it.
    let alpha = clamp(bright + dark, 0.0, 1.0);
    if alpha <= 0.001 { discard; }
    return vec4(tint * min(bright / alpha, 1.0), alpha);
}
// The volumetric fog: rain haze and ground fog over the scene, one full-screen pass
// before the particles. Each pixel marches its ray to the surface it shows and counts
// only open-sky air (the cover): a roof keeps the fog out as it keeps the rain out.
// Ground fog is densest at each column's floor, thins upwards and drifts with the
// wind through the noise volume.
@vertex fn vertex_volume(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2(f32((vertex << 1u) & 2u), f32(vertex & 2u));
    return vec4(corner * 2.0 - 1.0, 0.0, 1.0);
}

// The far cover's span under `p` (x bottom, y top), its edge columns carried on
// beyond it; empty until it is surveyed.
fn far_span(p: vec2<f32>) -> vec2<f32> {
    if weather.far.w == 0.0 { return vec2(3.0e38, -3.0e38); }
    let last = vec2<i32>(textureDimensions(far_cover)) - 1;
    let cell = clamp(vec2<i32>(floor((p - weather.far.xy) / weather.far.z)), vec2(0), last);
    return textureLoad(far_cover, cell, 0).xy;
}

// The cover for the fog: the window's columns, and the far cover beyond the window and
// wherever a column holds none of the map's air (past its walls, or not surveyed yet).
// Neither depends on where the camera is, so distant fog keeps its height as the player
// moves and jumps, and haze carries on past the walls to the horizon.
fn fog_span(p: vec2<f32>) -> vec2<f32> {
    if weather.cover.z == 0.0 { return vec2(-3.0e38, 3.0e38); }
    let column = cover_at(p);
    if (u32(column.z) & VOID) == 0u { return column.xy; }
    return far_span(p);
}

@fragment fn fragment_volume(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let depth = textureLoad(scene_depth, vec2<i32>(pixel.xy), 0);
    let ndc = vec2(pixel.x / weather.view.x * 2.0 - 1.0, 1.0 - pixel.y / weather.view.y * 2.0);
    let far = weather.inverse_view_projection * vec4(ndc, min(depth, 0.999999), 1.0);
    let ray = far.xyz / far.w - camera.position;
    let reach = min(length(ray), weather.fog_flow.w);
    let direction = ray / max(length(ray), 0.001);
    let steps = u32(weather.haze.w);
    let step = reach / f32(max(steps, 1u));
    let jitter = fract(52.9829189 * fract(dot(pixel.xy, vec2(0.06711056, 0.00583715))));
    var optical = 0.0;
    for (var i = 0u; i < steps; i++) {
        let p = camera.position + direction * ((f32(i) + jitter) * step);
        let span = fog_span(p.xy);
        if p.z < span.x - 16.0 || p.z > span.y { continue; }
        var density = weather.haze.x;
        if weather.haze.y > 0.0 {
            let above = max(p.z - span.x, 0.0);
            var billow = 1.0;
            if weather.fog_color.w != 0.0 {
                let q = p + weather.fog_flow.xyz;
                let n = textureSampleLevel(noise, noise_sampler, q * vec3(1.0 / 1400.0, 1.0 / 1400.0, 1.0 / 500.0), 0.0);
                billow = smoothstep(0.3, 0.9, n.r) * 1.4 + 0.25 * n.g;
            }
            density += weather.haze.y * exp(-above / weather.haze.z) * billow;
        }
        optical += density;
    }
    let alpha = 1.0 - exp(-optical * step);
    return vec4(weather.fog_color.rgb, alpha);
}

// Rain-wet surfaces: one full-screen pass over the opaque world before the players are
// drawn, so the depth holds only the world. A surface is wet where the air just in front
// of it is under open sky (the cover, and the far cover beyond the window): it darkens
// as water fills its pores, and its film mirrors the overcast sky, most at grazing
// angles. From level 2 water runs down slopes and walls in streaks; at level 3 flat
// ground gathers puddles. The alpha multiplies the scene and the colour is added
// (blend One, SrcAlpha), so the pass never reads the scene.

// The world point under texel `xy` of the depth, and that depth.
fn wet_point(xy: vec2<i32>, size: vec2<f32>) -> vec4<f32> {
    let depth = textureLoad(scene_depth, xy, 0);
    let uv = (vec2<f32>(xy) + 0.5) / size;
    let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let p = weather.inverse_view_projection * vec4(ndc, depth, 1.0);
    return vec4(p.xyz / p.w, depth);
}

// 1 where `q` lies in a column's open air. A little below the column's floor still
// counts: on a slope, a pixel off the column's centre lies below the floor surveyed there.
fn under_sky(q: vec3<f32>) -> f32 {
    let span = fog_span(q.xy);
    return select(0.0, 1.0, q.z >= span.x - 8.0 && q.z <= span.y);
}

// Running water's streaks: one noise tile is this long (units) and this wide.
const STREAK_LENGTH: f32 = 520.0;
const STREAK_WIDTH: f32 = 40.0;

fn streak_mask(q: vec3<f32>) -> f32 {
    let sample = textureSampleLevel(noise, noise_sampler, q, 0.0);
    return smoothstep(0.8, 0.845, sample.r) * (0.6 + 0.8 * sample.g);
}

// Running water on a wall, 0 to 1: noise stretched along the height and scrolled down
// it, laid on the wall's plane in world coordinates (across y and up z for a wall facing
// x, across x for one facing y, blended between the two on a slanted wall; on a slope the
// height runs downhill). The face's normal, which the depth gives a little unsteadily,
// only chooses the blend, so the streaks stay put on the wall however the camera moves.
fn streaks(p: vec3<f32>, n: vec3<f32>) -> f32 {
    let weights = pow(abs(n.xy), vec2(4.0));
    let blend = weights / max(weights.x + weights.y, 1e-6);
    let along = (p.z + weather.wet.z) / STREAK_LENGTH;
    var water = 0.0;
    if blend.x > 0.01 {
        water += blend.x * streak_mask(vec3(p.y / STREAK_WIDTH, along, p.x / 700.0 + weather.wet.w));
    }
    if blend.y > 0.01 {
        water += blend.y * streak_mask(vec3(p.x / STREAK_WIDTH, along, p.y / 700.0 + weather.wet.w));
    }
    return water;
}

// Rain rings on standing water, 0 to 1: a drop lands in each 16-unit cell now and
// then (about every other second), at a random spot, and its thin ring widens and fades.
fn ripples(xy: vec2<f32>, time: f32) -> f32 {
    let size = 16.0;
    let base = floor(xy / size - 0.5);
    var ring = 0.0;
    for (var k = 0u; k < 4u; k++) {
        let cell = base + vec2(f32(k & 1u), f32(k >> 1u));
        let key = bitcast<u32>(i32(cell.x)) * 0x8DA6B343u ^ bitcast<u32>(i32(cell.y)) * 0xD8163841u;
        let timing = hash3(key);
        let clock = time / (0.5 + 0.4 * timing.z) + timing.x;
        let age = fract(clock);
        let spot = hash3(key ^ (u32(floor(clock)) * 0x9E3779B1u));
        if spot.z > 0.5 { continue; }
        let centre = (cell + 0.2 + 0.6 * spot.xy) * size;
        let off = (length(xy - centre) - (0.5 + 6.5 * age)) / 0.45;
        let fading = 1.0 - age;
        ring += exp(-off * off) * fading * fading * fading;
    }
    return min(ring, 1.0);
}

@fragment fn fragment_wet(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(scene_depth));
    let last = vec2<i32>(size) - 1;
    let xy = vec2<i32>(pixel.xy);
    let centre = wet_point(xy, size);
    if centre.w >= 1.0 { discard; }
    let p = centre.xyz;
    // The face's normal, from the neighbour on each axis whose depth is nearer this
    // pixel's, so a pixel at an edge takes the face it belongs to.
    let left = wet_point(max(xy - vec2(1, 0), vec2(0)), size);
    let right = wet_point(min(xy + vec2(1, 0), last), size);
    let above = wet_point(max(xy - vec2(0, 1), vec2(0)), size);
    let below = wet_point(min(xy + vec2(0, 1), last), size);
    let use_right = xy.x == 0 || (xy.x < last.x && abs(right.w - centre.w) < abs(left.w - centre.w));
    let use_below = xy.y == 0 || (xy.y < last.y && abs(below.w - centre.w) < abs(above.w - centre.w));
    let dx = select(p - left.xyz, right.xyz - p, use_right);
    let dy = select(p - above.xyz, below.xyz - p, use_below);
    var n = cross(dx, dy);
    if dot(n, n) < 1e-12 { discard; }
    n = normalize(n);
    let view = camera.position - p;
    if dot(n, view) < 0.0 { n = -n; }

    // Rain reaches the face where the air in front of it is under open sky, blended
    // between the four nearest columns so a roof's shelter ends in a soft line, not in
    // the columns' steps.
    let lift = p + n * 6.0;
    let cell = lift.xy / weather.cover.x - 0.5;
    let first = floor(cell);
    let blend = cell - first;
    let centre_of = (first + 0.5) * weather.cover.x;
    let step = weather.cover.x;
    let open = mix(
        mix(under_sky(vec3(centre_of, lift.z)), under_sky(vec3(centre_of + vec2(step, 0.0), lift.z)), blend.x),
        mix(under_sky(vec3(centre_of + vec2(0.0, step), lift.z)),
            under_sky(vec3(centre_of + vec2(step, step), lift.z)), blend.x),
        blend.y);
    // Faces the rain slants onto take more of it, the lee side of a wall less.
    let facing = clamp(0.55 + 0.9 * dot(n, -weather.rain.xyz), 0.3, 1.0);
    let wet = weather.wet.x * open * facing;
    if wet <= 0.004 { discard; }

    let level = u32(weather.wet.y);
    let distance = length(view);
    let ground = smoothstep(0.75, 0.97, n.z);
    // Water fills the pores and darkens the surface; its film mirrors the sky, thin and
    // rough on a wall, smoother on the ground.
    var darken = 0.36 * wet;
    var film = wet * mix(0.1, 0.22, ground);
    var glint = 0.0;
    var rings = 0.0;
    let steep = length(n.xy);
    if level >= 2u && n.z > -0.2 {
        // Running water on walls and steep slopes; fine detail, so it fades out with
        // distance and at grazing views, where it would shimmer.
        let fade = smoothstep(0.4, 0.8, steep) * (1.0 - smoothstep(500.0, 1400.0, distance))
            * smoothstep(0.06, 0.25, abs(dot(n, view)) / max(distance, 0.001));
        if fade > 0.0 {
            let rivulet = streaks(p, n) * fade * wet;
            darken += 0.25 * rivulet;
            film += 0.4 * rivulet;
            glint = rivulet;
        }
    }
    if level >= 3u {
        // Standing water on flat ground: darker still, and smooth enough to mirror the sky.
        let patches = textureSampleLevel(noise, noise_sampler, vec3(p.xy / 1100.0, 0.37), 0.0);
        let puddle = smoothstep(0.785, 0.815, patches.r) * smoothstep(0.96, 0.995, n.z) * open
            * weather.wet.x;
        darken = mix(darken, 0.55, puddle);
        film = mix(film, 1.0, puddle);
        // Rings where drops land in it, near enough to see.
        rings = puddle * ripples(p.xy, weather.cover.w) * (1.0 - smoothstep(300.0, 700.0, distance));
    }
    let to_eye = view / max(distance, 0.001);
    let fresnel = 0.02 + 0.98 * pow(1.0 - clamp(dot(n, to_eye), 0.0, 1.0), 5.0);
    // The mirrored ray sees the sky only above the horizon.
    let sky = smoothstep(-0.05, 0.35, reflect(-to_eye, n).z);
    let mirror = clamp(fresnel * film * sky, 0.0, 1.0);
    // Running water and rain rings catch the light even seen from above.
    let added = weather.wet_sky.rgb * (mirror + 0.2 * glint + 0.2 * rings);
    return vec4(added, (1.0 - clamp(darken, 0.0, 0.6)) * (1.0 - mirror));
}
