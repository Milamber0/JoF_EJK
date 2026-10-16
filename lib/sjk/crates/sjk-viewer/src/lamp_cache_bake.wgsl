// Lamp light cache bake (`lamp_cache.rs`): world surfaces rasterized in their lightmap
// coordinates. Depth carries an oblique projection of the world position, so a nearest
// and a farthest pass expose texels that two separate surfaces share.
struct Bake { project: vec4<f32>, limits: vec4<f32> };
@group(0) @binding(7) var<uniform> bake: Bake;
struct BakeInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(4) lightmap: vec2<f32>,
};
struct BakeOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
};
@vertex fn bake_vertex(input: BakeInput) -> BakeOutput {
    let along = dot(input.position, bake.project.xyz) + bake.project.w;
    return BakeOutput(vec4(input.lightmap.x*2.0 - 1.0, 1.0 - input.lightmap.y*2.0, along, 1.0),
        input.position, input.normal);
}
