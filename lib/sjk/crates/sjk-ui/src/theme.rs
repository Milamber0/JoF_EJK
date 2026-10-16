//! Resolved design tokens shared by widgets.

use crate::Color;

/// Named spacing scale in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpacingTokens {
    /// Extra-small spacing.
    pub xs: f32,
    /// Small spacing.
    pub sm: f32,
    /// Medium spacing.
    pub md: f32,
    /// Large spacing.
    pub lg: f32,
    /// Extra-large spacing.
    pub xl: f32,
}

/// Named corner-radius scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadiusTokens {
    /// Small radius.
    pub sm: f32,
    /// Medium radius.
    pub md: f32,
    /// Large radius.
    pub lg: f32,
}

/// Named typography scale in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeTokens {
    /// Compact caption.
    pub caption: f32,
    /// Body copy.
    pub body: f32,
    /// Emphasized HUD value.
    pub value: f32,
    /// Section title.
    pub title: f32,
}

/// Named animation durations in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionTokens {
    /// Immediate feedback.
    pub fast: u32,
    /// Standard transition.
    pub normal: u32,
    /// Deliberate transition.
    pub slow: u32,
}

/// Fully resolved immutable theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// Main foreground.
    pub foreground: Color,
    /// Muted foreground.
    pub muted: Color,
    /// Translucent surface.
    pub surface: Color,
    /// Strong surface.
    pub surface_strong: Color,
    /// Accent.
    pub accent: Color,
    /// Critical-state accent.
    pub critical: Color,
    /// Spacing tokens.
    pub spacing: SpacingTokens,
    /// Radius tokens.
    pub radii: RadiusTokens,
    /// Typography tokens.
    pub typography: TypeTokens,
    /// Motion tokens.
    pub motion: MotionTokens,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            foreground: Color::new(0.973, 0.987, 1.0, 1.0),
            muted: Color::new(0.798, 0.843, 0.886, 1.0),
            surface: Color::new(0.018, 0.035, 0.055, 0.78),
            surface_strong: Color::new(0.012, 0.026, 0.043, 0.92),
            accent: Color::new(1.0, 0.416, 0.239, 1.0),
            critical: Color::new(1.0, 0.72, 0.68, 1.0),
            spacing: SpacingTokens {
                xs: 4.0,
                sm: 8.0,
                md: 16.0,
                lg: 24.0,
                xl: 32.0,
            },
            radii: RadiusTokens {
                sm: 3.0,
                md: 7.0,
                lg: 12.0,
            },
            typography: TypeTokens {
                caption: 12.0,
                body: 16.0,
                value: 21.0,
                title: 30.0,
            },
            motion: MotionTokens {
                fast: 90,
                normal: 160,
                slow: 260,
            },
        }
    }
}
