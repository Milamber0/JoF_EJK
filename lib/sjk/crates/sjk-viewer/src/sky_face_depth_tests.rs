//! The sky faces' depth push (`sky_face_clip` in sky_stage.wgsl, clouds.wgsl and
//! scene_view.wgsl): a ceiling built a unit or two under its sky brush must stay in front
//! of the sky and the clouds, whatever the absolute world coordinates round.

use glam::{Mat4, Vec3, Vec4};

/// `SKY_FACE_PUSH` in the three programs.
const PUSH: f32 = 1.0 / 32.0;

/// `sky_face_clip`: depth moved `push` of the way towards the far plane.
fn pushed(clip: Vec4, push: f32) -> Vec4 {
    Vec4::new(clip.x, clip.y, clip.z + push * (clip.w - clip.z), clip.w)
}

#[test]
fn every_sky_program_pushes_its_faces_alike() {
    let constant = "const SKY_FACE_PUSH: f32 = 1.0 / 32.0;";
    for (name, source) in [
        ("sky_stage.wgsl", include_str!("sky_stage.wgsl")),
        ("clouds.wgsl", include_str!("clouds.wgsl")),
        ("scene_view.wgsl", include_str!("scene_view.wgsl")),
    ] {
        assert!(
            source.contains(constant),
            "{name} pushes its sky faces by 1/32"
        );
    }
    let sky = include_str!("sky_stage.wgsl");
    assert_eq!(sky.matches("SKY_FACE_PUSH)").count(), 2, "faces and mask");
    // The clouds stay between the face and the pushed sky, so they pass on its pixels.
    assert!(include_str!("clouds.wgsl").contains("SKY_FACE_PUSH * 0.5);"));
    assert!(include_str!("scene_view.wgsl").contains("fn sky_portal_main("));
}

/// jofmemorial (Sol's note of 12/10/2026): a ceiling at z 832 under the sky brush at
/// 834, 2328 units up from the player. Over small camera moves the unpushed sky face
/// rounded in front of the ceiling at some points; pushed, never.
#[test]
fn a_ceiling_just_under_the_sky_stays_in_front() {
    let projection = glam::camera::rh::proj::directx::perspective(
        80_f32.to_radians(),
        16.0 / 9.0,
        2.0,
        60_000.0,
    );
    let ceiling = Vec3::new(-15641.6, 2015.8, 832.0);
    let mut unpushed_losses = 0;
    let mut checked = 0;
    for step in 0..400 {
        let jitter = step as f32 * 0.37;
        let eye = Vec3::new(
            -14577.7 + jitter * 0.3,
            1692.4 - jitter * 0.2,
            -1213.8 + jitter * 0.1,
        );
        let view = glam::camera::rh::view::look_at_mat4(eye, ceiling, Vec3::Z);
        let matrix: Mat4 = projection * view;
        for offset in [-40.0, -13.0, 0.0, 21.0, 37.0] {
            // A point of the ceiling and the sky face's point behind it on the same ray.
            let point = ceiling + Vec3::new(offset, offset * 0.5, 0.0);
            let ray = (point - eye).normalize();
            let sky = point + ray * (2.0 / ray.z);
            let depth = |clip: Vec4| clip.z / clip.w;
            let wall = depth(matrix * point.extend(1.0));
            let face = matrix * sky.extend(1.0);
            unpushed_losses += usize::from(depth(face) <= wall);
            assert!(
                depth(pushed(face, PUSH)) > wall,
                "the sky covered the ceiling from {eye:?}"
            );
            assert!(
                depth(pushed(face, PUSH * 0.5)) > wall,
                "the clouds covered the ceiling from {eye:?}"
            );
            // The clouds still pass the depth test on the sky they lie on.
            assert!(depth(pushed(face, PUSH * 0.5)) <= depth(pushed(face, PUSH)));
            checked += 1;
        }
    }
    eprintln!("unpushed sky in front of the ceiling at {unpushed_losses} of {checked} points");
}
