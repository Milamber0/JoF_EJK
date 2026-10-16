//! Host cost of animating the players of a recorded game: the demo `SJK_TEST_DEMO`
//! names is played off screen without frame pacing and the frame budget's phases
//! (`pose+skin+upload` is the actor evaluation, joint upload and corpse work) are
//! printed. Ignored like the other world shots; nothing is rendered to a file.

use super::*;

#[test]
#[ignore = "renders with the GPU, the installed game data and the demo SJK_TEST_DEMO names"]
fn animation_cost_on_a_demo() {
    on_big_stack(|| {
        let Some(demo) = std::env::var_os("SJK_TEST_DEMO").map(PathBuf::from) else {
            println!("SJK_TEST_DEMO is not set: skipped");
            return;
        };
        let Some(mut gpu) =
            card_outline_shots::open_demo(&demo, demo_playback::Camera::FirstPerson, [1280, 720])
        else {
            return;
        };
        for _ in 0..120 {
            let _ = gpu.render(&mut None);
        }
        let frames: usize = std::env::var("SJK_PROFILE_FRAMES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1200);
        let mut live = 0usize;
        let mut samples = Vec::with_capacity(frames);
        for _ in 0..frames {
            let _ = gpu.render(&mut None);
            live += gpu
                .actor_meshes
                .iter()
                .filter(|mesh| mesh.entity_id.is_some())
                .count();
            samples.push(gpu.frame_pacer.last);
        }
        let pose = |sample: &frame_pacing::budget::Sample| {
            sample.phases[frame_pacing::budget::Phase::Pose as usize]
        };
        let mut poses: Vec<f64> = samples.iter().map(pose).collect();
        poses.sort_by(|a, b| a.total_cmp(b));
        let mean = poses.iter().sum::<f64>() / poses.len() as f64;
        let work: f64 = samples.iter().map(|s| s.work).sum::<f64>() / samples.len() as f64;
        println!(
            "animation profile: {frames} frames, {} actor slots, mean {:.1} actors with an entity",
            gpu.actor_meshes.len(),
            live as f64 / frames as f64
        );
        println!(
            "pose+skin+upload ms: mean {mean:.3}, median {:.3}, p99 {:.3}, max {:.3}",
            poses[poses.len() / 2],
            poses[poses.len() * 99 / 100],
            poses[poses.len() - 1]
        );
        println!("whole frame host work ms: mean {work:.3}");
        let stats = gpu.frame_pacer.budget.stats();
        for (name, value) in frame_pacing::budget::NAMES.iter().zip(stats.mean_phases) {
            if value >= 0.05 {
                println!("  {name:<32} {value:.3}");
            }
        }
    });
}
