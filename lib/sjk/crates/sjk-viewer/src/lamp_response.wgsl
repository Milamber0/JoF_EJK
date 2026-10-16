// Expose direct fixture lighting before the display shoulder. Legacy compiler
// flux can otherwise push lit and partially shadowed surfaces toward the same
// ceiling, washing out shadows. Source transport and visible inserts are separate.
// RGB shares one scale; dim lighting retains a linear response.
fn lamp_response(irradiance: vec3<f32>) -> vec3<f32> {
    let exposed = irradiance*0.35;
    let peak = max(exposed.r,max(exposed.g,exposed.b));
    let knee = 0.25;
    let headroom = 2.0 - knee;
    if peak <= knee { return exposed; }
    let excess = peak - knee;
    let mapped = knee + excess/(1.0 + excess/headroom);
    return exposed*(mapped/peak);
}
