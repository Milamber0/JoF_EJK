// Sample LOCAL scattering at the mote, never the eye-to-surface beam integral:
// a beam elsewhere along the same screen ray must not light dust in dark air.
@group(2) @binding(0) var<uniform> p: Parameters;
@group(2) @binding(1) var beam_source: texture_3d<f32>;
@group(2) @binding(2) var beam_means: texture_3d<f32>;
@group(2) @binding(3) var beam_sampler: sampler;

// RGB is the local beam tint, alpha its smooth visibility weight.
fn dust_beam(point: vec3<f32>, clip: vec4<f32>) -> vec4<f32> {
    if clip.w <= 0.0 { return vec4(0.0); }
    let uv = clip.xy/clip.w*vec2(0.5,-0.5)+0.5;
    let axial = dot(point-p.eye.xyz,p.forward.xyz);
    if any(uv <= vec2(0.0)) || any(uv >= vec2(1.0))
        || axial <= p.range.x || axial >= p.far_range.z { return vec4(0.0); }
    let at = vec3(uv, depth_layer(axial)/f32(p.grid.z));
    let source = textureSampleLevel(beam_source,beam_sampler,at,0.0).rgb;
    let wide = textureSampleLevel(beam_means,beam_sampler,at,0.0).rgb;
    // Match the godray integrator's clarity subtraction. Uniform sunlit air at
    // clarity 1 and shadowed cells have no positive beam contrast.
    let radiance = max(source-p.range.z*wide,vec3(0.0))/max(p.color.w,1e-8);
    let luminance = dot(radiance,vec3(0.2126,0.7152,0.0722));
    // A soft radiance threshold avoids amplifying half-float noise or faint haze.
    let weight = smoothstep(0.005,0.05,luminance);
    let peak = max(max(radiance.r,radiance.g),radiance.b);
    return vec4(radiance/max(peak,1e-8),weight);
}
