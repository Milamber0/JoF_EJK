//! Stock two-key held state and partial-frame accounting (cl_input.cpp:391-510).
#[derive(Clone, Copy, Default)]
pub(super) struct KeyState {
    keys: [Option<u64>; 2],
    down_at: u64,
    /// Time of the press that made the key active; unlike `down_at`, sampling
    /// leaves it alone, as `cl_idrive` leaves EJK's `downtime` (cl_input.cpp:499).
    pub pressed_at: u64,
    /// Time of the release that made the key inactive; `cl_idrive`'s delay counts from
    /// it when the key that won a pair is let go.
    pub released_at: u64,
    elapsed: u64,
    pub command_elapsed: u64,
    pub active: bool,
    pub pressed: bool,
    pub fraction: f32,
}

/// Key id of a hold typed at the console without one (`IN_KeyDown`'s `k = -1`,
/// cl_input.cpp:403): it lasts until a matching command without a key.
const TYPED: u64 = u64::MAX;

impl KeyState {
    pub fn event(&mut self, down: bool, key: Option<u64>, time: u64) -> bool {
        if down {
            let key = key.unwrap_or(TYPED);
            if self.keys.contains(&Some(key)) {
                return false;
            }
            let Some(slot) = self.keys.iter_mut().find(|slot| slot.is_none()) else {
                return false;
            };
            *slot = Some(key);
            if self.active {
                return false;
            }
            self.active = true;
            self.pressed = true;
            self.down_at = time;
            self.pressed_at = time;
            self.fraction = 1.0;
            true
        } else {
            if let Some(key) = key {
                for slot in &mut self.keys {
                    if *slot == Some(key) {
                        *slot = None;
                    }
                }
                if self.keys.iter().any(Option::is_some) {
                    return false;
                }
            } else {
                self.keys = [None; 2];
            }
            if !self.active {
                return false;
            }
            self.elapsed += time.saturating_sub(self.down_at);
            self.released_at = time;
            self.active = false;
            self.fraction = 0.0;
            true
        }
    }

    /// `Key_ClearStates` (cl_keys.cpp:1606) releases every physical key when a
    /// console, menu or chat catcher takes the keyboard; a typed hold survives.
    pub fn release_keys(&mut self) {
        if self.keys.contains(&Some(TYPED)) {
            for slot in &mut self.keys {
                if *slot != Some(TYPED) {
                    *slot = None;
                }
            }
        } else {
            *self = Self::default();
        }
    }

    pub fn sample(&mut self, now: u64, millis: u64) {
        let mut elapsed = std::mem::take(&mut self.elapsed);
        if self.active {
            elapsed += now.saturating_sub(self.down_at);
            self.down_at = now;
        }
        self.fraction = (elapsed as f32 / millis.max(1) as f32).clamp(0.0, 1.0);
        self.command_elapsed += elapsed.min(millis);
    }
}

#[cfg(test)]
mod tests {
    use super::KeyState;

    #[test]
    fn released_keys_keep_a_typed_hold() {
        let mut state = KeyState::default();
        state.event(true, None, 10);
        state.event(true, Some(4), 20);
        state.release_keys();
        assert!(state.active);
        // The key is gone: its own release no longer matters, the typed `-` does.
        assert!(!state.event(false, Some(4), 30));
        assert!(state.active);
        assert!(state.event(false, None, 40));
        assert!(!state.active);
    }

    #[test]
    fn released_keys_drop_a_key_hold() {
        let mut state = KeyState::default();
        state.event(true, Some(4), 10);
        state.release_keys();
        assert!(!state.active);
        assert!(!state.pressed);
    }
}
