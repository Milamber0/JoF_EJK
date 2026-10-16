//! Typed widget identity and fixed-capacity hierarchy storage.

use crate::{LayoutKind, SizeSpec};

/// Stable frame-local widget identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WidgetId(pub u32);

/// One node in a renderer-independent widget tree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Widget {
    /// Typed identifier.
    pub id: WidgetId,
    /// Parent widget, or `None` for a root.
    pub parent: Option<WidgetId>,
    /// Positioning or child-flow policy.
    pub layout: LayoutKind,
    /// Preferred and constrained size.
    pub size: SizeSpec,
    /// Whether this node participates in layout and input.
    pub visible: bool,
    /// Whether keyboard/gamepad focus may stop here.
    pub focusable: bool,
    /// Whether wheel input targets this node when it or a descendant is hit.
    pub scrollable: bool,
    /// Local opacity multiplier.
    pub opacity: f32,
    /// Stable layer order.
    pub z: i16,
}

/// Fixed-capacity hierarchy in parent-before-child order.
#[derive(Debug)]
pub struct WidgetTree {
    nodes: Vec<Widget>,
    parent_indices: Vec<Option<usize>>,
    capacity: usize,
}

impl WidgetTree {
    /// Allocate storage for at most `capacity` widgets.
    pub fn new(capacity: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(capacity),
            parent_indices: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Remove nodes while retaining storage.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.parent_indices.clear();
    }

    /// Append a node, rejecting non-compact ids, missing parents, or overflow.
    ///
    /// IDs are assigned in insertion order (`WidgetId(0)`, `WidgetId(1)`, …),
    /// which makes hierarchy construction constant-time and lookup by id direct.
    pub fn add(&mut self, widget: Widget) -> bool {
        if self.nodes.len() == self.capacity || widget.id.0 as usize != self.nodes.len() {
            return false;
        }
        let parent_index = match widget.parent {
            Some(parent) if (parent.0 as usize) < self.nodes.len() => Some(parent.0 as usize),
            Some(_) => return false,
            None => None,
        };
        self.nodes.push(widget);
        self.parent_indices.push(parent_index);
        true
    }

    /// Ordered nodes.
    pub fn nodes(&self) -> &[Widget] {
        &self.nodes
    }

    /// Find one node by typed id.
    pub fn get(&self, id: WidgetId) -> Option<&Widget> {
        self.nodes.get(id.0 as usize).filter(|node| node.id == id)
    }

    /// Resolve an id to its compact index.
    pub fn index_of(&self, id: WidgetId) -> Option<usize> {
        self.get(id).map(|_| id.0 as usize)
    }

    /// Compact parent index recorded when a node was inserted.
    pub fn parent_index(&self, index: usize) -> Option<usize> {
        self.parent_indices.get(index).copied().flatten()
    }
}
