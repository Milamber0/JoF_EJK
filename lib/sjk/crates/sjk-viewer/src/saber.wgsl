// Saber blade: a smooth glow capsule and a flat hot core, blended GL_ONE GL_ONE into the
// legacy effect layer (display values, per-channel clamp; `effect_layer.rs`).
//
// Stock draws the glow as RB_SurfaceSaberGlow's chain of camera-facing sprites, one every
// 0.65·radius from the tip down, each 0.017 wider, plus a hilt sprite of radius 5.5-5.75
// (rd-vanilla tr_surface.cpp:470-490), and the core as RB_SurfaceLine's flat quad from the
// tip to one unit behind the hilt (tr_surface.cpp:511-566, CG_DoSaber cg_players.c:5359).
// SJK keeps one smooth capsule (owner decision, step 526i) with the same brightness: the
// chain's sum is integrated along the blade instead of summed per sprite, which is its
// continuous limit. For sprites at density 1/(0.65 r) along a blade whose projection is
// `shaft` long, a point at (x, y) receives
//     (L / (0.65 r)) · mean over v in [v_lo, v_hi] of glow(u, v),
//     u = 0.5 + x / 2r, v_hi = 0.5 + y / 2r, v_lo = 0.5 + (y - shaft) / 2r,
// and the mean comes from the glow image's column integral, `glow_integral`.
//
// Blade skins (saber_skins.rs) have their own grey glow/core pair and are coloured and
// animated here from the instance's time and seed and the skin's parameters, which come
// from its blade-skin file (blade_skin_file.rs) in the uniform array `skins`: a hot core
// with a fringe, a corona graded from its rim to the inside, granulation drifting along
// the blade, flame tongues licking outward, shimmer, and flares running from the hilt to
// the tip, swelling the glow; optionally lightning arcs, drifting motes and a turning hue,
// and since 10/10/2026 a sputtering edge and length, glitches, hologram scan lines, a
// heartbeat, falling embers, veins, a team or surroundings tint and the wearer's name in
// glyphs (afterimages are instances of their own); since 11/10/2026 a glint at the tip (an
// instance of its own, a quad round the tip), wisps of smoke rising off the blade, heat
// haze (`fragment_haze`, drawn first into the effect layer, bending the scene behind it)
// and an echo (an afterimage swaying beside the blade). The sections a wearer switched off
// (`off`, a bit each, blade_skin_extras.rs `Section`) are drawn as absent: `skin_at`
// zeroes their lanes, as a file without them has. A skin's blade ends round: past the tip the corona widens, grades from its inside to its
// rim and licks out about the tip, and the core line narrows to a rounded point over its
// last `tip` half-widths (the stock line ends flat, hidden under its glow). Nothing here
// knows any one skin. The dynamic glow pass shades the glow the same way, so its bloom
// follows the flares and arcs.
struct Camera {
    view_projection: mat4x4<f32>,
    position: vec3<f32>,
    _padding: f32,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var glow_texture: texture_2d<f32>;
@group(1) @binding(1)
var core_texture: texture_2d<f32>;
@group(1) @binding(2)
var saber_sampler: sampler;
// Row j holds the glow image's mean of rows 0..j-1 per column; row 0 is zero.
@group(1) @binding(3)
var glow_integral: texture_2d<f32>;
// The core line is sampled as stock samples it: rd-vulkan's box mip chain, trilinear with its
// default 2x anisotropy (saber.rs `Samplers`). At normal distances the line is a few pixels
// wide for 64 texels, and the mip average widens the white core's fringe.
@group(1) @binding(4)
var core_sampler: sampler;

// One loaded skin's parameters (saber_skins.rs `SkinUniform`, blade_skin_file.rs for the
// meaning of each). `grain` is the granulation in [0, 1] and `flare` the flares' sum.
struct Skin {
    // Core: rgb the hot centre's colour, w how much a flare brightens it.
    core_white: vec4<f32>,
    // Fringe colours, cool and hot; w: fringe_heat base and grain.
    core_fringe_cool: vec4<f32>,
    core_fringe_hot: vec4<f32>,
    // Fringe brightness: base, grain, flare; w the rounded tip's length in half-widths.
    core_fringe: vec4<f32>,
    // The core's breathing across: amount, rate, along, seed.
    core_breathe: vec4<f32>,
    // Corona colours, rim cool and hot and the inside; w: inner_width, rim heat by
    // grain and by flare.
    rim_cool: vec4<f32>,
    rim_hot: vec4<f32>,
    inner: vec4<f32>,
    // The inside colour's share: base, grain, flare.
    inner_mix: vec4<f32>,
    // The glow's brightness: base, grain, flare.
    brightness: vec4<f32>,
    // Reach (widest, in stock capsules), swell by grain and by flare.
    swell: vec4<f32>,
    // Granulation octaves: scale, speed, evolve, offset.
    grain_coarse: vec4<f32>,
    grain_fine: vec4<f32>,
    // Octave weights, then the contrast's low and high.
    grain_mix: vec4<f32>,
    // Flares: rate, rate per track, rate per seed, track count.
    flare_rate: vec4<f32>,
    // Phase per seed and per track, the lit threshold, overshoot.
    flare_shape: vec4<f32>,
    // Size and its jitter.
    flare_size: vec4<f32>,
    // Shimmer waves: amount, rate, along, seed.
    shimmer_a: vec4<f32>,
    shimmer_b: vec4<f32>,
    // Tongues: along, offset, out, speed; then edge low and high, low, range.
    tongue_a: vec4<f32>,
    tongue_b: vec4<f32>,
    // Lightning arcs: colour and brightness; width, halo, jag, kinks; count, rate,
    // threshold, decay; reach, span low and high, tip share; jitter, crawl. No arcs: zeros.
    arc_color: vec4<f32>,
    arc_shape: vec4<f32>,
    arc_strike: vec4<f32>,
    arc_place: vec4<f32>,
    arc_motion: vec4<f32>,
    // Motes: colour and brightness; density, cells, rings, size; drift along and out,
    // twinkle, stretch; inner, outer, focus. No motes: zeros.
    mote_color: vec4<f32>,
    mote_field: vec4<f32>,
    mote_motion: vec4<f32>,
    mote_band: vec4<f32>,
    // Hue turning: turns a second, per unit along, per capsule radius out. None: zeros.
    hue: vec4<f32>,
    // The later sections (blade_skin_effects.rs); each zeros when absent.
    // Sputter: ragged, scale, speed; cut, rate, threshold.
    sputter_edge: vec4<f32>,
    sputter_cut: vec4<f32>,
    // Glitch: split, blocks, rate, threshold; shift, flash.
    glitch_a: vec4<f32>,
    glitch_b: vec4<f32>,
    // Scan (hologram): wireframe colour and brightness; lines, speed, depth, rings; edge,
    // width, hollow; jitter and its rate.
    scan_color: vec4<f32>,
    scan_lines: vec4<f32>,
    scan_edge: vec4<f32>,
    scan_jitter: vec4<f32>,
    // Pulse (heartbeat): rate, amount, second, gap; width, swell.
    pulse_a: vec4<f32>,
    pulse_b: vec4<f32>,
    // Embers: colour and brightness; density, cells, fall, life; size, spread.
    ember_color: vec4<f32>,
    ember_field: vec4<f32>,
    ember_shape: vec4<f32>,
    // Veins: colour and brightness; scale, speed, width, core.
    vein_color: vec4<f32>,
    vein_shape: vec4<f32>,
    // Team colours, red (w the amount), blue and none.
    team_red: vec4<f32>,
    team_blue: vec4<f32>,
    team_none: vec4<f32>,
    // The light where it is: amount, saturate, floor.
    ambient: vec4<f32>,
    // Glyphs: colour and brightness; size, spacing, speed, width.
    glyph_color: vec4<f32>,
    glyph_shape: vec4<f32>,
    // The third set (blade_skin_extras.rs); each zeros when absent.
    // Star (the tip's glint): colour and brightness; rays, length, width, halo; spin,
    // twinkle, depth, and the glints circling the blade (0: one at the tip). Their orbit:
    // how far below the tip (units), how far out (units), turns a second, tilt (radians).
    star_color: vec4<f32>,
    star_shape: vec4<f32>,
    star_motion: vec4<f32>,
    star_orbit: vec4<f32>,
    // Black holes in the glints' places: the ring's colour and brightness; the horizon's
    // radius (units), the lens (horizon radii), the swirl's turns a second, the quad's
    // reach (units).
    hole_color: vec4<f32>,
    hole_shape: vec4<f32>,
    // Wisps: colour and brightness; rise, speed, scale, curl; density.
    wisp_color: vec4<f32>,
    wisp_shape: vec4<f32>,
    wisp_field: vec4<f32>,
    // Heat haze: strength (units), scale, speed, reach (units past the glow).
    haze: vec4<f32>,
    // The fifth set (blade_skin_lab.rs); each zeros when absent.
    // Bands: colour and brightness; spacing (units), width (share), twist (bands a radius
    // across), speed (units a second); mix, pinch.
    band_color: vec4<f32>,
    band_shape: vec4<f32>,
    band_mix: vec4<f32>,
    // Segments: count, draws a second, threshold, depth; gap (units), hum.
    segment_a: vec4<f32>,
    segment_b: vec4<f32>,
    // Film: amount, scale, speed, saturation; clear.
    film: vec4<f32>,
    film_b: vec4<f32>,
    // Blobs: colour and brightness; count, size (radii), speed (units a second), wobble;
    // the pools' size.
    blob_color: vec4<f32>,
    blob_shape: vec4<f32>,
    blob_more: vec4<f32>,
    // Front: colour and amount; period (s), duty, hold, reach (shares); soft (units), jag,
    // veins.
    front_color: vec4<f32>,
    front_shape: vec4<f32>,
    front_edge: vec4<f32>,
    // The sixth set (blade_skin_figures.rs); each zeros when absent.
    // Sketch: colour and brightness; rate, wobble, hatch, glow kept.
    sketch_color: vec4<f32>,
    sketch_shape: vec4<f32>,
    // Hourglass: sand colour and brightness; period, waist, glass.
    hour_color: vec4<f32>,
    hour_shape: vec4<f32>,
    // Serpent: colour and brightness; coils, amplitude, speed, thickness.
    serpent_color: vec4<f32>,
    serpent_shape: vec4<f32>,
    // Strands: count, spread, sway, brightness; hue, saturation.
    strand_shape: vec4<f32>,
    strand_hue: vec4<f32>,
    // Butterflies: colour and brightness; count, size, flap, flight; roam.
    fly_color: vec4<f32>,
    fly_shape: vec4<f32>,
    fly_roam: vec4<f32>,
    // Wings: colour and brightness; span, rate, feathers.
    wing_color: vec4<f32>,
    wing_shape: vec4<f32>,
    // Copies: count, angle, fade, period; flicker.
    copy_shape: vec4<f32>,
    copy_more: vec4<f32>,
    // The seventh set (blade_skin_solids.rs), drawn blended after the blades; zeros when
    // absent. Ink: body colour and opacity; outline, width (blade radii), edge (radii),
    // point (units); gloss, halftone (dots a unit).
    ink_color: vec4<f32>,
    ink_shape: vec4<f32>,
    ink_more: vec4<f32>,
    // Chrome: tint and width (blade radii); ripple, beads, speed.
    chrome_color: vec4<f32>,
    chrome_shape: vec4<f32>,
}

@group(2) @binding(0)
var<storage, read> skins: array<Skin, MAX_SKINS>;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    // Across and along the projected blade, world units; along is 0 at the hilt.
    @location(1) blade: vec2<f32>,
    // The projected shaft's length; for a glint or hole, how hidden it is behind the blade.
    @location(2) @interpolate(flat) shaft: f32,
    @location(3) @interpolate(flat) radius: f32,
    @location(4) @interpolate(flat) length: f32,
    // Hilt sprite radius for the glow; zero selects the core line.
    @location(5) @interpolate(flat) hilt: f32,
    // KIND_RETAIL, KIND_NEUTRAL, or KIND_SKIN plus the skin's number.
    @location(6) @interpolate(flat) kind: u32,
    // Presentation seconds (wrapped) and the blade's seed in [0, 1).
    @location(7) @interpolate(flat) animation: vec2<f32>,
    // Who wears it (saber_persona.rs): the light where it is and the afterimage's
    // brightness; the team and the name's letters.
    @location(8) @interpolate(flat) persona: vec4<u32>,
    // The world's down in the projected blade's plane (across, along).
    @location(9) @interpolate(flat) down: vec2<f32>,
    // A chroma skin's turn to its wearer's colour (saber_skins.rs `Chroma`); 0 otherwise.
    @location(10) @interpolate(flat) chroma: f32,
    // The sections its wearer switched off (`OFF_*` bits); 0 otherwise.
    @location(11) @interpolate(flat) off: u32,
}

