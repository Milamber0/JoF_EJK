//! The radial status HUD: ratio meters drawn as arcs around the crosshair.
//!
//! A layout document ([`assets/hud/radial.json`](../../assets/hud/radial.json)) places
//! [`HudWidgetKind::Arc`] widgets; this module turns each into smooth stroked arcs.
//! The strokes are anti-aliased in the shader, so they stay clean at any resolution.

use super::widgets::EmitContext;
use crate::menu_hud::frame::{AMMO_INDEX, AMMO_MAX};
use sjk_ui::{
    Color, DrawCommand, DrawList, HudDataSource, HudWidget, Rect, Theme, arc_segments, arc_span,
};

/// How full the current weapon's ammunition is, `0..=1`; zero for weapons without any.
///
/// `ammoData[].max` (`bg_weapons.c`) doubles with the Double Ammo (`EF_DOUBLE_AMMO`) rune.
pub(super) fn ammo_ratio(weapon: u8, ammo: Option<i32>, double_ammo: bool) -> f32 {
    let Some(ammo) = ammo else { return 0.0 };
    let index = AMMO_INDEX.get(usize::from(weapon)).copied().unwrap_or(0);
    let maximum = AMMO_MAX[index] * if double_ammo { 2 } else { 1 };
    if maximum <= 0 {
        return 0.0;
    }
    (ammo as f32 / maximum as f32).clamp(0.0, 1.0)
}

/// The colour of a saber style's line: Fast blue, Medium yellow, Strong red, Dual green and
/// Staff magenta, as TheRisqe Radial HUD's style pictures are drawn. The NPC styles (Desann,
/// Tavion) and unknown values get a pale violet.
pub(super) fn saber_style_color(style: u8) -> Color {
    match style {
        1 => Color::new(0.3, 0.55, 1.0, 1.0),
        2 => Color::new(0.96, 0.88, 0.2, 1.0),
        3 => Color::new(1.0, 0.25, 0.21, 1.0),
        6 => Color::new(0.34, 0.93, 0.34, 1.0),
        7 => Color::new(1.0, 0.27, 0.89, 1.0),
        _ => Color::new(0.84, 0.74, 1.0, 1.0),
    }
}

/// Stroke geometry shared by the track, the fill and the glow of one arc widget.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Ring {
    pub(super) center: [f32; 2],
    pub(super) radius: f32,
    pub(super) width: f32,
}

impl Ring {
    /// The circle inscribed in `rect`, stroked `width` pixels wide inside it.
    pub(super) fn inscribed(rect: Rect, width: f32) -> Self {
        let width = width.max(1.0);
        Self {
            center: [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5],
            radius: (rect.width.min(rect.height) * 0.5 - width * 0.5).max(0.0),
            width,
        }
    }

    /// Angle that keeps a round cap inside its segment.
    pub(super) fn cap_inset(self) -> f32 {
        if self.radius > 0.0 {
            self.width * 0.5 / self.radius
        } else {
            0.0
        }
    }
}

/// Thickness of the band over the maximum, as a share of the stroke.
const OVERFLOW_WIDTH: f32 = 0.55;
/// Alpha of the soft halo drawn behind the filled part of an arc.
const GLOW_ALPHA: f32 = 0.16;
/// How much wider than the stroke the halo is. It stays inside the shadow band (a margin of
/// 7 px round a 7 px bar makes it 3 times the stroke), so the shadow is the outermost layer.
const GLOW_WIDTH: f32 = 2.0;

