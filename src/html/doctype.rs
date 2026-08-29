//! The DOCTYPE token and the quirks-mode algorithm (HTML §13.2.6.4.1).
//!
//! The tokenizer used to throw the doctype away entirely — `Token::Doctype`
//! was a unit variant — which cost two observable things: `document.doctype`,
//! and `document.compatMode`, whose whole input is the doctype's name and its
//! two identifiers.
//!
//! Every row in `QUIRKS_PUBLIC_PREFIXES` and its neighbours is from the spec's
//! own lists, and the boundary cases were then read off Chrome
//! (`/tmp/webcore-html/dt/`): HTML 4.01 Transitional WITHOUT a system
//! identifier is quirks and WITH one is limited-quirks; a name that is not
//! `html` is quirks whatever the identifiers say; the comparison is ASCII
//! case-insensitive.

/// A document's rendering mode. Two of the three collapse in `compatMode`,
/// which is why the mode is stored and the collapse is computed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuirksMode {
    #[default]
    NoQuirks,
    /// Only reachable through the XHTML 1.0 Frameset/Transitional public
    /// identifiers, or the HTML 4.01 pair WITH a system identifier. It exists
    /// to change line-box height and nothing else, so `compatMode` cannot see
    /// it — measured on `dt/d101` and `dt/d103`, which report `CSS1Compat`
    /// exactly as a no-quirks document does.
    LimitedQuirks,
    Quirks,
}

impl QuirksMode {
    /// `document.compatMode` — the two-way collapse.
    pub fn compat_mode(self) -> &'static str {
        match self {
            QuirksMode::Quirks => "BackCompat",
            _ => "CSS1Compat",
        }
    }
}

/// The parsed `<!DOCTYPE …>` token.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Doctype {
    pub name: String,
    /// The empty string when absent — `doctype.publicId` is a `DOMString`,
    /// never null (measured: `<!DOCTYPE html>` answers `""` for both).
    pub public_id: String,
    pub system_id: String,
    /// Set by the tokenizer for a doctype it could not parse at all. Forces
    /// quirks regardless of what was recovered.
    pub force_quirks: bool,
}

/// Public identifiers that mean quirks whatever else is present.
const QUIRKS_PUBLIC_EXACT: &[&str] = &[
    "-//w3o//dtd w3 html strict 3.0//en//",
    "-/w3c/dtd html 4.0 transitional/en",
    "html",
];

/// The system identifier that means quirks on its own.
const QUIRKS_SYSTEM_EXACT: &[&str] =
    &["http://www.ibm.com/data/dtd/v11/ibmxhtml1-transitional.dtd"];

/// Public-identifier prefixes that mean quirks. Straight from the spec list.
const QUIRKS_PUBLIC_PREFIXES: &[&str] = &[
    "+//silmaril//dtd html pro v0r11 19970101//",
    "-//as//dtd html 3.0 aswedit + extensions//",
    "-//advasoft ltd//dtd html 3.0 aswedit + extensions//",
    "-//ietf//dtd html 2.0 level 1//",
    "-//ietf//dtd html 2.0 level 2//",
    "-//ietf//dtd html 2.0 strict level 1//",
    "-//ietf//dtd html 2.0 strict level 2//",
    "-//ietf//dtd html 2.0 strict//",
    "-//ietf//dtd html 2.0//",
    "-//ietf//dtd html 2.1e//",
    "-//ietf//dtd html 3.0//",
    "-//ietf//dtd html 3.2 final//",
    "-//ietf//dtd html 3.2//",
    "-//ietf//dtd html 3//",
    "-//ietf//dtd html level 0//",
    "-//ietf//dtd html level 1//",
    "-//ietf//dtd html level 2//",
    "-//ietf//dtd html level 3//",
    "-//ietf//dtd html strict level 0//",
    "-//ietf//dtd html strict level 1//",
    "-//ietf//dtd html strict level 2//",
    "-//ietf//dtd html strict level 3//",
    "-//ietf//dtd html strict//",
    "-//ietf//dtd html//",
    "-//metrius//dtd metrius presentational//",
    "-//microsoft//dtd internet explorer 2.0 html strict//",
    "-//microsoft//dtd internet explorer 2.0 html//",
    "-//microsoft//dtd internet explorer 2.0 tables//",
    "-//microsoft//dtd internet explorer 3.0 html strict//",
    "-//microsoft//dtd internet explorer 3.0 html//",
    "-//microsoft//dtd internet explorer 3.0 tables//",
    "-//netscape comm. corp.//dtd html//",
    "-//netscape comm. corp.//dtd strict html//",
    "-//o'reilly and associates//dtd html 2.0//",
    "-//o'reilly and associates//dtd html extended 1.0//",
    "-//o'reilly and associates//dtd html extended relaxed 1.0//",
    "-//sq//dtd html 2.0 hotmetal + extensions//",
    "-//softquad software//dtd hotmetal pro 6.0::19990601::extensions to html 4.0//",
    "-//softquad//dtd hotmetal pro 4.0::19971010::extensions to html 4.0//",
    "-//spyglass//dtd html 2.0 extended//",
    "-//sun microsystems corp.//dtd hotjava html//",
    "-//sun microsystems corp.//dtd hotjava strict html//",
    "-//w3c//dtd html 3 1995-03-24//",
    "-//w3c//dtd html 3.2 draft//",
    "-//w3c//dtd html 3.2 final//",
    "-//w3c//dtd html 3.2//",
    "-//w3c//dtd html 3.2s draft//",
    "-//w3c//dtd html 4.0 frameset//",
    "-//w3c//dtd html 4.0 transitional//",
    "-//w3c//dtd html experimental 19960712//",
    "-//w3c//dtd html experimental 970421//",
    "-//w3c//dtd w3 html//",
    "-//w3o//dtd w3 html 3.0//",
    "-//webtechs//dtd mozilla html 2.0//",
    "-//webtechs//dtd mozilla html//",
];

