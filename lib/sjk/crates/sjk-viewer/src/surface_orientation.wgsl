// Orient smooth shading normals by the actual triangle, not by their own view dot.
// A smooth normal can point behind the eye on a still-visible hillside. Flipping it
// there creates a false dark band that follows the camera. Triangle derivatives
// keep the authored hemisphere until the viewer crosses the geometric surface.
fn surface_normal(world: vec3<f32>, normal: vec3<f32>, eye: vec3<f32>) -> vec3<f32> {
    let geometric = cross(dpdx(world), dpdy(world));
    let reverse = dot(geometric, normal) * dot(geometric, eye - world) < 0.0;
    return select(normal, -normal, reverse);
}
