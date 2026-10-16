//! Segment geometry of arc meters ([`crate::HudWidgetKind::Arc`]).

use crate::ArcStyle;

/// Most segments one arc meter is cut into.
pub const MAX_ARC_SEGMENTS: usize = 16;

/// One segment of an arc meter, in radians (see [`crate::DrawCommand::Arc`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcSegment {
    /// Where the empty track segment starts.
    pub start: f32,
    /// Signed angle the track segment covers.
    pub sweep: f32,
    /// Where the filled part starts: the track's start, or its end when reversed.
    pub fill_start: f32,
    /// Signed angle the filled part covers; zero when the segment is empty.
    pub fill_sweep: f32,
    /// How much of the segment is filled, `0..=1`.
    pub amount: f32,
}

/// The segments of `style` filled to `ratio`, in track order, with `inset` radians
/// trimmed from both ends of each so round caps stay inside the segment.
///
/// Every segment carries the same share of the ratio, so the meter fills one
/// segment after another. Allocation-free.
pub fn segments(
    style: &ArcStyle,
    ratio: f32,
    inset: f32,
) -> impl Iterator<Item = ArcSegment> + use<> {
    let count = usize::from(style.segments).clamp(1, MAX_ARC_SEGMENTS);
    let direction = if style.sweep_degrees < 0.0 { -1.0 } else { 1.0 };
    let total = style.sweep_degrees.abs().to_radians();
    let gap = style.gap_degrees.max(0.0).to_radians();
    let length = ((total - gap * (count - 1) as f32) / count as f32).max(0.0);
    let start = style.start_degrees.to_radians();
    let reversed = style.reversed;
    let ratio = if ratio.is_finite() {
        ratio.clamp(0.0, 1.0)
    } else {
        0.0
    };
    (0..count).map(move |index| {
        let step = index as f32 * (length + gap);
        let usable = (length - 2.0 * inset).max(0.0);
        let first = start + direction * (step + inset);
        let order = if reversed { count - 1 - index } else { index };
        let amount = (ratio * count as f32 - order as f32).clamp(0.0, 1.0);
        let fill = amount * usable;
        ArcSegment {
            start: first,
            sweep: direction * usable,
            fill_start: if reversed {
                first + direction * (usable - fill)
            } else {
                first
            },
            fill_sweep: direction * fill,
            amount,
        }
    })
}

/// Start and signed sweep of the whole meter of `style` with `inset` radians trimmed from both
/// ends: the span from where [`segments`]' first segment starts to where its last one ends.
///
/// One round-capped stroke over it has caps concentric with those of the end segments, so a
/// wider stroke makes an even rim (a shadow) around the whole meter.
pub fn span(style: &ArcStyle, inset: f32) -> (f32, f32) {
    let direction = if style.sweep_degrees < 0.0 { -1.0 } else { 1.0 };
    let total = style.sweep_degrees.abs().to_radians();
    (
        style.start_degrees.to_radians() + direction * inset,
        direction * (total - 2.0 * inset).max(0.0),
    )
}

/// How much of the stripe `x_range` wide and `half_height` either side of the centre line
/// covers `point` (relative to the arc's centre), `0..=1`, anti-aliased over one pixel.
///
/// The reference for `ui_shapes.wgsl`, which feeds it to [`knockout_remainder`] for an arc's
/// knockout, and for CPU previews. A zero `half_height` covers nothing.
pub fn knockout_coverage(point: [f32; 2], x_range: [f32; 2], half_height: f32) -> f32 {
    if half_height <= 0.0 {
        return 0.0;
    }
    let outside = [
        (point[0] - (x_range[0] + x_range[1]) * 0.5).abs() - (x_range[1] - x_range[0]) * 0.5,
        point[1].abs() - half_height,
    ];
    let distance =
        outside[0].max(0.0).hypot(outside[1].max(0.0)) + outside[0].max(outside[1]).min(0.0);
    (0.5 - distance).clamp(0.0, 1.0)
}

/// The coverage an arc stroke still needs where a panel of the same colour and `alpha` has
/// already covered `knock` of the pixel and the stroke itself would cover `coverage`, so that
/// the two together show `alpha * max(coverage, knock)`, as one shape would, instead of the
/// two translucent layers stacking into a darker one (or, at an anti-aliased edge, into a
/// lighter seam). Solves `1 - (1 - alpha * knock) * (1 - alpha * remainder) = alpha *
/// max(coverage, knock)` for `remainder`.
pub fn knockout_remainder(coverage: f32, knock: f32, alpha: f32) -> f32 {
    ((coverage - knock) / (1.0 - alpha * knock).max(1.0e-4)).clamp(0.0, 1.0)
}

