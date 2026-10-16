//! Renderer-neutral retained draw commands.

use serde::{Deserialize, Serialize};

use crate::{Color, Rect};

/// Stable identifier for text owned by the application.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextId(pub u32);

/// Stable identifier for a texture owned by the renderer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextureId(pub u32);

/// Horizontal text alignment inside its rectangle.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    /// Align to the leading edge.
    #[default]
    Start,
    /// Center the run.
    Center,
    /// Align to the trailing edge.
    End,
}

/// Handling for text that exceeds its rectangle.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextOverflow {
    /// Clip at the rectangle boundary.
    #[default]
    Clip,
    /// Replace the tail with an ellipsis.
    Ellipsis,
    /// Wrap onto subsequent lines.
    Wrap,
}

/// Renderer-independent font-weight request for a text run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FontWeight {
    /// Normal body/caption weight.
    #[default]
    Regular,
    /// Strong emphasis without changing the type size.
    Semibold,
}

/// Linear two-color gradient description.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gradient {
    /// Color at the start edge.
    pub start: Color,
    /// Color at the end edge.
    pub end: Color,
    /// `true` for a vertical gradient, `false` for horizontal.
    pub vertical: bool,
}

/// One renderer-independent drawing operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DrawCommand {
    /// Filled rectangle.
    SolidRect { rect: Rect, color: Color },
    /// Filled rounded rectangle.
    RoundedRect {
        rect: Rect,
        radius: f32,
        color: Color,
    },
    /// Filled linear gradient.
    GradientRect {
        rect: Rect,
        radius: f32,
        gradient: Gradient,
    },
    /// Rectangle outline.
    Border {
        rect: Rect,
        radius: f32,
        width: f32,
        color: Color,
    },
    /// Text resolved by the application at submission time.
    Text {
        rect: Rect,
        text: TextId,
        size: f32,
        color: Color,
        align: TextAlign,
        overflow: TextOverflow,
        /// Requested font weight.
        weight: FontWeight,
        /// Additional logical pixels between glyph advances.
        letter_spacing: f32,
    },
    /// Textured rectangle.
    TexturedQuad {
        rect: Rect,
        texture: TextureId,
        color: Color,
    },
    /// Textured rectangle with explicit texture coordinates at its corners
    /// (top-left, top-right, bottom-right, bottom-left), for rotated or
    /// scrolling images.
    TexturedQuadUv {
        rect: Rect,
        texture: TextureId,
        color: Color,
        uv: [[f32; 2]; 4],
    },
    /// Textured rectangle turned `degrees` about `pivot` (positive turns clockwise, as
    /// y grows downwards), as the classic HUD's `R_DrawRotatePic` turns a picture.
    TexturedQuadTurned {
        rect: Rect,
        texture: TextureId,
        color: Color,
        pivot: [f32; 2],
        degrees: f32,
    },
    /// Circular arc stroke with round caps, anti-aliased by the renderer. Angles are
    /// radians in screen space: 0 points right and positive turns clockwise (y grows
    /// downwards). `radius` is the centre line, `width` the stroke thickness.
    ///
    /// The stroke is left out inside `knockout`, a rectangle centred vertically on `center`
    /// (see [`crate::knockout_coverage`]): a translucent shadow there would stack on the
    /// translucent panel already drawn, and the two look like one shape only when the
    /// shadow adds just what the panel leaves ([`crate::knockout_remainder`]), the panel
    /// being the stroke's own colour and alpha.
    Arc {
        center: [f32; 2],
        radius: f32,
        width: f32,
        start: f32,
        sweep: f32,
        color: Color,
        knockout: Option<Rect>,
    },
    /// Straight stroke from `from` to `to` with square ends, anti-aliased along its
    /// sides by the renderer. `width` is the stroke thickness in pixels.
    Line {
        from: [f32; 2],
        to: [f32; 2],
        width: f32,
        color: Color,
    },
    /// Begin clipping descendants.
    PushClip(Rect),
    /// Begin clipping descendants to `rect` with its corners rounded by `radius`: strokes
    /// ([`DrawCommand::Line`]) are cut to the rounded shape, everything else to the
    /// rectangle. Ended by [`DrawCommand::PopClip`] like a plain clip.
    PushRoundedClip { rect: Rect, radius: f32 },
    /// End the most recent clip.
    PopClip,
    /// Draw descendants through `matrix`, a projective transform of window points
    /// (row-major 3x3, applied to `(x, y, 1)` and divided by the third coordinate), for
    /// a card tilted in perspective: shapes, pictures and text are laid out as usual,
    /// then every corner is mapped. Clips are taken before the mapping. Ended by
    /// [`DrawCommand::PopTransform`]; nested ones compose.
    PushTransform([f32; 9]),
    /// End the most recent transform.
    PopTransform,
    /// Multiply descendant opacity.
    PushOpacity(f32),
    /// End the most recent opacity group.
    PopOpacity,
}

/// Fixed-capacity command buffer reused between frames.
#[derive(Debug)]
pub struct DrawList {
    commands: Vec<DrawCommand>,
    capacity: usize,
}

impl DrawList {
    /// Allocate storage once for at most `capacity` commands.
    pub fn new(capacity: usize) -> Self {
        Self {
            commands: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Discard commands without releasing storage.
    pub fn clear(&mut self) {
        self.commands.clear();
    }

    /// Append a command, returning `false` if fixed storage is full.
    pub fn push(&mut self, command: DrawCommand) -> bool {
        if self.commands.len() == self.capacity {
            return false;
        }
        self.commands.push(command);
        true
    }

    /// Keep only the commands `keep` accepts, in their order, without releasing
    /// storage: for a screen that leaves out what a card drawn later covers.
    pub fn retain(&mut self, keep: impl FnMut(&DrawCommand) -> bool) {
        self.commands.retain(keep);
    }

    /// Commands in submission order.
    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }

    /// Number of retained commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Whether no commands are retained.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Fixed command limit.
    pub const fn limit(&self) -> usize {
        self.capacity
    }
}
