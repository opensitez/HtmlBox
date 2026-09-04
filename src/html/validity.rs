//! Constraint validation — HTML §4.10.19.
//!
//! The interface is one `ValidityState` plus five members on every submittable
//! element (`willValidate`, `validity`, `validationMessage`, `checkValidity`,
//! `reportValidity`, `setCustomValidity`) and two on the form. It is one
//! mechanism, so it is written once here and reflected onto the elements in
//! `dom::api` rather than per control.
//!
//! Chrome-verified (`/tmp/webcore-html/cv.html`). Three answers from that probe
//! that the spec text alone would not have led me to:
//!
//! * **`maxlength` and `minlength` only apply to a value the user edited.**
//!   `<input maxlength=3 value="abcdef">` is VALID. The constraint is on the
//!   dirty value flag (HTML §4.10.5.3.3), not on the string, which is why an
//!   author-set default that violates it does not block submission.
//! * **`readonly` bars a control from validation entirely** — `willValidate`
//!   is false, not just `valid`. So do `disabled`, `type=hidden`,
//!   `<button type=button>`, `<output>` and `<fieldset>`.
//! * **The form has `checkValidity()` but no `willValidate`.**
//!
//! The `validationMessage` wording is implementation-defined; the spec requires
//! only that it be a suitably localised message. The strings here are ours,
//! and the tests assert the FLAG plus a non-empty message rather than Chrome's
//! exact sentence.

/// The eleven booleans of `ValidityState`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ValidityState {
    pub value_missing: bool,
    pub type_mismatch: bool,
    pub pattern_mismatch: bool,
    pub too_long: bool,
    pub too_short: bool,
    pub range_underflow: bool,
    pub range_overflow: bool,
    pub step_mismatch: bool,
    pub bad_input: bool,
    pub custom_error: bool,
}

impl ValidityState {
    /// `validity.valid` — true when no other flag is set. It is derived rather
    /// than stored so it cannot drift out of step with the flags.
    pub fn valid(&self) -> bool {
        !(self.value_missing
            || self.type_mismatch
            || self.pattern_mismatch
            || self.too_long
            || self.too_short
            || self.range_underflow
            || self.range_overflow
            || self.step_mismatch
            || self.bad_input
            || self.custom_error)
    }
}

/// Whether a `type=` keyword puts an `<input>` in a mode that is barred from
/// constraint validation (HTML §4.10.19.2).
pub fn input_type_is_barred(input_type: &str) -> bool {
    matches!(input_type, "hidden" | "reset" | "button")
}

/// The `pattern` attribute is anchored — HTML compiles it as if wrapped in
/// `^(?:…)$` — and the value must match the WHOLE string.
///
/// This is a deliberately small regex engine: the character classes, literals,
/// alternation, grouping and the four quantifiers that appear in real `pattern`
/// attributes. Anything it cannot parse reports "no opinion", so an exotic
/// pattern makes the control valid rather than spuriously invalid — the same
/// direction HTML takes for a pattern that fails to compile.
pub fn pattern_matches(pattern: &str, value: &str) -> Option<bool> {
    let p: Vec<char> = pattern.chars().collect();
    let v: Vec<char> = value.chars().collect();
    let mut ok = false;
    let matched = match_alt(&p, 0, p.len(), &v, 0, &mut |rest| {
        if rest == v.len() {
            ok = true;
        }
        rest == v.len()
    })?;
    Some(matched && ok)
}

/// Match `pattern[start..end]` — a sequence of alternatives — against
/// `value[at..]`, calling `k` with each end position that a full match reaches.
fn match_alt(
    p: &[char],
    start: usize,
    end: usize,
    v: &[char],
    at: usize,
    k: &mut dyn FnMut(usize) -> bool,
) -> Option<bool> {
    for (from, to) in split_alternatives(p, start, end)? {
        if match_seq(p, from, to, v, at, k)? {
            return Some(true);
        }
    }
    Some(false)
}

