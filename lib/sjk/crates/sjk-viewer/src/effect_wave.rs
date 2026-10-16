//! Analytic `rgbGen wave` / `alphaGen wave` evaluation for effect layers.
//!
//! World stages sample the rd-vanilla 1024-entry tables on the GPU
//! (`world_stage::evaluate_wave` is their CPU twin); effect layers keep this
//! closed-form evaluation, clamped like `RB_CalcWaveColor`.

use super::*;

/// Wave value at `time`; `1.0` when the stage has no wave.
pub(crate) fn evaluate(wave: Option<&WaveForm>, time: f32) -> f32 {
    let Some(wave) = wave else {
        return 1.0;
    };
    let phase = wave.phase + time.max(0.0) * wave.frequency;
    let cycle = phase - phase.floor();
    let value = match wave.function.as_str() {
        "sin" => (phase * std::f32::consts::TAU).sin(),
        "square" => {
            if cycle < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        "triangle" => 1.0 - 4.0 * (cycle - 0.5).abs(),
        "sawtooth" => cycle,
        "inversesawtooth" => 1.0 - cycle,
        _ => 0.0,
    };
    (wave.base + value * wave.amplitude).clamp(0.0, 1.0)
}
