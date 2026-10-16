//! Fixed-size presentation tracks, sampled from absolute elapsed time.
/// One pending edit to a fixed-size shot track.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Request<const N: usize> {
    Target([f32; N], f64),
    Angle(f32, f64),
    Auto(f64),
    Orbit(f32),
    Stop,
}

#[derive(Clone, Copy)]
struct Transition<const N: usize> {
    from: [f32; N],
    to: Option<[f32; N]>,
    start: f64,
    seconds: f64,
    auto_yaw: f32,
}

/// Held pose, transition or orbit with no frame-time allocation.
pub(super) struct Motion<const N: usize> {
    held: Option<[f32; N]>,
    transition: Option<Transition<N>>,
    orbit: Option<(f64, f32, [f32; N])>,
    pub(super) pending: Option<Request<N>>,
}

impl<const N: usize> Default for Motion<N> {
    fn default() -> Self {
        Self {
            held: None,
            transition: None,
            orbit: None,
            pending: None,
        }
    }
}

/// Signed shortest rotation, with a deterministic choice at 180 degrees.
pub(super) fn angle_delta(from: f32, to: f32) -> f32 {
    (to - from + 180.).rem_euclid(360.) - 180.
}

impl<const N: usize> Motion<N> {
    /// Whether the latest request or running transition releases manual control.
    pub(super) fn returning(&self) -> bool {
        match self.pending {
            Some(Request::Auto(_)) => true,
            Some(_) => false,
            None => self.transition.is_some_and(|t| t.to.is_none()),
        }
    }

    pub(super) fn active(&self) -> bool {
        self.held.is_some()
            || self.transition.is_some()
            || self.orbit.is_some()
            || self.pending.is_some()
    }

    pub(super) fn sample(&mut self, fallback: [f32; N], now: f64) -> Option<[f32; N]> {
        let current = self.evaluate(fallback, now);
        if let Some(request) = self.pending.take() {
            let from = current.unwrap_or(fallback);
            self.transition = None;
            self.orbit = None;
            self.held = Some(from);
            let (to, seconds) = match request {
                Request::Stop => return self.held,
                Request::Orbit(speed) => {
                    self.orbit = Some((now, speed, from));
                    return self.held;
                }
                Request::Auto(seconds) => (None, seconds),
                Request::Target(to, seconds) => (Some(to), seconds),
                Request::Angle(angle, seconds) => {
                    let mut to = from;
                    to[0] = angle;
                    (Some(to), seconds)
                }
            };
            if seconds == 0. {
                self.held = to;
            } else {
                self.transition = Some(Transition {
                    from,
                    to,
                    start: now,
                    seconds,
                    auto_yaw: from[0] + angle_delta(from[0], fallback[0]),
                });
            }
        }
        self.evaluate(fallback, now)
    }

    fn evaluate(&mut self, fallback: [f32; N], now: f64) -> Option<[f32; N]> {
        if let Some((start, speed, mut pose)) = self.orbit {
            pose[0] =
                (f64::from(pose[0]) + (now - start) * f64::from(speed)).rem_euclid(360.) as f32;
            return Some(pose);
        }
        if let Some(mut t) = self.transition {
            let u = ((now - t.start) / t.seconds).clamp(0., 1.) as f32;
            if u >= 1. {
                self.transition = None;
                self.held = t.to;
                return self.held;
            }
            // Zero velocity at either end; no frame-rate-dependent integration.
            let eased = u * u * (3. - 2. * u);
            let mut to = t.to.unwrap_or(fallback);
            let yaw_delta = if t.to.is_none() {
                t.auto_yaw += angle_delta(t.auto_yaw, to[0]);
                to[0] = t.auto_yaw;
                self.transition = Some(t);
                to[0] - t.from[0]
            } else {
                angle_delta(t.from[0], to[0])
            };
            let mut value = std::array::from_fn(|i| t.from[i] + (to[i] - t.from[i]) * eased);
            value[0] = t.from[0] + yaw_delta * eased;
            return Some(value);
        }
        self.held
    }
}
