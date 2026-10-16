//! Logical-pixel layout with anchors, flex flow, DPI and safe areas.

use serde::{Deserialize, Serialize};

use crate::{Insets, Rect, Vec2, WidgetTree};

/// A point on the parent rectangle used to anchor a child.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    /// Top-left corner.
    #[default]
    TopLeft,
    /// Top-center edge.
    Top,
    /// Top-right corner.
    TopRight,
    /// Center-left edge.
    Left,
    /// Rectangle center.
    Center,
    /// Center-right edge.
    Right,
    /// Bottom-left corner.
    BottomLeft,
    /// Bottom-center edge.
    Bottom,
    /// Bottom-right corner.
    BottomRight,
}

/// Primary direction of a flex container.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    /// Lay children from left to right.
    #[default]
    Row,
    /// Lay children from top to bottom.
    Column,
}

/// Cross-axis alignment for flex children.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    /// Align to the leading edge.
    #[default]
    Start,
    /// Center on the cross axis.
    Center,
    /// Align to the trailing edge.
    End,
    /// Fill the available cross axis.
    Stretch,
}

/// A logical size expression.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "unit", content = "value")]
pub enum Dimension {
    /// Use the available extent.
    #[default]
    Auto,
    /// Fixed logical pixels.
    Px(f32),
    /// Fraction of the parent's extent in the range `0..=1`.
    Percent(f32),
}

/// Preferred and constrained widget size.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SizeSpec {
    /// Preferred width.
    pub width: Dimension,
    /// Preferred height.
    pub height: Dimension,
    /// Minimum width in logical pixels.
    #[serde(default)]
    pub min_width: f32,
    /// Minimum height in logical pixels.
    #[serde(default)]
    pub min_height: f32,
    /// Optional maximum width in logical pixels.
    #[serde(default)]
    pub max_width: Option<f32>,
    /// Optional maximum height in logical pixels.
    #[serde(default)]
    pub max_height: Option<f32>,
}

impl Default for SizeSpec {
    fn default() -> Self {
        Self {
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: 0.0,
            min_height: 0.0,
            max_width: None,
            max_height: None,
        }
    }
}

/// Child-flow settings for a flex container.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FlexLayout {
    /// Main flow direction.
    pub axis: Axis,
    /// Gap between consecutive children.
    #[serde(default)]
    pub gap: f32,
    /// Inner padding.
    #[serde(default)]
    pub padding: Insets,
    /// Cross-axis alignment.
    #[serde(default)]
    pub align: Align,
}

/// Widget positioning or container policy.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum LayoutKind {
    /// Place this widget relative to a parent anchor.
    Anchored {
        /// Parent anchor point.
        anchor: Anchor,
        /// Logical offset away from the anchor.
        #[serde(default)]
        offset: Vec2,
    },
    /// Anchor this widget and flow its children.
    Flex {
        /// Parent anchor point for the container itself.
        #[serde(default)]
        anchor: Anchor,
        /// Logical offset away from the anchor.
        #[serde(default)]
        offset: Vec2,
        /// Child flow settings.
        flow: FlexLayout,
    },
}

impl Default for LayoutKind {
    fn default() -> Self {
        Self::Anchored {
            anchor: Anchor::TopLeft,
            offset: Vec2::new(0.0, 0.0),
        }
    }
}

/// Per-frame viewport inputs to layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutContext {
    /// Physical framebuffer dimensions.
    pub viewport_physical: Vec2,
    /// Physical pixels per logical pixel.
    pub dpi_scale: f32,
    /// Safe-area margins in logical pixels.
    pub safe_area: Insets,
}

/// Fixed-capacity layout output and flex cursors.
#[derive(Debug)]
pub struct LayoutScratch {
    rects: Vec<Rect>,
    cursors: Vec<f32>,
    capacity: usize,
}

