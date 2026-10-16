struct Attributes {
    @location(0) world: vec4<f32>,
    @location(1) normal: vec4<f32>,
};
fn receiver_attributes(input: LightOutput) -> Attributes {
    let normal = surface_normal(input.world, normalize(input.normal), camera.camera_position);
    // These derivatives must come from this primitive, including its helper lanes.
    // Recomputing them across fullscreen texels would cross surface boundaries.
    let visibility = sun_visibility(input.world, normal, camera.camera_position, camera.view_forward);
    return Attributes(vec4(input.world, visibility.x), vec4(normal, visibility.y));
}
@fragment fn attributes(input: LightOutput) -> Attributes {
    return receiver_attributes(input);
}
// With a lamp cache the receiver also keeps where its cached lamp light lies. A face
// seen from behind is lit on its far side, which the cache never baked.
struct CachedAttributes {
    @location(0) world: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) cache: vec4<f32>,
};
@fragment fn attributes_cached(input: LightOutput) -> CachedAttributes {
    let plain = receiver_attributes(input);
    let front = dot(plain.normal.xyz, input.normal) >= 0.0;
    let page = select(0.0, input.cache_page, front);
    return CachedAttributes(plain.world, plain.normal, vec4(input.cache_uv, page, 0.0));
}
