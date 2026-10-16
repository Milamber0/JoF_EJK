//! External GPU comparison of the unchanged personal-shield shader at one/four passes.
use crate::*;

#[test]
#[ignore = "needs a GPU, external JKA_GAME_DATA and SJK_GPU_SKINNING=0 for the fixed pose"]
fn body_shield_gpu_comparison() {
    world_shot::on_big_stack(|| {
        let (mut gpu, _profile) = world_shot::open(
            "maps/mp/duel6.bsp",
            [640, 480],
            None,
            &[
                ("r_hdr", "0"),
                ("r_sceneHdr", "0"),
                ("cg_materialMaps", "0"),
            ],
        )
        .expect("a GPU adapter");
        world_shot::aim(&mut gpu, [0.0, 0.0, 24.0], 0.0, 0.0);
        assert!(matches!(
            gpu.render(&mut None),
            gpu_context::FrameStatus::Rendered
        ));
        let mesh = gpu
            .build_live_actor(
                &actor_load::fallback_appearance(),
                EntityId::new(1),
                [None, None],
            )
            .unwrap();
        gpu.actor_meshes.push(mesh);
        let material = gpu
            .model_material_overrides
            .force("gfx/misc/personalshield")
            .expect("shield material");
        let instance = ActorInstance::new([100.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
        let mut shots = Vec::new();
        for passes in [0, 1, 4] {
            let mut instances = Vec::with_capacity(5);
            instances.push(instance);
            let overlays: Vec<_> = (0..passes)
                .map(|_| entity_materials::OverrideInstance {
                    mesh: entity_materials::OverrideMesh::Actor(0),
                    material: Some(material),
                    instance: instance.with_entity_color([76, 255, 76, 255]),
                    no_depth: false,
                    forced_alpha: false,
                })
                .collect();
            let mut ranges = Vec::with_capacity(4);
            entity_materials::append_override_ranges(&mut instances, &overlays, &mut ranges);
            let actor_ranges: Vec<_> = (0..gpu.actor_meshes.len())
                .map(|i| if i == 0 { 0..1 } else { 0..0 })
                .collect();
            let mut draws = entity_materials::Queue::new();
            draws.rebuild(
                &gpu.world_materials,
                &gpu.actor_meshes,
                &actor_ranges,
                &[],
                &[],
                &ranges,
                &instances,
                gpu.camera_position,
                crate::card_outline::Group::ALL
                    .map(|group| gpu.model_material_overrides.outline(group)),
            );
            assert!(!overlays.is_empty() || passes == 0);
            assert_eq!(ranges.len(), passes);
            gpu.queue.write_buffer(
                &gpu.actor_instance_buffer,
                0,
                bytemuck::cast_slice(&instances),
            );
            let texture = gpu.headless_frame.as_ref().unwrap();
            let view = texture.create_view(&Default::default());
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.05,
                                g: 0.05,
                                b: 0.05,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &gpu.depth.view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
                for list in [draws.opaque(), draws.blended()] {
                    gpu.world_materials.draw_entities(
                        &mut pass,
                        &gpu.camera_bind_group,
                        &gpu.geometry.vertex_buffer,
                        &gpu.geometry.index_buffer,
                        &gpu.actor_instance_buffer,
                        list,
                    );
                }
            }
            gpu.queue.submit([encoder.finish()]);
            shots.push(world_shot::read_back(&gpu.device, &gpu.queue, texture));
        }
        let gain = |shot: &image::RgbaImage| {
            shot.pixels()
                .zip(shots[0].pixels())
                .map(|(a, b)| {
                    (0..3)
                        .map(|i| u64::from(a[i].saturating_sub(b[i])))
                        .sum::<u64>()
                })
                .sum::<u64>()
        };
        let one = gain(&shots[1]);
        let four = gain(&shots[2]);
        println!("shield shader RGB gain: one={one}, four={four}");
        if let Some(directory) = std::env::var_os("SJK_SHIELD_FLASH_SHOTS") {
            for (shot, name) in shots.iter().zip(["body", "one", "four"]) {
                shot.save(PathBuf::from(&directory).join(format!("shield-{name}.png")))
                    .unwrap();
            }
        }
        assert!(one > 0 && four > one);
    });
}