impl LayoutScratch {
    /// Allocate scratch storage once for `capacity` widgets.
    pub fn new(capacity: usize) -> Self {
        Self {
            rects: Vec::with_capacity(capacity),
            cursors: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Most recently computed physical rectangles in tree order.
    pub fn rects(&self) -> &[Rect] {
        &self.rects
    }

    /// Current allocation capacities, exposed for allocation gates.
    pub fn storage_capacities(&self) -> (usize, usize) {
        (self.rects.capacity(), self.cursors.capacity())
    }
}

/// Stateless layout evaluator.
#[derive(Clone, Copy, Debug, Default)]
pub struct LayoutEngine;

impl LayoutEngine {
    /// Lay out a parent-before-child tree and return physical-pixel rectangles.
    ///
    /// Returns an empty slice if the caller's fixed scratch capacity is too
    /// small. Runtime evaluation is a single hierarchy pass.
    pub fn layout<'a>(
        &self,
        tree: &WidgetTree,
        context: LayoutContext,
        scratch: &'a mut LayoutScratch,
    ) -> &'a [Rect] {
        scratch.rects.clear();
        scratch.cursors.clear();
        if tree.nodes().len() > scratch.capacity || context.dpi_scale <= 0.0 {
            return &scratch.rects;
        }

        let logical_viewport = Rect::new(
            0.0,
            0.0,
            context.viewport_physical.x / context.dpi_scale,
            context.viewport_physical.y / context.dpi_scale,
        )
        .inset(context.safe_area);

        for (index, widget) in tree.nodes().iter().enumerate() {
            let parent_index = tree.parent_index(index);
            let parent = parent_index.map_or(logical_viewport, |parent| scratch.rects[parent]);
            let (width, height) = resolve_size(widget.size, parent);
            let mut rect = if let Some(parent_index) = parent_index {
                match tree.nodes()[parent_index].layout {
                    LayoutKind::Flex { flow, .. } => {
                        flex_child(parent, width, height, flow, scratch.cursors[parent_index])
                    }
                    _ => anchored(widget.layout, parent, width, height),
                }
            } else {
                anchored(widget.layout, parent, width, height)
            };
            if !widget.visible {
                rect.width = 0.0;
                rect.height = 0.0;
            }
            scratch.rects.push(rect);
            scratch.cursors.push(0.0);
            if let Some(parent_index) = parent_index {
                if let LayoutKind::Flex { flow, .. } = tree.nodes()[parent_index].layout {
                    let extent = match flow.axis {
                        Axis::Row => rect.width,
                        Axis::Column => rect.height,
                    };
                    scratch.cursors[parent_index] += extent + flow.gap;
                }
            }
        }

        for rect in &mut scratch.rects {
            *rect = rect.scaled(context.dpi_scale);
        }
        &scratch.rects
    }
}

fn resolve_dimension(dimension: Dimension, available: f32) -> f32 {
    match dimension {
        Dimension::Auto => available,
        Dimension::Px(value) => value,
        Dimension::Percent(value) => available * value.clamp(0.0, 1.0),
    }
}

fn resolve_size(size: SizeSpec, parent: Rect) -> (f32, f32) {
    let width = resolve_dimension(size.width, parent.width)
        .max(size.min_width)
        .min(size.max_width.unwrap_or(f32::MAX));
    let height = resolve_dimension(size.height, parent.height)
        .max(size.min_height)
        .min(size.max_height.unwrap_or(f32::MAX));
    (width, height)
}

fn anchored(kind: LayoutKind, parent: Rect, width: f32, height: f32) -> Rect {
    let (anchor, offset) = match kind {
        LayoutKind::Anchored { anchor, offset } | LayoutKind::Flex { anchor, offset, .. } => {
            (anchor, offset)
        }
    };
    let (x, y) = match anchor {
        Anchor::TopLeft => (parent.x, parent.y),
        Anchor::Top => (parent.x + (parent.width - width) * 0.5, parent.y),
        Anchor::TopRight => (parent.right() - width, parent.y),
        Anchor::Left => (parent.x, parent.y + (parent.height - height) * 0.5),
        Anchor::Center => (
            parent.x + (parent.width - width) * 0.5,
            parent.y + (parent.height - height) * 0.5,
        ),
        Anchor::Right => (
            parent.right() - width,
            parent.y + (parent.height - height) * 0.5,
        ),
        Anchor::BottomLeft => (parent.x, parent.bottom() - height),
        Anchor::Bottom => (
            parent.x + (parent.width - width) * 0.5,
            parent.bottom() - height,
        ),
        Anchor::BottomRight => (parent.right() - width, parent.bottom() - height),
    };
    Rect::new(x + offset.x, y + offset.y, width, height)
}

fn flex_child(parent: Rect, width: f32, height: f32, flow: FlexLayout, cursor: f32) -> Rect {
    let content = parent.inset(flow.padding);
    match flow.axis {
        Axis::Row => {
            let height = if flow.align == Align::Stretch {
                content.height
            } else {
                height
            };
            let y = cross_position(content.y, content.height, height, flow.align);
            Rect::new(content.x + cursor, y, width, height)
        }
        Axis::Column => {
            let width = if flow.align == Align::Stretch {
                content.width
            } else {
                width
            };
            let x = cross_position(content.x, content.width, width, flow.align);
            Rect::new(x, content.y + cursor, width, height)
        }
    }
}

fn cross_position(origin: f32, available: f32, extent: f32, align: Align) -> f32 {
    match align {
        Align::Start | Align::Stretch => origin,
        Align::Center => origin + (available - extent) * 0.5,
        Align::End => origin + available - extent,
    }
}
