//! Reuse a settled probe display only while every shading input is unchanged.
use super::Params;
use std::cell::Cell;

pub(super) struct Idle {
    previous: Cell<Option<Params>>,
    reference: Cell<bool>,
}
impl Idle {
    pub fn new() -> Self {
        Self {
            previous: Cell::new(None),
            reference: Cell::new(std::env::var("SJK_PROBE_IDLE_CACHE").as_deref() == Ok("0")),
        }
    }
    /// Refreshes always execute. With no refresh, identical inputs already have
    /// the same display coefficients; counters are not shading inputs.
    pub fn reuse(&self, params: Params, refresh_count: u32) -> bool {
        let Some(mut old) = self.previous.replace(Some(params)) else {
            return false;
        };
        if self.reference.get() || refresh_count != 0 {
            return false;
        }
        let mut current = params;
        old.window = [0; 4];
        current.window = [0; 4];
        old.origin_index[3] = 0;
        current.origin_index[3] = 0;
        bytemuck::bytes_of(&old) == bytemuck::bytes_of(&current)
    }
}
