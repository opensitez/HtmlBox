//! Event handler IDL attributes — `onclick`, `onload`, and the other 95.
//!
//! HTML §8.1.7.2 defines an event handler as an internal slot holding at most
//! ONE listener. `el.onclick = f` replaces whatever was there; `el.onclick =
//! null` clears it. That is the whole difference from `addEventListener`, which
//! appends, and it is why these cannot just be a naming convention over the
//! listener list — setting `onclick` twice must leave one handler, not two.
//!
//! The names are exhaustive and come from the specs' own IDL blocks
//! (`GlobalEventHandlers`, `WindowEventHandlers`,
//! `DocumentAndElementEventHandlers`), so this is a SURFACE rather than an
//! arbitrary string API: `is_event_handler_name` answers whether a browser has
//! the attribute, and the tables can be enumerated to check coverage.

/// `GlobalEventHandlers` — on every `HTMLElement`, `Document` and `Window`.
pub const GLOBAL_EVENT_HANDLERS: &[&str] = &[
    "onabort", "onauxclick", "onbeforeinput", "onbeforematch", "onbeforetoggle",
    "onblur", "oncancel", "oncanplay", "oncanplaythrough", "onchange", "onclick",
    "onclose", "oncommand", "oncontextlost", "oncontextmenu", "oncontextrestored",
    "oncopy", "oncuechange", "oncut", "ondblclick", "ondrag", "ondragend",
    "ondragenter", "ondragleave", "ondragover", "ondragstart", "ondrop",
    "ondurationchange", "onemptied", "onended", "onerror", "onfocus",
    "onformdata", "oninput", "oninvalid", "onkeydown", "onkeypress", "onkeyup",
    "onload", "onloadeddata", "onloadedmetadata", "onloadstart", "onmousedown",
    "onmouseenter", "onmouseleave", "onmousemove", "onmouseout", "onmouseover",
    "onmouseup", "onpaste", "onpause", "onplay", "onplaying", "onprogress",
    "onratechange", "onreset", "onresize", "onscroll", "onscrollend",
    "onsecuritypolicyviolation", "onseeked", "onseeking", "onselect",
    "onslotchange", "onstalled", "onsubmit", "onsuspend", "ontimeupdate",
    "ontoggle", "onvolumechange", "onwaiting", "onwebkitanimationend",
    "onwebkitanimationiteration", "onwebkitanimationstart",
    "onwebkittransitionend", "onwheel",
];

/// `WindowEventHandlers` — on `Window` and `<body>`/`<frameset>`.
pub const WINDOW_EVENT_HANDLERS: &[&str] = &[
    "onafterprint", "onbeforeprint", "onbeforeunload", "onhashchange",
    "onlanguagechange", "onmessage", "onmessageerror", "onoffline", "ononline",
    "onpagehide", "onpagereveal", "onpageshow", "onpageswap", "onpopstate",
    "onrejectionhandled", "onstorage", "onunhandledrejection", "onunload",
];

/// `DocumentAndElementEventHandlers`.
pub const DOCUMENT_AND_ELEMENT_EVENT_HANDLERS: &[&str] = &["oncopy", "oncut", "onpaste"];

/// Is `name` an event handler IDL attribute a browser has?
pub fn is_event_handler_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    GLOBAL_EVENT_HANDLERS.contains(&n.as_str())
        || WINDOW_EVENT_HANDLERS.contains(&n.as_str())
        || DOCUMENT_AND_ELEMENT_EVENT_HANDLERS.contains(&n.as_str())
}

/// The event type an `on*` attribute handles — `onclick` → `click`.
///
/// The four `onwebkit*` names are the exception: they are aliases whose event
/// type is the UNPREFIXED one, so `onwebkitanimationend` handles
/// `animationend` (HTML §8.1.7.2.1). Stripping `on` would give
/// `webkitanimationend`, a type nothing ever fires.
pub fn event_type_for_handler(name: &str) -> Option<String> {
    let n = name.to_ascii_lowercase();
    if !is_event_handler_name(&n) { return None; }
    Some(match n.as_str() {
        "onwebkitanimationend"       => "animationend".to_string(),
        "onwebkitanimationiteration" => "animationiteration".to_string(),
        "onwebkitanimationstart"     => "animationstart".to_string(),
        "onwebkittransitionend"      => "transitionend".to_string(),
        other => other.strip_prefix("on")?.to_string(),
    })
}

/// The handler attribute name for an event type — `click` → `onclick`.
/// `None` when no handler attribute exists for that type.
pub fn handler_name_for_event_type(event_type: &str) -> Option<&'static str> {
    let want = format!("on{}", event_type.to_ascii_lowercase());
    GLOBAL_EVENT_HANDLERS.iter()
        .chain(WINDOW_EVENT_HANDLERS.iter())
        .chain(DOCUMENT_AND_ELEMENT_EVENT_HANDLERS.iter())
        .find(|n| **n == want.as_str())
        .copied()
}

/// Every handler attribute name, deduplicated. `oncopy`/`oncut`/`onpaste`
/// appear in two mixins.
pub fn all_event_handler_names() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = GLOBAL_EVENT_HANDLERS.iter()
        .chain(WINDOW_EVENT_HANDLERS.iter())
        .chain(DOCUMENT_AND_ELEMENT_EVENT_HANDLERS.iter())
        .copied()
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}
