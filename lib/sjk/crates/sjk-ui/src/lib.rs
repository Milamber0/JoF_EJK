//! Renderer-independent retained UI primitives.
//!
//! The crate owns logical layout, widget identity, themes, input routing,
//! deterministic animation and renderer-neutral draw commands. Platform and
//! GPU integration deliberately remain with the consuming application.

mod animation;
mod arc;
mod document;
mod draw;
mod geometry;
mod input;
mod layout;

mod theme;
mod tree;

pub use animation::{Easing, Tween};
pub use arc::{
    ArcSegment, MAX_ARC_SEGMENTS, arc_distance, knockout_coverage, knockout_remainder,
    segments as arc_segments, span as arc_span,
};
pub use document::{
    ArcStyle, HudLayoutDocument, HudWidget, HudWidgetKind, StyleOverrides, VisibilityCondition,
};
pub use draw::{
    DrawCommand, DrawList, FontWeight, Gradient, TextAlign, TextId, TextOverflow, TextureId,
};
pub use geometry::{Color, Insets, Rect, Vec2};
pub use input::{AbstractAction, InputEvent, InputRouter, PointerButton, UiEvent, UiEventKind};
pub use layout::{
    Align, Anchor, Axis, Dimension, FlexLayout, LayoutContext, LayoutEngine, LayoutKind,
    LayoutScratch, SizeSpec,
};
pub use theme::{MotionTokens, RadiusTokens, SpacingTokens, Theme, TypeTokens};
pub use tree::{Widget, WidgetId, WidgetTree};

/// Read-only values consumed by data-bound widgets.
///
/// The framework assigns no meaning to binding names. A game/client adapter
/// exposes an intentionally narrow projection rather than raw simulation
/// structures.
pub trait HudDataSource {
    /// Resolve a numeric binding.
    fn number(&self, binding: &str) -> Option<f32>;

    /// Resolve a textual binding.
    fn text(&self, binding: &str) -> Option<&str>;

    /// Resolve a boolean binding or visibility predicate.
    fn flag(&self, binding: &str) -> Option<bool>;
}