/// Signed distance in pixels from `point` (relative to the circle's centre) to the
/// stroke of an arc: negative inside. The reference for `ui_shapes.wgsl`, which
/// evaluates the same expression per fragment, and for CPU previews.
pub fn arc_distance(point: [f32; 2], radius: f32, width: f32, start: f32, sweep: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let middle = start + sweep * 0.5;
    let mut delta = point[1].atan2(point[0]) - middle;
    // Wrap to [-pi, pi) so the arc may cross the +/-pi seam.
    delta -= TAU * ((delta + PI) / TAU).floor();
    let limit = sweep.abs() * 0.5;
    let angle = middle + delta.clamp(-limit, limit);
    let nearest = [radius * angle.cos(), radius * angle.sin()];
    (point[0] - nearest[0]).hypot(point[1] - nearest[1]) - width * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stroke_distance_is_negative_on_the_arc_and_grows_off_it() {
        let (radius, width) = (100.0, 10.0);
        let (start, sweep) = (-0.5, 1.0); // centred on +x
        let on = arc_distance([100.0, 0.0], radius, width, start, sweep);
        assert!((on + 5.0).abs() < 1e-4, "{on}");
        let outside = arc_distance([120.0, 0.0], radius, width, start, sweep);
        assert!((outside - 15.0).abs() < 1e-4, "{outside}");
        // Opposite side of the circle: far from the arc, nearest the cap.
        assert!(arc_distance([-100.0, 0.0], radius, width, start, sweep) > 100.0);
    }

    #[test]
    fn the_caps_are_round_and_the_seam_is_crossed_cleanly() {
        let (radius, width) = (50.0, 8.0);
        // An arc from 170 to 190 degrees straddles the atan2 seam at 180.
        let (start, sweep) = (170.0_f32.to_radians(), 20.0_f32.to_radians());
        assert!(arc_distance([-50.0, 0.0], radius, width, start, sweep) < -3.9);
        // Beyond the end, the distance is to the cap's centre, minus half the stroke.
        let end = start + sweep;
        let past = [radius * end.cos() + 6.0, radius * end.sin()];
        let distance = arc_distance(past, radius, width, start, sweep);
        assert!(distance > 1.0 && distance < 6.0, "{distance}");
        // A zero sweep is a dot.
        let dot = arc_distance([radius, 0.0], radius, width, 0.0, 0.0);
        assert!((dot + 4.0).abs() < 1e-4);
    }

    fn style(segments: u8, gap: f32) -> ArcStyle {
        ArcStyle {
            start_degrees: 100.0,
            sweep_degrees: 80.0,
            width: 6.0,
            segments,
            gap_degrees: gap,
            reversed: false,
            knockout: None,
        }
    }

    #[test]
    fn a_knocked_out_stroke_and_its_panel_show_one_uniform_alpha() {
        let alpha = 0.5_f32;
        // Stroke fully over the pixel, the panel's edge covering `knock` of it.
        for knock in [0.0, 0.1, 0.25, 0.4, 0.5, 0.75, 0.9, 1.0] {
            let remainder = knockout_remainder(1.0, knock, alpha);
            let shown = 1.0 - (1.0 - alpha * knock) * (1.0 - alpha * remainder);
            assert!((shown - alpha).abs() < 1e-6, "knock {knock}: {shown}");
        }
        // A stroke edge inside the panel adds nothing, outside it everything.
        assert_eq!(knockout_remainder(0.3, 1.0, alpha), 0.0);
        assert_eq!(knockout_remainder(0.7, 0.0, alpha), 0.7);
        // An opaque panel leaves nothing to add and cannot divide by zero.
        assert_eq!(knockout_remainder(1.0, 1.0, 1.0), 0.0);
    }

    #[test]
    fn the_knockout_stripe_covers_its_rectangle_and_fades_over_one_pixel() {
        let stripe = ([-300.0, -70.0], 22.0);
        let at = |x, y| knockout_coverage([x, y], stripe.0, stripe.1);
        assert_eq!(at(-185.0, 0.0), 1.0);
        assert_eq!(at(-185.0, 21.0), 1.0);
        assert_eq!(at(-185.0, 40.0), 0.0);
        assert_eq!(at(0.0, 0.0), 0.0);
        // The edge pixel is half covered, and the stripe's ends are not rounded.
        assert!((at(-185.0, 22.0) - 0.5).abs() < 1e-6);
        assert_eq!(at(-71.0, 0.0), 1.0);
        // No height, no stripe.
        assert_eq!(knockout_coverage([-185.0, 0.0], [-300.0, -70.0], 0.0), 0.0);
    }

    #[test]
    fn segments_and_gaps_cover_the_sweep_exactly() {
        let all: Vec<_> = segments(&style(4, 4.0), 1.0, 0.0).collect();
        assert_eq!(all.len(), 4);
        let length = (80.0_f32 - 12.0) / 4.0;
        for (index, segment) in all.iter().enumerate() {
            let expected = (100.0 + index as f32 * (length + 4.0)).to_radians();
            assert!((segment.start - expected).abs() < 1e-5);
            assert!((segment.sweep - length.to_radians()).abs() < 1e-5);
        }
        let last = all[3];
        assert!((last.start + last.sweep - 180.0_f32.to_radians()).abs() < 1e-5);
    }

    #[test]
    fn the_span_runs_from_the_first_segment_to_the_last() {
        for (style, inset) in [
            (style(4, 4.0), 0.03),
            (style(1, 0.0), 0.0),
            (
                ArcStyle {
                    sweep_degrees: -80.0,
                    ..style(3, 5.0)
                },
                0.05,
            ),
        ] {
            let all: Vec<_> = segments(&style, 0.5, inset).collect();
            let (start, sweep) = span(&style, inset);
            let last = all[all.len() - 1];
            assert!((start - all[0].start).abs() < 1e-5);
            assert!((start + sweep - (last.start + last.sweep)).abs() < 1e-5);
        }
        // An inset wider than the meter leaves a dot, not a reversed arc.
        assert_eq!(span(&style(1, 0.0), 2.0).1, 0.0);
    }

    #[test]
    fn the_ratio_fills_one_segment_after_another() {
        let amounts = |ratio| -> Vec<f32> {
            segments(&style(4, 2.0), ratio, 0.0)
                .map(|s| s.amount)
                .collect()
        };
        assert_eq!(amounts(0.0), [0.0; 4]);
        assert_eq!(amounts(1.0), [1.0; 4]);
        assert_eq!(amounts(0.5), [1.0, 1.0, 0.0, 0.0]);
        let part = amounts(0.375);
        assert!((part[1] - 0.5).abs() < 1e-5 && part[0] == 1.0 && part[2] == 0.0);
        // Out-of-range and non-finite ratios stay in bounds.
        assert_eq!(amounts(7.0), [1.0; 4]);
        assert_eq!(amounts(-1.0), [0.0; 4]);
        assert_eq!(amounts(f32::NAN), [0.0; 4]);
    }

    #[test]
    fn a_reversed_meter_fills_from_the_far_end_towards_the_start() {
        let mut reversed = style(2, 0.0);
        reversed.reversed = true;
        let half: Vec<_> = segments(&reversed, 0.5, 0.0).collect();
        assert_eq!(half[0].amount, 0.0);
        assert_eq!(half[1].amount, 1.0);
        let quarter: Vec<_> = segments(&reversed, 0.75, 0.0).collect();
        // Segment 0 is half full and its fill hugs the segment's far end.
        let segment = quarter[0];
        assert!((segment.amount - 0.5).abs() < 1e-5);
        let end = segment.start + segment.sweep;
        assert!((segment.fill_start + segment.fill_sweep - end).abs() < 1e-5);
    }

    #[test]
    fn a_negative_sweep_runs_the_other_way_and_an_inset_trims_the_ends() {
        let mut backwards = style(1, 0.0);
        backwards.sweep_degrees = -80.0;
        let inset = 5.0_f32.to_radians();
        let segment = segments(&backwards, 1.0, inset).next().unwrap();
        assert!((segment.start - (100.0_f32.to_radians() - inset)).abs() < 1e-5);
        assert!((segment.sweep + (80.0_f32.to_radians() - 2.0 * inset)).abs() < 1e-5);
        // Wider than the segment: nothing negative is produced.
        let squeezed = segments(&style(1, 0.0), 1.0, 1.0).next().unwrap();
        assert_eq!(squeezed.sweep, 0.0);
    }

    #[test]
    fn a_zero_segment_count_is_one_segment() {
        assert_eq!(segments(&style(0, 3.0), 1.0, 0.0).count(), 1);
        assert_eq!(
            segments(&style(200, 0.0), 1.0, 0.0).count(),
            MAX_ARC_SEGMENTS
        );
    }
}