// TaystJK's CG_DoSaber submits the core RT_LINE twice (taystjk cgame cg_players.c:6429 and 6465;
// the second adds `saber` where `sbak` was meant). The owner's reference look is TaystJK's. Two
// GL_ONE GL_ONE adds with a clamp after each are one add of twice the colour.
const CORE_DRAWS: f32 = 2.0;
// Slot of the engine-generated neutral glow/core pair (saber_rgb.rs), and of the first
// loaded blade skin's (saber_rgb.rs `SKIN_MATERIAL`); skins loaded at once.
const NEUTRAL_MATERIAL: u32 = 6u;
const SKIN_MATERIAL: u32 = 7u;
const MAX_SKINS: u32 = 256u;
const KIND_RETAIL: u32 = 0u;
const KIND_NEUTRAL: u32 = 1u;
const KIND_SKIN: u32 = 2u;
// Colour alpha of a skin's first glint (saber.rs `STAR`), glint n's `STAR - n`; a glow's
// is its hilt sprite's radius, a core's zero.
const STAR: f32 = -1.0;
// How bright a glint shows behind the blade, where its glow covers it.
const GLINT_BEHIND: f32 = 0.3;
// The sections a wearer may switch off, a bit each (blade_skin_extras.rs `Section`).
const OFF_ARCS: u32 = 1u;
const OFF_MOTES: u32 = 2u;
const OFF_HUE: u32 = 4u;
const OFF_SPUTTER: u32 = 8u;
const OFF_GLITCH: u32 = 16u;
const OFF_SCAN: u32 = 32u;
const OFF_PULSE: u32 = 64u;
const OFF_EMBERS: u32 = 256u;
const OFF_VEINS: u32 = 512u;
const OFF_TEAM: u32 = 1024u;
const OFF_AMBIENT: u32 = 2048u;
const OFF_GLYPHS: u32 = 4096u;
const OFF_STAR: u32 = 8192u;
const OFF_WISPS: u32 = 16384u;
const OFF_HAZE: u32 = 32768u;
const OFF_HOLES: u32 = 131072u;
const OFF_BANDS: u32 = 262144u;
const OFF_SEGMENTS: u32 = 524288u;
const OFF_FILM: u32 = 1048576u;
const OFF_BLOBS: u32 = 2097152u;
const OFF_FRONT: u32 = 4194304u;
const OFF_SKETCH: u32 = 8388608u;
const OFF_HOURGLASS: u32 = 16777216u;
const OFF_SERPENT: u32 = 33554432u;
const OFF_STRANDS: u32 = 67108864u;
const OFF_BUTTERFLIES: u32 = 134217728u;
const OFF_WINGS: u32 = 268435456u;
const OFF_COPIES: u32 = 536870912u;
const OFF_INK: u32 = 1073741824u;
const OFF_CHROME: u32 = 2147483648u;
// RB_SurfaceSaberGlow: sprite spacing and per-sprite growth.
const SPACING: f32 = 0.65;
const GROWTH: f32 = 0.017;

const QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2(-1.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    vec2(-1.0, 0.0), vec2(1.0, 1.0), vec2(-1.0, 1.0),
);

// Sprite radius at fraction `along` of the blade (0 hilt, 1 tip): the chain starts at the
// tip with the drawn radius and every sprite toward the hilt is GROWTH wider.
fn chain_radius(radius: f32, length: f32, along: f32) -> f32 {
    return radius + GROWTH * (1.0 - along) * length / (SPACING * radius);
}

// One blade instance, as the vertex buffer holds it (saber.rs `Instance`).
struct BladeInstance {
    @location(0) base: vec3<f32>,
    @location(1) length: f32,
    @location(2) direction: vec3<f32>,
    @location(3) radius: f32,
    @location(4) color: vec4<f32>,
    @location(5) material: u32,
    @location(6) animation: vec2<f32>,
    @location(7) persona: vec4<u32>,
    @location(8) chroma: f32,
    @location(9) off: u32,
}

// How far (units) a skin's glow quad reaches past the glow for the sections its wearer
// left on: the wisps' rise, the haze's reach, the butterflies' flights and the wings'
// span (blade_skin_extras.rs `room`).
fn skin_room(skin: Skin, off: u32) -> f32 {
    var room = 0.0;
    if (off & OFF_WISPS) == 0u && skin.wisp_color.w > 0.0 { room = skin.wisp_shape.x; }
    if (off & OFF_HAZE) == 0u && skin.haze.x > 0.0 { room = max(room, skin.haze.w); }
    if (off & OFF_BUTTERFLIES) == 0u && skin.fly_shape.x >= 1.0 {
        room = max(room, skin.fly_roam.x + skin.fly_shape.y);
    }
    if (off & OFF_WINGS) == 0u && skin.wing_shape.x > 0.0 { room = max(room, skin.wing_shape.x); }
    return min(room, 16.0);
}

@vertex
fn vertex_main(@builtin(vertex_index) vertex_index: u32, blade: BladeInstance) -> VertexOutput {
    return blade_vertex(vertex_index, blade);
}

// The haze's quads: a skin's glow quad when its haze is on, for the blade itself (not an
// afterimage); every other instance is folded away.
@vertex
fn vertex_haze(@builtin(vertex_index) vertex_index: u32, blade: BladeInstance) -> VertexOutput {
    var output = blade_vertex(vertex_index, blade);
    var hazy = output.kind >= KIND_SKIN && output.hilt > 0.0 && (blade.persona.x >> 24u) == 255u
        && (blade.off & OFF_HAZE) == 0u;
    if hazy {
        hazy = skins[skin_index(output)].haze.x > 0.0;
    }
    if !hazy {
        output.clip_position = vec4(0.0, 0.0, 0.0, 1.0);
    }
    return output;
}

fn blade_vertex(vertex_index: u32, blade: BladeInstance) -> VertexOutput {
    let blade_base = blade.base;
    let blade_length = blade.length;
    let blade_direction = blade.direction;
    let blade_radius = blade.radius;
    let blade_color = blade.color;
    let blade_material = blade.material;
    let blade_animation = blade.animation;
    let blade_persona = blade.persona;
    let blade_chroma = blade.chroma;
    let direction = normalize(blade_direction);
    let view_direction = normalize(camera.position - (blade_base + direction * blade_length * 0.5));
    var side = cross(direction, view_direction);
    let projected = length(side);
    if projected < 0.001 {
        // Pick an axis that cannot be parallel, including a blade aimed along Z.
        let axis = select(vec3(0.0,0.0,1.0),vec3(0.0,1.0,0.0),abs(view_direction.z) > 0.9);
        side = cross(axis,view_direction);
    }
    side = normalize(side);
    let up = normalize(cross(view_direction,side));
    let local = QUAD[vertex_index];
    let hilt = blade_color.a;
    var output: VertexOutput;
    output.kind = KIND_RETAIL;
    if blade_material == NEUTRAL_MATERIAL { output.kind = KIND_NEUTRAL; }
    var reach = 1.0;
    var room = 0.0;
    var star = 0.0;
    var glints = 0.0;
    var orbit = vec4(0.0);
    output.off = 0u;
    if blade_material >= SKIN_MATERIAL && blade_material < SKIN_MATERIAL + MAX_SKINS {
        output.kind = KIND_SKIN + (blade_material - SKIN_MATERIAL);
        output.off = blade.off;
        let skin = skins[blade_material - SKIN_MATERIAL];
        // A skin's glow reaches further, for its flares and shimmer, and its quad further
        // still for its wisps and haze.
        reach = skin.swell.x;
        room = skin_room(skin, blade.off);
        star = skin.star_shape.y;
        if skin.hole_shape.x > 0.0 { star = skin.hole_shape.w; }
        glints = skin.star_motion.w;
        orbit = skin.star_orbit;
    }
    output.animation = blade_animation;
    output.persona = blade_persona;
    output.chroma = blade_chroma;
    output.down = vec2(-side.z, -up.z);
    var world: vec3<f32>;
    if hilt <= STAR {
        // A skin's glint (or black hole): a square facing the camera, as wide as the rays
        // are long (the hole's reach), round the tip, or round its place on the orbit
        // below it; `blade` is the place on it from its heart, `shaft` how hidden it is.
        let n = STAR - hilt;
        let size = max(star, 0.001) * glint_scale(n);
        var heart = blade_base + direction * blade_length;
        output.shaft = 0.0;
        if glints > 0.0 {
            // Round the blade's axis, measured from the camera's side of it (the near side
            // at a quarter turn), so the circle reads as a ring however the blade is seen;
            // the tilt lifts the near side and drops the far one.
            let toward = normalize(cross(side, direction));
            let angle = TAU * (blade_animation.x * orbit.z + blade_animation.y + n / glints);
            let across = cos(angle);
            let near = sin(angle);
            heart += direction * (orbit.y * tan(orbit.w) * near - min(orbit.x, blade_length))
                + (side * across + toward * near) * orbit.y;
            // Behind the blade, and close enough to its axis for the glow to cover it.
            let behind = smoothstep(0.1, -0.5, near);
            let covered = 1.0 - smoothstep(0.6, 1.6, abs(across) * orbit.y / max(blade_radius, 0.001));
            output.shaft = behind * covered;
        }
        let corner = vec2(local.x, local.y * 2.0 - 1.0);
        world = heart + (side * corner.x + up * corner.y) * size;
        output.blade = corner * size;
    } else if hilt > 0.0 {
        // Wide enough for the grown hilt-end sprites and the hilt sprite itself, and the
        // skin's room.
        let extent = max(chain_radius(blade_radius, blade_length, 0.0) * reach, hilt) + room;
        let shaft = blade_length * projected;
        world = blade_base + direction * (local.y * blade_length)
            + (side * local.x + up * (local.y * 2.0 - 1.0)) * extent;
        output.blade = vec2(local.x * extent, local.y * (shaft + 2.0 * extent) - extent);
        output.shaft = shaft;
    } else {
        // RB_SurfaceLine: flat, from one unit behind the hilt to the tip.
        world = blade_base + direction * mix(-1.0, blade_length, local.y)
            + side * (local.x * blade_radius);
        output.blade = vec2(local.x * blade_radius, local.y);
        output.shaft = 0.0;
    }
    output.clip_position = camera.view_projection * vec4(world, 1.0);
    output.color = blade_color.rgb;
    output.radius = blade_radius;
    output.length = blade_length;
    output.hilt = hilt;
    return output;
}

fn glow(u: f32, v: f32) -> vec3<f32> {
    if u < 0.0 || u > 1.0 || v < 0.0 || v > 1.0 { return vec3(0.0); }
    return textureSampleLevel(glow_texture, saber_sampler, vec2(u, v), 0.0).rgb;
}

fn integral(u: f32, v: f32) -> vec3<f32> {
    let rows = f32(textureDimensions(glow_integral).y) - 1.0;
    let y = (clamp(v, 0.0, 1.0) * rows + 0.5) / (rows + 1.0);
    return textureSampleLevel(glow_integral, saber_sampler, vec2(u, y), 0.0).rgb;
}

fn glow_capsule(input: VertexOutput) -> vec3<f32> {
    let x = input.blade.x;
    let y = input.blade.y;
    let r = chain_radius(input.radius, input.length,
        clamp(y / max(input.shaft, 0.0001), 0.0, 1.0));
    var sum = vec3(0.0);
    let u = 0.5 + x / (2.0 * r);
    if u >= 0.0 && u <= 1.0 {
        let high = 0.5 + y / (2.0 * r);
        let low = high - input.shaft / (2.0 * r);
        let count = input.length / (SPACING * r);
        if high - low < 1.0 / 64.0 {
            // Seen almost end-on the sprites stack: `count` copies of one texel.
            sum = count * glow(u, 0.5 * (high + low));
        } else {
            sum = count * (integral(u, high) - integral(u, low)) / (high - low);
        }
    }
    // The hilt sprite, centred on the blade's base.
    let hilt = 0.5 + vec2(x, y) / (2.0 * input.hilt);
    return sum + glow(hilt.x, hilt.y);
}

// --- Blade skins ---------------------------------------------------------------------------

const PI: f32 = 3.14159265;
const TAU: f32 = 6.2831853;
// Most lightning arcs a skin may have (blade_skin_file.rs `MAX_ARCS`).
const MAX_ARCS: i32 = 4;

// PCG hash of a lattice point, in [0, 1].
fn skin_hash(cell: vec2<i32>) -> f32 {
    var v = bitcast<u32>(cell.x) * 747796405u + 2891336453u;
    v = v ^ (bitcast<u32>(cell.y) * 2654435761u);
    v = v * 747796405u + 2891336453u;
    let word = ((v >> ((v >> 28u) + 4u)) ^ v) * 277803737u;
    return f32((word >> 22u) ^ word) / 4294967295.0;
}

