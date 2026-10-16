// Authored lighting: material maps redistribute the baked lightmap (`material_maps.wgsl`).
fn material_map_lightmap(input: VertexOutput, texel: vec4<f32>) -> vec4<f32> {
    return material_map_baked(input, texel);
}
