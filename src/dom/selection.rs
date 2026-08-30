//! Text selection on form controls — HTML §4.10.19.3.

use crate::types::Document;

// ─── Text selection on form controls (HTML §4.10.19.3) ──────────────────────
//
// Offsets here are **UTF-16 code units**, which is what the IDL says and what
// `character_data_length` above already answers in. The internal cursor is a
// CHAR index, because that is what the key handler and the paint path want, so
// every entry point converts. The two agree on ASCII, which is every existing
// fixture — so the tests for this deliberately use a BMP non-ASCII string and
// an astral one, which fail differently.
//
// `None`/`false` is the thrown exception, following the rest of this file:
// `InvalidStateError` when the control does not support selection, and
// `IndexSizeError` when `setRangeText` gets a start past its end.

/// Char index → UTF-16 offset.
fn char_to_utf16(s: &str, chars: usize) -> usize {
    s.chars().take(chars).map(char::len_utf16).sum()
}

/// UTF-16 offset → char index, rounding **down** to a scalar boundary.
///
/// ⛔ An offset can land between the halves of a surrogate pair — Chrome
/// happily selects one half, and `setRangeText` there yields a lone surrogate.
/// A Rust `String` cannot hold one, so the boundary moves outward instead.
/// This is the single named deviation in this area.
fn utf16_to_char_floor(s: &str, units: usize) -> usize {
    let mut seen = 0usize;
    for (i, c) in s.chars().enumerate() {
        if seen >= units { return i; }
        // The offset falls INSIDE this character — round back to where it
        // starts. Advancing first and testing after would round the other way
        // and cut the pair off at its far edge.
        if seen + c.len_utf16() > units { return i; }
        seen += c.len_utf16();
    }
    s.chars().count()
}

/// UTF-16 offset → char index, rounding **up** to a scalar boundary.
fn utf16_to_char_ceil(s: &str, units: usize) -> usize {
    let mut seen = 0usize;
    for (i, c) in s.chars().enumerate() {
        if seen >= units { return i; }
        seen += c.len_utf16();
        if seen >= units { return i + 1; }
    }
    s.chars().count()
}

impl Document {
    /// The control's value as the selection API sees it, plus its length in
    /// UTF-16 code units.
    ///
    /// A `<textarea>`'s API value is LF-normalised, and the offsets index THAT
    /// — not the raw child text (measured: `setRangeText("X",0,5)` on
    /// `"line1\nline2"` yields `"X\nline2"`).
    fn selection_value(&self, id: u32) -> Option<String> {
        let node = self.find_webcore(id)?;
        if !crate::types::selection_api_applies(node) { return None; }
        Some(self.value(id))
    }

    /// Read (start, end) in UTF-16 units, clamped to the current value.
    fn selection_pair(&self, id: u32) -> Option<(usize, usize)> {
        let value = self.selection_value(id)?;
        let node = self.find_webcore(id)?;
        let len = value.chars().count();
        let cursor = node.input_cursor.min(len);
        let anchor = node.input_sel_anchor.min(len);
        Some((
            char_to_utf16(&value, cursor.min(anchor)),
            char_to_utf16(&value, cursor.max(anchor)),
        ))
    }

    /// Write (start, end) in UTF-16 units. `direction` of `None` leaves the
    /// existing direction alone — which is what the `selectionStart` and
    /// `selectionEnd` setters do, and what `setSelectionRange` does NOT.
    fn store_selection(
        &mut self,
        id: u32,
        start: usize,
        end: usize,
        direction: Option<crate::types::SelectionDirection>,
    ) {
        let Some(value) = self.selection_value(id) else { return };
        // No clamp to the value's length here: `utf16_to_char_floor` saturates,
        // so an offset past the end already lands on the last character. A
        // `.min(len)` in front of it was redundant, and a mutation run proved
        // no test could tell it from its absence.
        //
        // "If end is less than start then set start to end" — the range
        // collapses onto its END, not its start (measured: `(3,1)` → `[1,1]`).
        let start = start.min(end);
        let start_c = utf16_to_char_floor(&value, start);
        let end_c = utf16_to_char_floor(&value, end);
        let Some(node) = self.find_webcore_mut(id) else { return };
        if let Some(d) = direction { node.input_sel_direction = d; }
        // A backward selection's cursor sits at its START — the key handler
        // reads the pair as min/max either way, so only the direction field
        // depends on the ordering.
        if node.input_sel_direction == crate::types::SelectionDirection::Backward {
            node.input_cursor = start_c;
            node.input_sel_anchor = end_c;
        } else {
            node.input_cursor = end_c;
            node.input_sel_anchor = start_c;
        }
    }

