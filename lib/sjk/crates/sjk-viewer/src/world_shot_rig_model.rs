//! World shots of a rig model ([`crate::rig_model`]) on a recorded game: the demo
//! `SJK_TEST_DEMO` names is played without a server, one player is given the
//! appearance `SJK_TEST_RIG` names (a model folder holding `sjk_rig.cfg`) through the
//! mid-match model change path, and the camera orbits them. Ignored like the other
//! world shots; skipped when either variable is not set.

use super::*;

#[test]
#[ignore = "renders with the GPU, the installed game data and the demo SJK_TEST_DEMO names"]
fn rig_model_on_a_demo() {
    on_big_stack(|| {
        let (Some(demo), Some(rig)) = (
            std::env::var_os("SJK_TEST_DEMO").map(PathBuf::from),
            std::env::var("SJK_TEST_RIG").ok(),
        ) else {
            println!("SJK_TEST_DEMO or SJK_TEST_RIG is not set: skipped");
            return;
        };
        let probe = demo_playback::Session::open(&demo, demo_playback::Camera::FirstPerson, 60)
            .expect("the demo");
        let own = probe.latest_snapshot().player.client_num();
        let players: Vec<u16> = probe
            .latest_snapshot()
            .entities
            .iter()
            .map(|entity| entity.number())
            .filter(|number| *number < 32 && *number != own)
            .collect();
        let Some(&target) = players.first() else {
            println!("no other player in the demo's first snapshot: skipped");
            return;
        };
        let Some(mut gpu) = card_outline_shots::open_demo(
            &demo,
            demo_playback::Camera::Orbit(target),
            [2560, 1920],
        ) else {
            return;
        };
        let _ = frame(&mut gpu, 10);
        let appearance = Appearance {
            model: format!("models/players/{rig}"),
            variant: "default".to_owned(),
        };
        let entity = EntityId::new(u64::from(target) + 1);
        let names = [Some("single_1".to_owned()), None];
        let mesh = gpu
            .build_live_actor(&appearance, entity, names)
            .expect("the rig model loads and uploads");
        assert!(mesh.preview.rig.is_some(), "{rig} is not a rig model");
        gpu.place_client_mesh(target, mesh, &appearance);
        let _ = frame(&mut gpu, 5);
        for mesh in gpu
            .actor_meshes
            .iter()
            .filter(|mesh| mesh.entity_id.is_some())
        {
            let id = mesh.entity_id.map(|id| id.get() - 1);
            let at = mesh
                .entity_id
                .and_then(|id| gpu.demo_session.as_ref()?.world().entity(id))
                .map(|entity| entity.sample(0).translation);
            println!(
                "actor {id:?} {} rig={} draws={} palette={} requested={} at {at:?}",
                mesh.appearance.model,
                mesh.preview.rig.is_some(),
                mesh.draws.len(),
                mesh.gpu_palette.is_some(),
                mesh.animator.requested.is_some(),
            );
        }
        println!("camera {:?} target {target}", gpu.camera_position);
        // SJK_TEST_EMOTE: the player plays that emote of the installed catalogue (a
        // rig clip), shots taken while it runs.
        let emote = std::env::var("SJK_TEST_EMOTE").ok();
        if let Some(id) = &emote {
            let mut state = crate::emotes::lock();
            let catalogue = crate::emotes::load(gpu.vfs.as_deref().expect("the game's files"));
            let def = catalogue.iter().find(|def| &def.id == id).cloned();
            println!("emote {id}: {def:?}");
            let slot = u8::try_from(target).expect("a player slot");
            let at = gpu
                .demo_session
                .as_ref()
                .and_then(|session| session.world().entity(entity))
                .map(|entity| entity.current().transform.translation)
                .unwrap_or_default();
            state.catalogue = Some(catalogue);
            state
                .active
                .start(slot, id, def.as_ref(), at, std::time::Instant::now());
        }
        let between = if emote.is_some() { 6 } else { 50 };
        // Close-ups about a second apart: the orbit keeps the player near the centre.
        let mut shots = Vec::new();
        for _ in 0..12 {
            let image = frame(&mut gpu, between);
            let (width, height) = image.dimensions();
            let side = height / 2;
            shots.push(
                image::imageops::crop_imm(&image, (width - side) / 2, height / 4, side, side)
                    .to_image(),
            );
        }
        let name = if emote.is_some() {
            "rig_model_emote"
        } else {
            "rig_model_orbit"
        };
        println!("{}", sheet(&shots, 4, 480, name).display());
    });
}