pub(super) fn emit(
    draw_list: &mut DrawList,
    theme: Theme,
    widget: &HudWidget,
    rect: Rect,
    context: &EmitContext<'_>,
) {
    let Some(style) = widget.style.arc else {
        return;
    };
    let binding = widget.binding.as_deref().unwrap_or_default();
    let ratio = context.data.number(binding).unwrap_or(0.0);
    let ring = Ring::inscribed(rect, style.width * context.dpi_scale);
    let mut fill = widget.style.foreground.unwrap_or(theme.foreground);
    if binding == "health_ratio" && context.low_health {
        fill = theme.critical;
        fill.a *= context.pulse;
    }
    if binding == "style_ratio" {
        if let Some(style) = context.data.saber_style {
            fill = saber_style_color(style);
        }
    }
    let track = widget
        .style
        .background
        .unwrap_or(Color::new(1.0, 1.0, 1.0, 0.16));
    let stroke = |draw_list: &mut DrawList, start, sweep, width, color| {
        let _ = draw_list.push(DrawCommand::Arc {
            center: ring.center,
            radius: ring.radius,
            width,
            start,
            sweep,
            color,
            knockout: None,
        });
    };
    // The shadow (the widget's border) is one rounded dark band under the whole meter, a
    // margin of `border_width` all round the bars, drawn first so it lies below them. It
    // skips the band the readout pill already darkens, so the two are one shape.
    if let Some((color, margin)) = widget.style.border.zip(widget.style.border_width) {
        let (start, sweep) = arc_span(&style, ring.cap_inset());
        let scale = context.dpi_scale;
        let _ = draw_list.push(DrawCommand::Arc {
            center: ring.center,
            radius: ring.radius,
            width: ring.width + 2.0 * margin * scale,
            start,
            sweep,
            color,
            knockout: style.knockout.map(|band| {
                Rect::new(
                    ring.center[0] + band.from_x * scale,
                    ring.center[1] - band.half_height * scale,
                    (band.to_x - band.from_x) * scale,
                    2.0 * band.half_height * scale,
                )
            }),
        });
    }
    for segment in arc_segments(&style, ratio.clamp(0.0, 1.0), ring.cap_inset()) {
        stroke(draw_list, segment.start, segment.sweep, ring.width, track);
        if segment.amount > 0.0 {
            let mut glow = fill;
            glow.a *= GLOW_ALPHA;
            stroke(
                draw_list,
                segment.fill_start,
                segment.fill_sweep,
                ring.width * GLOW_WIDTH,
                glow,
            );
            stroke(
                draw_list,
                segment.fill_start,
                segment.fill_sweep,
                ring.width,
                fill,
            );
        }
    }
    // Over the maximum (overheal, overshield): the same segments again, from the
    // start, as a thinner band in a deeper shade down the middle of the stroke.
    let over = (ratio - 1.0).clamp(0.0, 1.0);
    if over > 0.0 {
        let deep = super::nameplate_math::saturated(fill);
        for segment in arc_segments(&style, over, ring.cap_inset()) {
            if segment.amount > 0.0 {
                stroke(
                    draw_list,
                    segment.fill_start,
                    segment.fill_sweep,
                    ring.width * OVERFLOW_WIDTH,
                    deep,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_ui::TextAlign;

    #[test]
    fn ammo_fills_against_the_weapons_pool_maximum() {
        // The blaster (weapon 5) draws from the 300-round pool.
        assert!((ammo_ratio(5, Some(150), false) - 0.5).abs() < 1e-6);
        assert!((ammo_ratio(5, Some(150), true) - 0.25).abs() < 1e-6);
        assert_eq!(ammo_ratio(5, Some(900), false), 1.0);
        assert_eq!(ammo_ratio(5, Some(-1), false), 0.0);
    }

    /// Every arc stroke's width and colour, with health and armour at `health` and
    /// `armor` shares of the maximum.
    fn arcs(health: f32, armor: f32) -> Vec<(f32, Color)> {
        use crate::hud::{HudLook, HudOverlay, HudVisibility};
        let font = crate::text::load_modern(1.0, None).unwrap().font;
        let mut hud = HudOverlay::new();
        hud.preview_values(
            sjk_client::HudDataSource {
                health: (health * 100.0) as i32,
                armor: (armor * 100.0) as i32,
                force: 100,
                weapon: 5,
                ammo: Some(300),
                saber_style: None,
            },
            [health, armor, 1.0, 1.0],
        );
        let visibility = HudVisibility {
            hud: true,
            status: true,
            weapon: true,
            crosshair: false,
            crosshair_names: false,
            timer: false,
            lagometer: false,
            ground_hud: false,
            menu_hud: false,
        };
        let _ = hud.layout(&font, HudLook::Radial, [1920.0, 1080.0], 1.0, visibility, 0);
        hud.draw_list()
            .commands()
            .iter()
            .filter_map(|command| match *command {
                DrawCommand::Arc { width, color, .. } => Some((width, color)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn health_and_armour_over_the_maximum_add_a_deeper_inner_band() {
        let plain = arcs(1.0, 0.25);
        let stroke = plain
            .iter()
            .map(|(width, _)| *width)
            .fold(f32::MAX, f32::min);
        let thin = |arcs: Vec<(f32, Color)>| -> Vec<(f32, Color)> {
            arcs.into_iter()
                .filter(|(width, _)| *width < stroke * 0.9)
                .collect()
        };
        assert!(thin(plain.clone()).is_empty(), "nothing over the maximum");
        let over = thin(arcs(1.25, 1.99));
        assert!(!over.is_empty());
        // Both meters have a band, each a deeper shade of its own fill.
        let (red, green) = (
            over.iter().filter(|(_, c)| c.r > c.g).count(),
            over.iter().filter(|(_, c)| c.g > c.r).count(),
        );
        assert!(
            red > 0 && green > red,
            "health a quarter, armour almost all"
        );
    }

    #[test]
    fn no_ammo_or_no_pool_is_empty() {
        assert_eq!(ammo_ratio(5, None, false), 0.0);
        // The saber and melee draw no ammunition.
        assert_eq!(ammo_ratio(3, Some(50), false), 0.0);
        assert_eq!(ammo_ratio(200, Some(50), false), 0.0);
    }

    /// One arc stroke of the draw list.
    #[derive(Clone, Copy)]
    struct Stroke {
        /// Position in the draw list, which is the paint order.
        index: usize,
        radius: f32,
        width: f32,
        start: f32,
        sweep: f32,
        color: Color,
        knockout: Option<Rect>,
    }

    impl Stroke {
        /// The dark outline under a bar.
        fn is_shadow(self) -> bool {
            self.color.r == 0.0 && self.color.g == 0.0 && self.color.b == 0.0
        }
    }

    /// Where the radial layout puts its pieces for a player with full meters.
    struct Placed {
        center: [f32; 2],
        /// Centre line of the outer and inner rings.
        radii: [f32; 2],
        /// Stroke thickness of the track and the fill.
        stroke: f32,
        /// How far the shadow outline reaches past each side of a bar.
        shadow: f32,
        strokes: Vec<Stroke>,
        texts: Vec<(u32, Rect, Color, TextAlign)>,
        pills: Vec<Rect>,
        /// Draw-list positions of the pills.
        pill_indices: Vec<usize>,
    }

    impl Placed {
        /// Holding the blaster, with the weapon name still showing.
        fn new(viewport: [f32; 2], user_scale: f32) -> Self {
            Self::holding(viewport, user_scale, 5, Some(300), None)
        }

        /// Holding the saber in `style`.
        fn with_saber(viewport: [f32; 2], style: u8) -> Self {
            Self::holding(viewport, 1.0, 3, None, Some(style))
        }

        fn holding(
            viewport: [f32; 2],
            user_scale: f32,
            weapon: u8,
            ammo: Option<i32>,
            saber_style: Option<u8>,
        ) -> Self {
            use crate::hud::{HudLook, HudOverlay, HudVisibility};
            let font = crate::text::load_modern(1.0, None).unwrap().font;
            let mut hud = HudOverlay::new();
            hud.preview_values(
                sjk_client::HudDataSource {
                    health: 100,
                    armor: 100,
                    force: 100,
                    weapon,
                    ammo,
                    saber_style,
                },
                [1.0; 4],
            );
            let visibility = HudVisibility {
                hud: true,
                status: true,
                weapon: true,
                crosshair: true,
                crosshair_names: false,
                timer: false,
                lagometer: false,
                ground_hud: false,
                menu_hud: false,
            };
            let _ = hud.layout(&font, HudLook::Radial, viewport, user_scale, visibility, 0);
            let mut placed = Self {
                center: [f32::NAN; 2],
                radii: [0.0, f32::MAX],
                stroke: f32::MAX,
                shadow: 0.0,
                strokes: Vec::new(),
                texts: Vec::new(),
                pills: Vec::new(),
                pill_indices: Vec::new(),
            };
            for (index, command) in hud.draw_list().commands().iter().enumerate() {
                match *command {
                    DrawCommand::Arc {
                        center,
                        radius,
                        width,
                        start,
                        sweep,
                        color,
                        knockout,
                    } => {
                        // Every arc is a stroke of one ring, so they share one centre
                        // (up to the rounding of rectangles of different sizes).
                        assert!(
                            placed.center[0].is_nan()
                                || (placed.center[0] - center[0]).abs() < 0.01
                                    && (placed.center[1] - center[1]).abs() < 0.01,
                            "{:?} against {center:?}",
                            placed.center
                        );
                        placed.center = center;
                        placed.radii[0] = placed.radii[0].max(radius);
                        placed.radii[1] = placed.radii[1].min(radius);
                        placed.stroke = placed.stroke.min(width);
                        placed.strokes.push(Stroke {
                            index,
                            radius,
                            width,
                            start,
                            sweep,
                            color,
                            knockout,
                        });
                    }
                    DrawCommand::Text {
                        rect,
                        text,
                        color,
                        align,
                        ..
                    } => placed.texts.push((text.0, rect, color, align)),
                    DrawCommand::RoundedRect { rect, radius, .. } if radius > 0.0 => {
                        placed.pills.push(rect);
                        placed.pill_indices.push(index);
                    }
                    _ => {}
                }
            }
            let widest = placed
                .strokes
                .iter()
                .filter(|stroke| stroke.is_shadow())
                .map(|stroke| stroke.width)
                .fold(0.0, f32::max);
            placed.shadow = ((widest - placed.stroke) * 0.5).max(0.0);
            placed
        }

        fn text(&self, id: u32) -> Rect {
            self.texts
                .iter()
                .find(|(text, ..)| *text == id)
                .unwrap_or_else(|| panic!("text {id} is not drawn"))
                .1
        }

        fn align(&self, id: u32) -> TextAlign {
            self.texts
                .iter()
                .find(|(text, ..)| *text == id)
                .unwrap_or_else(|| panic!("text {id} is not drawn"))
                .3
        }

        /// Distance from the ring centre to the outside of the outer bars' shadow.
        fn outer_edge(&self) -> f32 {
            self.radii[0] + self.stroke * 0.5 + self.shadow
        }

        /// Distance from the ring centre to the inside of the inner bars' shadow.
        fn inner_edge(&self) -> f32 {
            self.radii[1] - self.stroke * 0.5 - self.shadow
        }
    }

    const SCREENS: [[f32; 2]; 9] = [
        [640.0, 360.0],
        [1280.0, 720.0],
        [1280.0, 1024.0],
        [1440.0, 1080.0],
        [1920.0, 1080.0],
        [1920.0, 1200.0],
        [2560.0, 1440.0],
        [3440.0, 1440.0],
        [3840.0, 2160.0],
    ];

    #[test]
    fn the_rings_keep_one_spot_of_the_screen_at_every_size_and_hud_scale() {
        // 19 % of the height below the crosshair, where TheRisqe's bars sit (their middle
        // is 92.6 of 480 lines below the centre). The 8K screen is past the UI scale's
        // upper clamp and the 360-line one under its lower one.
        let mut screens = SCREENS.to_vec();
        screens.push([7680.0, 4320.0]);
        for viewport in screens {
            for user_scale in [0.5, 1.0, 1.5] {
                let placed = Placed::new(viewport, user_scale);
                let [x, y] = placed.center;
                assert!(
                    (x - viewport[0] * 0.5).abs() < 0.5 && (y - viewport[1] * 0.69).abs() < 0.5,
                    "{viewport:?} x{user_scale}: centre {:?}",
                    placed.center
                );
            }
        }
    }

    #[test]
    fn each_meter_has_one_rounded_shadow_band_below_its_bars() {
        for placed in [
            Placed::new([1920.0, 1080.0], 1.0),
            // The saber's style line takes the ammunition meter's place, shadow and all.
            Placed::with_saber([1920.0, 1080.0], 2),
        ] {
            let shadows: Vec<_> = placed.strokes.iter().filter(|s| s.is_shadow()).collect();
            // Health, armor, Force and ammunition (or the style).
            assert_eq!(shadows.len(), 4);
            for shadow in shadows {
                // 5 logical pixels all round a 7-pixel bar.
                assert!((shadow.width / placed.stroke - 17.0 / 7.0).abs() < 1e-3);
                // One band over the whole meter: its 68 degrees less a cap at each end,
                // so the round caps sit concentric with the end bars' own.
                let inset = 2.0 * (placed.stroke * 0.5 / shadow.radius);
                assert!((shadow.sweep.abs() - (68.0_f32.to_radians() - inset)).abs() < 1e-4);
                // Every bar of that meter is painted after it, so the band lies below.
                let (low, high) = if shadow.sweep < 0.0 {
                    (shadow.start + shadow.sweep, shadow.start)
                } else {
                    (shadow.start, shadow.start + shadow.sweep)
                };
                let bars: Vec<_> = placed
                    .strokes
                    .iter()
                    .filter(|bar| {
                        !bar.is_shadow()
                            && bar.radius == shadow.radius
                            && (low - 1e-4..=high + 1e-4).contains(&bar.start)
                    })
                    .collect();
                assert!(!bars.is_empty());
                assert!(bars.iter().all(|bar| bar.index > shadow.index));
            }
            // The pills are painted before every bar and shadow.
            assert_eq!(placed.pill_indices.len(), 2);
            for pill in &placed.pill_indices {
                assert!(placed.strokes.iter().all(|stroke| stroke.index > *pill));
            }
        }
    }

    #[test]
    fn the_shadows_skip_their_sides_pill_so_the_two_are_one_shape() {
        let near = |a: Rect, b: Rect| {
            (a.x - b.x).abs() < 0.01
                && (a.y - b.y).abs() < 0.01
                && (a.width - b.width).abs() < 0.01
                && (a.height - b.height).abs() < 0.01
        };
        for viewport in SCREENS {
            let placed = Placed::new(viewport, 1.0);
            let (left, right) = (placed.pills[0], placed.pills[1]);
            let mut sides = [0, 0];
            for stroke in &placed.strokes {
                if !stroke.is_shadow() {
                    // The bars themselves draw over the pill.
                    assert!(stroke.knockout.is_none());
                    continue;
                }
                let knockout = stroke.knockout.expect("a shadow skips the pill");
                let on_left = (stroke.start + stroke.sweep * 0.5).cos() < 0.0;
                assert!(
                    near(knockout, if on_left { left } else { right }),
                    "{viewport:?}: {knockout:?} against {left:?} / {right:?}"
                );
                sides[usize::from(!on_left)] += 1;
            }
            // Two meters on each side.
            assert_eq!(sides, [2, 2], "{viewport:?}");
        }
    }

    #[test]
    fn the_numbers_ride_a_pill_through_the_bars() {
        for viewport in SCREENS {
            let placed = Placed::new(viewport, 1.0);
            let [x, y] = placed.center;
            let (outer, inner) = (placed.outer_edge(), placed.inner_edge());
            // A logical pixel, and the widest number (three digits) in them.
            let dpi = placed.stroke / 7.0;
            let digits = 52.0 * dpi;
            // Health outside the left bars and armor inside them; ammunition inside the
            // right bars and Force outside; all on the line through the bars' middle.
            let health = placed.text(6);
            let armor = placed.text(8);
            let force = placed.text(10);
            let ammo = placed.text(14);
            for rect in [health, armor, force, ammo] {
                // The layout lowers them a pixel or two to centre Inter's digits.
                assert!(
                    (rect.y + rect.height * 0.5 - y).abs() < rect.height * 0.2,
                    "{viewport:?} {rect:?}"
                );
            }
            // Each number is aligned toward its bars and starts 5 px from their shadow,
            // whatever its length.
            let gap = 5.0 * dpi;
            assert_eq!(placed.align(6), TextAlign::End);
            assert_eq!(placed.align(8), TextAlign::Start);
            assert_eq!(placed.align(14), TextAlign::End);
            assert_eq!(placed.align(10), TextAlign::Start);
            let near = |a: f32, b: f32| (a - b).abs() < 0.5;
            assert!(
                near(health.right(), x - outer - gap),
                "{viewport:?} {health:?}"
            );
            assert!(near(armor.x, x - inner + gap), "{viewport:?} {armor:?}");
            assert!(near(ammo.right(), x + inner - gap), "{viewport:?} {ammo:?}");
            assert!(near(force.x, x + outer + gap), "{viewport:?} {force:?}");
            // Each pill runs from the outside number to the inside one, behind the bars.
            assert_eq!(placed.pills.len(), 2, "{viewport:?}");
            let (left, right) = (placed.pills[0], placed.pills[1]);
            assert!(left.x < health.right() - digits && left.right() > armor.x + digits);
            assert!(right.x < ammo.right() - digits && right.right() > force.x + digits);
            assert!(left.right() < x && right.x > x, "{viewport:?}");
            for pill in [left, right] {
                assert!((pill.y + pill.height * 0.5 - y).abs() < 0.5);
                assert!(
                    pill.x >= 0.0 && pill.right() <= viewport[0] && pill.bottom() <= viewport[1],
                    "{viewport:?} pill {pill:?} leaves the screen"
                );
            }
            // The weapon name rests in the hollow between the bars, above the pills.
            let name = placed.text(12);
            assert!((name.x + name.width * 0.5 - x).abs() < 0.5);
            assert!(name.bottom() < left.y, "{viewport:?}");
            for corner in [
                [name.x - x, name.y - y],
                [name.right() - x, name.y - y],
                [name.x - x, name.bottom() - y],
                [name.right() - x, name.bottom() - y],
            ] {
                assert!(
                    corner[0].hypot(corner[1]) < inner,
                    "{viewport:?} {corner:?}"
                );
            }
        }
    }

    #[test]
    fn a_saber_draws_its_style_as_one_full_line_in_the_styles_colour() {
        let styles = [1, 2, 3, 6, 7];
        for (index, style) in styles.iter().enumerate() {
            for other in &styles[index + 1..] {
                assert_ne!(saber_style_color(*style), saber_style_color(*other));
            }
        }
        let ammo_slot = Placed::new([1920.0, 1080.0], 1.0).text(14);
        for style in styles {
            let color = saber_style_color(style);
            let placed = Placed::with_saber([1920.0, 1080.0], style);
            // The ammunition's amber bars and number give way to the style.
            assert!(placed.texts.iter().all(|(id, ..)| *id != 14));
            let amber = Color::new(1.0, 0.76, 0.16, 1.0);
            assert!(placed.strokes.iter().all(|s| s.color != amber));
            // One full-width stroke in the style's colour (the glow is a fainter copy).
            let lines: Vec<_> = placed
                .strokes
                .iter()
                .filter(|s| s.color == color && s.width == placed.stroke)
                .collect();
            assert_eq!(lines.len(), 1, "style {style}");
            let inset = 2.0 * (placed.stroke * 0.5 / lines[0].radius);
            assert!((lines[0].sweep.abs() - (68.0_f32.to_radians() - inset)).abs() < 1e-4);
            // The style's name takes the same colour, where the ammunition number was.
            let (_, label, label_color, label_align) =
                placed.texts.iter().find(|(id, ..)| *id == 15).unwrap();
            assert_eq!(*label_color, color);
            // Aligned to the same edge, toward the bars, as the number.
            assert_eq!(*label_align, TextAlign::End);
            assert!((label.right() - ammo_slot.right()).abs() < 0.5);
        }
    }

    #[test]
    fn the_ring_sits_inside_its_rect_with_caps_inside_the_ends() {
        let ring = Ring::inscribed(Rect::new(10.0, 20.0, 200.0, 200.0), 8.0);
        assert_eq!(ring.center, [110.0, 120.0]);
        assert_eq!(ring.radius, 96.0);
        assert!((ring.cap_inset() - 4.0 / 96.0).abs() < 1e-6);
        // A degenerate rect cannot divide by zero.
        let flat = Ring::inscribed(Rect::new(0.0, 0.0, 0.0, 0.0), 8.0);
        assert_eq!(flat.radius, 0.0);
        assert_eq!(flat.cap_inset(), 0.0);
    }
}
