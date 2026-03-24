//! NodeId-based event dispatch with capture/bubble phases.
//!
//! This is the DOM spec-compliant event system. Listeners are registered on
//! specific nodes (by node_id), and events flow through three phases:
//! 1. **Capture**: root → target (listeners with `capture: true`)
//! 2. **Target**: fire on the target node
//! 3. **Bubble**: target → root (listeners with `capture: false`)
//!
//! This coexists with the old CSS-selector-based EventListeners during migration.

use std::collections::HashMap;
use crate::dom::arena::{DomArena, NodeId};

// ─── Event Phase ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPhase {
    None,
    Capture,
    Target,
    Bubble,
}

// ─── Event ──────────────────────────────────────────────────────────────────

/// DOM event — flows through capture → target → bubble phases.
#[derive(Debug)]
pub struct DomEvent {
    pub event_type: String,
    pub target: u32,          // node_id of the deepest hit element
    pub current_target: u32,  // node_id of the element currently handling
    pub phase: EventPhase,
    /// Mouse/pointer position in document coordinates.
    pub client_x: f32,
    pub client_y: f32,
    /// Mouse button (0=left, 1=middle, 2=right).
    pub button: u8,
    /// Keyboard key code.
    pub key_code: u32,
    pub key: String,
    pub char_code: Option<char>,
    /// Modifier keys.
    pub ctrl_key: bool,
    pub shift_key: bool,
    pub alt_key: bool,
    pub meta_key: bool,
    /// Wheel delta.
    pub delta_x: f32,
    pub delta_y: f32,
    /// Related target (e.g. for mouseenter/leave, the element being entered/left).
    pub related_target: u32,
    /// Call to prevent default browser behavior.
    prevented: bool,
    /// Call to stop event from reaching further listeners.
    stopped: bool,
    /// Call to stop event from reaching listeners on the same element.
    immediate_stopped: bool,
}

impl DomEvent {
    pub fn new(event_type: impl Into<String>, target: u32) -> Self {
        Self {
            event_type: event_type.into(),
            target,
            current_target: 0,
            phase: EventPhase::None,
            client_x: 0.0,
            client_y: 0.0,
            button: 0,
            key_code: 0,
            key: String::new(),
            char_code: None,
            ctrl_key: false,
            shift_key: false,
            alt_key: false,
            meta_key: false,
            delta_x: 0.0,
            delta_y: 0.0,
            related_target: 0,
            prevented: false,
            stopped: false,
            immediate_stopped: false,
        }
    }

    pub fn prevent_default(&mut self) { self.prevented = true; }
    pub fn stop_propagation(&mut self) { self.stopped = true; }
    pub fn stop_immediate_propagation(&mut self) {
        self.stopped = true;
        self.immediate_stopped = true;
    }
    pub fn default_prevented(&self) -> bool { self.prevented }
    pub fn propagation_stopped(&self) -> bool { self.stopped }
}

// ─── Event Handler ──────────────────────────────────────────────────────────

pub type EventHandler = Box<dyn Fn(&mut DomEvent) + Send + Sync>;

struct ListenerEntry {
    id: u32,
    event_type: String,
    handler: EventHandler,
    capture: bool,
}

// ─── Event Target Map ───────────────────────────────────────────────────────

/// Manages event listeners registered on specific nodes.
pub struct EventTargetMap {
    /// Listeners keyed by node_id.
    listeners: HashMap<u32, Vec<ListenerEntry>>,
    next_id: u32,
}

impl EventTargetMap {
    pub fn new() -> Self {
        Self {
            listeners: HashMap::new(),
            next_id: 1,
        }
    }

    /// Register an event listener on a node. Returns a listener ID for removal.
    pub fn add_event_listener(
        &mut self,
        node_id: u32,
        event_type: &str,
        handler: EventHandler,
        capture: bool,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.listeners.entry(node_id).or_default().push(ListenerEntry {
            id,
            event_type: event_type.to_string(),
            handler,
            capture,
        });
        id
    }

    /// Remove a listener by its ID.
    pub fn remove_event_listener(&mut self, listener_id: u32) {
        for entries in self.listeners.values_mut() {
            entries.retain(|e| e.id != listener_id);
        }
        // Remove empty entries
        self.listeners.retain(|_, v| !v.is_empty());
    }

    /// Remove all listeners on a specific node.
    pub fn remove_all_listeners(&mut self, node_id: u32) {
        self.listeners.remove(&node_id);
    }

    /// Check if any listeners are registered.
    pub fn is_empty(&self) -> bool {
        self.listeners.is_empty()
    }

    /// Dispatch an event through the DOM tree with capture → target → bubble.
    /// Returns true if any handler was called.
    pub fn dispatch_event(&self, arena: &DomArena, event: &mut DomEvent) -> bool {
        if event.target == 0 { return false; }

        // Build the propagation path: root → ... → parent → target
        let path = arena.ancestor_chain(NodeId(event.target));
        // ancestor_chain returns [target, parent, ..., root] — reverse for capture order
        let path_root_to_target: Vec<u32> = path.iter().rev().map(|id| id.0).collect();

        let mut any_handled = false;

        // ── Phase 1: Capture (root → target, excluding target) ──
        event.phase = EventPhase::Capture;
        for &node_id in &path_root_to_target[..path_root_to_target.len().saturating_sub(1)] {
            event.current_target = node_id;
            if self.fire_listeners(node_id, event, true) { any_handled = true; }
            if event.stopped { return any_handled; }
        }

        // ── Phase 2: Target ──
        event.phase = EventPhase::Target;
        event.current_target = event.target;
        // At target, both capture and bubble listeners fire
        if self.fire_listeners(event.target, event, true) { any_handled = true; }
        if !event.immediate_stopped {
            if self.fire_listeners(event.target, event, false) { any_handled = true; }
        }
        if event.stopped { return any_handled; }

        // ── Phase 3: Bubble (target → root, excluding target) ──
        event.phase = EventPhase::Bubble;
        let ancestor_count = path_root_to_target.len().saturating_sub(1);
        for i in (0..ancestor_count).rev() {
            let node_id = path_root_to_target[i];
            event.current_target = node_id;
            if self.fire_listeners(node_id, event, false) { any_handled = true; }
            if event.stopped { return any_handled; }
        }

        any_handled
    }

    /// Fire listeners on a specific node for a specific phase.
    fn fire_listeners(&self, node_id: u32, event: &mut DomEvent, capture_phase: bool) -> bool {
        let entries = match self.listeners.get(&node_id) {
            Some(e) => e,
            None => return false,
        };
        let mut any = false;
        for entry in entries {
            if entry.event_type != event.event_type { continue; }
            if entry.capture != capture_phase { continue; }
            (entry.handler)(event);
            any = true;
            if event.immediate_stopped { break; }
        }
        any
    }
}

impl Default for EventTargetMap {
    fn default() -> Self { Self::new() }
}

impl std::fmt::Debug for EventTargetMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventTargetMap")
            .field("listener_count", &self.listeners.values().map(|v| v.len()).sum::<usize>())
            .finish()
    }
}