// Smooth value noise in [0, 1].
fn skin_noise(p: vec2<f32>) -> f32 {
    let cell = vec2<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = skin_hash(cell);
    let b = skin_hash(cell + vec2(1, 0));
    let c = skin_hash(cell + vec2(0, 1));
    let d = skin_hash(cell + vec2(1, 1));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Piecewise-linear noise in [-1, 1]: straight runs between random corners, one a unit of
// `k`, on lattice row `row`.
fn skin_zigzag(k: f32, row: i32) -> f32 {
    let i = floor(k);
    let a = skin_hash(vec2(i32(i), row));
    let b = skin_hash(vec2(i32(i) + 1, row));
    return mix(a, b, k - i) * 2.0 - 1.0;
}

// The skin's number, below MAX_SKINS.
fn skin_index(input: VertexOutput) -> u32 {
    return min(input.kind - KIND_SKIN, MAX_SKINS - 1u);
}

// The skin this instance wears, with the sections its wearer switched off zeroed: drawn as
// absent, as a file without them is (every section draws nothing when its lanes are zeros).
fn skin_at(input: VertexOutput) -> Skin {
    var skin = skins[skin_index(input)];
    let off = input.off;
    if off == 0u {
        return skin;
    }
    if (off & OFF_ARCS) != 0u { skin.arc_strike = vec4(0.0); }
    if (off & OFF_MOTES) != 0u { skin.mote_field = vec4(0.0); }
    if (off & OFF_HUE) != 0u { skin.hue = vec4(0.0); }
    if (off & OFF_SPUTTER) != 0u {
        skin.sputter_edge = vec4(0.0);
        skin.sputter_cut = vec4(0.0);
    }
    if (off & OFF_GLITCH) != 0u { skin.glitch_a = vec4(0.0); }
    if (off & OFF_SCAN) != 0u {
        skin.scan_lines = vec4(0.0);
        skin.scan_jitter = vec4(0.0);
    }
    if (off & OFF_PULSE) != 0u { skin.pulse_a = vec4(0.0); }
    if (off & OFF_EMBERS) != 0u { skin.ember_field = vec4(0.0); }
    if (off & OFF_VEINS) != 0u {
        skin.vein_color = vec4(0.0);
        skin.vein_shape = vec4(0.0);
    }
    if (off & OFF_TEAM) != 0u { skin.team_red = vec4(0.0); }
    if (off & OFF_AMBIENT) != 0u { skin.ambient = vec4(0.0); }
    if (off & OFF_GLYPHS) != 0u {
        skin.glyph_color = vec4(0.0);
        skin.glyph_shape = vec4(0.0);
    }
    if (off & OFF_STAR) != 0u { skin.star_color = vec4(0.0); }
    if (off & OFF_WISPS) != 0u { skin.wisp_color = vec4(0.0); }
    if (off & OFF_HAZE) != 0u { skin.haze = vec4(0.0); }
    if (off & OFF_HOLES) != 0u {
        skin.hole_color = vec4(0.0);
        skin.hole_shape = vec4(0.0);
    }
    if (off & OFF_BANDS) != 0u { skin.band_shape = vec4(0.0); }
    if (off & OFF_SEGMENTS) != 0u { skin.segment_a = vec4(0.0); }
    if (off & OFF_FILM) != 0u { skin.film = vec4(0.0); }
    if (off & OFF_BLOBS) != 0u { skin.blob_shape = vec4(0.0); }
    if (off & OFF_FRONT) != 0u { skin.front_shape = vec4(0.0); }
    if (off & OFF_SKETCH) != 0u { skin.sketch_shape = vec4(0.0); }
    if (off & OFF_HOURGLASS) != 0u { skin.hour_shape = vec4(0.0); }
    if (off & OFF_SERPENT) != 0u { skin.serpent_shape = vec4(0.0); }
    if (off & OFF_STRANDS) != 0u { skin.strand_shape = vec4(0.0); }
    if (off & OFF_BUTTERFLIES) != 0u { skin.fly_shape = vec4(0.0); }
    if (off & OFF_WINGS) != 0u { skin.wing_shape = vec4(0.0); }
    if (off & OFF_COPIES) != 0u { skin.copy_shape = vec4(0.0); }
    if (off & OFF_INK) != 0u {
        skin.ink_color = vec4(0.0);
        skin.ink_shape = vec4(0.0);
        skin.ink_more = vec4(0.0);
    }
    if (off & OFF_CHROME) != 0u { skin.chrome_color = vec4(0.0); }
    return skin;
}

// How far out the glow's own parts may reach (embers, arcs and motes fade out before it):
// the grown chain's widest at the reach, or the hilt sprite. The quad reaches further by
// the skin's room (vertex_main's `extent`), for its wisps and haze only, so switching
// those off changes nothing else.
fn skin_extent(skin: Skin, input: VertexOutput) -> f32 {
    return max(chain_radius(input.radius, input.length, 0.0) * skin.swell.x, input.hilt);
}

// Granulation at `along` world units from the hilt: two octaves drifting toward the tip and
// slowly changing as they go.
fn skin_granulation(skin: Skin, along: f32, time: f32, seed: f32) -> f32 {
    let c = skin.grain_coarse;
    let f = skin.grain_fine;
    let coarse = skin_noise(vec2(along * c.x - time * c.y, time * c.z + seed * c.w));
    let fine = skin_noise(vec2(along * f.x - time * f.y, time * f.z + seed * f.w));
    let mixed = skin.grain_mix;
    return smoothstep(mixed.z, mixed.w, coarse * mixed.x + fine * mixed.y);
}

// Flares at `along`: tracks each sending a bright knot from the hilt to the tip once a cycle,
// lit on the cycles whose draw reaches the threshold, so they come now and then and never in
// step.
fn skin_flares(skin: Skin, along: f32, length: f32, time: f32, seed: f32) -> f32 {
    let rates = skin.flare_rate;
    let shape = skin.flare_shape;
    let size = skin.flare_size;
    var total = 0.0;
    for (var track = 0; track < i32(rates.w); track++) {
        let rate = rates.x + rates.y * f32(track) + rates.z * seed;
        let phase = time * rate + seed * shape.x + f32(track) * shape.y;
        let cycle = floor(phase);
        let travel = fract(phase);
        let lit = step(shape.z, skin_hash(vec2(i32(cycle), track * 7919 + i32(seed * 4096.0))));
        let position = travel * (length + 2.0 * shape.w) - shape.w;
        let width = size.x + size.y * skin_hash(vec2(i32(cycle), track + 31));
        let offset = (along - position) / width;
        total += lit * sin(PI * travel) * exp(-offset * offset);
    }
    return total;
}

// One wave of shimmer or breathing: amount · sin(time · rate + along · along + seed · seed).
fn skin_wave(wave: vec4<f32>, time: f32, along: f32, seed: f32) -> f32 {
    return wave.x * sin(time * wave.y + along * wave.z + seed * wave.w);
}

// Along the blade from the hilt, world units: the glow's projected `y`, or the core line's
// texture `v` (one unit behind the hilt to the tip).
fn skin_along(input: VertexOutput) -> f32 {
    if input.hilt > 0.0 {
        return clamp(input.blade.y / max(input.shaft, 0.0001), 0.0, 1.0) * input.length;
    }
    return mix(-1.0, input.length, input.blade.y);
}

// How much of the core line's width is left `along` units from the hilt: all of it until the
// last `cap` units, then a quarter circle down to nothing at the tip, so the line ends in a
// rounded point instead of a square.
fn skin_tip_taper(along: f32, length: f32, cap: f32) -> f32 {
    let into = clamp((along - (length - cap)) / max(cap, 0.0001), 0.0, 1.0);
    // Exactly 1 along the shaft, whatever the GPU's square root.
    return select(sqrt(max(1.0 - into * into, 0.0001)), 1.0, into <= 0.0);
}

// Lightning arcs at `along` (world units from the hilt, running on past the tip) and `x`
// (signed across): `count` arcs, each struck anew `rate` times a second at a random place
// (strikes whose draw is under `threshold` stay dark), a jagged filament leaving the blade,
// bulging out to one side up to `reach` capsule radii (`r`) and coming back a `span` of the
// blade further on, crawling `crawl` units a second; a share `tip` of the strikes leap from
// just below the tip into the air (up to `room` units past it) and die out there. A strike
// flashes and fades by `decay`, re-shaped `jitter` times a second (the crackle). A filament
// thinner than a pixel widens to it and dims, so it does not break up at a distance.
fn skin_arcs(
    skin: Skin, along: f32, x: f32, r: f32, length: f32, room: f32, pixel: f32, time: f32,
    seed: f32,
) -> f32 {
    let strike = skin.arc_strike;
    let place = skin.arc_place;
    let shape = skin.arc_shape;
    let motion = skin.arc_motion;
    let seeded = i32(seed * 4096.0);
    let width = max(shape.x, pixel);
    var total = 0.0;
    for (var arc = 0; arc < min(i32(strike.x), MAX_ARCS); arc++) {
        let phase = time * strike.y + seed * 3.1 + f32(arc) * 0.618;
        let cycle = i32(floor(phase));
        let age = fract(phase);
        let row = arc * 7919 + seeded;
        if skin_hash(vec2(cycle, row)) < strike.z {
            continue;
        }
        let pick = skin_hash(vec2(cycle, row + 101));
        let side = select(-1.0, 1.0, skin_hash(vec2(cycle, row + 303)) < 0.5);
        var t: f32;
        var bulge: f32;
        var fade = 1.0;
        if skin_hash(vec2(cycle, row + 404)) < place.w {
            // From just below the tip into the air past it, bending away and dying out.
            let span = (0.8 + 0.6 * pick) * room;
            t = (along - (length - 0.35 * span)) / span;
            bulge = sin(0.5 * PI * clamp(t, 0.0, 1.0));
            fade = 1.0 - clamp(t, 0.0, 1.0);
        } else {
            let span = mix(place.y, place.z, pick) * length;
            let start = skin_hash(vec2(cycle, row + 202)) * (length - span)
                + motion.y * age / max(strike.y, 0.0001);
            t = (along - start) / span;
            bulge = sin(PI * clamp(t, 0.0, 1.0));
        }
        if t < 0.0 || t > 1.0 {
            continue;
        }
        let jagged = i32(floor(time * motion.x + f32(arc) * 0.37)) * 13 + arc * 131 + seeded;
        let k = along * shape.w + f32(arc) * 17.0;
        let jag = skin_zigzag(k, jagged) + 0.45 * skin_zigzag(k * 2.7 + 5.0, jagged + 57);
        let offset = side * bulge * place.x * r + jag * shape.z * r * sqrt(bulge);
        let d = (x - offset) / width;
        let filament = shape.x / width * exp(-d * d) + shape.y * 0.35 * exp(-d * d / 16.0);
        total += pow(1.0 - age, strike.w) * fade * filament;
    }
    return total;
}

// Motes at `around` (world units along the blade and on round its tip) and `out` (capsule
// radii from the blade's axis, or from the tip past it), on the `left` or right: a field of
// specks in cells, `cells` a unit along by `rings` a radius out, a share `density` of them
// holding one, `size` of its cell across and `stretch` times that along. The field drifts
// `drift_along` units a second toward the tip and `drift_out` radii a second outward
// (negative: inward); each speck twinkles about `twinkle` times a second; they show between
// `inner` and `outer` radii out, by `focus` only toward the tip, and not behind the hilt
// (`along`, unclamped).
fn skin_motes(
    skin: Skin, around: f32, out: f32, left: bool, along: f32, length: f32, time: f32,
    seed: f32,
) -> f32 {
    let field = skin.mote_field;
    let motion = skin.mote_motion;
    let band = skin.mote_band;
    let p_along = (around - time * motion.x) * field.y;
    let column = i32(floor(p_along));
    // Each column of cells is staggered outward by its own amount, so they do not line up.
    let p_out = (out - time * motion.y) * field.z + skin_hash(vec2(column, 4242));
    let cell = vec2(column, i32(floor(p_out)) + i32(seed * 4096.0) * 3 + select(0, 7777, left));
    if skin_hash(cell) >= field.x {
        return 0.0;
    }
    let half = vec2(field.w * motion.w, field.w);
    let centre = half + (1.0 - 2.0 * half)
        * vec2(skin_hash(cell + vec2(0, 911)), skin_hash(cell + vec2(37, 0)));
    let d = (vec2(fract(p_along), fract(p_out)) - centre) / half;
    let falloff = max(1.0 - dot(d, d), 0.0);
    var twinkle = 1.0;
    if motion.z > 0.0 {
        let rate = motion.z * (0.5 + skin_hash(cell + vec2(3, 5)));
        twinkle = 0.5 + 0.5 * sin(TAU * (time * rate + skin_hash(cell + vec2(7, 2))));
    }
    let span = band.y - band.x;
    let shown = smoothstep(band.x, band.x + 0.25 * span, out)
        * (1.0 - smoothstep(band.y - 0.4 * span, band.y, out));
    let toward_tip = mix(1.0, smoothstep(0.6 * length, length, around), band.z);
    return falloff * falloff * twinkle * shown * toward_tip * smoothstep(0.0, 2.0, along);
}

// `color` turned `turns` round the grey axis (a hue shift keeping its brightness and
// saturation), negative channels clipped: the blend adds.
fn skin_turn_hue(color: vec3<f32>, turns: f32) -> vec3<f32> {
    let axis = vec3(0.57735027);
    let c = cos(TAU * turns);
    let s = sin(TAU * turns);
    let turned = color * c + cross(axis, color) * s + axis * dot(axis, color) * (1.0 - c);
    return max(turned, vec3(0.0));
}

// --- Later effects (blade_skin_effects.rs): each draws nothing when its lanes are zeros ---

// Where the first heartbeat peaks in a cycle, seconds (saber_skins.rs `Beat::FIRST`).
const BEAT_FIRST: f32 = 0.1;

// The afterimage's brightness from the instance (1 for a blade itself).
fn persona_fade(input: VertexOutput) -> f32 {
    return f32(input.persona.x >> 24u) / 255.0;
}

// The light where the blade is, at full brightness (saber_persona.rs `pack_light`).
fn persona_light(input: VertexOutput) -> vec3<f32> {
    let packed = input.persona.x;
    return vec3(f32(packed & 255u), f32((packed >> 8u) & 255u), f32((packed >> 16u) & 255u))
        / 255.0;
}

// The wearer's letter `index` (5 bits; 0 ends the name).
fn persona_letter(input: VertexOutput, index: u32) -> u32 {
    let lanes = vec3(input.persona.y & 0x3fffffffu, input.persona.z, input.persona.w);
    return (lanes[index / 6u] >> (5u * (index % 6u))) & 31u;
}

// The heartbeat at `time`: two Gaussian beats a cycle, on the clock alone so the blade
// and its light (saber_skins.rs `Beat::at`) beat together.
fn skin_beat(skin: Skin, time: f32) -> f32 {
    let p = skin.pulse_a;
    if p.x <= 0.0 {
        return 0.0;
    }
    let into = fract(time * p.x) / p.x;
    let width = max(skin.pulse_b.x, 0.001);
    let a = (into - BEAT_FIRST) / width;
    let b = (into - BEAT_FIRST - p.w) / width;
    return p.y * (exp(-a * a) + p.z * exp(-b * b));
}

// How long the blade shows this moment: all of it, or, on a sputtering draw, cut short.
fn skin_cut(skin: Skin, length: f32, time: f32, seed: f32) -> f32 {
    let c = skin.sputter_cut;
    if c.x <= 0.0 || c.y <= 0.0 {
        return length;
    }
    let cycle = i32(floor(time * c.y));
    let seeded = i32(seed * 4096.0);
    if skin_hash(vec2(cycle, 5151 + seeded)) < c.z {
        return length;
    }
    return length * (1.0 - c.x * skin_hash(vec2(cycle, 6161 + seeded)));
}

// Glitch at `along` units: x the jump (capsule radii), y the colour split (radii), z the
// flash. A block glitches on its own draws; now and then the whole blade does.
fn skin_glitch(skin: Skin, along: f32, time: f32, seed: f32) -> vec3<f32> {
    let g = skin.glitch_a;
    if g.y <= 0.0 {
        return vec3(0.0);
    }
    let frame = i32(floor(time * g.z));
    let seeded = i32(seed * 4096.0);
    let block = i32(floor(along / g.y));
    let whole = skin_hash(vec2(frame, 919 + seeded)) >= 0.5 + 0.5 * g.w;
    let draw = skin_hash(vec2(block * 31 + seeded, frame));
    if !whole && draw < g.w {
        return vec3(0.0, g.x, 0.0);
    }
    let jump = skin.glitch_b.x * (skin_hash(vec2(block + 77, frame * 3 + seeded)) * 2.0 - 1.0);
    return vec3(select(jump, jump * 0.5, whole), 2.0 * g.x, skin.glitch_b.y);
}

// The hologram's jitter: the whole projection thrown sideways (capsule radii) on some draws.
fn skin_jitter(skin: Skin, time: f32, seed: f32) -> f32 {
    let j = skin.scan_jitter;
    if j.x <= 0.0 || j.y <= 0.0 {
        return 0.0;
    }
    let frame = i32(floor(time * j.y));
    let seeded = i32(seed * 4096.0);
    if skin_hash(vec2(frame, 4321 + seeded)) < 0.75 {
        return 0.0;
    }
    return j.x * (skin_hash(vec2(frame, 8765 + seeded)) * 2.0 - 1.0);
}

// Veins: ridged noise along and across (`x` units from the axis), bright where it crosses
// its middle, a network of cracks running along the blade.
fn skin_veins(skin: Skin, along: f32, x: f32, time: f32, seed: f32) -> f32 {
    let v = skin.vein_shape;
    if v.x <= 0.0 {
        return 0.0;
    }
    let p = vec2(along * v.x - time * v.y + seed * 31.0, x * v.x * 1.7 + seed * 17.0);
    let n = 0.65 * skin_noise(p) + 0.35 * skin_noise(p * 2.3 + vec2(7.0, 3.0));
    return 1.0 - smoothstep(0.5 * v.z, v.z, abs(n - 0.5));
}

// The colour a skin takes from its wearer's team or the light where it is: rgb (largest
// channel 1) and how much; none, zero.
fn skin_tint(skin: Skin, input: VertexOutput) -> vec4<f32> {
    if skin.team_red.w > 0.0 {
        let team = input.persona.y >> 30u;
        var color = skin.team_none.rgb;
        if team == 1u { color = skin.team_red.rgb; }
        if team == 2u { color = skin.team_blue.rgb; }
        return vec4(color, skin.team_red.w);
    }
    let a = skin.ambient;
    if a.x > 0.0 {
        let light = persona_light(input);
        let grey = vec3(dot(light, vec3(0.299, 0.587, 0.114)));
        let color = max(mix(grey, light, a.y), vec3(a.z));
        return vec4(color / max(max(color.r, color.g), max(color.b, 0.0001)), a.x);
    }
    return vec4(0.0);
}

// `color` recoloured toward `tint` at its own strongest channel's level.
fn skin_apply_tint(color: vec3<f32>, tint: vec4<f32>) -> vec3<f32> {
    if tint.w <= 0.0 {
        return color;
    }
    let level = max(color.r, max(color.g, color.b));
    return mix(color, level * tint.rgb, tint.w);
}

// Embers dripping from the blade and falling, at `p` (across, along the projected blade,
// units) with `down` the world's down in that plane and `scale` projected units a world
// unit along. Each drip place `cells` a unit along drips (if its draw is under `density`)
// one ember a `life`, accelerating `fall` units a second; where gravity runs along the
// blade, they are pushed off to either side. A fragment reads three places a side.
fn skin_embers(
    skin: Skin, p: vec2<f32>, down: vec2<f32>, scale: f32, blade_length: f32, time: f32,
    seed: f32, pixel: f32,
) -> f32 {
    let f = skin.ember_field;
    let shape = skin.ember_shape;
    let seeded = i32(seed * 4096.0);
    var total = 0.0;
    for (var s = 0; s < 2; s++) {
        let side = select(-1.0, 1.0, s == 1);
        var d = down;
        if abs(d.x) < 0.35 {
            d.x = side * 0.35;
        } else if sign(d.x) != side {
            continue;
        }
        d = normalize(d);
        if p.x * side < 0.0 {
            continue;
        }
        let travel_here = p.x / d.x;
        let start = (p.y - travel_here * d.y) / max(scale, 0.0001);
        let k = i32(floor(start * f.y));
        for (var dk = -1; dk <= 1; dk++) {
            let column = k + dk;
            let row = column * 2 + s + seeded * 5;
            if skin_hash(vec2(row, 1717)) >= f.x {
                continue;
            }
            let origin = (f32(column) + skin_hash(vec2(row, 2727))) / f.y;
            if origin < 0.5 || origin > blade_length {
                continue;
            }
            let phase = fract(time / f.w + skin_hash(vec2(row, 3737)));
            let travel = f.z * f.w * phase * phase;
            let stray = shape.y * (skin_hash(vec2(row, 4747)) * 2.0 - 1.0);
            let q = vec2(0.0, origin * scale) + d * travel + vec2(-d.y, d.x) * stray * travel * 0.3;
            let size = shape.x * (1.0 - 0.5 * phase);
            let width = max(size, pixel);
            let e = length(p - q) / width;
            total += exp(-e * e) * pow(1.0 - phase, 1.5) * size / width;
        }
    }
    return total;
}

// SJK's stroke glyphs: sixteen strokes on a 3 × 3 grid (six across, six up, four
// diagonals from the middle), each letter a fixed handful of them drawn from its code.
const GLYPH_STROKES: array<vec4<f32>, 16> = array<vec4<f32>, 16>(
    vec4(0.0, 0.0, 1.0, 0.0), vec4(1.0, 0.0, 2.0, 0.0),
    vec4(0.0, 1.0, 1.0, 1.0), vec4(1.0, 1.0, 2.0, 1.0),
    vec4(0.0, 2.0, 1.0, 2.0), vec4(1.0, 2.0, 2.0, 2.0),
    vec4(0.0, 0.0, 0.0, 1.0), vec4(0.0, 1.0, 0.0, 2.0),
    vec4(1.0, 0.0, 1.0, 1.0), vec4(1.0, 1.0, 1.0, 2.0),
    vec4(2.0, 0.0, 2.0, 1.0), vec4(2.0, 1.0, 2.0, 2.0),
    vec4(1.0, 1.0, 0.0, 0.0), vec4(1.0, 1.0, 2.0, 0.0),
    vec4(1.0, 1.0, 0.0, 2.0), vec4(1.0, 1.0, 2.0, 2.0),
);

// The strokes letter `code` draws: three to seven of the sixteen, the same every time
// (saber_persona.rs's tests pin that every letter differs).
fn glyph_bits(code: u32) -> u32 {
    var v = code * 747796405u + 2891336453u;
    v = ((v >> ((v >> 28u) + 4u)) ^ v) * 277803737u;
    let h = (v >> 22u) ^ v;
    var w = (code + 101u) * 2654435761u;
    w = w ^ (w >> 15u);
    w = w * 2246822519u;
    w = w ^ (w >> 13u);
    var bits = (h & w) & 0xffffu;
    bits = bits | (1u << (w % 16u)) | (1u << ((w >> 8u) % 16u)) | (1u << ((h >> 4u) % 16u));
    // At most seven strokes: drop the lowest while there are more.
    for (var i = 0; i < 16 && countOneBits(bits) > 7u; i++) {
        bits = bits & (bits - 1u);
    }
    return bits;
}

// Letter `code`'s strokes at `uv` (0 to 1 across and along the glyph), `width` a stroke's
// half-width and `soft` a pixel, both in glyph units.
fn glyph(code: u32, uv: vec2<f32>, width: f32, soft: f32) -> f32 {
    let bits = glyph_bits(code);
    var nearest = 4.0;
    for (var i = 0u; i < 16u; i++) {
        if (bits & (1u << i)) == 0u {
            continue;
        }
        let stroke = GLYPH_STROKES[i];
        let a = vec2(0.12, 0.08) + stroke.xy * vec2(0.38, 0.42);
        let b = vec2(0.12, 0.08) + stroke.zw * vec2(0.38, 0.42);
        let ab = b - a;
        let t = clamp(dot(uv - a, ab) / dot(ab, ab), 0.0, 1.0);
        nearest = min(nearest, length(uv - a - ab * t));
    }
    let half = max(width, soft);
    return (1.0 - smoothstep(half - soft, half + soft, nearest)) * width / half;
}

// Glyphs at `along` units from the hilt and `x` across: the wearer's name, a letter a cell
// of `size` along plus `spacing`, scrolling toward the tip, a cell left empty between
// repeats; only along the blade, not round its tip.
fn skin_glyphs(
    skin: Skin, input: VertexOutput, along: f32, x: f32, time: f32, pixel: f32,
) -> f32 {
    let g = skin.glyph_shape;
    let half_width = g.x * 0.3;
    if g.x <= 0.0 || abs(x) > half_width + pixel || along < 1.0 || along > input.length - 1.0 {
        return 0.0;
    }
    var count = 0u;
    for (var i = 0u; i < 18u; i++) {
        if persona_letter(input, i) == 0u {
            break;
        }
        count = i + 1u;
    }
    let cell = g.x + g.y;
    let k = (along - time * g.z) / cell;
    let v = fract(k) * cell / g.x;
    if v > 1.0 || count == 0u {
        return 0.0;
    }
    let index = i32(floor(k));
    let slot = u32(((index % i32(count + 1u)) + i32(count + 1u)) % i32(count + 1u));
    if slot == count {
        return 0.0;
    }
    let uv = vec2(x / (2.0 * half_width) + 0.5, v);
    let ends = smoothstep(1.0, 3.0, along) * (1.0 - smoothstep(input.length - 3.0, input.length - 1.0, along));
    return glyph(persona_letter(input, slot), uv, g.w * 0.5, pixel / g.x) * ends;
}

// --- The third set (blade_skin_extras.rs): each draws nothing when its lanes are zeros ---

// The world's up in the projected blade's plane (across, along), from `down`; seen from
// straight above or below, where it has no length, the blade's own direction.
fn skin_up(down: vec2<f32>) -> vec2<f32> {
    let size = length(down);
    return select(vec2(0.0, 1.0), -down / max(size, 0.0001), size > 0.001);
}

// Rising noise at `p` (across, along the projected blade, units) for a skin's wisps and
// haze: two octaves of value noise in the plane turned so `up` runs up it, rising `speed`
// units a second and warped by `curl` (the smoke curling as it goes).
fn skin_rising(
    p: vec2<f32>, up: vec2<f32>, scale: f32, speed: f32, curl: f32, time: f32, seed: f32,
) -> f32 {
    let q = vec2(dot(p, vec2(up.y, -up.x)), dot(p, up) - time * speed) * scale
        + vec2(seed * 37.0, seed * 11.0);
    let warp = vec2(skin_noise(q * 0.7 + vec2(3.1, time * 0.21)),
        skin_noise(q * 0.7 + vec2(7.7, -time * 0.17))) - 0.5;
    let w = q + curl * 2.0 * warp;
    return 0.65 * skin_noise(w) + 0.35 * skin_noise(w * 2.3 + vec2(5.2, 1.3));
}

// Where a point `p` is from the blade (the segment from the hilt to `shaft` along it):
// x how high it is above the nearest point of the blade, along `up`, y how far aside.
fn skin_above(p: vec2<f32>, shaft: f32, up: vec2<f32>) -> vec2<f32> {
    let v = p - vec2(0.0, clamp(p.y, 0.0, shaft));
    return vec2(dot(v, up), abs(dot(v, vec2(up.y, -up.x))));
}

// Wisps at `p`: smoke curling off the blade and rising up to `rise` units above it, a
// sheet over a blade held level, a plume over the tip of one held upright, thin along its
// sides; `r` the glow's radius there.
fn skin_wisps(skin: Skin, input: VertexOutput, r: f32, time: f32, seed: f32) -> f32 {
    let w = skin.wisp_shape;
    let up = skin_up(input.down);
    let at = skin_above(input.blade, input.shaft, up);
    let rise = max(w.x, 0.001);
    let risen = clamp(at.x / rise, 0.0, 1.0);
    // Mostly above the blade, wider as it rises, gone at the top.
    let width = r * (0.7 + 1.6 * risen);
    let column = exp(-(at.y * at.y) / (width * width));
    let shown = smoothstep(-0.6 * r, 0.4 * r, at.x) * (1.0 - smoothstep(0.55 * rise, rise, at.x));
    let n = skin_rising(input.blade, up, w.z, w.y, w.w, time, seed);
    let density = skin.wisp_field.x;
    let smoke = smoothstep(1.0 - density, 1.0 - density + 0.3, n);
    return smoke * column * shown * pow(1.0 - risen, 1.5);
}

// The tip's glint at `p` (units from the tip, facing the camera): `rays` rays turning
// `spin` turns a second, every other one shorter, tapering to their ends, and a round glow
// at the heart, twinkling `twinkle` times a second by `depth`.
fn skin_star(skin: Skin, p: vec2<f32>, time: f32, seed: f32, pixel: f32) -> f32 {
    let shape = skin.star_shape;
    let motion = skin.star_motion;
    let size = max(shape.y, 0.001);
    let rays = max(shape.x, 2.0);
    let r = length(p);
    let turn = atan2(p.y, p.x) / TAU + time * motion.x + seed;
    let k = turn * rays;
    let nearest = round(k);
    let reach = select(size, 0.55 * size, (i32(nearest) & 1) == 1 && rays >= 4.0);
    let aside = r * sin(abs(k - nearest) * TAU / rays);
    let along = clamp(1.0 - r / reach, 0.0, 1.0);
    let width = max(shape.z * reach * along, pixel);
    let ray = exp(-(aside * aside) / (width * width)) * along * along
        * (shape.z * reach * along + 0.0001) / width;
    let heart = shape.w * exp(-(r * r) / (0.03 * size * size));
    let twinkle = 1.0 - motion.z * (0.5 + 0.25 * sin(TAU * time * motion.y + seed * 13.0)
        + 0.25 * sin(TAU * time * motion.y * 1.618 + seed * 29.0));
    return (ray + heart) * twinkle;
}

// The glow, `pixel` world units across a screen pixel: the glow at `x` across (the glitch
// and the hologram's jitter move it, and split its colours), then what stays where the
// blade is: embers, glyphs, the sputtering cut and an afterimage's fading.
// --- The fifth set (blade_skin_lab.rs): each draws nothing when its lanes are zeros ------

// `color`'s strongest channel.
fn skin_level(color: vec3<f32>) -> f32 {
    return max(color.r, max(color.g, color.b));
}

// Hue `h` (turns), saturation and value as RGB.
fn skin_hsv(h: f32, s: f32, v: f32) -> vec3<f32> {
    let k = fract(vec3(h) + vec3(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0;
    let rgb = clamp(abs(k - 3.0) - 1.0, vec3(0.0), vec3(1.0));
    return v * mix(vec3(1.0), rgb, s);
}

// How much of a band covers `around` units along and `across` capsule radii out (signed):
// bands `spacing` apart, a `width` share of it wide, slanting by `twist`, running `speed`
// units a second, their edges soft over a pixel.
fn skin_band(skin: Skin, around: f32, across: f32, time: f32, pixel: f32) -> f32 {
    let b = skin.band_shape;
    if b.x <= 0.0 {
        return 0.0;
    }
    let phase = (around - time * b.w) / b.x + b.z * across;
    let d = abs(fract(phase) - 0.5) * 2.0;
    let soft = 0.04 + pixel / b.x;
    return 1.0 - smoothstep(b.y - soft, b.y + soft, d);
}

// The light left of a segmented tube at `along`: a section that stutters on this draw, the
// dark gaps between sections, and a faint hum; 1 without segments.
fn skin_segments(skin: Skin, along: f32, length: f32, time: f32, seed: f32) -> f32 {
    let s = skin.segment_a;
    if s.x < 2.0 {
        return 1.0;
    }
    let q = clamp(along / max(length, 0.0001), 0.0, 0.9999) * s.x;
    let section = floor(q);
    var lit = 1.0;
    if s.y > 0.0 {
        let draw = skin_hash(vec2(i32(section) * 7 + i32(seed * 4096.0), i32(floor(time * s.y))));
        if draw >= s.z {
            lit = 1.0 - s.w;
        }
    }
    let b = skin.segment_b;
    var gap = 1.0;
    if b.x > 0.0 {
        let f = fract(q);
        // No gap before the first section or after the last.
        let below = select(f, 1.0, section < 0.5);
        let above = select(1.0 - f, 1.0, section > s.x - 1.5);
        let edge = min(below, above) * length / s.x;
        gap = smoothstep(0.2 * b.x, b.x, edge);
    }
    let hum = 1.0 - b.y * skin_noise(vec2(time * 23.0, seed * 9.0));
    return lit * gap * hum;
}

// A soap film's colour at `around` units along and `across` radii out.
fn skin_film(skin: Skin, around: f32, across: f32, time: f32) -> vec3<f32> {
    let f = skin.film;
    let p = vec2(around * f.y + time * f.z * 0.2, across * 1.6 - time * f.z * 0.35);
    let n = 0.6 * skin_noise(p) + 0.3 * skin_noise(p * 2.1 + vec2(5.0, 1.0))
        + 0.1 * skin_noise(p * 4.3 + vec2(11.0, 7.0))
        + 0.3 * sin(around * f.y * 2.0 - time * f.z * 0.8);
    return skin_hsv(n * 1.8 + time * 0.03, f.w, 1.0);
}

// Blobs of wax at `along` units (unclamped) and `x` units across: metaballs rising in the
// column, merging where they touch, with pools at the hilt and the tip; 0 to 1.
fn skin_blobs(skin: Skin, along: f32, x: f32, r: f32, length: f32, time: f32, seed: f32) -> f32 {
    let s = skin.blob_shape;
    if s.x < 1.0 {
        return 0.0;
    }
    let seeded = i32(seed * 4096.0);
    let size = s.y * r;
    let margin = 2.0 * size;
    let span = length + 2.0 * margin;
    var field = 0.0;
    for (var i = 0; i < min(i32(s.x), 8); i++) {
        let a = skin_hash(vec2(i, 77 + seeded));
        let b = skin_hash(vec2(i, 131 + seeded));
        let c = skin_hash(vec2(i, 199 + seeded));
        let pace = s.z * (0.6 + 0.8 * a);
        let place = fract(b + time * pace / span) * span - margin;
        let sway = sin(time * 0.6 + f32(i) * 2.0 + c * 6.0) * s.w * r * 0.5;
        let radius = size * (0.7 + 0.6 * c);
        let q = vec2((along - place) * 0.75, x - sway);
        let near = radius * radius / (dot(q, q) + 0.0001);
        // Squared, so a blob's pull falls off fast and they stay apart until they touch.
        field += near * near;
    }
    let pool = skin.blob_more.x * r;
    if pool > 0.0 {
        let hilt = vec2(along * 0.6, x);
        let tip = vec2((along - length) * 0.6, x);
        let low = pool * pool / (dot(hilt, hilt) + 0.0001);
        let high = pool * pool / (dot(tip, tip) + 0.0001);
        field += low * low + high * high;
    }
    return smoothstep(0.85, 1.15, field) * (1.0 - smoothstep(0.85 * r, r, abs(x)));
}

// How far up the blade a front is, a share of it (blade_skin_lab.rs `Front::height`).
fn skin_front_height(skin: Skin, time: f32, seed: f32) -> f32 {
    let s = skin.front_shape;
    if s.x <= 0.0 {
        return 0.0;
    }
    let p = fract(time / s.x + seed * 0.37);
    if p >= s.y {
        return 0.0;
    }
    let a = p / s.y;
    let ramp = max((1.0 - s.z) * 0.5, 0.001);
    return s.w * smoothstep(0.0, ramp, a) * (1.0 - smoothstep(1.0 - ramp, 1.0, a));
}

// How much the front covers `along` units (and `x` across, for its wandering edge): 0 to 1.
fn skin_front(skin: Skin, along: f32, x: f32, length: f32, time: f32, seed: f32) -> f32 {
    let height = skin_front_height(skin, time, seed);
    if height <= 0.0 {
        return 0.0;
    }
    let e = skin.front_edge;
    let line = height * length;
    let jag = e.y * e.x * 2.0 * (skin_noise(vec2(along * 0.35 + seed * 9.0, x * 0.8 + time * 0.7)) - 0.5);
    return (1.0 - smoothstep(line - e.x, line + e.x, along + jag)) * smoothstep(0.0, 0.05, height);
}

// `color` recoloured toward `rgb` at its own level by `amount`.
fn skin_recolour(color: vec3<f32>, rgb: vec3<f32>, amount: f32) -> vec3<f32> {
    return mix(color, rgb * skin_level(color), amount);
}

// --- The sixth set (blade_skin_figures.rs): figures in the glow, nothing when zeros -----

// A thin line's light `d` units from it: a Gaussian `width` wide, never thinner than a
// pixel (a thinner one widens to it and dims, so it does not break up at a distance).
fn skin_line(d: f32, width: f32, pixel: f32) -> f32 {
    let w = max(width, pixel);
    let e = d / w;
    return exp(-e * e) * width / w;
}

// A pencil sketch at `along` units (unclamped) and `x` across: two wobbly outlines and
// hatching, redrawn `rate` times a second; the ink's light (0 and up).
fn skin_sketch(skin: Skin, along: f32, x: f32, r: f32, blade_length: f32, time: f32, pixel: f32) -> f32 {
    let s = skin.sketch_shape;
    if s.x <= 0.0 || along < 0.0 || along > blade_length {
        return 0.0;
    }
    let frame = floor(time * s.x);
    // Pointed at the tip over its last few units.
    let taper = 1.0 - smoothstep(blade_length - 2.8, blade_length, along);
    let outline = 0.45 * r * mix(0.08, 1.0, taper);
    let w1 = (skin_noise(vec2(along * 0.35, frame * 3.1)) - 0.5) * s.y * r;
    let w2 = (skin_noise(vec2(along * 0.28 + 7.0, frame * 5.7)) - 0.5) * s.y * r;
    let width = 0.05 * r;
    let l1 = skin_line(abs(abs(x - w1) - outline), width, pixel);
    let l2 = skin_line(abs(abs(x - w2) - outline * 1.05), width * 0.8, pixel) * 0.6;
    var hatch = 0.0;
    if s.z > 0.0 {
        let h = abs(fract((along * 0.7 + x) * s.z + skin_hash(vec2(i32(frame), 17))) - 0.5);
        hatch = smoothstep(0.08, 0.0, h) * step(abs(x), outline * 0.9) * 0.42;
    }
    let grain = 0.72 + 0.28 * skin_noise(vec2(along * 9.0, x * 9.0 + frame));
    return (l1 + l2 + hatch) * grain;
}

// The hourglass's width at `along`: `waist` at the middle, all of it at either end.
fn skin_hourglass_profile(skin: Skin, along: f32, blade_length: f32) -> f32 {
    let w = skin.hour_shape.y;
    let xn = clamp(along / max(blade_length, 0.0001), 0.0, 1.0);
    return w + (1.0 - w) * pow(abs(2.0 * xn - 1.0), 0.75);
}

// The hourglass at `along` (unclamped) and `x`: rgb its sand's share (x), the glass
// rim's (y) and the flip's flash (z), 0 to about 1 each.
fn skin_hourglass(
    skin: Skin, along: f32, x: f32, r: f32, blade_length: f32, time: f32, seed: f32, pixel: f32,
) -> vec3<f32> {
    let h = skin.hour_shape;
    if h.x <= 0.0 || along < 0.0 || along > blade_length {
        return vec3(0.0);
    }
    let radius = 0.6 * r * skin_hourglass_profile(skin, along, blade_length);
    let inside = 1.0 - smoothstep(radius * 0.85, radius, abs(x));
    let rim = skin_line(abs(abs(x) - radius), 0.05 * r, pixel) * h.z;
    let p = fract(time / h.x + seed * 0.37);
    let middle = blade_length * 0.5;
    let top_end = middle + (1.0 - p) * blade_length * 0.36;
    let bottom_end = p * blade_length * 0.36;
    let sand_top = step(middle, along) * step(along, top_end);
    let sand_bottom = step(along, bottom_end);
    let grain = 0.7 + 0.3 * skin_noise(vec2(along * 6.0, x * 6.0));
    let stream = step(bottom_end, along) * step(along, middle)
        * skin_line(abs(x), 0.04 * r, pixel)
        * (0.55 + 0.45 * skin_noise(vec2(along * 1.5 - time * 12.0, 0.0))) * step(p, 0.985);
    let flash = exp(-p * 30.0) + exp(-(1.0 - p) * 60.0);
    return vec3((sand_top + sand_bottom) * inside * grain * 1.1 + stream * 1.4, rim,
        flash * inside);
}

// A serpent at `along` (unclamped) and `x`: x its body's light, y how much it hides the
// blade (in front of it), z its eyes', w its tongue's.
fn skin_serpent(
    skin: Skin, along: f32, x: f32, r: f32, blade_length: f32, time: f32, pixel: f32,
) -> vec4<f32> {
    let s = skin.serpent_shape;
    if s.x <= 0.0 {
        return vec4(0.0);
    }
    let a = s.y * r;
    let k = TAU * s.x / max(blade_length, 0.0001);
    let w = TAU * s.z;
    let phase = k * along - time * w;
    let y = a * sin(phase);
    let z = cos(phase);
    let slope = a * k * cos(phase);
    let d = abs(x - y) / sqrt(1.0 + slope * slope);
    let head_at = blade_length * 0.9 + sin(time * 1.1) * 0.3;
    let tail = smoothstep(0.02 * blade_length, 0.25 * blade_length, along);
    let thick = s.w * r * mix(0.35, 1.0, tail);
    let on = step(0.02 * blade_length, along) * step(along, head_at - 0.2 * r);
    let body = smoothstep(thick, thick * 0.5, d) * on;
    let scales = 0.72 + 0.28 * sin(along * 6.5 - time * 2.0) * sin((x - y) / max(r, 0.0001) * 30.0);
    let shown = mix(0.28, 1.0, step(0.0, z));
    var light = body * scales * shown * 1.25 + exp(-d / max(0.35 * r, 0.0001)) * 0.22 * on * shown;
    // The head, its jaws opening now and then, two eyes and a tongue.
    let head_y = a * sin(k * head_at - time * w);
    let hp = vec2(along - head_at, x - head_y);
    let size = vec2(max(0.8 * a, 0.9 * r), max(0.45 * a, 0.5 * r));
    let hq = hp / size;
    let jaw = smoothstep(0.7, 1.0, 0.5 + 0.5 * sin(time * 1.7));
    var head = smoothstep(1.0, 0.75, length(hq - vec2(-0.1, 0.0)));
    head *= 1.0 - step(0.3, hq.x) * step(abs(hq.y), 0.5 * jaw);
    light += head * 1.05;
    let eye = 0.09 * size.y;
    let eyes = exp(-length(hp - vec2(0.18 * size.x, 0.4 * size.y)) / max(eye, pixel))
        + exp(-length(hp - vec2(0.18 * size.x, -0.4 * size.y)) / max(eye, pixel));
    let tongue = skin_line(abs(hp.y - sin(time * 40.0) * 0.1 * size.y), 0.04 * size.y, pixel)
        * step(0.9 * size.x, hp.x) * step(hp.x, 1.8 * size.x) * step(0.5, jaw);
    let hide = (body * step(0.0, z) + head) * 0.6;
    return vec4(light, min(hide, 0.9), eyes, tongue);
}

// Glass strands fanning from the hilt at `along` (unclamped) and `x`: their light.
fn skin_strands(
    skin: Skin, along: f32, x: f32, r: f32, blade_length: f32, time: f32, seed: f32, pixel: f32,
) -> vec3<f32> {
    let s = skin.strand_shape;
    if s.x < 2.0 || along < -1.0 {
        return vec3(0.0);
    }
    let seeded = i32(seed * 4096.0);
    let xn = clamp(along / max(blade_length, 0.0001), 0.0, 1.2);
    var total = vec3(0.0);
    for (var i = 0; i < min(i32(s.x), 32); i++) {
        let fi = f32(i);
        let spread = skin_hash(vec2(i, 11 + seeded)) * 2.0 - 1.0;
        let tip = blade_length * (0.72 + 0.28 * skin_hash(vec2(i, 23 + seeded)));
        let tn = tip / max(blade_length, 0.0001);
        let colour = skin_hsv(fract(skin.strand_hue.x + skin_hash(vec2(i, 37)) * 0.35 + time * 0.04),
            skin.strand_hue.y, 1.0);
        let sway = sin(time * 1.2 + fi) * 0.25 * s.z * r;
        let y = spread * s.y * r * pow(xn, 1.5) + sway * xn;
        let on = step(0.0, along) * step(along, tip);
        total += colour * skin_line(abs(x - y), 0.03 * r, pixel) * 0.18 * on;
        let tip_y = spread * s.y * r * pow(tn, 1.5) + sway * tn;
        let dd = length(vec2(along - tip, x - tip_y));
        let twinkle = 0.7 + 0.3 * sin(time * 3.0 + fi * 2.0);
        total += colour * (exp(-dd / max(0.1 * r, pixel)) * 1.3 + exp(-dd / (0.45 * r)) * 0.16) * twinkle;
    }
    return total * s.w;
}

// Butterflies resting on the blade and flying off now and then, at `along` (unclamped)
// and `x`: their wings' light.
fn skin_butterflies(
    skin: Skin, along: f32, x: f32, r: f32, blade_length: f32, time: f32, seed: f32,
) -> f32 {
    let s = skin.fly_shape;
    if s.x < 1.0 {
        return 0.0;
    }
    let seeded = i32(seed * 4096.0);
    let size = s.y;
    let roam = skin.fly_roam.x;
    var total = 0.0;
    for (var i = 0; i < min(i32(s.x), 8); i++) {
        let fi = f32(i);
        let h1 = skin_hash(vec2(i, 51 + seeded));
        let h2 = skin_hash(vec2(i, 67 + seeded));
        let h3 = skin_hash(vec2(i, 83 + seeded));
        let side = select(-1.0, 1.0, h2 < 0.5);
        var spot = vec2((0.12 + 0.78 * h1) * blade_length, side * 0.8 * r);
        let cycle = 6.0 + 4.0 * h3;
        let p = fract(time / cycle + h1 * 3.7);
        var open = 0.55 + 0.45 * sin(time * 0.8 + fi * 1.7);
        if p < s.w {
            // Out and back on a loop, flapping fast.
            let f = p / max(s.w, 0.0001);
            let away = sin(PI * f);
            spot += roam * away * vec2(0.6 * cos(TAU * f * 1.3 + h3 * 6.0),
                side * (0.6 + 0.4 * sin(TAU * f * 2.0)));
            open = 0.15 + 0.85 * abs(sin(time * s.z * PI + fi));
        }
        let q = vec2(along, x) - spot;
        if dot(q, q) > size * size * 1.2 {
            continue;
        }
        var wings = 0.0;
        for (var w = 0; w < 2; w++) {
            let ws = select(-1.0, 1.0, w == 1);
            let spread = max(open, 0.12);
            let upper = (q - vec2(0.18 * size, ws * 0.3 * size * spread))
                / vec2(0.32 * size, 0.28 * size * spread);
            let lower = (q - vec2(-0.16 * size, ws * 0.24 * size * spread))
                / vec2(0.24 * size, 0.2 * size * spread);
            for (var part = 0; part < 2; part++) {
                let e = select(lower, upper, part == 1);
                let l = length(e);
                // A dim rim and veins, as a monarch's wing.
                let wing = 1.0 - smoothstep(0.85, 1.0, l);
                let rim = smoothstep(1.0, 0.7, l);
                let veins = 0.75 + 0.25 * sin(atan2(e.y, e.x) * 9.0);
                wings = max(wings, wing * (0.35 + 0.65 * rim) * veins);
            }
        }
        total += wings;
    }
    return total;
}

// Wings of fire spreading from near the hilt at `along` (unclamped) and `x`: their light.
fn skin_wings(skin: Skin, along: f32, x: f32, r: f32, blade_length: f32, time: f32) -> f32 {
    let s = skin.wing_shape;
    if s.x <= 0.0 {
        return 0.0;
    }
    let origin = vec2(0.15 * blade_length, 0.0);
    let q = vec2(along, x) - origin;
    if length(q) > s.x * 1.1 {
        return 0.0;
    }
    let unfold = 0.35 + 0.65 * smoothstep(-0.3, 0.6, sin(TAU * time * s.y));
    let feathers = i32(s.z);
    var total = 0.0;
    for (var w = 0; w < 2; w++) {
        let ws = select(-1.0, 1.0, w == 1);
        for (var f = 0; f < feathers; f++) {
            let k = f32(f) / max(f32(feathers - 1), 1.0);
            // Fanned from along the blade toward its side as the wing opens.
            let angle = (0.25 + 1.05 * k) * unfold;
            let dir = vec2(cos(angle), ws * sin(angle));
            let span = s.x * (0.55 + 0.45 * (1.0 - abs(k - 0.6)));
            let t = clamp(dot(q, dir) / span, 0.0, 1.0);
            let d = length(q - dir * t * span);
            let width = (0.12 + 0.18 * sin(PI * t)) * r;
            let flame = 0.6 + 0.6 * skin_noise(vec2(t * 6.0 - time * 3.0, f32(f) * 3.1 + ws * 7.0));
            total += smoothstep(width, width * 0.3, d) * (1.0 - 0.6 * t) * flame * unfold;
        }
    }
    return min(total, 2.0);
}

fn skin_glow(input: VertexOutput, pixel: f32) -> vec3<f32> {
    let skin = skin_at(input);
    let time = input.animation.x;
    let seed = input.animation.y;
    let along = skin_along(input);
    let r = chain_radius(input.radius, input.length, clamp(along / max(input.length, 0.0001), 0.0, 1.0));
    let glitch = skin_glitch(skin, along, time, seed);
    let x = input.blade.x - (glitch.x + skin_jitter(skin, time, seed)) * r;
    var glow: vec3<f32>;
    if glitch.y > 0.0 {
        let split = glitch.y * r;
        glow = vec3(
            skin_glow_at(input, pixel, x + split).r,
            skin_glow_at(input, pixel, x).g,
            skin_glow_at(input, pixel, x - split).b,
        ) * (1.0 + glitch.z);
    } else {
        glow = skin_glow_at(input, pixel, x);
    }
    let copies = skin.copy_shape;
    if copies.x >= 1.0 {
        // Ghost copies at other angles drifting apart, then collapsing with a flash.
        let p = fract(time / copies.w + seed * 0.37);
        let apart = smoothstep(0.0, 0.85, p) * (1.0 - smoothstep(0.92, 1.0, p));
        let flicker = 1.0 - skin.copy_more.x * skin_hash(vec2(i32(floor(time * 12.0)), 4141));
        var ghosts = vec3(0.0);
        // Their angle in the quad's own width (an angle of 0.1 reaches 0.6 of it at the
        // tip, the outer copy), so they stay inside it whatever the blade's size.
        let reach = skin_extent(skin, input) * 0.6 * copies.y / 0.1 / max(copies.x, 1.0);
        for (var c = 1; c <= min(i32(copies.x), 2); c++) {
            for (var s = 0; s < 2; s++) {
                let offset = select(-1.0, 1.0, s == 1) * f32(c) * reach * apart
                    * along / max(input.length, 0.0001);
                // Its glow, and a thin bright line of its own core.
                ghosts += (skin_glow_at(input, pixel, x - offset)
                    + skin_line(abs(x - offset), 0.1 * input.radius, pixel) * skin.core_white.rgb * 1.5)
                    / f32(c);
            }
        }
        glow = glow * (1.0 + 0.8 * exp(-p * 25.0)) + ghosts * copies.z * flicker * apart;
    }
    let extent = skin_extent(skin, input);
    let beyond = max(input.blade.y - input.shaft, 0.0);
    let room = (1.0 - smoothstep(0.75, 1.0, abs(input.blade.x) / extent))
        * (1.0 - smoothstep(0.75, 1.0, beyond / extent))
        * (1.0 - smoothstep(0.75, 1.0, -min(input.blade.y, 0.0) / extent));
    if skin.ember_field.x > 0.0 {
        let scale = input.shaft / max(input.length, 0.0001);
        let embers = skin_embers(skin, input.blade, input.down, scale, input.length, time, seed,
            pixel);
        glow += embers * room * skin.ember_color.rgb * skin.ember_color.w;
    }
    if skin.glyph_color.w > 0.0 {
        glow += skin_glyphs(skin, input, along, x, time, pixel) * skin.glyph_color.rgb
            * skin.glyph_color.w;
    }
    if skin.wisp_color.w > 0.0 {
        var smoke = skin_wisps(skin, input, r, time, seed) * skin.wisp_color.rgb
            * skin.wisp_color.w;
        smoke = skin_apply_tint(smoke, skin_tint(skin, input));
        if input.chroma != 0.0 {
            smoke = skin_turn_hue(smoke, input.chroma);
        }
        glow += smoke;
    }
    let cut = skin_cut(skin, input.length, time, seed);
    if cut < input.length {
        // Past the cut nothing shows, the embers already falling aside.
        let reach = select(along, input.length + beyond, beyond > 0.0);
        glow *= 1.0 - smoothstep(cut - 1.5, cut + 0.5, reach);
    }
    return glow * persona_fade(input);
}

// The glow at `x` across (the fragment's own, or moved by a glitch).
fn skin_glow_at(input: VertexOutput, pixel: f32, x: f32) -> vec3<f32> {
    let skin = skin_at(input);
    let time = input.animation.x;
    let seed = input.animation.y;
    let along = skin_along(input);
    let grain = skin_granulation(skin, along, time, seed);
    let flare = skin_flares(skin, along, input.length, time, seed);
    let beat = skin_beat(skin, time);
    // Shimmer and swell: the corona breathes a little, a flare widens it, a beat too.
    let shimmer = skin_wave(skin.shimmer_a, time, along, seed)
        + skin_wave(skin.shimmer_b, time, along, seed);
    let swell = skin.swell;
    var widen = min(1.0 + shimmer + swell.y * (grain - 0.5) + swell.z * flare
        + skin.pulse_b.y * beat, swell.x);
    // Past the tip (`beyond`, projected units) the capsule ends round: it widens about the
    // tip as it widens about the shaft, and the distance out is taken from the tip, so the
    // corona's grading and its tongues go round the end instead of running on straight.
    let beyond = max(input.blade.y - input.shaft, 0.0);
    let r = chain_radius(input.radius, input.length, clamp(along / max(input.length, 0.0001), 0.0, 1.0));
    let radial = select(abs(x), length(vec2(x, beyond)), beyond > 0.0);
    let around = select(along, along + r * atan2(beyond, max(abs(x), 0.0001)), beyond > 0.0);
    let ragged = skin.sputter_edge;
    if ragged.x > 0.0 {
        // A frayed edge: the width wanders by noise, each side its own.
        let fray = skin_noise(vec2(around * ragged.y - time * ragged.z,
            time * ragged.z * 0.37 + seed * 13.0 + select(0.0, 50.0, x < 0.0)));
        widen = clamp(widen * (1.0 + ragged.x * (fray * 2.0 - 1.0)), 0.3, swell.x);
    }
    // Rings squeezing the glow where the bands pass (taken on the axis, so they stay
    // round).
    let pinch = skin.band_mix.y;
    if pinch > 0.0 {
        widen *= 1.0 - pinch * skin_band(skin, around, 0.0, time, pixel);
    }
    // An hourglass's pinched waist.
    if skin.hour_shape.x > 0.0 {
        widen *= 0.2 + 0.8 * skin_hourglass_profile(skin, along, input.length);
    }
    var shaded = input;
    shaded.blade = vec2(x / widen, min(input.blade.y, input.shaft) + beyond / widen);
    let capsule = glow_capsule(shaded) / widen;
    let out = radial / (r * widen);
    // Flame tongues licking outward in the corona's edge.
    let ta = skin.tongue_a;
    let tb = skin.tongue_b;
    let tongue = skin_noise(vec2(around * ta.x + seed * ta.y, out * ta.z - time * ta.w));
    let edge = smoothstep(tb.x, tb.y, out);
    let flicker = mix(1.0, tb.z + tb.w * tongue, edge);
    // The inside colour by the core, the rim's at the edge; granulation and flares heat it.
    let inner = 1.0 - smoothstep(0.0, skin.rim_cool.w, out);
    let rim = mix(skin.rim_cool.rgb, skin.rim_hot.rgb,
        clamp(grain * skin.rim_hot.w + flare * skin.inner.w, 0.0, 1.0));
    let share = skin.inner_mix;
    let color = mix(rim, skin.inner.rgb,
        clamp(inner * inner * (share.x + share.y * grain) + flare * share.z, 0.0, 1.0));
    let light = skin.brightness;
    var brightness = (light.x + light.y * grain + light.z * flare) * flicker * (1.0 + beat);
    var colour = color;
    let band = skin_band(skin, around, x / max(r * widen, 0.0001), time, pixel);
    if band > 0.0 {
        colour = mix(colour, skin.band_color.rgb, skin.band_mix.x * band);
        brightness *= mix(1.0, skin.band_color.w, band);
    }
    var glow = capsule * colour * brightness;
    // Where a front covers the blade (veins may be kept to it).
    let front = skin_front(skin, along, x, input.length, time, seed);
    if skin.vein_color.w > 0.0 {
        let veins = skin_veins(skin, along, x, time, seed)
            * mix(1.0, front, skin.front_edge.z);
        glow += veins * (1.0 - smoothstep(0.15, 0.6, out)) * capsule.r * skin.vein_color.rgb
            * skin.vein_color.w * (1.0 + beat);
    }
    if skin.blob_shape.x >= 1.0 {
        let unclamped = input.blade.y / max(input.shaft, 0.0001) * input.length;
        glow += skin_blobs(skin, unclamped, x, r, input.length, time, seed)
            * skin.blob_color.rgb * skin.blob_color.w;
    }
    if skin.film.x > 0.0 {
        // A bubble: clear inside, bright at its rim, in the film's sliding colours.
        let tint = skin_film(skin, around, x / max(r * widen, 0.0001), time);
        let rim = exp(-pow((out - 0.8) / 0.22, 2.0));
        glow *= (1.0 - skin.film_b.x * (1.0 - smoothstep(0.1, 0.7, out)))
            * (1.0 + 1.5 * skin.film_b.x * rim);
        glow = skin_recolour(glow, tint, skin.film.x);
    }
    if front > 0.0 {
        glow = skin_recolour(glow, skin.front_color.rgb, skin.front_color.w * front);
    }
    // The sixth set's figures, on the blade's place along it unclamped.
    let place = input.blade.y / max(input.shaft, 0.0001) * input.length;
    if skin.sketch_shape.x > 0.0 {
        glow = glow * skin.sketch_shape.w + skin_sketch(skin, place, x, r, input.length, time, pixel)
            * skin.sketch_color.rgb * skin.sketch_color.w;
    }
    if skin.hour_shape.x > 0.0 {
        let h = skin_hourglass(skin, place, x, r, input.length, time, seed, pixel);
        glow = glow * (0.45 + 1.5 * h.z) + h.x * skin.hour_color.rgb * skin.hour_color.w
            + h.y * skin.rim_hot.rgb;
    }
    if skin.serpent_shape.x > 0.0 {
        let s = skin_serpent(skin, place, x, r, input.length, time, pixel);
        glow = glow * (1.0 - s.y) + s.x * skin.serpent_color.rgb * skin.serpent_color.w
            + s.z * vec3(2.2, 0.18, 0.08) + s.w * vec3(1.0, 0.35, 0.4);
    }
    if skin.strand_shape.x >= 2.0 {
        glow += skin_strands(skin, place, x, r, input.length, time, seed, pixel);
    }
    if skin.fly_shape.x >= 1.0 {
        glow += skin_butterflies(skin, place, x, r, input.length, time, seed)
            * skin.fly_color.rgb * skin.fly_color.w;
    }
    if skin.wing_shape.x > 0.0 {
        let fire = skin_wings(skin, place, x, r, input.length, time);
        glow += fire * mix(skin.wing_color.rgb, vec3(1.0, 0.9, 0.6), smoothstep(1.2, 2.0, fire))
            * skin.wing_color.w;
    }
    let scan = skin.scan_lines;
    if scan.x > 0.0 {
        // A hologram: scan lines darken it as they run, its inside is hollow, and a
        // wireframe (side lines and rings) is drawn over it.
        let lines = 1.0 - scan.z * (0.5 + 0.5 * sin(TAU * (around * scan.x - time * scan.y)));
        let e = skin.scan_edge;
        glow *= lines * (1.0 - e.z * (1.0 - smoothstep(0.0, e.x, out)));
        let soft = pixel / max(r * widen, 0.0001);
        // Not behind the hilt, where the place along stops at the hilt.
        let shown = smoothstep(-pixel, 0.0, input.blade.y);
        let sides = (1.0 - smoothstep(e.y, e.y + soft, abs(out - e.x))) * shown;
        var rings = 0.0;
        if scan.w > 0.0 {
            let f = fract(around * scan.w);
            let units = min(f, 1.0 - f) / scan.w;
            let half = e.y * r * 0.5;
            rings = (1.0 - smoothstep(half, half + pixel, units))
                * (1.0 - smoothstep(e.x, e.x + soft, out)) * shown;
        }
        glow += (sides + rings) * lines * skin.scan_color.rgb * skin.scan_color.w;
    }
    if skin.arc_strike.x > 0.0 || skin.mote_field.x > 0.0 {
        // Both fade out before the quad's edge (vertex_main's `extent`) instead of being cut.
        let extent = skin_extent(skin, input);
        let room = (1.0 - smoothstep(0.75, 1.0, abs(x) / extent))
            * (1.0 - smoothstep(0.75, 1.0, beyond / extent));
        if skin.arc_strike.x > 0.0 {
            // Along the axis and straight on past the tip.
            let reach = select(input.blade.y / max(input.shaft, 0.0001) * input.length,
                input.length + beyond, beyond > 0.0);
            let arcs = skin_arcs(skin, reach, x, r, input.length, extent, pixel, time, seed);
            glow += arcs * room * skin.arc_color.rgb * skin.arc_color.w;
        }
        if skin.mote_field.x > 0.0 {
            let unclamped = input.blade.y / max(input.shaft, 0.0001) * input.length;
            let motes = skin_motes(skin, around, radial / r, x < 0.0, unclamped, input.length,
                time, seed);
            glow += motes * room * skin.mote_color.rgb * skin.mote_color.w;
        }
    }
    let halftone = skin.ink_more.y;
    if halftone > 0.0 {
        // The glow printed as halftone dots, bigger where it is brighter; the inside stays
        // solid, as a comic's flat colour.
        let level = clamp(skin_level(glow) / 1.2, 0.0, 1.0);
        let cell = fract(vec2(around, x) * halftone) - 0.5;
        let dots = clamp((0.5 * sqrt(level) - length(cell)) / max(pixel * halftone, 0.02) + 0.5,
            0.0, 1.0);
        let printed = smoothstep(0.35, 0.6, out);
        glow = mix(glow, glow / max(level, 0.05) * dots * 0.9, printed);
    }
    glow *= skin_segments(skin, along, input.length, time, seed);
    glow = skin_apply_tint(glow, skin_tint(skin, input));
    let hue = skin.hue;
    if any(hue.xyz != vec3(0.0)) {
        glow = skin_turn_hue(glow, time * hue.x + around * hue.y + radial / r * hue.z);
    }
    // A chroma takes its wearer's colour: everything it drew, turned at once.
    if input.chroma != 0.0 {
        glow = skin_turn_hue(glow, input.chroma);
    }
    return glow;
}

// The core line: `uv` its texture coordinates (`core_coordinates`) and `footprint` how far
// they move in a pixel, which softens the rounded tip's edge.
fn skin_core(input: VertexOutput, texel: vec4<f32>, uv: vec2<f32>, footprint: f32) -> vec3<f32> {
    let skin = skin_at(input);
    let time = input.animation.x;
    let seed = input.animation.y;
    let along = skin_along(input);
    let grain = skin_granulation(skin, along, time, seed);
    let flare = skin_flares(skin, along, input.length, time, seed);
    let beat = skin_beat(skin, time);
    let bright = skin.core_fringe;
    var fringe = mix(skin.core_fringe_cool.rgb, skin.core_fringe_hot.rgb,
        skin.core_fringe_cool.w + skin.core_fringe_hot.w * grain)
        * (bright.x + bright.y * grain + bright.z * flare);
    fringe = skin_apply_tint(fringe, skin_tint(skin, input));
    let hue = skin.hue;
    if any(hue.xyz != vec3(0.0)) {
        fringe = skin_turn_hue(fringe, time * hue.x + along * hue.y);
    }
    var core = CORE_DRAWS * (skin.core_white.rgb * texel.r * (1.0 + skin.core_white.w * flare)
        + fringe * texel.g) * (1.0 + beat);
    let x = (uv.x - 0.5) * 2.0 * input.radius;
    let front = skin_front(skin, along, x, input.length, time, seed);
    if skin.vein_color.w > 0.0 {
        core += skin_veins(skin, along, x, time, seed) * (texel.r + texel.g) * skin.vein_color.rgb
            * skin.vein_shape.w * (1.0 + beat) * mix(1.0, front, skin.front_edge.z);
    }
    // The fifth set: the bands' colour (on the line's own axis), the front's, the
    // segments' stutter.
    let band = skin_band(skin, along, 0.0, time, 0.0);
    if band > 0.0 {
        core = skin_recolour(core, skin.band_color.rgb, skin.band_mix.x * band)
            * mix(1.0, skin.band_color.w, band);
    }
    if front > 0.0 {
        core = skin_recolour(core, skin.front_color.rgb, skin.front_color.w * front);
    }
    core *= skin_segments(skin, along, input.length, time, seed);
    // The sixth set: a sketch keeps its glow's share of the line, an hourglass's glass and
    // a bundle of strands keep a faint one (their sand and fibres carry the light), a
    // serpent hides it where its body passes in front.
    if skin.sketch_shape.x > 0.0 {
        core *= skin.sketch_shape.w;
    }
    if skin.hour_shape.x > 0.0 || skin.strand_shape.x >= 2.0 {
        core *= 0.25;
    }
    if skin.serpent_shape.x > 0.0 {
        core *= 1.0 - skin_serpent(skin, along, x, input.radius * 3.0, input.length, time, 0.0).y;
    }
    let scan = skin.scan_lines;
    if scan.x > 0.0 {
        core *= 1.0 - scan.z * (0.5 + 0.5 * sin(TAU * (along * scan.x - time * scan.y)));
    }
    // In the rounded tip the line is cut at the narrowing edge `core_coordinates` maps to the
    // texture's border (which the sampler would otherwise smear out to the quad's corners).
    if along > input.length - skin.core_fringe.w * input.radius {
        core *= clamp((0.5 - abs(uv.x - 0.5)) / max(footprint, 0.00001) + 0.5, 0.0, 1.0);
    }
    let cut = skin_cut(skin, input.length, time, seed);
    if cut < input.length {
        core *= 1.0 - smoothstep(cut - 1.5, cut + 0.5, along);
    }
    if input.chroma != 0.0 {
        core = skin_turn_hue(core, input.chroma);
    }
    return core;
}

// The core line's texture coordinates; a skin's core breathes a little across, narrows to
// its rounded tip and moves with a glitch or the hologram's jitter (in core half-widths,
// the glow's radius about three of them).
fn core_coordinates(input: VertexOutput) -> vec2<f32> {
    var across = input.blade.x / (2.0 * input.radius) * core_stretch(input);
    if input.kind >= KIND_SKIN {
        let along = mix(-1.0, input.length, input.blade.y);
        let skin = skin_at(input);
        let moved = skin_glitch(skin, along, input.animation.x, input.animation.y).x
            + skin_jitter(skin, input.animation.x, input.animation.y);
        across -= 1.5 * moved;
    }
    return vec2(0.5 + across, 1.0 - input.blade.y);
}

// How much a skin's core line is stretched across here: its breathing, and its rounded tip,
// where the width left goes to nothing (1 for a stock line).
fn core_stretch(input: VertexOutput) -> f32 {
    if input.kind < KIND_SKIN {
        return 1.0;
    }
    let along = mix(-1.0, input.length, input.blade.y);
    let skin = skin_at(input);
    return (1.0 + skin_wave(skin.core_breathe, input.animation.x, along, input.animation.y))
        / skin_tip_taper(along, input.length, skin.core_fringe.w * input.radius);
}

// The colour split a glitch gives the core at this fragment, in texture units across.
fn core_split(input: VertexOutput) -> f32 {
    if input.kind < KIND_SKIN {
        return 0.0;
    }
    let skin = skin_at(input);
    let along = mix(-1.0, input.length, input.blade.y);
    return 1.5 * skin_glitch(skin, along, input.animation.x, input.animation.y).y;
}

// Glint n's size: every other one of an orbit is smaller, a companion to the first.
fn glint_scale(n: f32) -> f32 {
    return select(1.0, 0.7, (i32(round(n)) & 1) == 1);
}

// A black hole's light at `p` (units from its heart, facing the camera): a thin photon
// ring just outside the horizon and a fainter accretion swirl round it, two spiral arms
// turning `spin` turns a second; nothing inside the horizon.
fn hole_ring(skin: Skin, p: vec2<f32>, time: f32, seed: f32, pixel: f32) -> f32 {
    let size = skin.hole_shape.x;
    let r = length(p);
    let width = max(0.07 * size, pixel);
    let ring = exp(-pow((r - 1.15 * size) / width, 2.0));
    let turn = atan2(p.y, p.x) / TAU;
    let arms = 0.5 + 0.5 * sin(TAU * (2.0 * turn + time * skin.hole_shape.z + seed)
        + 2.5 * r / size);
    let swirl = exp(-pow((r - 1.7 * size) / (0.55 * size), 2.0)) * (0.3 + 0.7 * arms) * 0.6;
    return (ring + swirl) * smoothstep(0.9 * size, 1.05 * size, r);
}

// A skin's glint, coloured as its glow is (the wearer's tint and chroma, an afterimage's
// fading), dimmed behind the blade; each of an orbit twinkles and turns at its own pace.
// A black hole's ring and swirl instead when the skin has holes.
fn star_glow(input: VertexOutput, pixel: f32) -> vec3<f32> {
    let skin = skin_at(input);
    let n = STAR - input.hilt;
    let scale = glint_scale(n);
    var glint: vec3<f32>;
    if skin.hole_shape.x > 0.0 {
        glint = hole_ring(skin, input.blade / scale, input.animation.x,
            input.animation.y + n * 0.37, pixel / scale) * skin.hole_color.rgb
            * skin.hole_color.w;
    } else if skin.star_color.w > 0.0 {
        glint = skin_star(skin, input.blade / scale, input.animation.x,
            input.animation.y + n * 0.37, pixel / scale) * skin.star_color.rgb
            * skin.star_color.w;
    } else {
        return vec3(0.0);
    }
    glint = skin_apply_tint(glint, skin_tint(skin, input));
    if input.chroma != 0.0 {
        glint = skin_turn_hue(glint, input.chroma);
    }
    return glint * persona_fade(input) * (1.0 - (1.0 - GLINT_BEHIND) * input.shaft);
}

// Dynamic glow: the glow capsule only; the core line's shader has no `glow` stage.
@fragment
fn fragment_glow(input: VertexOutput) -> @location(0) vec4<f32> {
    // Taken before any branch (uniform control flow).
    let pixel = fwidth(input.blade.x);
    if input.hilt <= STAR { return vec4(star_glow(input, pixel), 1.0); }
    if input.hilt <= 0.0 { discard; }
    if input.kind >= KIND_SKIN { return vec4(skin_glow(input, pixel), 1.0); }
    return vec4(glow_capsule(input) * input.color, 1.0);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // v runs from the tip (0) to behind the hilt (1), as DoLine's texture coordinates do.
    // Derivatives are taken before any branch (uniform control flow); the glow ignores the
    // core's and a skin's glow takes the pixel's size.
    let core_uv = core_coordinates(input);
    let core_dx = dpdx(core_uv);
    let core_dy = dpdy(core_uv);
    let pixel = fwidth(input.blade.x);
    if input.hilt <= STAR {
        return vec4(star_glow(input, pixel), 1.0);
    }
    if input.hilt > 0.0 {
        if input.kind >= KIND_SKIN { return vec4(skin_glow(input, pixel), 1.0); }
        return vec4(glow_capsule(input) * input.color, 1.0);
    }
    // How far the core's `u` moves in a pixel, from the straight line's across scaled by the
    // stretch here: the derivatives of `core_uv` itself blow up on the tip's last row, where
    // the stretch runs to a hundred within one pixel, and the rounded tip's cut then left
    // half the line's width lit there, a bar across the tip.
    let footprint = pixel / (2.0 * input.radius) * core_stretch(input);
    var texel = textureSampleGrad(core_texture, core_sampler, core_uv, core_dx, core_dy);
    let split = core_split(input);
    if split > 0.0 {
        // A glitch parts the core's colours: red one way, blue the other.
        let offset = vec2(split, 0.0);
        let red = textureSampleGrad(core_texture, core_sampler, core_uv + offset, core_dx, core_dy);
        let blue = textureSampleGrad(core_texture, core_sampler, core_uv - offset, core_dx, core_dy);
        let skin = skin_core(input, texel, core_uv, footprint);
        return vec4(
            skin_core(input, red, core_uv + offset, footprint).r,
            skin.g,
            skin_core(input, blue, core_uv - offset, footprint).b,
            1.0,
        );
    }
    if input.kind >= KIND_SKIN {
        return vec4(skin_core(input, texel, core_uv, footprint), 1.0);
    }
    if input.kind == KIND_NEUTRAL {
        // Neutral core: red = white-hot core, green = tinted fringe.
        return vec4(CORE_DRAWS * (vec3(texel.r) + input.color * texel.g), 1.0);
    }
    return vec4(CORE_DRAWS * texel.rgb * input.color, 1.0);
}

// --- Heat haze: drawn first into the effect layer, replacing its pixels ---------------------

// The scene as the effect layer holds it before any effect (effect_layer.rs `original`).
@group(3) @binding(0)
var haze_scene: texture_2d<f32>;
@group(3) @binding(1)
var haze_sampler: sampler;

// The scene behind a skin's glow bent by its heat: a rising, curling shift (at most
// `strength` world units, turned into pixels with the quad's own derivatives), strongest
// by the blade and above it, nothing at the reach's edge, so the quad's edge is the scene
// itself, unchanged.
@fragment
fn fragment_haze(input: VertexOutput) -> @location(0) vec4<f32> {
    // Taken before any branch (uniform control flow): units across and along a pixel.
    let dx = dpdx(input.blade);
    let dy = dpdy(input.blade);
    let size = vec2<f32>(textureDimensions(haze_scene));
    let here = input.clip_position.xy;
    let skin = skin_at(input);
    let h = skin.haze;
    let up = skin_up(input.down);
    let at = skin_above(input.blade, input.shaft, up);
    let distance = length(at);
    let r = chain_radius(input.radius, input.length,
        clamp(input.blade.y / max(input.shaft, 0.0001), 0.0, 1.0)) * skin.swell.x;
    // Full by the glow, gone at its edge plus the reach; more above the blade (heat rises).
    let fall = 1.0 - smoothstep(0.3 * r, r + h.w, distance);
    let rising = mix(0.55, 1.0, smoothstep(-r, r, at.x));
    let time = input.animation.x;
    let seed = input.animation.y;
    let a = skin_rising(input.blade, up, h.y, h.z, 0.6, time, seed);
    let b = skin_rising(input.blade + vec2(17.3, -9.1), up, h.y, h.z, 0.6, time, seed);
    let shift = (vec2(a, b) - 0.5) * 2.0 * h.x * fall * rising;
    // Never more than a few dozen pixels, however close.
    let pixels = to_pixels(shift, dx, dy, 0.02 * max(size.x, size.y));
    let uv = (here + pixels) / size;
    return vec4(textureSampleLevel(haze_scene, haze_sampler, uv, 0.0).rgb, 1.0);
}

// `shift` in the quad's units as pixels (the inverse of its Jacobian `dx`, `dy`), at
// most `most` long.
fn to_pixels(shift: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, most: f32) -> vec2<f32> {
    let det = dx.x * dy.y - dx.y * dy.x;
    var pixels = vec2(0.0);
    if abs(det) > 1e-8 {
        pixels = vec2(shift.x * dy.y - shift.y * dy.x, -shift.x * dx.y + shift.y * dx.x) / det;
    }
    let len = length(pixels);
    if len > most {
        pixels *= most / len;
    }
    return pixels;
}

// The black holes' quads (the lens and the shade): a skin's glint instances when it has
// holes its wearer left on; every other instance is folded away.
@vertex
fn vertex_hole(@builtin(vertex_index) vertex_index: u32, blade: BladeInstance) -> VertexOutput {
    var output = blade_vertex(vertex_index, blade);
    var hole = output.kind >= KIND_SKIN && output.hilt <= STAR && (blade.off & OFF_HOLES) == 0u;
    if hole {
        hole = skins[skin_index(output)].hole_shape.x > 0.0;
    }
    if !hole {
        output.clip_position = vec4(0.0, 0.0, 0.0, 1.0);
    }
    return output;
}

// A black hole's lens, drawn with the haze before any glow: the scene round it bent as
// a point mass bends light, what is behind it drawn into a ring (`lens` horizon radii
// past the horizon) and squeezed toward it, fading to the scene unbent at the quad's
// edge (its alpha), and weaker while it is behind the blade.
@fragment
fn fragment_lens(input: VertexOutput) -> @location(0) vec4<f32> {
    // Taken before any branch (uniform control flow).
    let dx = dpdx(input.blade);
    let dy = dpdy(input.blade);
    let size = vec2<f32>(textureDimensions(haze_scene));
    let skin = skin_at(input);
    let scale = glint_scale(STAR - input.hilt);
    let horizon = skin.hole_shape.x * scale;
    let reach = skin.hole_shape.w * scale;
    let ring = horizon * (1.0 + skin.hole_shape.y);
    let p = input.blade;
    let r = length(p);
    let fade = 1.0 - smoothstep(0.6 * reach, reach, r);
    let shift = -p / max(r, 0.0001) * (ring * ring / max(r, horizon)) * fade;
    let pixels = to_pixels(shift, dx, dy, 0.06 * max(size.x, size.y));
    let uv = (input.clip_position.xy + pixels) / size;
    let seen = textureSampleLevel(haze_scene, haze_sampler, uv, 0.0).rgb;
    return vec4(seen, fade * (1.0 - 0.6 * input.shaft));
}

// --- The seventh set (blade_skin_solids.rs): solid blades, blended after the glow -------

// The solid pass's quads: a skin's glow quad when it has ink or chrome on, for the blade
// itself (not an afterimage); every other instance is folded away.
@vertex
fn vertex_solid(@builtin(vertex_index) vertex_index: u32, blade: BladeInstance) -> VertexOutput {
    var output = blade_vertex(vertex_index, blade);
    var solid = output.kind >= KIND_SKIN && output.hilt > 0.0 && (blade.persona.x >> 24u) == 255u;
    if solid {
        let skin = skins[skin_index(output)];
        let ink = (blade.off & OFF_INK) == 0u
            && (skin.ink_color.w > 0.0 || skin.ink_shape.x > 0.0);
        let chrome = (blade.off & OFF_CHROME) == 0u && skin.chrome_color.w > 0.0;
        solid = ink || chrome;
    }
    if !solid {
        output.clip_position = vec4(0.0, 0.0, 0.0, 1.0);
    }
    return output;
}

// A liquid mirror's colour and coverage at `place` units along (unclamped) and `x` across:
// rippling, beads rolling up it, a round end; it reflects a made-up surrounding (a dark
// ground, a bright sky, a key highlight, a lit rim) in the light where the blade is.
fn solid_chrome(skin: Skin, input: VertexOutput, place: f32, x: f32, pixel: f32) -> vec4<f32> {
    let c = skin.chrome_color;
    let s = skin.chrome_shape;
    let time = input.animation.x;
    let seed = input.animation.y;
    let seeded = i32(seed * 4096.0);
    var radius = c.w * input.radius
        * (1.0 + s.x * sin(place * 1.2 - time * s.z * 0.25 + seed * 6.0));
    let span = input.length + 6.0;
    var swell = 0.0;
    for (var b = 0; b < min(i32(s.y), 8); b++) {
        let pace = s.z * (0.6 + 0.8 * skin_hash(vec2(b, 301 + seeded)));
        let at = fract(skin_hash(vec2(b, 307 + seeded)) + time * pace / span) * span - 3.0;
        swell = max(swell, exp(-pow((place - at) / 1.5, 2.0)));
    }
    radius *= 1.0 + 0.6 * swell;
    let beyond = max(place - input.length, 0.0);
    let reach = sqrt(max(radius * radius - beyond * beyond, 0.0));
    let inside = clamp((reach - abs(x)) / max(pixel, 0.0001) + 0.5, 0.0, 1.0)
        * clamp(place / max(pixel, 0.0001) + 0.5, 0.0, 1.0);
    let n = clamp(x / max(reach, 0.0001), -1.0, 1.0);
    let warp = n + 0.25 * sin(place * 0.7 - time * 1.5 + n * 2.0);
    var env = mix(vec3(0.05, 0.055, 0.07), vec3(0.8, 0.85, 0.95), smoothstep(-0.8, 0.9, warp));
    env += vec3(1.0) * exp(-pow((warp + 0.45) / 0.1, 2.0)) * 1.3;
    env += vec3(0.6, 0.7, 0.9) * pow(abs(n), 6.0) * 0.6;
    env *= c.rgb * (0.65 + 0.5 * persona_light(input));
    return vec4(env, inside);
}

// The solid pass: a skin's chrome, then its ink (a body, a glossy streak and an outline)
// over the blade's glow just drawn, alpha-blended; a chroma's ink body takes its colour.
@fragment
fn fragment_solid(input: VertexOutput) -> @location(0) vec4<f32> {
    let pixel = fwidth(input.blade.x);
    let skin = skin_at(input);
    let time = input.animation.x;
    let place = input.blade.y / max(input.shaft, 0.0001) * input.length;
    let x = input.blade.x;
    var out = vec4(0.0);
    if skin.chrome_color.w > 0.0 {
        out = solid_chrome(skin, input, place, x, pixel);
    }
    let ink = skin.ink_color;
    let shape = skin.ink_shape;
    if ink.w > 0.0 || shape.x > 0.0 {
        var radius = shape.y * input.radius;
        let beyond = place - input.length;
        // Signed distance to the body's edge (negative inside): a capsule, or drawn to a
        // point over its last `point` units; flat at the hilt.
        var d: f32;
        if shape.w > 0.0 {
            radius *= clamp((input.length - place) / shape.w, 0.0, 1.0);
            d = abs(x) - radius;
        } else {
            d = select(abs(x) - radius, length(vec2(x, beyond)) - radius, beyond > 0.0);
        }
        d = max(d, -place);
        let soft = max(pixel, 0.0001);
        let body = ink.w * clamp(-d / soft + 0.5, 0.0, 1.0);
        let ring = shape.x * clamp((0.5 * shape.z * input.radius - abs(d)) / soft + 0.5, 0.0, 1.0);
        let gloss = skin.ink_more.x * exp(-pow((x / max(radius, 0.0001) - 0.35) / 0.12, 2.0))
            * (0.5 + 0.5 * sin(place * 0.25 - time * 0.8));
        var colour = ink.rgb;
        if input.chroma != 0.0 {
            colour = skin_turn_hue(colour, input.chroma);
        }
        var rgb = mix(out.rgb, colour + vec3(gloss), body);
        var alpha = max(out.a, body);
        rgb = mix(rgb, vec3(0.0), ring);
        alpha = max(alpha, ring);
        out = vec4(rgb, alpha);
    }
    return out;
}

// A black hole's horizon, drawn after the glow: black, over the blade's glow when the
// hole is in front of it; behind the blade (and close to its axis) the glow shows.
@fragment
fn fragment_shade(input: VertexOutput) -> @location(0) vec4<f32> {
    let pixel = fwidth(input.blade.x);
    let skin = skin_at(input);
    let horizon = skin.hole_shape.x * glint_scale(STAR - input.hilt);
    let disc = clamp((horizon - length(input.blade)) / max(pixel, 0.0001) + 0.5, 0.0, 1.0);
    return vec4(0.0, 0.0, 0.0, disc * (1.0 - input.shaft));
}
