//! Small serializable geometry/value types used across the UI framework.

use serde::{Deserialize, Serialize};

/// Two-dimensional logical or physical coordinate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    /// Horizontal component.
    pub x: f32,
    /// Vertical component.
    pub y: f32,
}

impl Vec2 {
    /// Construct a coordinate.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Axis-aligned rectangle expressed as origin and extent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl Rect {
    /// Construct a rectangle.
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Right edge.
    pub fn right(self) -> f32 {
        self.x + self.width
    }

    /// Bottom edge.
    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    /// Whether a point falls inside the rectangle.
    pub fn contains(self, point: Vec2) -> bool {
        point.x >= self.x
            && point.x <= self.right()
            && point.y >= self.y
            && point.y <= self.bottom()
    }

    /// Whether the two rectangles share some area (touching edges do not count).
    pub fn overlaps(self, other: Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// Inset the rectangle, saturating negative extents at zero.
    pub fn inset(self, insets: Insets) -> Self {
        Self {
            x: self.x + insets.left,
            y: self.y + insets.top,
            width: (self.width - insets.left - insets.right).max(0.0),
            height: (self.height - insets.top - insets.bottom).max(0.0),
        }
    }

    /// Convert all components from logical to physical pixels.
    pub fn scaled(self, scale: f32) -> Self {
        Self::new(
            self.x * scale,
            self.y * scale,
            self.width * scale,
            self.height * scale,
        )
    }
}

/// Four-sided inset in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Insets {
    /// Top inset.
    pub top: f32,
    /// Right inset.
    pub right: f32,
    /// Bottom inset.
    pub bottom: f32,
    /// Left inset.
    pub left: f32,
}

impl Insets {
    /// Equal inset on every side.
    pub const fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

/// Straight-alpha color token in display (sRGB-encoded) values: the viewer draws
/// the 2D layer without a hardware sRGB encode and blends these values as retail did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Color {
    /// Red channel.
    pub r: f32,
    /// Green channel.
    pub g: f32,
    /// Blue channel.
    pub b: f32,
    /// Alpha channel.
    pub a: f32,
}

impl Color {
    /// Construct a color.
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

#[cfg(test)]
mod tests {
    use super::Rect;

    #[test]
    fn rectangles_overlap_only_when_they_share_area() {
        let card = Rect::new(10.0, 10.0, 100.0, 50.0);
        assert!(card.overlaps(Rect::new(100.0, 50.0, 40.0, 20.0)));
        assert!(card.overlaps(Rect::new(0.0, 0.0, 300.0, 300.0)));
        assert!(
            !card.overlaps(Rect::new(110.0, 10.0, 40.0, 20.0)),
            "touching on the right"
        );
        assert!(
            !card.overlaps(Rect::new(10.0, 60.0, 40.0, 20.0)),
            "touching below"
        );
        assert!(!card.overlaps(Rect::new(200.0, 200.0, 5.0, 5.0)));
    }
}
