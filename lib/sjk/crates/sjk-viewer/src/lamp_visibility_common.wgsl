// Shared octahedral mapping for map-lifetime lamp visibility and its receiver.
// Unnormalized octahedral rays have unit L1 length. The static atlas stores
// biased ray parameters in this basis, avoiding normalization in each PCF tap.
fn lamp_octa_vector(p: vec2<f32>) -> vec3<f32> {
    var n = vec3(p, 1.0-abs(p.x)-abs(p.y));
    if n.z < 0.0 {
        n = vec3((1.0-abs(n.yx))*select(vec2(-1.0),vec2(1.0),n.xy>=vec2(0.0)),n.z);
    }
    return n;
}
fn lamp_octa_direction(p: vec2<f32>) -> vec3<f32> {
    return normalize(lamp_octa_vector(p));
}
fn lamp_octa_coordinates(direction: vec3<f32>) -> vec2<f32> {
    let n = direction/(abs(direction.x)+abs(direction.y)+abs(direction.z));
    if n.z < 0.0 {
        return (1.0-abs(n.yx))*select(vec2(-1.0),vec2(1.0),n.xy>=vec2(0.0));
    }
    return n.xy;
}
