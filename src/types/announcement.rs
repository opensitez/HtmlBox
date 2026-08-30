//! `aria-live` announcement types.

#![allow(unused_imports)]
use super::*;
use std::collections::{HashMap, HashSet};
use crate::css::*;
use crate::dom::*;
use crate::html::*;

// ─── aria-live announcement types ─────────────────────────────────────────────

/// How urgently an aria-live announcement should be delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivePoliteness {
    /// No announcement (aria-live="off" or no attribute).
    Off,
    /// Deliver after the user's current action completes (aria-live="polite").
    Polite,
    /// Interrupt the user immediately (aria-live="assertive").
    Assertive,
}

/// An accessibility announcement queued by a change to an `aria-live` region.
#[derive(Debug, Clone)]
pub struct Announcement {
    /// Text content to announce.
    pub text:        String,
    /// Urgency — how the host or AT should prioritise this announcement.
    pub politeness:  LivePoliteness,
    /// `true` when `aria-atomic="true"` was set on the region.
    /// Hosts should announce the full `text`; `false` means only the diff matters.
    pub atomic:      bool,
}
