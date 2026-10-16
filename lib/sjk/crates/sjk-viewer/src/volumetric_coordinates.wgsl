// Shared froxel parameter ABI and depth mapping. Each consumer declares `p`.
struct Parameters {
    inverse: mat4x4<f32>, shadow: mat4x4<f32>, eye: vec4<f32>, forward: vec4<f32>,
    sun: vec4<f32>, color: vec4<f32>, grid: vec4<u32>, range: vec4<f32>,
    close: mat4x4<f32>, close_range: vec4<f32>,
    far: mat4x4<f32>, far_range: vec4<f32>,
};

// Keep every original near slice; extra slices cover the distant part independently.
fn slice_depth(z: f32) -> f32 {
    let layer = z*f32(p.grid.z);
    let near_layers = p.far_range.w;
    if layer <= near_layers || p.far_range.y == 0.0 {
        return p.range.x*pow(p.range.y/p.range.x,layer/near_layers);
    }
    return p.range.y*pow(p.far_range.z/p.range.y,
        (layer-near_layers)/(f32(p.grid.z)-near_layers));
}
fn depth_layer(distance: f32) -> f32 {
    if distance <= p.range.y || p.far_range.y == 0.0 {
        return log(distance/p.range.x)/log(p.range.y/p.range.x)*p.far_range.w;
    }
    return p.far_range.w + log(distance/p.range.y)/log(p.far_range.z/p.range.y)
        *(f32(p.grid.z)-p.far_range.w);
}
