//! Allocation-free pointer, keyboard and gamepad-ready input routing.

use crate::{Rect, Vec2, WidgetId, WidgetTree};

/// Device-independent navigation action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbstractAction {
    /// Focus the next widget.
    Next,
    /// Focus the previous widget.
    Previous,
    /// Navigate left.
    Left,
    /// Navigate right.
    Right,
    /// Activate the focused widget.
    Accept,
    /// Close/cancel the current UI layer.
    Cancel,
}

/// Abstract pointer button.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerButton {
    /// Primary/select button.
    Primary,
    /// Secondary/context button.
    Secondary,
    /// Middle button.
    Middle,
}

/// One platform-normalized input event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputEvent {
    /// Pointer motion.
    PointerMove(Vec2),
    /// Pointer button press.
    PointerPress {
        position: Vec2,
        button: PointerButton,
    },
    /// Pointer button release.
    PointerRelease {
        position: Vec2,
        button: PointerButton,
    },
    /// Pointer wheel delta at the current pointer position.
    PointerWheel { position: Vec2, delta: Vec2 },
    /// Pointer left the containing surface.
    PointerLeave,
    /// Keyboard/gamepad navigation.
    Action(AbstractAction),
}

/// Semantic event delivered to widgets or the owning screen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiEventKind {
    /// Pointer entered the target.
    HoverEnter,
    /// Pointer moved while remaining over the target.
    Hover,
    /// Pointer left the target.
    HoverLeave,
    /// Pointer pressed the target.
    Press,
    /// Pointer released over the target.
    Release,
    /// Pointer dragged the target.
    Drag,
    /// Press and release completed on the same target.
    Click,
    /// Pointer wheel moved over a scrollable target.
    Wheel,
    /// Keyboard focus changed to the target.
    Focus,
    /// Focused target was activated.
    Activate,
    /// Current UI layer should close or cancel.
    Cancel,
}

/// Routed UI event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiEvent {
    /// Target widget, absent for screen-level events such as cancel.
    pub target: Option<WidgetId>,
    /// Semantic event kind.
    pub kind: UiEventKind,
    /// Pointer position when applicable.
    pub position: Option<Vec2>,
    /// Pointer movement for drag events.
    pub delta: Option<Vec2>,
}

/// Fixed-capacity input router with focus and pointer capture state.
#[derive(Debug)]
pub struct InputRouter {
    events: Vec<UiEvent>,
    focus_order: Vec<WidgetId>,
    capacity: usize,
    focused: Option<usize>,
    hovered: Option<WidgetId>,
    pressed: Option<(WidgetId, PointerButton)>,
    pointer_position: Option<Vec2>,
}

impl InputRouter {
    /// Allocate queues for at most `capacity` routed events/focusable widgets.
    pub fn new(capacity: usize) -> Self {
        Self {
            events: Vec::with_capacity(capacity),
            focus_order: Vec::with_capacity(capacity),
            capacity,
            focused: None,
            hovered: None,
            pressed: None,
            pointer_position: None,
        }
    }

    /// Begin a frame and rebuild focus order without reallocating.
    pub fn begin_frame(&mut self, tree: &WidgetTree) {
        self.events.clear();
        self.focus_order.clear();
        self.focus_order.extend(
            tree.nodes()
                .iter()
                .filter(|widget| widget.visible && widget.focusable)
                .take(self.capacity)
                .map(|widget| widget.id),
        );
        if self.focus_order.is_empty() {
            self.focused = None;
        } else if self
            .focused
            .is_none_or(|index| index >= self.focus_order.len())
        {
            self.focused = Some(0);
        }
    }

    /// Route one normalized event against physical widget rectangles.
    pub fn route(&mut self, input: InputEvent, tree: &WidgetTree, rects: &[Rect]) {
        self.route_with_scroll(input, tree, rects, &[]);
    }

