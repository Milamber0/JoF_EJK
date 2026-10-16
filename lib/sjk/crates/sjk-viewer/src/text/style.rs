//! Player text-size and letter-spacing preferences for retained UI text.
//!
//! The style is resolved once per frame from `ui_textScale` and
//! `ui_letterSpacing`, carried on [`UiFont`](super::UiFont), and applied while
//! retained draw commands become glyph quads. Screen layouts are unchanged: a
//! larger line grows evenly about the centre of the line it replaces, so rows,
//! buttons and labels keep their positions. The neutral style reproduces the
//! unstyled output exactly.

use sjk_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use sjk_ui::Rect;
use std::sync::atomic::{AtomicU32, Ordering};

/// Menu text size multiplier cvar.
pub(crate) const SCALE_CVAR: &str = "ui_textScale";
/// Extra letter spacing cvar, as a fraction of the text size.
pub(crate) const TRACKING_CVAR: &str = "ui_letterSpacing";
/// Accepted `ui_textScale` range; larger text would overrun menu rows.
pub(crate) const SCALE_RANGE: (f32, f32) = (0.8, 1.2);
/// Accepted `ui_letterSpacing` range.
pub(crate) const TRACKING_RANGE: (f32, f32) = (-0.05, 0.15);

/// Size and tracking applied on top of each retained text command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextStyle {
    /// Multiplier of the command's text size.
    pub(crate) scale: f32,
    /// Extra pen advance per glyph, as a fraction of the (scaled) text size.
    pub(crate) tracking: f32,
}

impl TextStyle {
    /// No change: the layout's own size and spacing.
    pub(crate) const NEUTRAL: Self = Self {
        scale: 1.0,
        tracking: 0.0,
    };

    /// Clamp raw cvar values; a missing or non-finite value keeps the default.
    pub(crate) fn from_cvars(scale: Option<f64>, tracking: Option<f64>) -> Self {
        let read = |value: Option<f64>, default: f32, (low, high): (f32, f32)| {
            value
                .map(|value| value as f32)
                .filter(|value| value.is_finite())
                .map_or(default, |value| value.clamp(low, high))
        };
        Self {
            scale: read(scale, 1.0, SCALE_RANGE),
            tracking: read(tracking, 0.0, TRACKING_RANGE),
        }
    }

    /// Resolve one text command laid out at `size` inside `rect`.
    pub(crate) fn place(self, rect: Rect, size: f32, letter_spacing: f32) -> Placement {
        let styled = size * self.scale;
        // Grow (or shrink) about the centre of the original line box.
        let shift = (styled - size) * 0.5;
        let grow = shift.max(0.0);
        Placement {
            size: styled,
            y: rect.y - shift,
            letter_spacing: letter_spacing + self.tracking * styled,
            bounds: Rect::new(rect.x, rect.y - grow, rect.width, rect.height + grow * 2.0),
        }
    }
}

/// The style the menus draw with, as [`publish`] last set it: its scale and tracking
/// as `f32` bits.
static CURRENT: [AtomicU32; 2] = [AtomicU32::new(1.0_f32.to_bits()), AtomicU32::new(0)];

/// Make `style` the one [`current`] answers, once a frame as the cvars are read.
pub(crate) fn publish(style: TextStyle) {
    CURRENT[0].store(style.scale.to_bits(), Ordering::Relaxed);
    CURRENT[1].store(style.tracking.to_bits(), Ordering::Relaxed);
}

/// The style the menus draw with this frame, for a layout that sets words side by
/// side and must leave each its styled width (the Profile screen's tabs); neutral
/// until [`publish`].
pub(crate) fn current() -> TextStyle {
    TextStyle {
        scale: f32::from_bits(CURRENT[0].load(Ordering::Relaxed)),
        tracking: f32::from_bits(CURRENT[1].load(Ordering::Relaxed)),
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

/// Where and how large one styled text command is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placement {
    /// Line-box height in physical pixels.
    pub(crate) size: f32,
    /// Top of the line box.
    pub(crate) y: f32,
    /// Pen advance added after every glyph.
    pub(crate) letter_spacing: f32,
    /// Layout rectangle widened vertically to fit a grown line.
    pub(crate) bounds: Rect,
}

/// Register the archived menu text cvars before configuration loading.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    for (name, value, help) in [
        (SCALE_CVAR, 1.0, "Menu text size multiplier (0.8 to 1.2)"),
        (
            TRACKING_CVAR,
            0.0,
            "Extra letter spacing for menu and console text, as a fraction of text size",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_style_leaves_layout_untouched() {
        let rect = Rect::new(10.0, 20.0, 300.0, 22.0);
        let placement = TextStyle::NEUTRAL.place(rect, 17.0, 0.4);
        assert_eq!(
            placement,
            Placement {
                size: 17.0,
                y: 20.0,
                letter_spacing: 0.4,
                bounds: rect,
            }
        );
    }

    #[test]
    fn larger_text_grows_about_the_line_centre() {
        let rect = Rect::new(0.0, 100.0, 200.0, 20.0);
        let style = TextStyle {
            scale: 1.2,
            tracking: 0.0,
        };
        let placement = style.place(rect, 10.0, 0.0);
        assert!((placement.size - 12.0).abs() < 1e-5);
        assert!((placement.y - 99.0).abs() < 1e-5);
        assert!((placement.bounds.y - 99.0).abs() < 1e-5);
        assert!((placement.bounds.height - 22.0).abs() < 1e-5);
    }

    #[test]
    fn smaller_text_keeps_the_original_bounds() {
        let rect = Rect::new(0.0, 100.0, 200.0, 20.0);
        let style = TextStyle {
            scale: 0.8,
            tracking: 0.0,
        };
        let placement = style.place(rect, 10.0, 0.0);
        assert!((placement.y - 101.0).abs() < 1e-5);
        assert_eq!(placement.bounds, rect);
    }

    #[test]
    fn tracking_scales_with_the_styled_size() {
        let style = TextStyle {
            scale: 1.0,
            tracking: 0.1,
        };
        let placement = style.place(Rect::new(0.0, 0.0, 10.0, 10.0), 20.0, 1.0);
        assert!((placement.letter_spacing - 3.0).abs() < 1e-5);
    }

    #[test]
    fn cvar_values_are_clamped_and_defaulted() {
        assert_eq!(TextStyle::from_cvars(None, None), TextStyle::NEUTRAL);
        assert_eq!(
            TextStyle::from_cvars(Some(f64::NAN), Some(f64::INFINITY)),
            TextStyle::NEUTRAL
        );
        let style = TextStyle::from_cvars(Some(4.0), Some(-1.0));
        assert_eq!(style.scale, SCALE_RANGE.1);
        assert_eq!(style.tracking, TRACKING_RANGE.0);
    }
}
