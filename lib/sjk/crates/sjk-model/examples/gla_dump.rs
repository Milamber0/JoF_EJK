//! Dump a GLA's bones and some animations as JSON, for looking at them in Blender:
//! `cargo run -p sjk-model --example gla_dump -- <gla> <animation.cfg> <out.json> NAME...`.
//! Each frame holds every bone's model-space matrix (`frame_matrices`: the matrix that
//! moves a base-pose vertex), so a bone's posed transform is that times its base pose.

use sjk_model::{AnimationConfig, Gla};
use std::fmt::Write as _;

fn matrix(out: &mut String, m: &[[f32; 4]; 3]) {
    out.push('[');
    for (index, value) in m.iter().flatten().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "{value:.6}");
    }
    out.push(']');
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [gla, cfg, output, names @ ..] = args.as_slice() else {
        return Err("usage: gla_dump <gla> <animation.cfg> <out.json> NAME...".into());
    };
    let gla = Gla::parse(&std::fs::read(gla)?)?;
    let config = AnimationConfig::parse(&std::fs::read(cfg)?)?;
    let mut out = String::from("{\"bones\":[");
    for (index, bone) in gla.bones.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let parent = bone.parent.map_or(-1, |parent| parent as i64);
        let _ = write!(
            out,
            "{{\"name\":\"{}\",\"parent\":{parent},\"base\":",
            bone.name
        );
        matrix(&mut out, &bone.base_pose);
        out.push('}');
    }
    out.push_str("],\"animations\":[");
    let mut first = true;
    for name in names {
        let Some(sequence) = config.get(name) else {
            eprintln!("{name}: not in animation.cfg");
            continue;
        };
        if !first {
            out.push(',');
        }
        first = false;
        let _ = write!(
            out,
            "{{\"name\":\"{name}\",\"fps\":{},\"frames\":[",
            sequence.frames_per_second
        );
        for frame in 0..sequence.frame_count {
            if frame > 0 {
                out.push(',');
            }
            out.push('[');
            let matrices = gla.frame_matrices(sequence.first_frame + frame)?;
            for (index, m) in matrices.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                matrix(&mut out, m);
            }
            out.push(']');
        }
        out.push_str("]}");
        println!(
            "{name}: {} frames at {} fps",
            sequence.frame_count, sequence.frames_per_second
        );
    }
    out.push_str("]}");
    std::fs::write(output, out)?;
    Ok(())
}