/// The pair whose mode depends on whether a system identifier is present:
/// quirks without one, limited-quirks with one.
const CONDITIONAL_PREFIXES: &[&str] =
    &["-//w3c//dtd html 4.01 frameset//", "-//w3c//dtd html 4.01 transitional//"];

/// Public-identifier prefixes that always mean limited-quirks.
const LIMITED_PREFIXES: &[&str] =
    &["-//w3c//dtd xhtml 1.0 frameset//", "-//w3c//dtd xhtml 1.0 transitional//"];

/// Which mode a document with this doctype is in.
///
/// `None` is "no doctype at all", which is quirks — the single most common way
/// a real page ends up in it.
pub fn quirks_mode(doctype: Option<&Doctype>) -> QuirksMode {
    let Some(dt) = doctype else { return QuirksMode::Quirks };
    // ⛔ The name is checked BEFORE the identifiers. `<!DOCTYPE foo>` is quirks
    // with empty identifiers, which no prefix list would catch (measured).
    if dt.force_quirks || !dt.name.eq_ignore_ascii_case("html") {
        return QuirksMode::Quirks;
    }
    let public = dt.public_id.to_ascii_lowercase();
    let system = dt.system_id.to_ascii_lowercase();
    let has_system = !dt.system_id.is_empty();

    if QUIRKS_PUBLIC_EXACT.contains(&public.as_str())
        || QUIRKS_SYSTEM_EXACT.contains(&system.as_str())
        || QUIRKS_PUBLIC_PREFIXES.iter().any(|p| public.starts_with(p))
    {
        return QuirksMode::Quirks;
    }
    if CONDITIONAL_PREFIXES.iter().any(|p| public.starts_with(p)) {
        return if has_system { QuirksMode::LimitedQuirks } else { QuirksMode::Quirks };
    }
    if LIMITED_PREFIXES.iter().any(|p| public.starts_with(p)) {
        return QuirksMode::LimitedQuirks;
    }
    QuirksMode::NoQuirks
}

/// Parse the inside of a `<!DOCTYPE …>`, with `<!doctype` and `>` stripped.
///
/// Recovery rather than rejection throughout: a malformed doctype still names
/// a document, and the spec's own answer to one is the force-quirks flag, not
/// a discarded token.
pub fn parse_doctype(inner: &str) -> Doctype {
    let rest = inner.trim();
    let mut dt = Doctype::default();
    if rest.is_empty() {
        // `<!DOCTYPE>` — no name at all. The spec sets force-quirks here.
        dt.force_quirks = true;
        return dt;
    }
    let (name, rest) = split_word(rest);
    dt.name = name.to_ascii_lowercase();
    let rest = rest.trim_start();

    let (keyword, rest) = split_word(rest);
    if keyword.eq_ignore_ascii_case("public") {
        let (first, rest) = read_quoted(rest.trim_start());
        match first {
            Some(p) => dt.public_id = p,
            // `PUBLIC` with nothing after it is a parse error.
            None => { dt.force_quirks = true; return dt; }
        }
        if let (Some(s), _) = read_quoted(rest.trim_start()) { dt.system_id = s; }
    } else if keyword.eq_ignore_ascii_case("system") {
        match read_quoted(rest.trim_start()).0 {
            Some(s) => dt.system_id = s,
            None => { dt.force_quirks = true; return dt; }
        }
    } else if !keyword.is_empty() {
        // Something that is neither PUBLIC nor SYSTEM after the name.
        dt.force_quirks = true;
    }
    dt
}

/// Split off the leading run of non-whitespace.
fn split_word(s: &str) -> (&str, &str) {
    match s.find(char::is_whitespace) {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    }
}

/// Read a `"…"` or `'…'` literal. An unterminated one runs to the end, which
/// is what the tokenizer does before it hits the `>`.
fn read_quoted(s: &str) -> (Option<String>, &str) {
    let mut chars = s.char_indices();
    let Some((_, quote)) = chars.next() else { return (None, s) };
    if quote != '"' && quote != '\'' { return (None, s); }
    let body = &s[quote.len_utf8()..];
    match body.find(quote) {
        Some(i) => (Some(body[..i].to_string()), &body[i + quote.len_utf8()..]),
        None => (Some(body.to_string()), ""),
    }
}
