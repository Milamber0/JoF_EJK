// Eye adaptation (`post_exposure.rs`): meter the finished scene, then move the exposure
// the next frame's resolve uses towards a target, entirely on the GPU.
//
// `meter` builds a centre-weighted histogram of log2 luminance from 1/8-resolution cells
// of the scene target, before exposure, effects and HUD, so its result never feeds back
// into its input. `adapt` (one thread) averages the histogram between the 30th and 97th
// percentiles, which ignores dark corners and small bright sources (sky glints, lamps),
// turns that into a target in EV, clamps it to the player's range and smooths towards it.

// Bin 0 holds near-black cells (cleared frames, voids), which never count as metering;
// bins 1..63 cover LOG_MIN..LOG_MIN + LOG_RANGE (log2 linear luminance).
override LOG_MIN: f32 = -12.0;
override LOG_RANGE: f32 = 16.0;
// The scene stores display (sRGB-encoded) values: r_sceneHdr 0.
override DECODE: bool = false;

const BINS: u32 = 64u;
const CELL: u32 = 8u;

// Kept in step with `SceneExposure` in post_hdr.wgsl and `State` in post_exposure.rs.
struct State {
    // Linear multiplier the resolve applies: r_hdrExposure × 2^adapt.
    exposure: f32,
    // Current adaptation in EV, within [min_ev, max_ev].
    adapt: f32,
    // Last valid metering: mean log2 luminance of the percentile window.
    metered: f32,
    // Nonzero: the next valid metering snaps to its target instead of smoothing.
    pending: u32,
}

// Kept in step with `Params` in post_exposure.rs.
struct Params {
    base: f32,
    min_ev: f32,
    max_ev: f32,
    log_key: f32,
    // Smoothing fractions for this frame, 1 - exp(-dt / tau), computed on the CPU.
    to_bright: f32,
    to_dark: f32,
    // 0 inactive (hold base, snap later), 1 frozen, 2 metering, 3 metering and snap.
    mode: u32,
    _padding: u32,
}

@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var linear_clamp: sampler;
@group(0) @binding(2) var<storage, read_write> histogram: array<atomic<u32>, 64>;
@group(0) @binding(3) var<storage, read_write> state: State;
@group(0) @binding(4) var<uniform> params: Params;

var<workgroup> local_bins: array<atomic<u32>, 64>;

fn decode(c: vec3<f32>) -> vec3<f32> {
    return select(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), c > vec3(0.04045));
}

fn bin_of(luminance: f32) -> u32 {
    // NaN fails the comparison and lands in the ignored bin.
    if !(luminance >= exp2(LOG_MIN)) { return 0u; }
    let unit = (log2(luminance) - LOG_MIN) / LOG_RANGE;
    return 1u + u32(clamp(unit * f32(BINS - 1u), 0.0, f32(BINS - 2u)));
}

// Centre of bin `index` (1..63) in log2 luminance.
fn bin_centre(index: u32) -> f32 {
    return LOG_MIN + (f32(index) - 0.5) * LOG_RANGE / f32(BINS - 1u);
}

@compute @workgroup_size(16, 16)
fn meter(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_index) local: u32,
) {
    if local < BINS { atomicStore(&local_bins[local], 0u); }
    workgroupBarrier();
    let size = textureDimensions(scene);
    let cells = (size + vec2(CELL - 1u)) / CELL;
    if all(id.xy < cells) {
        let texel = 1.0 / vec2<f32>(size);
        let origin = vec2<f32>(id.xy * CELL);
        var sum = 0.0;
        // Four bilinear taps on texel corners: 16 of the cell's 64 pixels.
        for (var i = 0u; i < 4u; i++) {
            let offset = vec2(f32(i & 1u), f32(i >> 1u)) * 4.0 + 2.0;
            var c = textureSampleLevel(scene, linear_clamp, (origin + offset) * texel, 0.0).rgb;
            c = max(c, vec3(0.0));
            if DECODE { c = decode(min(c, vec3(1.0))); }
            sum += dot(c, vec3(0.2126, 0.7152, 0.0722));
        }
        // Centre weighting 4..1: what the player looks at matters most.
        var p = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(cells) - 0.5;
        p.x *= f32(size.x) / f32(max(size.y, 1u));
        let weight = 1u + u32(round(3.0 * (1.0 - smoothstep(0.1, 0.6, length(p)))));
        atomicAdd(&local_bins[bin_of(sum * 0.25)], weight);
    }
    workgroupBarrier();
    if local < BINS {
        let count = atomicLoad(&local_bins[local]);
        if count != 0u { atomicAdd(&histogram[local], count); }
    }
}

@compute @workgroup_size(1)
fn adapt() {
    // Read and clear: the next frame's metering starts from an empty histogram.
    var counts: array<u32, 64>;
    var total = 0u;
    for (var i = 0u; i < BINS; i++) {
        counts[i] = atomicLoad(&histogram[i]);
        atomicStore(&histogram[i], 0u);
        if i != 0u { total += counts[i]; }
    }
    var s = state;
    let mode = params.mode;
    // A black or almost black frame (cleared target, fade) never moves the exposure.
    let valid = total >= 64u && f32(total) >= 0.05 * f32(total + counts[0]);
    if mode == 0u {
        s.adapt = 0.0;
        s.pending = 1u;
    } else if mode >= 2u && valid {
        let low = 0.30 * f32(total);
        let high = 0.97 * f32(total);
        var below = 0.0;
        var sum = 0.0;
        for (var i = 1u; i < BINS; i++) {
            let n = f32(counts[i]);
            let overlap = min(below + n, high) - max(below, low);
            if overlap > 0.0 { sum += overlap * bin_centre(i); }
            below += n;
        }
        s.metered = sum / (high - low);
        let goal = clamp(params.log_key - s.metered, params.min_ev, params.max_ev);
        if mode == 3u || s.pending != 0u {
            s.adapt = goal;
            s.pending = 0u;
        } else {
            // Exposure falling means the view got brighter: the faster time constant.
            let rate = select(params.to_dark, params.to_bright, goal < s.adapt);
            s.adapt += (goal - s.adapt) * rate;
        }
    } else if mode == 3u {
        s.pending = 1u;
    }
    s.adapt = clamp(s.adapt, params.min_ev, params.max_ev);
    // Exactly the base at 0 EV, so r_autoExposure 0 reproduces the fixed exposure.
    s.exposure = select(params.base * exp2(s.adapt), params.base, s.adapt == 0.0);
    state = s;
}