    /// Route an event while applying each ancestor's nested scroll offset.
    ///
    /// `scroll_offsets` is indexed like `tree.nodes()`. A container's offset
    /// moves its descendants, but not the container's own hit rectangle.
    pub fn route_with_scroll(
        &mut self,
        input: InputEvent,
        tree: &WidgetTree,
        rects: &[Rect],
        scroll_offsets: &[Vec2],
    ) {
        match input {
            InputEvent::PointerMove(position) => {
                self.move_pointer(position, tree, rects, scroll_offsets);
            }
            InputEvent::PointerPress { position, button } => {
                self.move_pointer(position, tree, rects, scroll_offsets);
                let target = hit_test(tree, rects, scroll_offsets, position);
                self.pressed = target.map(|target| (target, button));
                self.emit(target, UiEventKind::Press, Some(position), None);
            }
            InputEvent::PointerRelease { position, button } => {
                self.move_pointer(position, tree, rects, scroll_offsets);
                let hit = hit_test(tree, rects, scroll_offsets, position);
                let captured = self
                    .pressed
                    .filter(|(_, pressed_button)| *pressed_button == button)
                    .map(|(target, _)| target);
                self.emit(captured.or(hit), UiEventKind::Release, Some(position), None);
                if hit.is_some() && hit == captured {
                    self.set_focus(hit);
                    self.emit(hit, UiEventKind::Click, Some(position), None);
                    if button == PointerButton::Primary {
                        self.emit(hit, UiEventKind::Activate, Some(position), None);
                    }
                }
                self.pressed = None;
            }
            InputEvent::PointerWheel { position, delta } => {
                self.move_pointer(position, tree, rects, scroll_offsets);
                let hit = hit_test(tree, rects, scroll_offsets, position);
                // A row stacked over a scroll region without being its child
                // (flat trees) still scrolls the region under it.
                let target = hit
                    .and_then(|widget| scroll_target(tree, widget))
                    .or_else(|| scrollable_under(tree, rects, scroll_offsets, position));
                self.emit(target, UiEventKind::Wheel, Some(position), Some(delta));
            }
            InputEvent::PointerLeave => {
                if let Some(previous) = self.hovered.take() {
                    self.emit(Some(previous), UiEventKind::HoverLeave, None, None);
                }
                self.pointer_position = None;
            }
            InputEvent::Action(AbstractAction::Next | AbstractAction::Right) => self.advance(1),
            InputEvent::Action(AbstractAction::Previous | AbstractAction::Left) => self.advance(-1),
            InputEvent::Action(AbstractAction::Accept) => {
                let target = self
                    .focused
                    .and_then(|index| self.focus_order.get(index).copied());
                self.emit(target, UiEventKind::Activate, None, None);
            }
            InputEvent::Action(AbstractAction::Cancel) => {
                self.emit(None, UiEventKind::Cancel, None, None);
            }
        }
    }

    /// Routed events for this frame.
    pub fn events(&self) -> &[UiEvent] {
        &self.events
    }

    /// Currently focused widget.
    pub fn focused(&self) -> Option<WidgetId> {
        self.focused
            .and_then(|index| self.focus_order.get(index).copied())
    }

    /// Widget currently under the pointer.
    pub fn hovered(&self) -> Option<WidgetId> {
        self.hovered
    }

    /// Widget retaining pointer capture until the matching release.
    pub fn pressed(&self) -> Option<WidgetId> {
        self.pressed.map(|(widget, _)| widget)
    }

    /// Move focus to a specific focusable widget already present this frame.
    ///
    /// This lets a retained screen restore its semantic selection after a
    /// rebuild without synthesizing navigation events.
    pub fn focus(&mut self, widget: WidgetId) -> bool {
        let Some(index) = self
            .focus_order
            .iter()
            .position(|candidate| *candidate == widget)
        else {
            return false;
        };
        self.focused = Some(index);
        self.emit(Some(widget), UiEventKind::Focus, None, None);
        true
    }

    /// Queue/focus storage capacities, exposed for allocation gates.
    pub fn storage_capacities(&self) -> (usize, usize) {
        (self.events.capacity(), self.focus_order.capacity())
    }

    fn advance(&mut self, direction: isize) {
        if self.focus_order.is_empty() {
            return;
        }
        let current = self.focused.unwrap_or(0) as isize;
        let next = (current + direction).rem_euclid(self.focus_order.len() as isize) as usize;
        self.focused = Some(next);
        self.emit(Some(self.focus_order[next]), UiEventKind::Focus, None, None);
    }