/// Top-level `|` positions, respecting groups and classes.
fn split_alternatives(p: &[char], start: usize, end: usize) -> Option<Vec<(usize, usize)>> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut in_class = false;
    let mut from = start;
    let mut i = start;
    while i < end {
        match p[i] {
            '\\' => i += 1,
            '[' if !in_class => in_class = true,
            ']' if in_class => in_class = false,
            '(' if !in_class => depth += 1,
            ')' if !in_class => depth = depth.checked_sub(1)?,
            '|' if !in_class && depth == 0 => {
                parts.push((from, i));
                from = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if in_class || depth != 0 {
        return None;
    }
    parts.push((from, end));
    Some(parts)
}

/// Match a sequence of terms with backtracking.
fn match_seq(
    p: &[char],
    start: usize,
    end: usize,
    v: &[char],
    at: usize,
    k: &mut dyn FnMut(usize) -> bool,
) -> Option<bool> {
    if start >= end {
        return Some(k(at));
    }
    let (atom_end, quant_end, min, max) = read_term(p, start, end)?;
    let rest = quant_end;

    // Collect every position reachable by matching the atom `n` times, then
    // try the remainder from the longest first — greedy, as regex is.
    let mut ends = vec![at];
    let mut count = 0usize;
    while max.map_or(true, |m| count < m) {
        let cursor = *ends.last().unwrap();
        let mut next = None;
        match_atom(p, start, atom_end, v, cursor, &mut |e| {
            next = Some(e);
            true
        })?;
        match next {
            Some(e) if e != cursor => {
                ends.push(e);
                count += 1;
            }
            // A zero-width atom would loop forever; one pass is all it can add.
            _ => break,
        }
    }
    while ends.len() > min {
        let e = *ends.last().unwrap();
        if match_seq(p, rest, end, v, e, k)? {
            return Some(true);
        }
        ends.pop();
    }
    if ends.len() == min {
        let e = *ends.last().unwrap();
        if match_seq(p, rest, end, v, e, k)? {
            return Some(true);
        }
    }
    Some(false)
}

/// Find where one atom ends, and read the quantifier after it.
/// Returns `(atom_end, term_end, min, max)`.
fn read_term(p: &[char], start: usize, end: usize) -> Option<(usize, usize, usize, Option<usize>)> {
    let atom_end = match p[start] {
        '\\' if start + 1 < end => start + 2,
        '[' => {
            let mut i = start + 1;
            if i < end && p[i] == '^' {
                i += 1;
            }
            if i < end && p[i] == ']' {
                i += 1;
            }
            while i < end && p[i] != ']' {
                if p[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            if i >= end {
                return None;
            }
            i + 1
        }
        '(' => {
            let mut depth = 1usize;
            let mut i = start + 1;
            let mut in_class = false;
            while i < end && depth > 0 {
                match p[i] {
                    '\\' => i += 1,
                    '[' if !in_class => in_class = true,
                    ']' if in_class => in_class = false,
                    '(' if !in_class => depth += 1,
                    ')' if !in_class => depth -= 1,
                    _ => {}
                }
                i += 1;
            }
            if depth != 0 {
                return None;
            }
            i
        }
        // `^` and `$` are meaningless in an anchored pattern, and `*`/`+`/`?`
        // with nothing in front is not a pattern we can read.
        '*' | '+' | '?' | ')' | ']' | '{' | '}' | '|' => return None,
        _ => start + 1,
    };
    let (term_end, min, max) = match p.get(atom_end) {
        Some('*') => (atom_end + 1, 0, None),
        Some('+') => (atom_end + 1, 1, None),
        Some('?') => (atom_end + 1, 0, Some(1)),
        Some('{') => {
            let close = (atom_end..end).find(|&i| p[i] == '}')?;
            let body: String = p[atom_end + 1..close].iter().collect();
            let (lo, hi) = match body.split_once(',') {
                None => {
                    let n = body.trim().parse().ok()?;
                    (n, Some(n))
                }
                Some((a, "")) => (a.trim().parse().ok()?, None),
                Some((a, b)) => (a.trim().parse().ok()?, Some(b.trim().parse().ok()?)),
            };
            (close + 1, lo, hi)
        }
        _ => (atom_end, 1, Some(1)),
    };
    Some((atom_end, term_end, min, max))
}

fn match_atom(
    p: &[char],
    start: usize,
    end: usize,
    v: &[char],
    at: usize,
    k: &mut dyn FnMut(usize) -> bool,
) -> Option<bool> {
    match p[start] {
        '(' => {
            // Skip a `?:` / `?=`-style prefix; only the non-capturing form is
            // meaningful here, and a lookaround is beyond this engine.
            let mut inner = start + 1;
            if p.get(inner) == Some(&'?') {
                if p.get(inner + 1) == Some(&':') {
                    inner += 2;
                } else {
                    return None;
                }
            }
            match_alt(p, inner, end - 1, v, at, k)
        }
        '[' => {
            if at >= v.len() {
                return Some(false);
            }
            if class_matches(p, start, end, v[at])? {
                Some(k(at + 1))
            } else {
                Some(false)
            }
        }
        '\\' => {
            if at >= v.len() {
                return Some(false);
            }
            if escape_matches(p[start + 1], v[at]) {
                Some(k(at + 1))
            } else {
                Some(false)
            }
        }
        '.' => {
            if at >= v.len() || v[at] == '\n' {
                return Some(false);
            }
            Some(k(at + 1))
        }
        c => {
            if at < v.len() && v[at] == c {
                Some(k(at + 1))
            } else {
                Some(false)
            }
        }
    }
}

fn escape_matches(class: char, c: char) -> bool {
    match class {
        'd' => c.is_ascii_digit(),
        'D' => !c.is_ascii_digit(),
        'w' => c.is_alphanumeric() || c == '_',
        'W' => !(c.is_alphanumeric() || c == '_'),
        's' => c.is_whitespace(),
        'S' => !c.is_whitespace(),
        'n' => c == '\n',
        't' => c == '\t',
        'r' => c == '\r',
        other => c == other,
    }
}

fn class_matches(p: &[char], start: usize, end: usize, c: char) -> Option<bool> {
    let mut i = start + 1;
    let negated = p.get(i) == Some(&'^');
    if negated {
        i += 1;
    }
    let close = end - 1;
    let mut hit = false;
    while i < close {
        if p[i] == '\\' && i + 1 < close {
            if escape_matches(p[i + 1], c) {
                hit = true;
            }
            i += 2;
            continue;
        }
        if i + 2 < close && p[i + 1] == '-' && p[i + 2] != ']' {
            if p[i] <= c && c <= p[i + 2] {
                hit = true;
            }
            i += 3;
            continue;
        }
        if p[i] == c {
            hit = true;
        }
        i += 1;
    }
    Some(hit != negated)
}

/// HTML §4.10.5.1.5 — a valid e-mail address, as the spec's own ABNF-ish
/// regular expression defines it. Deliberately laxer than RFC 5322, which is
/// what the spec says it is: "a willful violation".
pub fn is_valid_email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    let local_ok = local
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+/=?^_`{|}~.-".contains(c));
    if !local_ok {
        return false;
    }
    // A label is alphanumeric with interior hyphens; at least one label.
    domain.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

/// HTML §4.10.5.1.6 — a valid absolute URL. The check is deliberately shallow:
/// a scheme followed by `:`, because `typeMismatch` on a URL field is about
/// "this is not a URL at all", not about reachability.
pub fn is_valid_url(value: &str) -> bool {
    match value.split_once(':') {
        Some((scheme, _)) => {
            !scheme.is_empty()
                && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        }
        None => false,
    }
}
