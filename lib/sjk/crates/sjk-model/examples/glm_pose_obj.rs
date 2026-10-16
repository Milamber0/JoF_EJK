//! Write a GLM posed at frames of named animations as OBJ files, beside `gla_dump`:
//! `cargo run -p sjk-model --example glm_pose_obj -- <glm> <gla> <animation.cfg> <skin> <dir> NAME:FRAME...`.
//! Positions are in model units; hidden (`*off`), cap and tag surfaces are left out.

use sjk_model::{AnimationConfig, Gla, Glm, Skin};
use std::fmt::Write as _;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [glm, gla, cfg, skin, dir, picks @ ..] = args.as_slice() else {
        return Err("usage: glm_pose_obj <glm> <gla> <cfg> <skin> <dir> NAME:FRAME...".into());
    };
    let glm = Glm::parse(&std::fs::read(glm)?)?;
    let gla = Gla::parse(&std::fs::read(gla)?)?;
    let config = AnimationConfig::parse(&std::fs::read(cfg)?)?;
    let skin = Skin::parse(&std::fs::read(skin)?);
    for pick in picks {
        let (name, frame) = pick.split_once(':').ok_or("NAME:FRAME")?;
        let sequence = config.get(name).ok_or("animation not in animation.cfg")?;
        let frame: usize = frame.parse()?;
        let surfaces = glm.skin(
            &gla,
            &skin,
            sequence.first_frame + frame.min(sequence.frame_count - 1),
            0,
        )?;
        let mut obj = String::new();
        let mut base = 1;
        for surface in surfaces {
            if surface.shader.is_none()
                || surface.name.contains("_cap_")
                || surface.name.starts_with('*')
            {
                continue;
            }
            let _ = writeln!(obj, "o {}", surface.name);
            for vertex in &surface.vertices {
                let [x, y, z] = vertex.position;
                let _ = writeln!(obj, "v {x} {y} {z}");
            }
            for [a, b, c] in &surface.triangles {
                let _ = writeln!(obj, "f {} {} {}", base + a, base + b, base + c);
            }
            base += surface.vertices.len() as u32;
        }
        std::fs::write(format!("{dir}/{name}_{frame}.obj"), obj)?;
        println!("{name}:{frame}");
    }
    Ok(())
}