    fn move_pointer(
        &mut self,
        position: Vec2,
        tree: &WidgetTree,
        rects: &[Rect],
        scroll_offsets: &[Vec2],
    ) {
        let hit = hit_test(tree, rects, scroll_offsets, position);
        if hit != self.hovered {
            if let Some(previous) = self.hovered {
                self.emit(
                    Some(previous),
                    UiEventKind::HoverLeave,
                    Some(position),
                    None,
                );
            }
            if let Some(current) = hit {
                self.emit(Some(current), UiEventKind::HoverEnter, Some(position), None);
            }
            self.hovered = hit;
        } else {
            self.emit(hit, UiEventKind::Hover, Some(position), None);
        }
        if let (Some((target, _)), Some(previous)) = (self.pressed, self.pointer_position) {
            let delta = Vec2::new(position.x - previous.x, position.y - previous.y);
            if delta != Vec2::default() {
                self.emit(Some(target), UiEventKind::Drag, Some(position), Some(delta));
            }
        }
        self.pointer_position = Some(position);
    }

    fn set_focus(&mut self, target: Option<WidgetId>) {
        let Some(target) = target else {
            return;
        };
        if let Some(index) = self.focus_order.iter().position(|widget| *widget == target) {
            self.focused = Some(index);
            self.emit(Some(target), UiEventKind::Focus, None, None);
        }
    }

    fn emit(
        &mut self,
        target: Option<WidgetId>,
        kind: UiEventKind,
        position: Option<Vec2>,
        delta: Option<Vec2>,
    ) {
        if self.events.len() < self.capacity {
            self.events.push(UiEvent {
                target,
                kind,
                position,
                delta,
            });
        }
    }
}

fn hit_test(
    tree: &WidgetTree,
    rects: &[Rect],
    scroll_offsets: &[Vec2],
    point: Vec2,
) -> Option<WidgetId> {
    let mut best = None;
    for (index, (widget, rect)) in tree.nodes().iter().zip(rects).enumerate() {
        if !widget.visible || (!widget.focusable && !widget.scrollable) {
            continue;
        }
        let rect = scrolled_rect(tree, *rect, scroll_offsets, index);
        if rect.contains(point) && best.is_none_or(|(_, z, prior)| (widget.z, index) >= (z, prior))
        {
            best = Some((widget.id, widget.z, index));
        }
    }
    best.map(|(widget, _, _)| widget)
}

fn scrolled_rect(
    tree: &WidgetTree,
    mut rect: Rect,
    scroll_offsets: &[Vec2],
    mut index: usize,
) -> Rect {
    while let Some(parent) = tree.parent_index(index) {
        if let Some(offset) = scroll_offsets.get(parent) {
            rect.x -= offset.x;
            rect.y -= offset.y;
        }
        index = parent;
    }
    rect
}

/// Topmost scrollable widget containing `point`, ignoring focusable-only
/// widgets stacked above it.
fn scrollable_under(
    tree: &WidgetTree,
    rects: &[Rect],
    scroll_offsets: &[Vec2],
    point: Vec2,
) -> Option<WidgetId> {
    let mut best = None;
    for (index, (widget, rect)) in tree.nodes().iter().zip(rects).enumerate() {
        if !widget.visible || !widget.scrollable {
            continue;
        }
        let rect = scrolled_rect(tree, *rect, scroll_offsets, index);
        if rect.contains(point) && best.is_none_or(|(_, z, prior)| (widget.z, index) >= (z, prior))
        {
            best = Some((widget.id, widget.z, index));
        }
    }
    best.map(|(widget, _, _)| widget)
}

fn scroll_target(tree: &WidgetTree, mut widget: WidgetId) -> Option<WidgetId> {
    loop {
        let index = tree.index_of(widget)?;
        if tree.nodes()[index].scrollable {
            return Some(widget);
        }
        widget = tree
            .parent_index(index)
            .and_then(|parent| tree.nodes().get(parent))?
            .id;
    }
}
