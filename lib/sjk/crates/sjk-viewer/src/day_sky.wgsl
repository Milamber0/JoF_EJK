// Same absolute presentation clock as the CPU solar orbit; no temporal history.
fn day_hour() -> f32 {
    var hour = day_controls.clock.x;
    if day_controls.clock.y >= 1.0 { hour += camera.shader_time*0.4/day_controls.clock.y; }
    return hour - floor(hour/24.0)*24.0;
}
fn day_tint(altitude: f32) -> vec3<f32> {
    let daylight = clamp(altitude/0.25,0.0,1.0);
    let white = clamp((altitude-0.1)/0.6,0.0,1.0);
    let day = mix(vec3(1.0,0.42,0.16),vec3(1.0),white);
    return mix(vec3(0.018,0.035,0.085),day,daylight);
}
// The authored sky under the hour's tint, plus a twilight glow low on the sun's side of
// the horizon (day_controls.clock.zw is the authored sun azimuth; the sun sets opposite).
fn day_sky(texel: vec3<f32>, direction: vec3<f32>) -> vec3<f32> {
    let hour = day_hour();
    var altitude = sin((hour-6.0)*3.14159265359/12.0);
    var azimuth = day_controls.clock.zw;
    if hour >= 12.0 { azimuth = -azimuth; }
    if day_controls.sun.w > 0.0 {
        altitude = mix(altitude, day_controls.sun.z, day_controls.sun.w);
        azimuth = mix(azimuth, normalize(day_controls.sun.xy + vec2(1e-5, 0.0)), day_controls.sun.w);
    }
    let flat = normalize(direction.xy + vec2(1e-5, 0.0));
    let toward = 0.3 + 0.7*pow(max(dot(flat, azimuth), 0.0), 2.0);
    let horizon = pow(clamp(1.0 - abs(direction.z), 0.0, 1.0), 6.0);
    let twilight = clamp(1.0 - abs(altitude - 0.05)/0.3, 0.0, 1.0);
    let glow = vec3(1.0, 0.42, 0.14)*horizon*toward*twilight*0.8;
    return (texel*day_tint(altitude) + glow)*sky_radiance;
}
