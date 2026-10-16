//! The limits of `material_map_parallax` (`material_maps.wgsl`) worked out on the CPU, to
//! pin how far parallax reaches and what the near limit does. [`View::reach`] repeats the
//! shader's arithmetic for a pinhole camera looking straight at a point of a flat, evenly
//! mapped surface; [`shader_has_the_modelled_limits`] checks that the shader still holds
//! that arithmetic. Distances are in world units, at 1920 pixels across a 90-degree view
//! (1080p) unless a test says otherwise.

/// The material program's own source.
const SHADER: &str = include_str!("material_maps.wgsl");

/// Radians per pixel at 1920 pixels across 90 degrees (1080p), and at 3840 (4K).
const PIXEL_1080P: f64 = 2.0 / 1920.0;
const PIXEL_4K: f64 = 2.0 / 3840.0;
/// rend2's `parallaxDepth` without one (in texture repeats), at the default
/// `r_parallaxStrength` 0.1.
const DEFAULT_DEPTH: f64 = 0.05 * 0.1;

fn smoothstep(low: f64, high: f64, x: f64) -> f64 {
    let t = ((x - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A camera `height` units from a surface's plane, looking at the point `along` units
/// from its foot, `pixel` radians a pixel. The surface repeats its maps every
/// `world_per_uv` units; they are `texels` across, with `depth` (in repeats, the
/// strength applied) and `r_parallaxNearDistance` `near_distance`.
#[derive(Clone, Copy)]
struct View {
    height: f64,
    along: f64,
    pixel: f64,
    world_per_uv: f64,
    texels: f64,
    depth: f64,
    near_distance: f64,
}

/// What [`View::reach`] finds.
struct Reach {
    /// The share of the depth the far limits keep.
    far: f64,
    /// The share the near limit keeps.
    near: f64,
    /// Pixels the whole depth would move the texture.
    pixels: f64,
    /// Linear steps of the march.
    steps: f64,
}

impl Reach {
    /// Pixels the depth the limits keep moves the texture.
    fn shown(&self) -> f64 {
        self.pixels * self.far * self.near
    }
}

impl View {
    /// The standing eye (60 units over the floor) on a floor whose 256-texel texture
    /// repeats every 128 units (retail's scale 0.5), with a 1024-texel map (an HD pack's).
    fn floor(along: f64) -> Self {
        Self {
            height: 60.0,
            along,
            pixel: PIXEL_1080P,
            world_per_uv: 128.0,
            texels: 1024.0,
            depth: DEFAULT_DEPTH,
            near_distance: 24.0,
        }
    }

    /// The same surface as a wall seen `distance` units away at `degrees` from its normal.
    fn wall(distance: f64, degrees: f64) -> Self {
        let angle = degrees.to_radians();
        Self {
            height: distance * angle.cos(),
            along: distance * angle.sin(),
            ..Self::floor(0.0)
        }
    }

    /// `material_map_parallax`'s limits for this view.
    fn reach(self) -> Reach {
        let distance = self.height.hypot(self.along);
        let cos = self.height / distance;
        let sin = self.along / distance;
        // The coordinates' screen derivatives: across the view, and along it, where the
        // surface is foreshortened. The offset runs along the view.
        let across = self.pixel * distance / self.world_per_uv;
        let down = across / cos;
        let reach = self.depth * sin / cos.max(0.35);
        let pixels = reach / down;
        let footprint = (across * self.texels).max(down * self.texels / 16.0);
        let far = smoothstep(0.125, 0.5, pixels)
            * smoothstep(0.05, 0.15, cos)
            * (1.0 - smoothstep(4.0, 16.0, footprint));
        let near = if self.near_distance > 0.0 {
            (self.height / self.near_distance.max(1.0)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let steps = (2.0 * reach * far * near * self.texels / footprint.max(1.0))
            .ceil()
            .clamp(4.0, 24.0);
        Reach {
            far,
            near,
            pixels,
            steps,
        }
    }

    /// The share of the depth the limits before 08/10/2026 kept: from 20 down to 8.6
    /// degrees above the surface and from 1.5 to 4 texels a pixel on the longer axis.
    fn before(self) -> f64 {
        let distance = self.height.hypot(self.along);
        let cos = self.height / distance;
        let longer = self.pixel * distance / self.world_per_uv / cos * self.texels;
        smoothstep(0.15, 0.35, cos) * (1.0 - smoothstep(1.5, 4.0, longer))
    }
}

#[test]
fn shader_has_the_modelled_limits() {
    for line in [
        "let reach = direction.xy*(-material_map.normal_scale.w*strength/max(direction.z, 0.35));",
        "let pixels = length(vec2(dy.y*reach.x - dy.x*reach.y, dx.x*reach.y - dx.y*reach.x))/area;",
        "let footprint = max(minor, major/16.0);",
        "let far = smoothstep(0.125, 0.5, pixels)*smoothstep(0.05, 0.15, direction.z)\n        \
         *(1.0 - smoothstep(4.0, 16.0, footprint));",
        "let near = select(1.0, clamp(tangent_view.z/max(near_distance, 1.0), 0.0, 1.0),\n        \
         near_distance > 0.0);",
        "let ds = reach*(far*near);",
        "let steps = clamp(ceil(2.0*length(ds*size)/max(footprint, 1.0)), 4.0, 24.0);",
        "if !(far*near > 0.0) {",
        "let linear = clamp(i32(steps), 4, 24);",
        "for (var i = 1; i < linear; i++) {",
        "for (var i = 0; i < 6; i++) {",
    ] {
        assert_eq!(SHADER.matches(line).count(), 1, "{line}");
    }
    // The old limits are gone.
    assert!(!SHADER.contains("smoothstep(1.5, 4.0"));
    assert!(!SHADER.contains("smoothstep(0.15, 0.35"));
}

#[test]
fn floors_keep_parallax_farther_than_before() {
    // With an HD-sized map the old limits flattened a floor 90 to 160 units ahead of a
    // standing player; it now keeps its whole depth to 400 and fades out by 1000.
    assert!(View::floor(150.0).before() < 0.1);
    assert_eq!(View::floor(200.0).before(), 0.0);
    for along in [100.0, 200.0, 300.0, 400.0] {
        assert!(View::floor(along).reach().far > 0.99, "{along}");
    }
    assert!(View::floor(500.0).reach().far > 0.5);
    assert!(View::floor(800.0).reach().far < 0.01);
    assert_eq!(View::floor(1000.0).reach().far, 0.0);
    // A retail-sized map lost it by 340 units.
    let retail = |along| View {
        texels: 256.0,
        ..View::floor(along)
    };
    assert_eq!(retail(400.0).before(), 0.0);
    assert!(retail(400.0).reach().far > 0.99);
    // At 4K a pixel is half the size, and the floor keeps it farther.
    let sharp = |along| View {
        pixel: PIXEL_4K,
        ..View::floor(along)
    };
    assert!(sharp(500.0).reach().far > View::floor(500.0).reach().far);
    assert!(sharp(600.0).reach().far > 0.4);
}

#[test]
fn walls_keep_parallax_farther_than_before() {
    // Seen at 45 degrees, an HD-sized map lost its parallax by 400 units.
    assert_eq!(View::wall(400.0, 45.0).before(), 0.0);
    assert!(View::wall(500.0, 45.0).reach().far > 0.99);
    assert!(View::wall(1000.0, 45.0).reach().far > 0.5);
    assert_eq!(View::wall(2000.0, 45.0).reach().far, 0.0);
    // Deeper relief shows farther, so it keeps its depth farther (here on a retail-sized
    // map, whose mip levels hold the relief longer).
    let retail = |depth| View {
        depth,
        texels: 256.0,
        ..View::wall(2000.0, 45.0)
    };
    assert!(retail(DEFAULT_DEPTH).reach().far < 0.2);
    assert!(retail(0.05).reach().far > 0.99);
}

#[test]
fn far_limits_drop_only_parallax_under_half_a_pixel() {
    // Wherever the new far limits keep less of the depth than the old ones did, what the
    // old ones showed moved the texture by less than half a pixel: nothing visible is
    // lost. Near head-on views at a distance are such places.
    for pixel in [PIXEL_1080P, PIXEL_4K] {
        for texels in [256.0, 512.0, 1024.0, 2048.0] {
            for strength in [0.1, 0.25, 0.5, 1.0, 1.575] {
                let surface = View {
                    pixel,
                    texels,
                    depth: 0.05 * strength,
                    near_distance: 0.0,
                    ..View::floor(0.0)
                };
                let mut views = Vec::new();
                for step in 4..1200 {
                    let distance = f64::from(step) * 5.0;
                    views.push(View {
                        along: distance,
                        ..surface
                    });
                    for degrees in [20.0, 30.0, 45.0, 60.0, 70.0, 80.0] {
                        let wall = View::wall(distance, degrees);
                        views.push(View {
                            height: wall.height,
                            along: wall.along,
                            ..surface
                        });
                    }
                }
                for view in views {
                    let reach = view.reach();
                    let before = view.before();
                    if reach.far < before - 1e-9 {
                        assert!(
                            before * reach.pixels < 0.5,
                            "{} {} {} {}",
                            view.height,
                            view.along,
                            texels,
                            strength
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn far_limits_fade_without_a_ring() {
    // Walking the floor or a wall at 45 degrees away from the camera, unit by unit, the
    // kept depth never comes back and never jumps.
    for strength in [0.1, 0.5, 1.0, 1.575] {
        for texels in [256.0, 1024.0] {
            for pixel in [PIXEL_1080P, PIXEL_4K] {
                let views: [&dyn Fn(f64) -> View; 2] = [
                    &|along| View {
                        depth: 0.05 * strength,
                        texels,
                        pixel,
                        ..View::floor(along)
                    },
                    &|distance| View {
                        depth: 0.05 * strength,
                        texels,
                        pixel,
                        ..View::wall(distance, 45.0)
                    },
                ];
                for view in views {
                    let mut last = view(30.0).reach().far;
                    for step in 31..4000 {
                        let far = view(f64::from(step)).reach().far;
                        assert!(far <= last + 1e-12, "{strength} {texels} {step}");
                        assert!(last - far < 0.02, "{strength} {texels} {step}");
                        last = far;
                    }
                }
            }
        }
    }
}

#[test]
fn near_limit_holds_the_parallax_on_screen() {
    // At a fixed angle the parallax on screen grows as 1/distance as the camera closes
    // in; within 24 units of the surface's plane it keeps the size it had at 24.
    let at = |height: f64, near_distance| {
        View {
            near_distance,
            ..View::wall(height / 45f64.to_radians().cos(), 45.0)
        }
        .reach()
    };
    let held = at(24.0, 24.0).shown();
    for height in [1.0, 4.0, 8.0, 15.0, 23.0] {
        assert!(
            (at(height, 24.0).shown() - held).abs() < 1e-9 * held,
            "{height}"
        );
        assert!(at(height, 24.0).near < 1.0);
    }
    assert!((at(48.0, 24.0).shown() - held / 2.0).abs() < 1e-9 * held);
    assert_eq!(at(48.0, 24.0).near, 1.0);
    // Off (0), the third-person camera 4 units from a wall saw six times as much.
    assert!((at(4.0, 0.0).shown() - 6.0 * held).abs() < 1e-9 * held);
    // A first-person eye stays 15 units from a wall: it keeps 15/24 of the depth.
    assert!((at(15.0, 24.0).near - 15.0 / 24.0).abs() < 1e-9);
}

#[test]
fn march_reads_stay_bounded() {
    // 4 to 24 linear steps (the last is the bottom, read by none), 6 binary and the final
    // two reads: at most 31 reads of the height a pixel (rend2's 15, 8 and 2 made 25). A
    // shallow or distant relief takes the fewest, a deep one up close the most.
    for height in [2.0, 8.0, 30.0, 60.0, 200.0, 1000.0] {
        for along in [0.0, 10.0, 100.0, 500.0, 3000.0] {
            for strength in [0.1, 1.0, 1.575] {
                let reach = View {
                    height,
                    along,
                    depth: 0.05 * strength,
                    ..View::floor(0.0)
                }
                .reach();
                assert!((4.0..=24.0).contains(&reach.steps));
            }
        }
    }
    assert_eq!(View::wall(1500.0, 45.0).reach().steps, 4.0);
    let deep = View {
        depth: 0.05,
        ..View::floor(100.0)
    };
    assert_eq!(deep.reach().steps, 24.0);
}

#[test]
fn reach_view_shows_the_limits() {
    // `r_materialMapsDebug 7` draws what the march recorded, on parallax stages only.
    assert!(SHADER.contains("if view == 7u {"));
    assert!(SHADER.contains("return vec4(material_map_parallax_reach, lit.a);"));
    assert!(SHADER.contains("material_map_parallax_reach = vec3(far, near, steps/24.0);"));
    assert!(SHADER.contains("material_map_parallax_reach = vec3(0.0);"));
}
