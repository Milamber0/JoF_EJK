@group(1) @binding(0) var portal_image: texture_2d<f32>;
@group(1) @binding(1) var<uniform> portal_control: vec4<f32>;
@vertex fn portal_main(vertex: VertexInput,
    instance: InstanceInput) -> @builtin(position) vec4<f32> {
    return camera.view_projection * vec4(instance_position(vertex, instance), 1.0);
}
// A sky face's depth, pushed back by `SKY_FACE_PUSH` of its distance (the clip-space
// share between its depth and the far plane's). The world's absolute coordinates round
// the depth of a far face by some tenths of a percent of its distance, so a ceiling a
// unit or two under the sky brush it is built against fought with the sky (and the
// clouds over it) and flickered as the camera moved (jofmemorial, Sol's note of
// 12/10/2026). rd-vanilla draws its box at the far plane and never covers the world;
// pushed this little, the sky still covers whatever lies further behind its faces.
const SKY_FACE_PUSH: f32 = 1.0 / 32.0;

fn sky_face_clip(clip: vec4<f32>, push: f32) -> vec4<f32> {
    return vec4(clip.xy, clip.z + push * (clip.w - clip.z), clip.w);
}

// A sky portal's view on the sky faces, as far behind them as the sky stage's own faces.
@vertex fn sky_portal_main(vertex: VertexInput,
    instance: InstanceInput) -> @builtin(position) vec4<f32> {
    return sky_face_clip(camera.view_projection * vec4(instance_position(vertex, instance), 1.0),
        SKY_FACE_PUSH);
}
@fragment fn sample_view(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = textureDimensions(portal_image);
    var pixel = vec2<i32>(position.xy);
    if portal_control.x != 0.0 { pixel.x = i32(size.x) - pixel.x - 1; }
    return textureLoad(portal_image, pixel, 0);
}
