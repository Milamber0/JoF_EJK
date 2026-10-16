// Shared by depth-only and buffered SSAO. One restores the original response;
// zero is neutral, and larger values strengthen contacts smoothly.
@group(2) @binding(0) var<uniform> ssao_strength: vec4<f32>;
fn ssao_visibility(obscurance: f32) -> f32 {
    return pow(clamp(1.0-obscurance,0.0,1.0),ssao_strength.x);
}
