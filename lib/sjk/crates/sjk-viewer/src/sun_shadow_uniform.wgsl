// Receiver uniform shared by the world receiver, the model program, the light pass and the
// stage hook; `realtime` = (light scale, light-buffer scale x, y, debug bits).
struct Shadow {
    vp: mat4x4<f32>, sun: vec4<f32>, quality: vec4<f32>,
    ambient: vec4<f32>, radiance: vec4<f32>,
    far_vp: mat4x4<f32>, far_quality: vec4<f32>,
    close_vp: mat4x4<f32>, close_quality: vec4<f32>,
    realtime: vec4<f32>,
    fill: vec4<f32>,
    readability: vec4<f32>, // indirect gain, fill occlusion fraction, reserved
};