    /// `input.selectionStart` / `textarea.selectionStart`.
    ///
    /// `None` is the spec's `null`, which every control the API does not apply
    /// to answers — including `number`, `date` and `email`, all of which hold
    /// a value and accept typing.
    pub fn selection_start(&self, id: u32) -> Option<u32> {
        self.selection_pair(id).map(|(s, _)| s as u32)
    }

    /// `element.selectionEnd`.
    pub fn selection_end(&self, id: u32) -> Option<u32> {
        self.selection_pair(id).map(|(_, e)| e as u32)
    }

    /// `element.selectionDirection` — `"none"`, `"forward"` or `"backward"`.
    pub fn selection_direction(&self, id: u32) -> Option<&'static str> {
        let node = self.find_webcore(id)?;
        if !crate::types::selection_api_applies(node) { return None; }
        Some(node.input_sel_direction.as_str())
    }

    /// `element.selectionStart = n`. `false` is `InvalidStateError`.
    ///
    /// ⛔ Not `set_selection_range` with the current end: a start past the end
    /// drags the END along (measured: start=8 over a (1,5) selection gives
    /// `[8,8]`, not `[5,5]`), and the direction is left untouched.
    pub fn set_selection_start(&mut self, id: u32, start: u32) -> bool {
        let Some((_, end)) = self.selection_pair(id) else { return false };
        let start = start as usize;
        self.store_selection(id, start, end.max(start), None);
        true
    }

    /// `element.selectionEnd = n`. An end before the start pulls the start
    /// back with it (measured: end=0 over a (3,5) selection gives `[0,0]`).
    pub fn set_selection_end(&mut self, id: u32, end: u32) -> bool {
        let Some((start, _)) = self.selection_pair(id) else { return false };
        self.store_selection(id, start, end as usize, None);
        true
    }

    /// `element.selectionDirection = s`.
    pub fn set_selection_direction(&mut self, id: u32, direction: &str) -> bool {
        let Some((start, end)) = self.selection_pair(id) else { return false };
        self.store_selection(id, start, end, Some(crate::types::SelectionDirection::parse(direction)));
        true
    }

    /// `element.setSelectionRange(start, end, direction)`.
    ///
    /// Passing `None` for `direction` is the two-argument call, which RESETS
    /// the direction to `"none"` — the one place it differs from the
    /// `selectionStart` setter.
    pub fn set_selection_range(
        &mut self,
        id: u32,
        start: u32,
        end: u32,
        direction: Option<&str>,
    ) -> bool {
        if self.selection_value(id).is_none() { return false; }
        let d = direction
            .map(crate::types::SelectionDirection::parse)
            .unwrap_or(crate::types::SelectionDirection::None);
        self.store_selection(id, start as usize, end as usize, Some(d));
        self.fire_select_event(id);
        true
    }

    /// `element.select()` — select everything the control holds.
    ///
    /// ⛔ Unlike every other member here, this one is NOT gated on the API
    /// applying: Chrome runs `checkbox.select()`, `number.select()` and
    /// `file.select()` without complaint. The spec's step is "if this has no
    /// selectable text, return" — a return, not a throw.
    pub fn select(&mut self, id: u32) {
        let Some(value) = self.selection_value(id) else { return };
        let len = value.encode_utf16().count();
        self.store_selection(id, 0, len, Some(crate::types::SelectionDirection::None));
        self.fire_select_event(id);
    }

    /// `element.setRangeText(replacement, start, end, selectMode)`.
    ///
    /// `range` of `None` is the one-argument call, which uses the current
    /// selection. `false` is `InvalidStateError` (the API does not apply) or
    /// `IndexSizeError` (`start` past `end`); an unrecognised `select_mode` is
    /// the IDL enum's `TypeError` and is likewise `false`.
    pub fn set_range_text(
        &mut self,
        id: u32,
        replacement: &str,
        range: Option<(u32, u32)>,
        select_mode: &str,
    ) -> bool {
        let Some(value) = self.selection_value(id) else { return false };
        if !matches!(select_mode, "select" | "start" | "end" | "preserve") { return false; }
        let (sel_start, sel_end) = match self.selection_pair(id) {
            Some(p) => p,
            None => return false,
        };
        let (mut start, mut end) = match range {
            Some((s, e)) => {
                // IndexSizeError. Checked BEFORE clamping, so `(10, 20)` on a
                // two-character value is legal and `(3, 1)` is not.
                if s > e { return false; }
                (s as usize, e as usize)
            }
            None => (sel_start, sel_end),
        };
        let max = value.encode_utf16().count();
        start = start.min(max);
        end = end.min(max);

        // Round the replaced range OUTWARD to whole scalar values. See
        // `utf16_to_char_floor`: a boundary inside a surrogate pair is
        // representable in Chrome and not in a Rust `String`.
        let start_c = utf16_to_char_floor(&value, start);
        let end_c = utf16_to_char_ceil(&value, end);
        let chars: Vec<char> = value.chars().collect();
        let mut next: String = chars[..start_c].iter().collect();
        next.push_str(replacement);
        next.extend(chars[end_c.min(chars.len())..].iter());

        // Recompute the offsets from what was actually cut, so the arithmetic
        // below stays consistent when a boundary moved.
        let start = char_to_utf16(&value, start_c);
        let end = char_to_utf16(&value, end_c);
        let new_length = replacement.encode_utf16().count();
        let new_end = start + new_length;

        // The plain `value` setter's "move the cursor to the end" is harmless
        // here: `store_selection` below positions the selection unconditionally
        // and clamps it to the value just written.
        self.set_value(id, &next);

        let (mut new_start, mut new_sel_end) = (sel_start, sel_end);
        match select_mode {
            "select" => { new_start = start; new_sel_end = new_end; }
            "start" => { new_start = start; new_sel_end = start; }
            "end" => { new_start = new_end; new_sel_end = new_end; }
            // The one that carries arithmetic. Verified against Chrome for all
            // seven positions the old selection can take relative to the
            // replaced range: before, after, straddling either edge, fully
            // inside, and with the replacement both growing and shrinking.
            _ => {
                let old_length = end.saturating_sub(start);
                let delta = new_length as i64 - old_length as i64;
                // An offset PAST the replaced range slides by the size change;
                // one INSIDE it collapses onto the edge it is nearest in role
                // — the start onto the range's start, the end onto its new end.
                if sel_start > end { new_start = (sel_start as i64 + delta).max(0) as usize; }
                else if sel_start > start { new_start = start; }
                if sel_end > end { new_sel_end = (sel_end as i64 + delta).max(0) as usize; }
                else if sel_end > start { new_sel_end = new_end; }
            }
        }
        // `setRangeText` never carries a direction — the range it leaves is
        // directionless whatever the selection it replaced was (measured).
        self.store_selection(id, new_start, new_sel_end, Some(crate::types::SelectionDirection::None));
        self.fire_select_event(id);
        true
    }

    /// `select` does not bubble and is not cancelable.
    ///
    /// ⛔ The spec QUEUES this task; there is no task queue in this crate, and
    /// `fire_invalid_event` already made the same trade. A listener runs — it
    /// just runs before the caller returns rather than after.
    fn fire_select_event(&mut self, id: u32) {
        let mut event = crate::dom::events::DomEvent::new("select", id);
        self.dispatch_event(&mut event);
    }
}
