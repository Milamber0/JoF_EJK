//! Bindable shot commands; parsing allocates only when a command is issued.
use super::{Director, motion::Request};

const CAMERA: &str = "demo_camera front|back|left|right [seconds]; yaw pitch range height [seconds]; orbit degrees/sec; stop; reset [seconds]";
const SUN: &str = "demo_sun azimuth elevation [seconds]; orbit degrees/sec; stop; auto [seconds]. Requires r_dayNight 1 at launch";

/// Advertise local commands in console completion and help.
pub(in crate::console) fn register(
    shell: &mut sjk_shell::Shell,
) -> Result<(), Box<dyn std::error::Error>> {
    for (name, help) in [("demo_camera", CAMERA), ("demo_sun", SUN)] {
        shell.commands.register(name, help, |_| Ok(Vec::new()))?;
    }
    Ok(())
}

fn number(value: &str, min: f32, max: f32) -> Result<f32, String> {
    value
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite() && *v >= min && *v <= max)
        .ok_or_else(|| format!("Expected a finite number in {min}..{max}, got {value}"))
}
fn duration(args: &[String]) -> Result<f64, String> {
    match args {
        [] => Ok(2.),
        [value] => number(value, 0., 3600.).map(f64::from),
        _ => Err("Expected one optional duration in seconds".into()),
    }
}
fn shared<const N: usize>(args: &[String]) -> Result<Option<Request<N>>, String> {
    if args[0].eq_ignore_ascii_case("reset") || args[0].eq_ignore_ascii_case("auto") {
        return Ok(Some(Request::Auto(duration(&args[1..])?)));
    }
    if args[0].eq_ignore_ascii_case("stop") && args.len() == 1 {
        return Ok(Some(Request::Stop));
    }
    if args[0].eq_ignore_ascii_case("orbit") && args.len() == 2 {
        return Ok(Some(Request::Orbit(number(&args[1], -90., 90.)?)));
    }
    Ok(None)
}

impl Director {
    /// Consume locally, including malformed commands; never forward camera
    /// control to servers.
    pub(crate) fn command(&mut self, tokens: &[String]) -> Option<Result<Vec<String>, String>> {
        let name = tokens.first()?;
        let camera = name.eq_ignore_ascii_case("demo_camera");
        if !camera && !name.eq_ignore_ascii_case("demo_sun") {
            return None;
        }
        let args = &tokens[1..];
        if args.is_empty() {
            return Some(Ok(vec![if camera { CAMERA } else { SUN }.into()]));
        }
        Some((|| {
            if camera {
                let request = if let Some(request) = shared(args)? {
                    request
                } else if let Some((_, angle)) = [
                    ("back", 0.),
                    ("front", 180.),
                    ("left", -90.),
                    ("right", 90.),
                ]
                .into_iter()
                .find(|(name, _)| args[0].eq_ignore_ascii_case(name))
                {
                    Request::Angle(angle, duration(&args[1..])?)
                } else if (4..=5).contains(&args.len()) {
                    Request::Target(
                        [
                            number(&args[0], -36000., 36000.)?.rem_euclid(360.),
                            number(&args[1], -85., 85.)?,
                            number(&args[2], 8., 4096.)?,
                            number(&args[3], -1024., 1024.)?,
                        ],
                        duration(&args[4..])?,
                    )
                } else {
                    return Err(CAMERA.into());
                };
                self.set_camera(request);
            } else {
                let request = if let Some(request) = shared(args)? {
                    request
                } else if (2..=3).contains(&args.len()) {
                    Request::Target(
                        [
                            number(&args[0], -36000., 36000.)?.rem_euclid(360.),
                            number(&args[1], -90., 90.)?,
                        ],
                        duration(&args[2..])?,
                    )
                } else {
                    return Err(SUN.into());
                };
                self.set_sun(request);
            }
            Ok(Vec::new())
        })())
    }
}
