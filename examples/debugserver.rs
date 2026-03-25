//! debugserver — headless debug server for remote control of rhtmledit.
//!
//! Loads a URL and listens for JSON commands over TCP (or stdin).
//! Lets you inspect, click, hover, type, scroll, and screenshot without restarting.
//!
//! Usage:
//!   cargo run --example debugserver -- <url> [OPTIONS]
//!
//! Options:
//!   --url <url>        URL or file path to render (or first positional arg)
//!   --port <n>         TCP port to listen on (default: 9222)
//!   --stdin            Read commands from stdin instead of TCP
//!   --width <px>       Viewport width in CSS pixels  (default: 1280)
//!   --height <px>      Viewport height in CSS pixels  (default: 900)
//!   --max-height <px>  Max render height (default: 4000)
//!   --scale <n>        Device pixel ratio (default: 1)
//!   --out <file.png>   Screenshot output path (default: snapshot.png)
//!   --no-images        Skip image fetching
//!   --no-cache         Skip cache
//!   --cache-dir <dir>  Cache directory (default: snapshot_cache)
//!
//! Commands (JSON, one per line):
//!   {"cmd":"screenshot"}                       → render + save PNG
//!   {"cmd":"screenshot","out":"other.png"}     → render + save to path
//!   {"cmd":"inspect","selector":"#id"}         → inspect element
//!   {"cmd":"click","x":100,"y":200}            → click at coordinates
//!   {"cmd":"click","selector":"#btn"}          → click center of element
//!   {"cmd":"hover","x":100,"y":200}            → hover at coordinates
//!   {"cmd":"hover","selector":".menu"}         → hover center of element
//!   {"cmd":"type","text":"hello"}              → type into focused element
//!   {"cmd":"key","key":"Enter"}                → send key
//!   {"cmd":"scroll","dy":100}                  → scroll viewport
//!   {"cmd":"resize","width":800}               → resize viewport + re-layout
//!   {"cmd":"navigate","url":"..."}             → load new page
//!   {"cmd":"tree"}                             → dump box tree
//!   {"cmd":"tree","selector":"div.foo"}        → dump subtree matching selector
//!   {"cmd":"find","selector":"a"}              → list all matching elements
//!   {"cmd":"text","selector":"#el"}            → get text content of element
//!   {"cmd":"attr","selector":"#el","name":"href"} → get attribute value
//!   {"cmd":"quit"}                             → shut down

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::mpsc;

use tiny_skia::{Color, Pixmap};

use rhtmledit::{parse_html_with_hooks, Renderer, Document};
use rhtmledit::css::apply_cascade_vp;
use rhtmledit::types::{Display, HtmlBox};
use rhtmledit::HtmlEventType;

// ─── Minimal JSON helpers (no serde dependency) ──────────────────────────────

/// Escape a string for JSON output.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\x20' => out.push_str(&format!("\\u{:04x}", c as u32)),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Extract a string value for a key from a JSON object string.
fn json_str(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let pos = json.find(&needle)?;
    let after = &json[pos + needle.len()..];
    let after = after.trim_start().strip_prefix(':')?;
    let after = after.trim_start();
    if after.starts_with('"') {
        // Find the closing quote (handle escapes)
        let s = &after[1..];
        let mut end = 0;
        let mut escaped = false;
        for c in s.chars() {
            if escaped { escaped = false; end += c.len_utf8(); continue; }
            if c == '\\' { escaped = true; end += 1; continue; }
            if c == '"' { break; }
            end += c.len_utf8();
        }
        Some(s[..end].replace("\\\"", "\"").replace("\\n", "\n").replace("\\\\", "\\"))
    } else {
        None
    }
}

/// Extract a numeric value for a key from a JSON object string.
fn json_num(json: &str, key: &str) -> Option<f32> {
    let needle = format!("\"{}\"", key);
    let pos = json.find(&needle)?;
    let after = &json[pos + needle.len()..];
    let after = after.trim_start().strip_prefix(':')?;
    let after = after.trim_start();
    let end = after.find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-').unwrap_or(after.len());
    after[..end].parse().ok()
}

// ─── Timing ───────────────────────────────────────────────────────────────────

use std::time::Instant;

#[derive(Default, Clone)]
struct Timing {
    /// Individual stage timings in microseconds
    stages: Vec<(String, u64)>,
    /// Integer stats (node counts, rule counts, etc.)
    stats: Vec<(String, u64)>,
    /// Total wall time in microseconds
    total_us: u64,
}

impl Timing {
    fn new() -> Self { Self::default() }

    fn to_json(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let total_ms = self.total_us as f64 / 1000.0;
        parts.push(format!(r#""total_ms":{:.2}"#, total_ms));
        for (name, us) in &self.stages {
            let ms = *us as f64 / 1000.0;
            parts.push(format!(r#"{}:{:.2}"#, json_escape(name), ms));
        }
        for (name, val) in &self.stats {
            parts.push(format!(r#"{}:{}"#, json_escape(name), val));
        }
        format!("{{{}}}", parts.join(","))
    }
}

/// Measure a block and push a named stage to the timing.
macro_rules! timed {
    ($timing:expr, $name:expr, $block:expr) => {{
        let _t0 = Instant::now();
        let _result = $block;
        $timing.stages.push(($name.to_string(), _t0.elapsed().as_micros() as u64));
        _result
    }};
}

// ─── Engine state ─────────────────────────────────────────────────────────────

struct EngineState {
    doc: Document,
    renderer: Renderer,
    url: String,
    width: f32,
    viewport_h: f32,
    max_h: f32,
    scale: f32,
    out: String,
    no_images: bool,
    no_cache: bool,
    cache_dir: String,
    /// Timing from the last load_url call
    load_timing: Timing,
    /// Timing from the last command
    last_cmd_timing: Option<(String, Timing)>,
    /// Chrome CDP port (0 = no chrome)
    chrome_port: u16,
}

impl EngineState {
    fn layout(&mut self) {
        let eng = self.renderer.layout_engine();
        eng.viewport_h = self.viewport_h;
        eng.layout(&mut self.doc, self.width);
        // Run layout twice to match real-world behavior (browsers re-layout
        // on resize, hover, image load, etc.) and catch accumulation bugs.
        eng.layout(&mut self.doc, self.width);
    }

    fn layout_no_cascade(&mut self) {
        let eng = self.renderer.layout_engine();
        eng.layout_no_cascade(&mut self.doc, self.width);
    }

    fn render_screenshot(&mut self, path: &str) -> String {
        let t0 = Instant::now();
        let doc_h = (self.doc.root.margin_rect.h.ceil() as u32).max(1);
        let render_h = doc_h.min(self.max_h as u32).max(1);
        let phys_w = (self.width * self.scale) as u32;
        let phys_h = (render_h as f32 * self.scale) as u32;

        let mut pixmap = match Pixmap::new(phys_w.max(1), phys_h.max(1)) {
            Some(p) => p,
            None => return r#"{"ok":false,"error":"pixmap allocation failed"}"#.to_string(),
        };
        pixmap.fill(Color::WHITE);

        let t_render = Instant::now();
        self.renderer.render(&mut self.doc, &mut pixmap, self.scale);
        let render_ms = t_render.elapsed().as_micros() as f64 / 1000.0;

        let t_save = Instant::now();
        let result = pixmap.save_png(path);
        let save_ms = t_save.elapsed().as_micros() as f64 / 1000.0;
        let total_ms = t0.elapsed().as_micros() as f64 / 1000.0;

        match result {
            Ok(_) => format!(
                r#"{{"ok":true,"path":{},"width":{},"height":{},"timing_ms":{{"render":{:.2},"save_png":{:.2},"total":{:.2}}}}}"#,
                json_escape(path), phys_w, phys_h, render_ms, save_ms, total_ms),
            Err(e) => format!(r#"{{"ok":false,"error":{}}}"#, json_escape(&e.to_string())),
        }
    }

    fn do_click(&mut self, x: f32, y: f32) -> String {
        let pt = (x, y + self.doc.scroll_y);
        self.doc.process_mouse_event(HtmlEventType::MouseDown, pt, 0);
        self.doc.process_mouse_event(HtmlEventType::MouseUp, pt, 0);
        self.layout_no_cascade();
        // Return what element was hit
        let hit = rhtmledit::point_to_hit(&self.doc.root, pt, 0);
        match hit {
            Some(h) => {
                let node = unsafe { &*h.box_ptr };
                let tag = &node.tag;
                let id = node.attributes.get("id").map(|v| v.as_str()).unwrap_or("");
                let cls = node.attributes.get("class").map(|v| v.as_str()).unwrap_or("");
                format!(r#"{{"ok":true,"hit":{{"tag":{},"id":{},"class":{},"x":{:.0},"y":{:.0}}}}}"#,
                    json_escape(tag), json_escape(id), json_escape(cls), x, y)
            }
            None => format!(r#"{{"ok":true,"hit":null}}"#),
        }
    }

    fn do_hover(&mut self, x: f32, y: f32) -> String {
        let pt = (x, y + self.doc.scroll_y);
        let changed = self.doc.process_mouse_event(HtmlEventType::MouseMove, pt, 0);
        if changed {
            self.layout_no_cascade();
        }
        let hit = rhtmledit::point_to_hit(&self.doc.root, pt, 0);
        match hit {
            Some(h) => {
                let node = unsafe { &*h.box_ptr };
                let tag = &node.tag;
                let id = node.attributes.get("id").map(|v| v.as_str()).unwrap_or("");
                format!(r#"{{"ok":true,"changed":{},"tag":{},"id":{}}}"#,
                    changed, json_escape(tag), json_escape(id))
            }
            None => format!(r#"{{"ok":true,"changed":{},"hit":null}}"#, changed),
        }
    }

    fn do_type_text(&mut self, text: &str) -> String {
        let mut any = false;
        for ch in text.chars() {
            if self.doc.process_key_event(HtmlEventType::KeyDown, ch as u32, Some(ch), false, false, false, false) {
                any = true;
            }
        }
        if any {
            self.layout_no_cascade();
        }
        format!(r#"{{"ok":true,"typed":{},"chars":{}}}"#, any, text.len())
    }

    fn do_key(&mut self, key: &str) -> String {
        let (code, ch) = match key {
            "Enter" => (13, Some('\r')),
            "Tab"   => (9, Some('\t')),
            "Backspace" => (8, None),
            "Delete" => (46, None),
            "Escape" => (27, None),
            "ArrowLeft" => (37, None),
            "ArrowRight" => (39, None),
            "ArrowUp" => (38, None),
            "ArrowDown" => (40, None),
            "Home" => (36, None),
            "End" => (35, None),
            "Space" => (32, Some(' ')),
            s if s.len() == 1 => (s.chars().next().unwrap() as u32, s.chars().next()),
            _ => return format!(r#"{{"ok":false,"error":"unknown key: {}"}}"#, json_escape(key)),
        };
        let changed = self.doc.process_key_event(HtmlEventType::KeyDown, code, ch, false, false, false, false);
        if changed { self.layout_no_cascade(); }
        format!(r#"{{"ok":true,"changed":{}}}"#, changed)
    }

    fn do_scroll(&mut self, dy: f32) -> String {
        self.doc.scroll_y = (self.doc.scroll_y + dy).max(0.0);
        format!(r#"{{"ok":true,"scroll_y":{:.0}}}"#, self.doc.scroll_y)
    }

    fn do_visible(&self) -> String {
        let sy = self.doc.scroll_y;
        let vh = self.viewport_h;
        let mut elements = Vec::new();
        Document::walk_all(&self.doc.root, &mut |node| {
            if node.tag == "#text" { return; }
            let cr = &node.content_rect;
            if cr.w <= 0.0 || cr.h <= 0.0 { return; }
            let bottom = cr.y + cr.h;
            if bottom < sy || cr.y > sy + vh { return; }
            let id = node.attributes.get("id").map(|v| v.as_str()).unwrap_or("");
            let cls = node.attributes.get("class").map(|v| v.as_str()).unwrap_or("");
            // Only include meaningful elements (not deep inline wrappers)
            if matches!(node.style.display, Display::Block | Display::Flex | Display::InlineFlex
                | Display::InlineBlock | Display::Table | Display::TableRow | Display::TableCell
                | Display::Grid | Display::ListItem | Display::FlowRoot) {
                let mut text = String::new();
                collect_text(node, &mut text);
                let preview: String = text.chars().take(60).collect();
                elements.push(format!(
                    r#"{{"tag":{},"id":{},"class":{},"display":{},"x":{:.0},"y":{:.0},"w":{:.0},"h":{:.0},"text":{}}}"#,
                    json_escape(&node.tag), json_escape(id), json_escape(cls),
                    json_escape(&format!("{:?}", node.style.display)),
                    cr.x, cr.y, cr.w, cr.h, json_escape(preview.trim())
                ));
            }
        });
        format!(r#"{{"ok":true,"scroll_y":{:.0},"viewport_h":{:.0},"count":{},"elements":[{}]}}"#,
            sy, vh, elements.len(), elements.join(","))
    }

    fn do_scroll_to(&mut self, selector: &str) -> String {
        let mut target_y: Option<f32> = None;
        Document::walk_all(&self.doc.root, &mut |node| {
            if target_y.is_some() { return; }
            if matches_query(node, selector) {
                target_y = Some(node.content_rect.y);
            }
        });
        match target_y {
            Some(y) => {
                self.doc.scroll_y = (y - 20.0).max(0.0); // small margin above
                format!(r#"{{"ok":true,"scroll_y":{:.0}}}"#, self.doc.scroll_y)
            }
            None => format!(r#"{{"ok":false,"error":"no element matches {}"}}"#, json_escape(selector)),
        }
    }

    fn do_resize(&mut self, w: Option<f32>, h: Option<f32>) -> String {
        if let Some(w) = w { self.width = w; }
        if let Some(h) = h { self.viewport_h = h; }
        // Re-cascade (media queries, vh/vw units may change) then layout
        self.doc.stylesheet.resolve_variables_for_viewport(self.width, self.viewport_h);
        apply_cascade_vp(&mut self.doc.root, &self.doc.stylesheet, None, 16.0,
            self.width, self.viewport_h, 0, false);
        self.layout();
        format!(r#"{{"ok":true,"width":{},"height":{}}}"#, self.width, self.viewport_h)
    }

    fn selector_center(&self, selector: &str) -> Option<(f32, f32)> {
        let mut result = None;
        Document::walk_all(&self.doc.root, &mut |node| {
            if result.is_some() { return; }
            if matches_query(node, selector) {
                let r = &node.content_rect;
                result = Some((r.x + r.w / 2.0, r.y + r.h / 2.0));
            }
        });
        result
    }

    fn do_find(&self, selector: &str) -> String {
        let mut results = Vec::new();
        Document::walk_all(&self.doc.root, &mut |node| {
            if matches_query(node, selector) {
                let id = node.attributes.get("id").map(|v| v.as_str()).unwrap_or("");
                let cls = node.attributes.get("class").map(|v| v.as_str()).unwrap_or("");
                let r = &node.content_rect;
                results.push(format!(
                    r#"{{"tag":{},"id":{},"class":{},"x":{:.0},"y":{:.0},"w":{:.0},"h":{:.0}}}"#,
                    json_escape(&node.tag), json_escape(id), json_escape(cls),
                    r.x, r.y, r.w, r.h
                ));
            }
        });
        format!(r#"{{"ok":true,"count":{},"elements":[{}]}}"#, results.len(), results.join(","))
    }

    fn do_text(&self, selector: &str) -> String {
        let mut texts = Vec::new();
        Document::walk_all(&self.doc.root, &mut |node| {
            if matches_query(node, selector) {
                let mut text = String::new();
                collect_text(node, &mut text);
                texts.push(text);
            }
        });
        let items: Vec<String> = texts.iter().map(|t| json_escape(t)).collect();
        format!(r#"{{"ok":true,"count":{},"texts":[{}]}}"#, items.len(), items.join(","))
    }

    fn do_attr(&self, selector: &str, name: &str) -> String {
        let mut values = Vec::new();
        Document::walk_all(&self.doc.root, &mut |node| {
            if matches_query(node, selector) {
                if let Some(v) = node.attributes.get(name) {
                    values.push(json_escape(v));
                }
            }
        });
        format!(r#"{{"ok":true,"count":{},"values":[{}]}}"#, values.len(), values.join(","))
    }

    fn do_inspect(&self, selector: &str) -> String {
        let mut parts = Vec::new();
        Document::walk_all(&self.doc.root, &mut |node| {
            if matches_query(node, selector) {
                parts.push(inspect_json(node));
            }
        });
        format!(r#"{{"ok":true,"count":{},"elements":[{}]}}"#, parts.len(), parts.join(","))
    }

    fn do_deep(&self, selector: &str) -> String {
        let mut parts = Vec::new();
        Document::walk_all(&self.doc.root, &mut |node| {
            if matches_query(node, selector) {
                parts.push(deep_inspect_json(node));
            }
        });
        format!(r#"{{"ok":true,"count":{},"elements":[{}]}}"#, parts.len(), parts.join(","))
    }

    fn do_subtree(&self, selector: &str) -> String {
        let mut parts = Vec::new();
        Document::walk_all(&self.doc.root, &mut |node| {
            if matches_query(node, selector) {
                let mut nodes = Vec::new();
                subtree_dump(node, 0, &mut nodes);
                let container = &node.content_rect;
                // Build response with container info + all descendants
                parts.push(format!(
                    r#"{{"container":{{"tag":{},"id":{},"class":{},"x":{:.1},"y":{:.1},"w":{:.1},"h":{:.1},"right":{:.1},"bottom":{:.1}}},"descendants":[{}]}}"#,
                    json_escape(&node.tag),
                    json_escape(node.attributes.get("id").map(|v| v.as_str()).unwrap_or("")),
                    json_escape(node.attributes.get("class").map(|v| v.as_str()).unwrap_or("")),
                    container.x, container.y, container.w, container.h,
                    container.x + container.w, container.y + container.h,
                    nodes.join(",")
                ));
            }
        });
        format!(r#"{{"ok":true,"count":{},"elements":[{}]}}"#, parts.len(), parts.join(","))
    }

    fn do_css(&self, selector: &str, props: &[String]) -> String {
        let mut results = Vec::new();
        Document::walk_all(&self.doc.root, &mut |node| {
            if matches_query(node, selector) {
                let mut kvs = Vec::new();
                for prop in props {
                    let val = get_css_property(node, prop);
                    kvs.push(format!("{}:{}", json_escape(prop), json_escape(&val)));
                }
                let tag_info = format!("\"tag\":{},\"id\":{},\"class\":{}",
                    json_escape(&node.tag),
                    json_escape(node.attributes.get("id").map(|v| v.as_str()).unwrap_or("")),
                    json_escape(node.attributes.get("class").map(|v| v.as_str()).unwrap_or("")));
                results.push(format!("{{{},{}}}", tag_info, kvs.join(",")));
            }
        });
        format!(r#"{{"ok":true,"count":{},"elements":[{}]}}"#, results.len(), results.join(","))
    }

    fn do_setattr(&mut self, selector: &str, name: &str, value: &str, relayout: bool) -> String {
        let mut count = 0;
        Document::walk_all_mut(&mut self.doc.root, &mut |node| {
            if matches_query(node, selector) {
                node.attributes.insert(name.to_string(), value.to_string());
                count += 1;
            }
        });
        if relayout && count > 0 { self.layout(); }
        format!(r#"{{"ok":true,"modified":{}}}"#, count)
    }

    fn do_setstyle(&mut self, selector: &str, prop: &str, value: &str) -> String {
        let mut count = 0;
        Document::walk_all_mut(&mut self.doc.root, &mut |node| {
            if matches_query(node, selector) {
                rhtmledit::css::apply_property(&mut node.style, prop, value);
                node.layout_dirty = true;
                count += 1;
            }
        });
        if count > 0 { self.layout(); }
        format!(r#"{{"ok":true,"modified":{}}}"#, count)
    }

    fn do_html(&self, selector: &str) -> String {
        let mut results = Vec::new();
        Document::walk_all(&self.doc.root, &mut |node| {
            if matches_query(node, selector) {
                let mut buf = String::new();
                serialize_html(node, &mut buf, 0);
                results.push(json_escape(&buf));
            }
        });
        format!(r#"{{"ok":true,"count":{},"html":[{}]}}"#, results.len(), results.join(","))
    }

    fn do_matched_rules(&self, selector: &str) -> String {
        let mut results = Vec::new();
        Document::walk_all(&self.doc.root, &mut |node| {
            if matches_query(node, selector) {
                let id = node.attributes.get("id").map(|v| v.as_str()).unwrap_or("");
                let cls = node.attributes.get("class").map(|v| v.as_str()).unwrap_or("");
                let rules: Vec<String> = node.matched_rules.iter().map(|r| {
                    let decls: Vec<String> = r.declarations.iter()
                        .filter(|(k, _)| !k.starts_with("--"))
                        .map(|(k, v)| format!("{}:{}", json_escape(k), json_escape(v)))
                        .collect();
                    format!(r#"{{"selector":{},"specificity":{},"source":{},"declarations":{{{}}}}}"#,
                        json_escape(&r.selector), r.specificity,
                        json_escape(&r.source), decls.join(","))
                }).collect();
                results.push(format!(
                    r#"{{"tag":{},"id":{},"class":{},"rules":[{}]}}"#,
                    json_escape(&node.tag), json_escape(id), json_escape(cls),
                    rules.join(",")));
            }
        });
        format!(r#"{{"ok":true,"count":{},"elements":[{}]}}"#, results.len(), results.join(","))
    }

    fn do_computed(&self, selector: &str) -> String {
        let mut results = Vec::new();
        Document::walk_all(&self.doc.root, &mut |node| {
            if matches_query(node, selector) {
                results.push(computed_json(node));
            }
        });
        format!(r#"{{"ok":true,"count":{},"elements":[{}]}}"#, results.len(), results.join(","))
    }

    fn do_highlight(&mut self, selector: &str, path: &str) -> String {
        // First render the page normally
        let doc_h = (self.doc.root.margin_rect.h.ceil() as u32).max(1);
        let render_h = doc_h.min(self.max_h as u32).max(1);
        let phys_w = (self.width * self.scale) as u32;
        let phys_h = (render_h as f32 * self.scale) as u32;

        let mut pixmap = match Pixmap::new(phys_w.max(1), phys_h.max(1)) {
            Some(p) => p,
            None => return r#"{"ok":false,"error":"pixmap allocation failed"}"#.to_string(),
        };
        pixmap.fill(Color::WHITE);
        self.renderer.render(&mut self.doc, &mut pixmap, self.scale);

        // Find matching elements and draw overlay on each
        let mut count = 0;
        let mut rects = Vec::new();
        Document::walk_all(&self.doc.root, &mut |node| {
            if matches_query(node, selector) {
                rhtmledit::draw_inspect_overlay(node, &mut pixmap, 0.0, 0.0, self.scale);
                let cr = &node.content_rect;
                rects.push(format!(r#"[{:.0},{:.0},{:.0},{:.0}]"#, cr.x, cr.y, cr.w, cr.h));
                count += 1;
            }
        });

        match pixmap.save_png(path) {
            Ok(_) => format!(r#"{{"ok":true,"path":{},"highlighted":{},"rects":[{}]}}"#,
                json_escape(path), count, rects.join(",")),
            Err(e) => format!(r#"{{"ok":false,"error":{}}}"#, json_escape(&e.to_string())),
        }
    }

    fn do_dom_path(&self, selector: &str) -> String {
        let mut results = Vec::new();
        walk_with_parent(&self.doc.root, &Vec::new(), &mut |node, parents| {
            if matches_query(node, selector) {
                let mut path_parts = Vec::new();
                for p in parents {
                    path_parts.push(dom_path_segment(p));
                }
                path_parts.push(dom_path_segment(node));
                results.push(json_escape(&path_parts.join(" > ")));
            }
        });
        format!(r#"{{"ok":true,"count":{},"paths":[{}]}}"#, results.len(), results.join(","))
    }

    fn do_parent(&self, selector: &str) -> String {
        // Walk tree, find matching node, report its parent chain
        let mut results = Vec::new();
        walk_with_parent(&self.doc.root, &Vec::new(), &mut |node, parents| {
            if matches_query(node, selector) {
                let chain: Vec<String> = parents.iter().map(|p| {
                    let id = p.attributes.get("id").map(|v| v.as_str()).unwrap_or("");
                    let cls = p.attributes.get("class").map(|v| v.as_str()).unwrap_or("");
                    format!(r#"{{"tag":{},"id":{},"class":{},"x":{:.0},"y":{:.0},"w":{:.0},"h":{:.0}}}"#,
                        json_escape(&p.tag), json_escape(id), json_escape(cls),
                        p.content_rect.x, p.content_rect.y, p.content_rect.w, p.content_rect.h)
                }).collect();
                results.push(format!("[{}]", chain.join(",")));
            }
        });
        format!(r#"{{"ok":true,"count":{},"chains":[{}]}}"#, results.len(), results.join(","))
    }

    fn do_tree(&self, selector: Option<&str>) -> String {
        let mut buf = String::new();
        match selector {
            Some(sel) => {
                Document::walk_all(&self.doc.root, &mut |node| {
                    if matches_query(node, sel) {
                        dump_box_to_string(0, node, &mut buf);
                    }
                });
            }
            None => dump_box_to_string(0, &self.doc.root, &mut buf),
        }
        format!(r#"{{"ok":true,"tree":{}}}"#, json_escape(&buf))
    }

    fn do_perf(&self) -> String {
        let load = &self.load_timing;
        let last = match &self.last_cmd_timing {
            Some((cmd, t)) => format!(r#","last_cmd":{},"last_cmd_timing":{}"#,
                json_escape(cmd), t.to_json()),
            None => String::new(),
        };
        format!(r#"{{"ok":true,"load_timing":{}{}}}"#, load.to_json(), last)
    }

    fn do_bench(&mut self, iterations: u32) -> String {
        let n = iterations.max(1).min(100);

        // Benchmark layout
        let mut layout_times = Vec::new();
        for _ in 0..n {
            let t = Instant::now();
            self.layout();
            layout_times.push(t.elapsed().as_micros() as f64 / 1000.0);
        }

        // Benchmark render
        let doc_h = (self.doc.root.margin_rect.h.ceil() as u32).max(1);
        let render_h = doc_h.min(self.max_h as u32).max(1);
        let phys_w = (self.width * self.scale) as u32;
        let phys_h = (render_h as f32 * self.scale) as u32;
        let mut render_times = Vec::new();
        for _ in 0..n {
            if let Some(mut pixmap) = Pixmap::new(phys_w.max(1), phys_h.max(1)) {
                pixmap.fill(Color::WHITE);
                let t = Instant::now();
                self.renderer.render(&mut self.doc, &mut pixmap, self.scale);
                render_times.push(t.elapsed().as_micros() as f64 / 1000.0);
            }
        }

        // Benchmark cascade
        let mut cascade_times = Vec::new();
        for _ in 0..n {
            let t = Instant::now();
            apply_cascade_vp(&mut self.doc.root, &self.doc.stylesheet, None, 16.0,
                self.width, self.max_h, 0, false);
            cascade_times.push(t.elapsed().as_micros() as f64 / 1000.0);
        }

        let avg = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let min = |v: &[f64]| v.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = |v: &[f64]| v.iter().cloned().fold(0.0f64, f64::max);

        format!(concat!(
            r#"{{"ok":true,"iterations":{},"#,
            r#""cascade_ms":{{"avg":{:.2},"min":{:.2},"max":{:.2}}},"#,
            r#""layout_ms":{{"avg":{:.2},"min":{:.2},"max":{:.2}}},"#,
            r#""render_ms":{{"avg":{:.2},"min":{:.2},"max":{:.2}}},"#,
            r#""total_avg_ms":{:.2}}}"#),
            n,
            avg(&cascade_times), min(&cascade_times), max(&cascade_times),
            avg(&layout_times), min(&layout_times), max(&layout_times),
            avg(&render_times), min(&render_times), max(&render_times),
            avg(&cascade_times) + avg(&layout_times) + avg(&render_times),
        )
    }

    fn handle_command(&mut self, line: &str) -> String {
        let line = line.trim();
        if line.is_empty() { return String::new(); }

        let cmd_start = Instant::now();
        let cmd = json_str(line, "cmd").unwrap_or_default();
        let mut result = match cmd.as_str() {
            "screenshot" => {
                let path = json_str(line, "out").unwrap_or_else(|| self.out.clone());
                self.render_screenshot(&path)
            }
            "click" => {
                if let Some(sel) = json_str(line, "selector") {
                    match self.selector_center(&sel) {
                        Some((x, y)) => self.do_click(x, y),
                        None => format!(r#"{{"ok":false,"error":"no element matches {}"}}"#, json_escape(&sel)),
                    }
                } else if let (Some(x), Some(y)) = (json_num(line, "x"), json_num(line, "y")) {
                    self.do_click(x, y)
                } else {
                    r#"{"ok":false,"error":"click needs x,y or selector"}"#.to_string()
                }
            }
            "hover" => {
                if let Some(sel) = json_str(line, "selector") {
                    match self.selector_center(&sel) {
                        Some((x, y)) => self.do_hover(x, y),
                        None => format!(r#"{{"ok":false,"error":"no element matches {}"}}"#, json_escape(&sel)),
                    }
                } else if let (Some(x), Some(y)) = (json_num(line, "x"), json_num(line, "y")) {
                    self.do_hover(x, y)
                } else {
                    r#"{"ok":false,"error":"hover needs x,y or selector"}"#.to_string()
                }
            }
            "type" => {
                match json_str(line, "text") {
                    Some(t) => self.do_type_text(&t),
                    None => r#"{"ok":false,"error":"type needs text"}"#.to_string(),
                }
            }
            "key" => {
                match json_str(line, "key") {
                    Some(k) => self.do_key(&k),
                    None => r#"{"ok":false,"error":"key needs key name"}"#.to_string(),
                }
            }
            "scroll" => {
                if let Some(sel) = json_str(line, "selector") {
                    self.do_scroll_to(&sel)
                } else if let Some(y) = json_num(line, "y") {
                    self.doc.scroll_y = y.max(0.0);
                    format!(r#"{{"ok":true,"scroll_y":{:.0}}}"#, self.doc.scroll_y)
                } else {
                    let dy = json_num(line, "dy").unwrap_or(0.0);
                    self.do_scroll(dy)
                }
            }
            "visible" => {
                self.do_visible()
            }
            "resize" => {
                self.do_resize(json_num(line, "width"), json_num(line, "height"))
            }
            "navigate" => {
                match json_str(line, "url") {
                    Some(u) => {
                        let new_url = normalize_url(u);
                        match self.load_url(&new_url) {
                            Ok(_) => format!(r#"{{"ok":true,"url":{}}}"#, json_escape(&new_url)),
                            Err(e) => format!(r#"{{"ok":false,"error":{}}}"#, json_escape(&e)),
                        }
                    }
                    None => r#"{"ok":false,"error":"navigate needs url"}"#.to_string(),
                }
            }
            "tree" => {
                let sel = json_str(line, "selector");
                self.do_tree(sel.as_deref())
            }
            "find" => {
                match json_str(line, "selector") {
                    Some(s) => self.do_find(&s),
                    None => r#"{"ok":false,"error":"find needs selector"}"#.to_string(),
                }
            }
            "text" => {
                match json_str(line, "selector") {
                    Some(s) => self.do_text(&s),
                    None => r#"{"ok":false,"error":"text needs selector"}"#.to_string(),
                }
            }
            "attr" => {
                match (json_str(line, "selector"), json_str(line, "name")) {
                    (Some(s), Some(n)) => self.do_attr(&s, &n),
                    _ => r#"{"ok":false,"error":"attr needs selector and name"}"#.to_string(),
                }
            }
            "inspect" => {
                match json_str(line, "selector") {
                    Some(s) => self.do_inspect(&s),
                    None => r#"{"ok":false,"error":"inspect needs selector"}"#.to_string(),
                }
            }
            "deep" => {
                match json_str(line, "selector") {
                    Some(s) => self.do_deep(&s),
                    None => r#"{"ok":false,"error":"deep needs selector"}"#.to_string(),
                }
            }
            "css" => {
                match json_str(line, "selector") {
                    Some(s) => {
                        let props_str = json_str(line, "props").unwrap_or_default();
                        let props: Vec<String> = props_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                        if props.is_empty() {
                            r#"{"ok":false,"error":"css needs props (comma-separated)"}"#.to_string()
                        } else {
                            self.do_css(&s, &props)
                        }
                    }
                    None => r#"{"ok":false,"error":"css needs selector and props"}"#.to_string(),
                }
            }
            "setattr" => {
                match (json_str(line, "selector"), json_str(line, "name"), json_str(line, "value")) {
                    (Some(s), Some(n), Some(v)) => self.do_setattr(&s, &n, &v, true),
                    _ => r#"{"ok":false,"error":"setattr needs selector, name, value"}"#.to_string(),
                }
            }
            "setstyle" => {
                match (json_str(line, "selector"), json_str(line, "prop"), json_str(line, "value")) {
                    (Some(s), Some(p), Some(v)) => self.do_setstyle(&s, &p, &v),
                    _ => r#"{"ok":false,"error":"setstyle needs selector, prop, value"}"#.to_string(),
                }
            }
            "html" => {
                match json_str(line, "selector") {
                    Some(s) => self.do_html(&s),
                    None => r#"{"ok":false,"error":"html needs selector"}"#.to_string(),
                }
            }
            "parent" => {
                match json_str(line, "selector") {
                    Some(s) => self.do_parent(&s),
                    None => r#"{"ok":false,"error":"parent needs selector"}"#.to_string(),
                }
            }
            "matched-rules" | "rules" => {
                match json_str(line, "selector") {
                    Some(s) => self.do_matched_rules(&s),
                    None => r#"{"ok":false,"error":"rules needs selector"}"#.to_string(),
                }
            }
            "computed" => {
                match json_str(line, "selector") {
                    Some(s) => self.do_computed(&s),
                    None => r#"{"ok":false,"error":"computed needs selector"}"#.to_string(),
                }
            }
            "highlight" => {
                match json_str(line, "selector") {
                    Some(s) => {
                        let path = json_str(line, "out").unwrap_or_else(|| self.out.clone());
                        self.do_highlight(&s, &path)
                    }
                    None => r#"{"ok":false,"error":"highlight needs selector"}"#.to_string(),
                }
            }
            "subtree" => {
                match json_str(line, "selector") {
                    Some(s) => self.do_subtree(&s),
                    None => r#"{"ok":false,"error":"subtree needs selector"}"#.to_string(),
                }
            }
            "dom-path" | "path" => {
                match json_str(line, "selector") {
                    Some(s) => self.do_dom_path(&s),
                    None => r#"{"ok":false,"error":"path needs selector"}"#.to_string(),
                }
            }
            "chrome-sync" | "sync" => {
                if self.chrome_port > 0 {
                    let sy = self.doc.scroll_y;
                    let params = format!(r#"{{"expression":"window.scrollTo(0,{sy})"}}"#);
                    match cdp_send(self.chrome_port, "Runtime.evaluate", &params) {
                        Ok(_) => format!(r#"{{"ok":true,"synced_scroll_y":{:.0}}}"#, sy),
                        Err(e) => format!(r#"{{"ok":false,"error":{}}}"#, json_escape(&e)),
                    }
                } else {
                    r#"{"ok":false,"error":"Chrome not running (use --chrome)"}"#.to_string()
                }
            }
            "chrome-screenshot" => {
                if self.chrome_port > 0 {
                    let out = json_str(line, "out").unwrap_or_else(|| "chrome_screenshot.png".to_string());
                    let params = r#"{"format":"png"}"#;
                    match cdp_send(self.chrome_port, "Page.captureScreenshot", params) {
                        Ok(resp) => {
                            // Extract base64 data from response
                            if let Some(data_start) = resp.find("\"data\":\"") {
                                let data = &resp[data_start + 8..];
                                if let Some(end) = data.find('"') {
                                    let b64 = &data[..end];
                                    match base64_decode(b64) {
                                        Ok(bytes) => {
                                            match std::fs::write(&out, &bytes) {
                                                Ok(_) => format!(r#"{{"ok":true,"path":{}}}"#, json_escape(&out)),
                                                Err(e) => format!(r#"{{"ok":false,"error":{}}}"#, json_escape(&e.to_string())),
                                            }
                                        }
                                        Err(e) => format!(r#"{{"ok":false,"error":{}}}"#, json_escape(&e)),
                                    }
                                } else {
                                    format!(r#"{{"ok":false,"error":"malformed CDP response"}}"#)
                                }
                            } else {
                                format!(r#"{{"ok":false,"error":"no screenshot data in response"}}"#)
                            }
                        }
                        Err(e) => format!(r#"{{"ok":false,"error":{}}}"#, json_escape(&e)),
                    }
                } else {
                    r#"{"ok":false,"error":"Chrome not running (use --chrome)"}"#.to_string()
                }
            }
            "perf" => {
                self.do_perf()
            }
            "bench" => {
                let iterations = json_num(line, "n").unwrap_or(1.0) as u32;
                self.do_bench(iterations)
            }
            "quit" => {
                std::process::exit(0);
            }
            _ => format!(r#"{{"ok":false,"error":"unknown command: {}"}}"#, json_escape(&cmd)),
        };

        // Record command timing
        let cmd_ms = cmd_start.elapsed().as_micros() as f64 / 1000.0;
        self.last_cmd_timing = Some((cmd.clone(), {
            let mut t = Timing::new();
            t.total_us = cmd_start.elapsed().as_micros() as u64;
            t
        }));

        // Inject timing into response if it's a JSON object
        if result.starts_with(r#"{"ok":true"#) && cmd != "perf" && cmd != "bench" {
            // Insert cmd_ms before the closing brace
            let insert_pos = result.len() - 1;
            result.insert_str(insert_pos, &format!(r#","cmd_ms":{:.2}"#, cmd_ms));
        }

        result
    }

    fn load_url(&mut self, url: &str) -> Result<(), String> {
        let load_start = Instant::now();
        let mut timing = Timing::new();

        eprintln!("Navigating to {url} …");
        let html = timed!(timing, "fetch_html", {
            if let Some(path) = url.strip_prefix("file://") {
                std::fs::read_to_string(path)
                    .map_err(|e| e.to_string())?
            } else {
                cached_fetch_text(url, self.no_cache, &self.cache_dir)?
            }
        });

        let (css_tx, css_rx) = mpsc::channel::<(usize, String, String)>();
        let base = url.to_string();
        let css_tx2 = css_tx.clone();
        let no_cache2 = self.no_cache;
        let cache_dir2 = self.cache_dir.clone();
        let css_idx = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let css_idx2 = css_idx.clone();

        let mut doc = timed!(timing, "parse_html", {
            parse_html_with_hooks(&html, url, move |tag, attrs| {
                if tag == "link"
                    && attrs.get("rel").map(|s| s == "stylesheet").unwrap_or(false)
                {
                    if let Some(href) = attrs.get("href") {
                        let abs = resolve_url(&base, href);
                        let sender = css_tx2.clone();
                        let cd = cache_dir2.clone();
                        let idx = css_idx2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        std::thread::spawn(move || {
                            eprintln!("  CSS  {abs}");
                            let text = cached_fetch_text(&abs, no_cache2, &cd).unwrap_or_default();
                            let _ = sender.send((idx, abs, text));
                        });
                    }
                }
            })
        });
        drop(css_tx);

        timed!(timing, "fetch_css", {
            let mut css_results: Vec<(usize, String, String)> = css_rx.iter().collect();
            css_results.sort_by_key(|(idx, _, _)| *idx);

            // Enable inspect mode for matched_rules collection
            doc.stylesheet.inspect_mode = true;

            for (_, css_url, sheet) in &css_results {
                if !sheet.is_empty() {
                    doc.stylesheet.parse_and_add_with_base(sheet, css_url);
                }
            }
        });

        timed!(timing, "cascade", {
            doc.stylesheet.resolve_variables_for_viewport(self.width, self.max_h);
            apply_cascade_vp(&mut doc.root, &doc.stylesheet, None, 16.0, self.width, self.max_h, 0, false);
        });

        self.doc = doc;
        self.url = url.to_string();

        timed!(timing, "layout", {
            self.layout();
        });

        // Count nodes for stats
        let mut node_count = 0u32;
        let mut text_count = 0u32;
        Document::walk_all(&self.doc.root, &mut |n| {
            node_count += 1;
            if n.tag == "#text" { text_count += 1; }
        });
        let rule_count = self.doc.stylesheet.rules.len() as u32;
        timing.stats.push(("nodes".to_string(), node_count as u64));
        timing.stats.push(("text_nodes".to_string(), text_count as u64));
        timing.stats.push(("css_rules".to_string(), rule_count as u64));
        timing.stats.push(("html_bytes".to_string(), html.len() as u64));

        // Fetch images
        if !self.no_images {
            timed!(timing, "fetch_images", {
                self.fetch_images();
            });
        }

        timing.total_us = load_start.elapsed().as_micros() as u64;
        eprintln!("Load timing: {}", timing.to_json());
        self.load_timing = timing;
        Ok(())
    }

    fn fetch_images(&mut self) {
        let url = self.url.clone();
        let no_cache = self.no_cache;
        let cache_dir = self.cache_dir.clone();

        let mut img_srcs: Vec<String> = Vec::new();
        Document::walk_all(&self.doc.root, &mut |b| {
            let is_img = b.tag == "img"
                || (b.tag == "input" && b.attributes.get("type").map(|s| s.as_str()) == Some("image"));
            if is_img {
                if let Some(src) = b.attributes.get("src") {
                    if src.starts_with("__svg_") { return; }
                    let abs = resolve_url(&url, src);
                    if !abs.is_empty() && !img_srcs.contains(&abs) {
                        img_srcs.push(abs);
                    }
                }
            }
        });

        let mut any_image = false;
        for src in &img_srcs {
            eprintln!("  IMG  {src}");
            let bytes_result = if src.starts_with("data:") {
                decode_data_url(src)
            } else {
                cached_fetch_bytes(src, no_cache, &cache_dir)
            };
            if let Ok(bytes) = bytes_result {
                let decoded = image::load_from_memory(&bytes)
                    .map(|img| {
                        let rgba = img.to_rgba8();
                        let (w, h) = (rgba.width(), rgba.height());
                        (rgba.into_raw(), w, h)
                    })
                    .ok()
                    .or_else(|| {
                        let svg_str = std::str::from_utf8(&bytes).ok()?;
                        rhtmledit::html::rasterize_svg_intrinsic(svg_str)
                    });
                if let Some((raw, iw, ih)) = decoded {
                    let src2 = src.clone();
                    Document::walk_all_mut(&mut self.doc.root, &mut |b| {
                        let is_img = b.tag == "img"
                            || (b.tag == "input" && b.attributes.get("type").map(|s| s.as_str()) == Some("image"));
                        if is_img {
                            if let Some(s) = b.attributes.get("src") {
                                if resolve_url(&url, s) == src2 {
                                    b.image_data = Some(raw.clone());
                                    b.image_width = iw;
                                    b.image_height = ih;
                                    b.layout_dirty = true;
                                }
                            }
                        }
                    });
                    any_image = true;
                }
            }
        }
        if any_image {
            self.layout();
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn walk_with_parent<'a>(node: &'a HtmlBox, parents: &Vec<&'a HtmlBox>, cb: &mut dyn FnMut(&'a HtmlBox, &Vec<&'a HtmlBox>)) {
    cb(node, parents);
    let mut new_parents = parents.clone();
    new_parents.push(node);
    for child in &node.children {
        walk_with_parent(child, &new_parents, cb);
    }
}

fn serialize_html(node: &HtmlBox, buf: &mut String, depth: usize) {
    use std::fmt::Write;
    if node.tag == "#text" {
        let t = node.text.trim();
        if !t.is_empty() {
            let _ = write!(buf, "{}", t);
        }
        return;
    }
    let indent = "  ".repeat(depth);
    let _ = write!(buf, "{}<{}", indent, node.tag);
    for (k, v) in &node.attributes {
        let _ = write!(buf, " {}={}", k, json_escape(v));
    }
    let _ = write!(buf, ">");
    if !node.children.is_empty() {
        let _ = writeln!(buf);
        for child in &node.children {
            serialize_html(child, buf, depth + 1);
        }
        let _ = write!(buf, "{}</{}>", indent, node.tag);
    } else {
        let _ = write!(buf, "</{}>", node.tag);
    }
    let _ = writeln!(buf);
}

fn get_css_property(node: &HtmlBox, prop: &str) -> String {
    let s = &node.style;
    match prop {
        "display" => format!("{:?}", s.display),
        "position" => format!("{:?}", s.position),
        "float" => format!("{:?}", s.float),
        "vertical-align" => format!("{:?}", s.vertical_align),
        "text-align" => format!("{:?}", s.text_align),
        "font-size" => format!("{:.1}px", s.font_size_px(16.0, 16.0)),
        "color" => format!("#{:02x}{:02x}{:02x}", s.color.r, s.color.g, s.color.b),
        "background-color" => {
            let bg = s.background_color;
            if bg.a > 0 { format!("#{:02x}{:02x}{:02x}", bg.r, bg.g, bg.b) }
            else { "transparent".to_string() }
        }
        "width" => format!("{:?}", s.width),
        "height" => format!("{:?}", s.height),
        "min-width" => format!("{:?}", s.min_width),
        "max-width" => format!("{:?}", s.max_width),
        "min-height" => format!("{:?}", s.min_height),
        "max-height" => format!("{:?}", s.max_height),
        "overflow-x" => format!("{:?}", s.overflow_x),
        "overflow-y" => format!("{:?}", s.overflow_y),
        "box-sizing" => format!("{:?}", s.box_sizing),
        "flex-direction" => format!("{:?}", s.flex_direction),
        "flex-wrap" => format!("{:?}", s.flex_wrap),
        "flex-grow" => format!("{}", s.flex_grow),
        "flex-shrink" => format!("{}", s.flex_shrink),
        "flex-basis" => format!("{:?}", s.flex_basis),
        "align-items" => format!("{:?}", s.align_items),
        "align-self" => format!("{:?}", s.align_self),
        "justify-content" => format!("{:?}", s.justify_content),
        "padding-top" => format!("{:?}", s.padding_top),
        "padding-right" => format!("{:?}", s.padding_right),
        "padding-bottom" => format!("{:?}", s.padding_bottom),
        "padding-left" => format!("{:?}", s.padding_left),
        "margin-top" => format!("{:?}", s.margin_top),
        "margin-right" => format!("{:?}", s.margin_right),
        "margin-bottom" => format!("{:?}", s.margin_bottom),
        "margin-left" => format!("{:?}", s.margin_left),
        "border-collapse" => format!("{}", if s.border_collapse { "collapse" } else { "separate" }),
        "cell-padding" => format!("{:?}", s.cell_padding),
        "border-spacing" => format!("{:?} {:?}", s.border_spacing_h, s.border_spacing_v),
        // Resolved values from layout
        "resolved-padding" => format!("{:.1} {:.1} {:.1} {:.1}",
            node.resolved_pad_top, node.resolved_pad_right, node.resolved_pad_bottom, node.resolved_pad_left),
        "resolved-margin" => format!("{:.1} {:.1} {:.1} {:.1}",
            node.resolved_margin_top, node.resolved_margin_right, node.resolved_margin_bottom, node.resolved_margin_left),
        "resolved-border" => format!("{:.1} {:.1} {:.1} {:.1}",
            node.resolved_border_top, node.resolved_border_right, node.resolved_border_bottom, node.resolved_border_left),
        "content-rect" => format!("{:.1},{:.1} {:.1}x{:.1}",
            node.content_rect.x, node.content_rect.y, node.content_rect.w, node.content_rect.h),
        "padding-rect" => format!("{:.1},{:.1} {:.1}x{:.1}",
            node.padding_rect.x, node.padding_rect.y, node.padding_rect.w, node.padding_rect.h),
        "margin-rect" => format!("{:.1},{:.1} {:.1}x{:.1}",
            node.margin_rect.x, node.margin_rect.y, node.margin_rect.w, node.margin_rect.h),
        "border-rect" => format!("{:.1},{:.1} {:.1}x{:.1}",
            node.border_rect.x, node.border_rect.y, node.border_rect.w, node.border_rect.h),
        "line-count" => format!("{}", node.line_cache.len()),
        "gradient-type" => format!("{:?}", s.gradient_type),
        "gradient-angle" => format!("{:.1}", s.gradient_angle),
        "gradient-stops" => {
            let stops: Vec<String> = s.gradient_stops.iter()
                .map(|gs| format!("#{:02x}{:02x}{:02x}@{:.0}%", gs.color.r, gs.color.g, gs.color.b, gs.position * 100.0))
                .collect();
            if stops.is_empty() { "none".to_string() } else { stops.join(", ") }
        }
        "background-image" => {
            if !s.background_image_url.is_empty() { s.background_image_url.clone() }
            else if s.gradient_type != rhtmledit::types::GradientType::None {
                format!("{:?} angle={:.0} stops={}", s.gradient_type, s.gradient_angle, s.gradient_stops.len())
            } else { "none".to_string() }
        }
        _ => format!("(unknown property: {})", prop),
    }
}

fn deep_inspect_json(node: &HtmlBox) -> String {
    use std::fmt::Write;
    let s = &node.style;
    let id = node.attributes.get("id").map(|v| v.as_str()).unwrap_or("");
    let cls = node.attributes.get("class").map(|v| v.as_str()).unwrap_or("");

    let mut buf = String::new();
    let _ = write!(buf, r#"{{"tag":{},"id":{},"class":{}"#,
        json_escape(&node.tag), json_escape(id), json_escape(cls));

    // All attributes
    let attrs: Vec<String> = node.attributes.iter()
        .map(|(k, v)| format!("{}:{}", json_escape(k), json_escape(v)))
        .collect();
    let _ = write!(buf, r#","attrs":{{{}}}"#, attrs.join(","));

    // Box model
    let _ = write!(buf, r#","content":{{"x":{:.1},"y":{:.1},"w":{:.1},"h":{:.1}}}"#,
        node.content_rect.x, node.content_rect.y, node.content_rect.w, node.content_rect.h);
    let _ = write!(buf, r#","padding":{{"x":{:.1},"y":{:.1},"w":{:.1},"h":{:.1}}}"#,
        node.padding_rect.x, node.padding_rect.y, node.padding_rect.w, node.padding_rect.h);
    let _ = write!(buf, r#","margin":{{"x":{:.1},"y":{:.1},"w":{:.1},"h":{:.1}}}"#,
        node.margin_rect.x, node.margin_rect.y, node.margin_rect.w, node.margin_rect.h);
    let _ = write!(buf, r#","border_rect":{{"x":{:.1},"y":{:.1},"w":{:.1},"h":{:.1}}}"#,
        node.border_rect.x, node.border_rect.y, node.border_rect.w, node.border_rect.h);

    // Resolved box values
    let _ = write!(buf, r#","resolved_padding":[{:.1},{:.1},{:.1},{:.1}]"#,
        node.resolved_pad_top, node.resolved_pad_right, node.resolved_pad_bottom, node.resolved_pad_left);
    let _ = write!(buf, r#","resolved_margin":[{:.1},{:.1},{:.1},{:.1}]"#,
        node.resolved_margin_top, node.resolved_margin_right, node.resolved_margin_bottom, node.resolved_margin_left);
    let _ = write!(buf, r#","resolved_border":[{:.1},{:.1},{:.1},{:.1}]"#,
        node.resolved_border_top, node.resolved_border_right, node.resolved_border_bottom, node.resolved_border_left);

    // Key CSS properties
    let _ = write!(buf, r#","display":{},"position":{},"vertical_align":{}"#,
        json_escape(&format!("{:?}", s.display)),
        json_escape(&format!("{:?}", s.position)),
        json_escape(&format!("{:?}", s.vertical_align)));
    let _ = write!(buf, r#","text_align":{},"font_size":{:.1}"#,
        json_escape(&format!("{:?}", s.text_align)),
        s.font_size_px(16.0, 16.0));
    let _ = write!(buf, r#","overflow":[{},{}]"#,
        json_escape(&format!("{:?}", s.overflow_x)),
        json_escape(&format!("{:?}", s.overflow_y)));

    // Padding/margin CSS values (before resolution)
    let _ = write!(buf, r#","css_padding":[{},{},{},{}]"#,
        json_escape(&format!("{:?}", s.padding_top)),
        json_escape(&format!("{:?}", s.padding_right)),
        json_escape(&format!("{:?}", s.padding_bottom)),
        json_escape(&format!("{:?}", s.padding_left)));

    // Table-specific
    let _ = write!(buf, r#","cell_padding":{},"border_collapse":{}"#,
        json_escape(&format!("{:?}", s.cell_padding)),
        s.border_collapse);

    // Line cache
    if !node.line_cache.is_empty() {
        let lines: Vec<String> = node.line_cache.iter().map(|line| {
            format!(r#"{{"x":{:.1},"y":{:.1},"w":{:.1},"h":{:.1},"chars":{}}}"#,
                line.x, line.y, line.width, line.height, line.char_x.len())
        }).collect();
        let _ = write!(buf, r#","line_cache":[{}]"#, lines.join(","));
    }

    // Children summary
    let children: Vec<String> = node.children.iter().filter(|c| c.tag != "#text").map(|c| {
        let cid = c.attributes.get("id").map(|v| v.as_str()).unwrap_or("");
        let ccls = c.attributes.get("class").map(|v| v.as_str()).unwrap_or("");
        format!(r#"{{"tag":{},"id":{},"class":{},"display":{},"c":[{:.0},{:.0},{:.0},{:.0}]}}"#,
            json_escape(&c.tag), json_escape(cid), json_escape(ccls),
            json_escape(&format!("{:?}", c.style.display)),
            c.content_rect.x, c.content_rect.y, c.content_rect.w, c.content_rect.h)
    }).collect();
    let _ = write!(buf, r#","children":[{}]"#, children.join(","));

    // Text children
    let text_children: Vec<String> = node.children.iter().filter(|c| c.tag == "#text" && !c.text.trim().is_empty()).map(|c| {
        let preview: String = c.text.chars().take(80).collect();
        format!(r#"{{"text":{},"c":[{:.0},{:.0},{:.0},{:.0}]}}"#,
            json_escape(preview.trim()),
            c.content_rect.x, c.content_rect.y, c.content_rect.w, c.content_rect.h)
    }).collect();
    if !text_children.is_empty() {
        let _ = write!(buf, r#","text_nodes":[{}]"#, text_children.join(","));
    }

    buf.push('}');
    buf
}

fn subtree_dump(node: &HtmlBox, depth: u32, out: &mut Vec<String>) {
    use std::fmt::Write;
    if matches!(node.style.display, Display::None) { return; }
    let id = node.attributes.get("id").map(|v| v.as_str()).unwrap_or("");
    let cls = node.attributes.get("class").map(|v| v.as_str()).unwrap_or("");
    let cr = &node.content_rect;
    let mr = &node.margin_rect;
    let pr = &node.padding_rect;
    let s = &node.style;

    let mut buf = String::with_capacity(512);
    let _ = write!(buf, r#"{{"depth":{},"tag":{},"id":{},"class":{}"#,
        depth, json_escape(&node.tag), json_escape(id), json_escape(cls));

    // Text content for text nodes
    if node.tag == "#text" && !node.text.is_empty() {
        let preview: String = node.text.chars().take(80).collect();
        let _ = write!(buf, r#","text":{}"#, json_escape(preview.trim()));
    }

    // All rects
    let _ = write!(buf, r#","content":[{:.1},{:.1},{:.1},{:.1}],"right":{:.1},"bottom":{:.1}"#,
        cr.x, cr.y, cr.w, cr.h, cr.x + cr.w, cr.y + cr.h);
    let _ = write!(buf, r#","margin":[{:.1},{:.1},{:.1},{:.1}]"#,
        mr.x, mr.y, mr.w, mr.h);
    let _ = write!(buf, r#","padding_rect":[{:.1},{:.1},{:.1},{:.1}]"#,
        pr.x, pr.y, pr.w, pr.h);

    // Key computed values
    let _ = write!(buf, r#","display":{},"position":{}"#,
        json_escape(&format!("{:?}", s.display)),
        json_escape(&format!("{:?}", s.position)));
    let _ = write!(buf, r#","overflow":[{},{}]"#,
        json_escape(&format!("{:?}", s.overflow_x)),
        json_escape(&format!("{:?}", s.overflow_y)));

    // Resolved spacing
    let _ = write!(buf, r#","resolved_padding":[{:.1},{:.1},{:.1},{:.1}]"#,
        node.resolved_pad_top, node.resolved_pad_right,
        node.resolved_pad_bottom, node.resolved_pad_left);
    let _ = write!(buf, r#","resolved_margin":[{:.1},{:.1},{:.1},{:.1}]"#,
        node.resolved_margin_top, node.resolved_margin_right,
        node.resolved_margin_bottom, node.resolved_margin_left);
    let _ = write!(buf, r#","resolved_border":[{:.1},{:.1},{:.1},{:.1}]"#,
        node.resolved_border_top, node.resolved_border_right,
        node.resolved_border_bottom, node.resolved_border_left);

    // CSS width/height
    let _ = write!(buf, r#","css_width":{},"css_height":{}"#,
        json_escape(&format!("{:?}", s.width)),
        json_escape(&format!("{:?}", s.height)));

    // Attributes (only non-empty)
    let attrs: Vec<String> = node.attributes.iter()
        .filter(|(k, _)| *k != "id" && *k != "class")
        .map(|(k, v)| format!("{}:{}", json_escape(k), json_escape(v)))
        .collect();
    if !attrs.is_empty() {
        let _ = write!(buf, r#","attrs":{{{}}}"#, attrs.join(","));
    }

    // Line cache (text layout lines)
    if !node.line_cache.is_empty() {
        let lines: Vec<String> = node.line_cache.iter().map(|ln| {
            format!(r#"[{:.1},{:.1},{:.1},{:.1}]"#, ln.x, ln.y, ln.width, ln.height)
        }).collect();
        let _ = write!(buf, r#","lines":[{}]"#, lines.join(","));
    }

    buf.push('}');
    out.push(buf);

    // Recurse into children
    for child in &node.children {
        subtree_dump(child, depth + 1, out);
    }
}

fn dom_path_segment(node: &HtmlBox) -> String {
    let mut s = node.tag.clone();
    if let Some(id) = node.attributes.get("id") {
        s.push('#');
        s.push_str(id);
    }
    if let Some(cls) = node.attributes.get("class") {
        for c in cls.split_whitespace().take(3) {
            s.push('.');
            s.push_str(c);
        }
    }
    s
}

fn computed_json(node: &HtmlBox) -> String {
    use std::fmt::Write;
    let s = &node.style;
    let id = node.attributes.get("id").map(|v| v.as_str()).unwrap_or("");
    let cls = node.attributes.get("class").map(|v| v.as_str()).unwrap_or("");

    let mut buf = String::with_capacity(2048);
    let _ = write!(buf, r#"{{"tag":{},"id":{},"class":{}"#,
        json_escape(&node.tag), json_escape(id), json_escape(cls));

    // Box model (resolved values from layout)
    let _ = write!(buf, r#","box":{{"content":[{:.1},{:.1},{:.1},{:.1}],"padding":[{:.1},{:.1},{:.1},{:.1}],"margin":[{:.1},{:.1},{:.1},{:.1}],"border":[{:.1},{:.1},{:.1},{:.1}]}}"#,
        node.content_rect.x, node.content_rect.y, node.content_rect.w, node.content_rect.h,
        node.padding_rect.x, node.padding_rect.y, node.padding_rect.w, node.padding_rect.h,
        node.margin_rect.x, node.margin_rect.y, node.margin_rect.w, node.margin_rect.h,
        node.border_rect.x, node.border_rect.y, node.border_rect.w, node.border_rect.h);

    // Resolved spacing
    let _ = write!(buf, r#","resolved_padding":[{:.1},{:.1},{:.1},{:.1}]"#,
        node.resolved_pad_top, node.resolved_pad_right, node.resolved_pad_bottom, node.resolved_pad_left);
    let _ = write!(buf, r#","resolved_margin":[{:.1},{:.1},{:.1},{:.1}]"#,
        node.resolved_margin_top, node.resolved_margin_right, node.resolved_margin_bottom, node.resolved_margin_left);
    let _ = write!(buf, r#","resolved_border":[{:.1},{:.1},{:.1},{:.1}]"#,
        node.resolved_border_top, node.resolved_border_right, node.resolved_border_bottom, node.resolved_border_left);

    // All computed CSS properties
    let bg = s.background_color;
    let bg_str = if bg.a > 0 { format!("#{:02x}{:02x}{:02x}", bg.r, bg.g, bg.b) } else { "transparent".to_string() };
    let c = s.color;

    let _ = write!(buf, concat!(
        r#","display":{},"position":{},"float":{}"#,
        r#","visibility":{},"opacity":{:.2}"#,
        r#","overflow":[{},{}]"#,
        r#","box_sizing":{}"#,
        // Sizing
        r#","width":{},"height":{}"#,
        r#","min_width":{},"min_height":{}"#,
        r#","max_width":{},"max_height":{}"#,
    ),
        json_escape(&format!("{:?}", s.display)),
        json_escape(&format!("{:?}", s.position)),
        json_escape(&format!("{:?}", s.float)),
        json_escape(&format!("{:?}", s.visibility)),
        s.opacity,
        json_escape(&format!("{:?}", s.overflow_x)),
        json_escape(&format!("{:?}", s.overflow_y)),
        json_escape(&format!("{:?}", s.box_sizing)),
        json_escape(&format!("{:?}", s.width)),
        json_escape(&format!("{:?}", s.height)),
        json_escape(&format!("{:?}", s.min_width)),
        json_escape(&format!("{:?}", s.min_height)),
        json_escape(&format!("{:?}", s.max_width)),
        json_escape(&format!("{:?}", s.max_height)),
    );

    // Typography
    let _ = write!(buf, concat!(
        r#","font_size":{:.1},"font_weight":{},"font_style":{}"#,
        r#","font_family":{},"font_stretch":{:.0}"#,
        r#","line_height":{},"letter_spacing":{}"#,
        r#","text_align":{},"text_indent":{}"#,
        r#","text_transform":{},"text_decoration":{}"#,
        r#","white_space":{},"word_break":{}"#,
        r#","vertical_align":{}"#,
    ),
        s.font_size_px(16.0, 16.0),
        json_escape(&format!("{:?}", s.font_weight)),
        json_escape(&format!("{:?}", s.font_style)),
        json_escape(&s.font_family),
        s.font_stretch,
        json_escape(&format!("{:?}", s.line_height)),
        json_escape(&format!("{:?}", s.letter_spacing)),
        json_escape(&format!("{:?}", s.text_align)),
        json_escape(&format!("{:?}", s.text_indent)),
        json_escape(&format!("{:?}", s.text_transform)),
        json_escape(&format!("{:?}", s.text_decoration)),
        json_escape(&format!("{:?}", s.white_space)),
        json_escape(&format!("{:?}", s.word_break)),
        json_escape(&format!("{:?}", s.vertical_align)),
    );

    // Colors
    let color_hex = format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
    let _ = write!(buf, r#","color":{},"background":{}"#,
        json_escape(&color_hex), json_escape(&bg_str));

    // Flex
    let _ = write!(buf, concat!(
        r#","flex_direction":{},"flex_wrap":{}"#,
        r#","flex_grow":{},"flex_shrink":{},"flex_basis":{}"#,
        r#","align_items":{},"align_self":{},"justify_content":{}"#,
        r#","gap":[{},{}]"#,
    ),
        json_escape(&format!("{:?}", s.flex_direction)),
        json_escape(&format!("{:?}", s.flex_wrap)),
        s.flex_grow, s.flex_shrink,
        json_escape(&format!("{:?}", s.flex_basis)),
        json_escape(&format!("{:?}", s.align_items)),
        json_escape(&format!("{:?}", s.align_self)),
        json_escape(&format!("{:?}", s.justify_content)),
        json_escape(&format!("{:?}", s.row_gap)),
        json_escape(&format!("{:?}", s.column_gap)),
    );

    // Table
    let _ = write!(buf, r#","border_collapse":{},"cell_padding":{}"#,
        s.border_collapse,
        json_escape(&format!("{:?}", s.cell_padding)));

    // CSS padding/margin raw values
    let _ = write!(buf, r#","css_padding":[{},{},{},{}]"#,
        json_escape(&format!("{:?}", s.padding_top)),
        json_escape(&format!("{:?}", s.padding_right)),
        json_escape(&format!("{:?}", s.padding_bottom)),
        json_escape(&format!("{:?}", s.padding_left)));
    let _ = write!(buf, r#","css_margin":[{},{},{},{}]"#,
        json_escape(&format!("{:?}", s.margin_top)),
        json_escape(&format!("{:?}", s.margin_right)),
        json_escape(&format!("{:?}", s.margin_bottom)),
        json_escape(&format!("{:?}", s.margin_left)));

    // Border styles
    let _ = write!(buf, r#","border_styles":[{},{},{},{}]"#,
        json_escape(&format!("{:?}", s.border_top_style)),
        json_escape(&format!("{:?}", s.border_right_style)),
        json_escape(&format!("{:?}", s.border_bottom_style)),
        json_escape(&format!("{:?}", s.border_left_style)));
    let bc_t = format!("#{:02x}{:02x}{:02x}", s.border_top_color.r, s.border_top_color.g, s.border_top_color.b);
    let bc_r = format!("#{:02x}{:02x}{:02x}", s.border_right_color.r, s.border_right_color.g, s.border_right_color.b);
    let bc_b = format!("#{:02x}{:02x}{:02x}", s.border_bottom_color.r, s.border_bottom_color.g, s.border_bottom_color.b);
    let bc_l = format!("#{:02x}{:02x}{:02x}", s.border_left_color.r, s.border_left_color.g, s.border_left_color.b);
    let _ = write!(buf, r#","border_colors":[{},{},{},{}]"#,
        json_escape(&bc_t), json_escape(&bc_r), json_escape(&bc_b), json_escape(&bc_l));
    let _ = write!(buf, r#","border_radius":[{},{},{},{}]"#,
        json_escape(&format!("{:?}", s.border_top_left_radius)),
        json_escape(&format!("{:?}", s.border_top_right_radius)),
        json_escape(&format!("{:?}", s.border_bottom_right_radius)),
        json_escape(&format!("{:?}", s.border_bottom_left_radius)));

    // Positioning
    let _ = write!(buf, r#","top":{},"right":{},"bottom":{},"left":{}"#,
        json_escape(&format!("{:?}", s.top)),
        json_escape(&format!("{:?}", s.right)),
        json_escape(&format!("{:?}", s.bottom)),
        json_escape(&format!("{:?}", s.left)));
    let _ = write!(buf, r#","z_index":{}"#, json_escape(&format!("{:?}", s.z_index)));

    // Matched rules count
    let _ = write!(buf, r#","matched_rules":{}"#, node.matched_rules.len());

    // Line cache
    let _ = write!(buf, r#","line_count":{}"#, node.line_cache.len());

    buf.push('}');
    buf
}

fn collect_text(node: &HtmlBox, out: &mut String) {
    if node.tag == "#text" {
        if !out.is_empty() && !out.ends_with(' ') { out.push(' '); }
        out.push_str(node.text.trim());
    }
    for child in &node.children {
        collect_text(child, out);
    }
}

fn inspect_json(node: &HtmlBox) -> String {
    let s = &node.style;
    let id = node.attributes.get("id").map(|v| v.as_str()).unwrap_or("");
    let cls = node.attributes.get("class").map(|v| v.as_str()).unwrap_or("");
    let bg = s.background_color;
    let bg_str = if bg.a > 0 {
        format!("#{:02x}{:02x}{:02x}", bg.r, bg.g, bg.b)
    } else {
        "transparent".to_string()
    };
    let color_str = format!("#{:02x}{:02x}{:02x}", s.color.r, s.color.g, s.color.b);
    format!(
        concat!(
            r#"{{"tag":{},"id":{},"class":{},"#,
            r#""content":{{"x":{:.1},"y":{:.1},"w":{:.1},"h":{:.1}}},"#,
            r#""padding":{{"x":{:.1},"y":{:.1},"w":{:.1},"h":{:.1}}},"#,
            r#""margin":{{"x":{:.1},"y":{:.1},"w":{:.1},"h":{:.1}}},"#,
            r#""display":{},"position":{},"#,
            r#""font_size":{:.1},"color":{},"background":{},"#,
            r#""margin_trbl":[{:.1},{:.1},{:.1},{:.1}],"#,
            r#""padding_trbl":[{:.1},{:.1},{:.1},{:.1}],"#,
            r#""border_trbl":[{:.1},{:.1},{:.1},{:.1}],"#,
            r#""children":{}}}"#,
        ),
        json_escape(&node.tag), json_escape(id), json_escape(cls),
        node.content_rect.x, node.content_rect.y, node.content_rect.w, node.content_rect.h,
        node.padding_rect.x, node.padding_rect.y, node.padding_rect.w, node.padding_rect.h,
        node.margin_rect.x, node.margin_rect.y, node.margin_rect.w, node.margin_rect.h,
        json_escape(&format!("{:?}", s.display)),
        json_escape(&format!("{:?}", s.position)),
        s.font_size_px(16.0, 16.0), json_escape(&color_str),
        json_escape(&bg_str),
        node.resolved_margin_top, node.resolved_margin_right, node.resolved_margin_bottom, node.resolved_margin_left,
        node.resolved_pad_top, node.resolved_pad_right, node.resolved_pad_bottom, node.resolved_pad_left,
        node.resolved_border_top, node.resolved_border_right, node.resolved_border_bottom, node.resolved_border_left,
        node.children.iter().filter(|c| c.tag != "#text").count(),
    )
}

fn dump_box_to_string(depth: usize, node: &HtmlBox, buf: &mut String) {
    use std::fmt::Write;
    if matches!(node.style.display, Display::None) { return; }
    let indent = "  ".repeat(depth);
    let tag = if node.tag.is_empty() { "(box)" } else { &node.tag };
    let id = node.attributes.get("id").map(|v| format!("#{v}")).unwrap_or_default();
    let cls = node.attributes.get("class")
        .map(|v| format!(".{}", v.split_whitespace().take(3).collect::<Vec<_>>().join(".")))
        .unwrap_or_default();
    let text_preview = if node.tag == "#text" && !node.text.is_empty() {
        let s: String = node.text.chars().take(40).collect();
        format!(" {:?}", s.trim())
    } else {
        String::new()
    };
    let _ = writeln!(buf, "{}{}{}{} [{:?}] c=[{:.0},{:.0} {:.0}x{:.0}] m=[{:.0},{:.0} {:.0}x{:.0}]{}",
        indent, tag, id, cls,
        node.style.display,
        node.content_rect.x, node.content_rect.y, node.content_rect.w, node.content_rect.h,
        node.margin_rect.x, node.margin_rect.y, node.margin_rect.w, node.margin_rect.h,
        text_preview,
    );
    for child in &node.children {
        dump_box_to_string(depth + 1, child, buf);
    }
}

/// Match an element against an inspect query.
/// Supports: "#id", ".class", "tag", "tag.class", "tag#id", ".class1.class2"
fn matches_query(node: &HtmlBox, query: &str) -> bool {
    if node.tag == "#text" { return false; }
    let query = query.trim();
    let mut tag_q = "";
    let mut id_q = "";
    let mut classes_q: Vec<&str> = Vec::new();
    let mut rest = query;
    if !rest.starts_with('#') && !rest.starts_with('.') {
        let end = rest.find(|c: char| c == '#' || c == '.').unwrap_or(rest.len());
        tag_q = &rest[..end];
        rest = &rest[end..];
    }
    while !rest.is_empty() {
        if rest.starts_with('#') {
            rest = &rest[1..];
            let end = rest.find(|c: char| c == '#' || c == '.').unwrap_or(rest.len());
            id_q = &rest[..end];
            rest = &rest[end..];
        } else if rest.starts_with('.') {
            rest = &rest[1..];
            let end = rest.find(|c: char| c == '#' || c == '.').unwrap_or(rest.len());
            classes_q.push(&rest[..end]);
            rest = &rest[end..];
        } else {
            break;
        }
    }
    if !tag_q.is_empty() && !node.tag.eq_ignore_ascii_case(tag_q) { return false; }
    if !id_q.is_empty() {
        if node.attributes.get("id").map(|s| s.as_str()) != Some(id_q) { return false; }
    }
    if !classes_q.is_empty() {
        let cls = node.attributes.get("class").map(|s| s.as_str()).unwrap_or("");
        let elem_classes: Vec<&str> = cls.split_whitespace().collect();
        for c in &classes_q {
            if !elem_classes.contains(c) { return false; }
        }
    }
    true
}

// ─── Main ─────────────────────────────────────────────────────────────────────

// ─── Windowed debug app ───────────────────────────────────────────────────────

use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;
use rhtmledit::platform::Platform;

/// A TCP command + response channel pair.
type CmdPair = (String, std::sync::mpsc::Sender<String>);

struct DebugApp {
    window:    Option<Arc<Window>>,
    platform:  Option<Platform>,
    state:     EngineState,
    mouse_pos: (f32, f32),
    cmd_rx:    std::sync::mpsc::Receiver<CmdPair>,
    headless:  bool,
    initial_url: String,
    chrome_port: u16,  // 0 = no chrome
    chrome_process: Option<std::process::Child>,
}

impl DebugApp {
    fn request_redraw(&self) { if let Some(w) = self.window.as_ref() { w.request_redraw(); } }
}

impl ApplicationHandler for DebugApp {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.headless { return; }
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("debugserver — rhtmledit")
                    .with_inner_size(winit::dpi::LogicalSize::new(
                        self.state.width as u32,
                        self.state.viewport_h as u32,
                    ))
            ).unwrap()
        );
        let platform = Platform::new_windowed(window.clone());
        let actual_w = platform.logical_width();
        self.state.width = actual_w;
        self.state.scale = platform.scale_factor();
        self.state.renderer.set_scale(self.state.scale);

        // Load the page using load_html (same path as demo.rs)
        let url = self.initial_url.clone();
        if let Err(e) = self.state.load_url(&url) {
            eprintln!("Failed to load {url}: {e}");
        }

        self.window   = Some(window);
        self.platform = Some(platform);

        // Launch Chrome reference window if requested
        if self.chrome_port > 0 && self.chrome_process.is_none() {
            let chrome = find_chrome();
            if let Some(chrome_path) = chrome {
                let url = &self.initial_url;
                let w = actual_w as u32;
                let h = self.state.viewport_h as u32;
                let port = self.chrome_port;
                eprintln!("Launching Chrome on port {port} ...");
                match std::process::Command::new(&chrome_path)
                    .arg(format!("--remote-debugging-port={port}"))
                    .arg(format!("--window-size={w},{h}"))
                    .arg("--disable-extensions")
                    .arg("--disable-gpu")
                    .arg("--no-first-run")
                    .arg("--no-default-browser-check")
                    .arg(format!("--user-data-dir=/tmp/debugserver-chrome-{port}"))
                    .arg(format!("--app={url}"))
                    .spawn()
                {
                    Ok(child) => {
                        eprintln!("Chrome launched (pid {}). Use c.chrome_label() to set title.", child.id());
                        self.chrome_process = Some(child);
                    }
                    Err(e) => eprintln!("Failed to launch Chrome: {e}"),
                }
            } else {
                eprintln!("Chrome not found — skipping reference window");
            }
        }

        self.request_redraw();
    }

    fn user_event(&mut self, _el: &winit::event_loop::ActiveEventLoop, _: ()) {
        // TCP command arrived — process it and redraw
        while let Ok((cmd_line, reply_tx)) = self.cmd_rx.try_recv() {
            let resp = self.state.handle_command(&cmd_line);
            let _ = reply_tx.send(resp);
        }
        self.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let platform = match self.platform.as_mut() {
            Some(p) => p,
            _ => return,
        };

        // Built-in zoom + pan
        if self.state.renderer.handle_window_event(&event, Some(&mut self.state.doc)) {
            self.request_redraw();
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                platform.resize(size.width, size.height);
                self.state.width = platform.logical_width();
                self.state.renderer.layout_engine().layout(&mut self.state.doc, self.state.width);
                self.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let sf = platform.scale_factor();
                let zoom = self.state.renderer.zoom;
                self.mouse_pos = (position.x as f32 / sf, position.y as f32 / sf);
                let mp = self.mouse_pos;
                let pt = (mp.0 / zoom, mp.1 / zoom + self.state.doc.scroll_y);
                if self.state.doc.process_mouse_event(HtmlEventType::MouseMove, pt, 0) {
                    self.request_redraw();
                }
            }
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                let etype = if btn_state == ElementState::Pressed { HtmlEventType::MouseDown } else { HtmlEventType::MouseUp };
                let bt = match button {
                    MouseButton::Left   => 0,
                    MouseButton::Middle => 1,
                    MouseButton::Right  => 2,
                    _ => 0,
                };
                let zoom = self.state.renderer.zoom;
                let mp = self.mouse_pos;
                let pt = (mp.0 / zoom, mp.1 / zoom + self.state.doc.scroll_y);
                if self.state.doc.process_mouse_event(etype, pt, bt) {
                    self.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y * 20.0,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 / platform.scale_factor(),
                };
                let zoom = self.state.renderer.zoom;
                let mp = self.mouse_pos;
                let doc_pt = (mp.0 / zoom, mp.1 / zoom + self.state.doc.scroll_y);
                self.state.doc.process_wheel_event(doc_pt, dy);
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let renderer = &mut self.state.renderer;
                let doc = &mut self.state.doc;
                platform.render(|scale, pixmap| {
                    renderer.render(doc, pixmap, scale);
                });
            }
            _ => {}
        }
    }
}

// ─── TCP listener thread ──────────────────────────────────────────────────────

fn spawn_tcp_listener(port: u16, cmd_tx: std::sync::mpsc::Sender<CmdPair>, proxy: Option<winit::event_loop::EventLoopProxy<()>>) {
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(format!("127.0.0.1:{port}")) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to bind port {port}: {e}");
                return;
            }
        };
        eprintln!("Listening on 127.0.0.1:{port}");

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
                    eprintln!("[connect] {peer}");
                    let reader = BufReader::new(stream.try_clone().unwrap());
                    let mut writer = stream;
                    let cmd_tx = cmd_tx.clone();
                    let proxy = proxy.clone();
                    for line in reader.lines() {
                        let line = match line {
                            Ok(l) => l,
                            Err(_) => break,
                        };
                        if line.trim().is_empty() { continue; }
                        // Send command to main thread and wait for response
                        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
                        let _ = cmd_tx.send((line, reply_tx));
                        // Wake up the event loop so it processes the command
                        if let Some(ref p) = proxy { let _ = p.send_event(()); }
                        // Wait for response
                        if let Ok(resp) = reply_rx.recv_timeout(std::time::Duration::from_secs(30)) {
                            if !resp.is_empty() {
                                let _ = writeln!(writer, "{}", resp);
                                let _ = writer.flush();
                            }
                        }
                    }
                    eprintln!("[disconnect] {peer}");
                }
                Err(e) => eprintln!("[accept error] {e}"),
            }
        }
    });
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut url        = String::new();
    let mut width: f32 = 900.0;
    let mut vp_h: f32  = 700.0;
    let mut max_h: f32 = 4000.0;
    let mut out        = String::from("snapshot.png");
    let mut scale: f32 = 1.0;
    let mut port: u16  = 9222;
    let mut headless   = false;
    let mut use_stdin  = false;
    let mut no_images  = false;
    let mut no_cache   = false;
    let mut cache_dir  = String::from("snapshot_cache");
    let mut chrome_port: u16 = 0;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--url"        => { i += 1; if i < args.len() { url       = args[i].clone(); } }
            "--width"      => { i += 1; if i < args.len() { width     = args[i].parse().unwrap_or(width); } }
            "--height"     => { i += 1; if i < args.len() { vp_h      = args[i].parse().unwrap_or(vp_h); } }
            "--max-height" => { i += 1; if i < args.len() { max_h     = args[i].parse().unwrap_or(max_h); } }
            "--out"        => { i += 1; if i < args.len() { out       = args[i].clone(); } }
            "--scale"      => { i += 1; if i < args.len() { scale     = args[i].parse().unwrap_or(scale); } }
            "--port"       => { i += 1; if i < args.len() { port      = args[i].parse().unwrap_or(port); } }
            "--cache-dir"  => { i += 1; if i < args.len() { cache_dir = args[i].clone(); } }
            "--headless"   => { headless = true; }
            "--chrome"     => { chrome_port = 9223; }
            "--chrome-port" => { i += 1; if i < args.len() { chrome_port = args[i].parse().unwrap_or(9223); } }
            "--stdin"      => { use_stdin = true; headless = true; }
            "--no-images"  => { no_images = true; }
            "--no-cache"   => { no_cache  = true; }
            other if !other.starts_with("--") && url.is_empty() => { url = other.to_string(); }
            other => { eprintln!("Unknown argument: {other}"); }
        }
        i += 1;
    }

    if url.is_empty() {
        eprintln!("Usage: cargo run --example debugserver -- <url> [OPTIONS]");
        eprintln!("  Default: opens a window + TCP listener on port 9222");
        eprintln!("  --headless   Run without a window (old behavior)");
        eprintln!("  --stdin      Read commands from stdin (implies --headless)");
        eprintln!("  --port <n>   TCP port (default: 9222)");
        eprintln!("  --chrome     Open Chrome side-by-side for comparison");
        std::process::exit(1);
    }

    let url = normalize_url(url);

    let empty_doc = rhtmledit::load_html("<html><body></body></html>", width);
    let mut renderer = Renderer::new();
    renderer.set_scale(scale);

    let state = EngineState {
        doc: empty_doc,
        renderer,
        url: url.clone(),
        width,
        viewport_h: vp_h,
        max_h,
        scale,
        out,
        no_images,
        no_cache,
        cache_dir,
        load_timing: Timing::new(),
        last_cmd_timing: None,
        chrome_port,
    };

    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<CmdPair>();

    if headless {
        // ── Headless mode ────────────────────────────────────────────────
        let mut state = state;
        if let Err(e) = state.load_url(&url) {
            eprintln!("Failed to load {url}: {e}");
            std::process::exit(1);
        }
        let res = state.render_screenshot(&state.out.clone());
        eprintln!("Initial: {res}");

        if use_stdin {
            eprintln!("Ready — reading commands from stdin");
            let stdin = std::io::stdin();
            let reader = BufReader::new(stdin.lock());
            let mut stdout = std::io::stdout();
            for line in reader.lines() {
                let line = match line { Ok(l) => l, Err(_) => break };
                let resp = state.handle_command(&line);
                if !resp.is_empty() {
                    let _ = writeln!(stdout, "{}", resp);
                    let _ = stdout.flush();
                }
            }
        } else {
            spawn_tcp_listener(port, cmd_tx, None);
            // Process commands on main thread
            loop {
                match cmd_rx.recv() {
                    Ok((line, reply_tx)) => {
                        let resp = state.handle_command(&line);
                        let _ = reply_tx.send(resp);
                    }
                    Err(_) => break,
                }
            }
        }
    } else {
        // ── Windowed mode (default) ──────────────────────────────────────
        let event_loop = EventLoop::with_user_event().build().unwrap();
        event_loop.set_control_flow(ControlFlow::Wait);

        // Spawn TCP listener with event loop proxy to wake it up
        let proxy = event_loop.create_proxy();
        spawn_tcp_listener(port, cmd_tx, Some(proxy));

        let mut app = DebugApp {
            window: None,
            platform: None,
            state,
            mouse_pos: (0.0, 0.0),
            cmd_rx,
            headless: false,
            initial_url: url.clone(),
            chrome_port,
            chrome_process: None,
        };
        event_loop.run_app(&mut app).unwrap();
    }
}

// ─── Chrome helpers ──────────────────────────────────────────────────────────

fn find_chrome() -> Option<String> {
    // macOS
    let mac = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
    if std::path::Path::new(mac).exists() { return Some(mac.to_string()); }
    // Linux
    for name in &["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"] {
        if std::process::Command::new("which").arg(name).output()
            .map(|o| o.status.success()).unwrap_or(false)
        {
            return Some(name.to_string());
        }
    }
    None
}

/// Send a CDP command to Chrome via HTTP. Returns the JSON response.
fn cdp_send(chrome_port: u16, method: &str, params: &str) -> Result<String, String> {
    // First get the WebSocket debugger URL
    let list_url = format!("http://127.0.0.1:{}/json", chrome_port);
    let resp = reqwest::blocking::Client::new()
        .get(&list_url)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .map_err(|e| format!("CDP connect: {e}"))?;
    let body = resp.text().map_err(|e| e.to_string())?;

    // We can't easily do WebSocket from here without a dependency,
    // so use the HTTP endpoint for simple commands
    // For scroll sync, we'll use the /json/protocol approach
    // Actually, let's use a simpler approach: Runtime.evaluate via HTTP

    // Find first page target
    let ws_url = body.split("\"webSocketDebuggerUrl\":\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .ok_or("No debugger URL found")?;

    // For now, use a Python subprocess for WebSocket CDP since we don't have ws in Rust deps
    let script = format!(
        r#"
import socket, json, ssl, hashlib, base64, os, struct, sys

url = "{ws_url}"
# Parse ws://host:port/path
parts = url.replace("ws://","").split("/", 1)
host_port = parts[0].split(":")
host = host_port[0]
port = int(host_port[1])
path = "/" + parts[1] if len(parts) > 1 else "/"

s = socket.socket()
s.settimeout(5)
s.connect((host, port))

# WebSocket handshake
import random
key = base64.b64encode(random.randbytes(16)).decode()
req = f"GET {{path}} HTTP/1.1\r\nHost: {{host}}:{{port}}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {{key}}\r\nSec-WebSocket-Version: 13\r\n\r\n"
s.sendall(req.encode())
resp = b""
while b"\r\n\r\n" not in resp:
    resp += s.recv(4096)

# Send CDP command
msg = json.dumps({{"id": 1, "method": "{method}", "params": {params}}})
payload = msg.encode()
frame = bytearray()
frame.append(0x81)  # text frame, fin
mask_key = random.randbytes(4)
length = len(payload)
if length < 126:
    frame.append(0x80 | length)
elif length < 65536:
    frame.append(0x80 | 126)
    frame.extend(struct.pack(">H", length))
frame.extend(mask_key)
masked = bytearray(b ^ mask_key[i % 4] for i, b in enumerate(payload))
frame.extend(masked)
s.sendall(bytes(frame))

# Read response
data = b""
while len(data) < 2:
    data += s.recv(4096)
opcode = data[0] & 0x0F
plen = data[1] & 0x7F
offset = 2
if plen == 126:
    plen = struct.unpack(">H", data[2:4])[0]
    offset = 4
elif plen == 127:
    plen = struct.unpack(">Q", data[2:10])[0]
    offset = 10
while len(data) < offset + plen:
    data += s.recv(4096)
result = data[offset:offset+plen].decode()
s.close()
print(result)
"#);

    let output = std::process::Command::new("python3")
        .arg("-c")
        .arg(&script)
        .output()
        .map_err(|e| format!("python3: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

// ─── URL & network helpers (same as snapshot.rs) ─────────────────────────────

fn normalize_url(s: String) -> String {
    let s = s.trim().to_string();
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("file://") {
        return s;
    }
    if s.starts_with('/') || s.starts_with('.') {
        return format!("file://{s}");
    }
    format!("https://{s}")
}

fn resolve_url(base: &str, href: &str) -> String {
    if href.is_empty()                           { return base.to_string(); }
    if href.starts_with("data:")                 { return href.to_string(); }
    if href.starts_with("http://") || href.starts_with("https://") { return href.to_string(); }
    if href.starts_with("//") {
        let scheme = if base.starts_with("https") { "https:" } else { "http:" };
        return format!("{scheme}{href}");
    }
    let origin = if let Some(p) = base.find("://") {
        let rest  = &base[p + 3..];
        let slash = rest.find('/').map(|i| p + 3 + i).unwrap_or(base.len());
        &base[..slash]
    } else { "" };
    if href.starts_with('/') {
        return format!("{origin}{href}");
    }
    let dir = if let Some(i) = base.rfind('/') {
        if &base[..i] == "https:" || &base[..i] == "http:" { base } else { &base[..i + 1] }
    } else { base };
    format!("{dir}{href}")
}

fn url_cache_path(url: &str, cache_dir: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let hash = hasher.finish();
    let suffix: String = url.chars()
        .filter(|c| c.is_alphanumeric() || *c == '.')
        .take(40)
        .collect();
    PathBuf::from(cache_dir).join(format!("{hash:016x}_{suffix}"))
}

fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let do_fetch = |client: &reqwest::blocking::Client| -> Result<Vec<u8>, String> {
        let resp = client.get(url).send().map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        Ok(resp.bytes().map_err(|e| e.to_string())?.to_vec())
    };
    match do_fetch(&rhtmledit::http_client()) {
        Ok(b) if !b.is_empty() => Ok(b),
        _ => do_fetch(&rhtmledit::http_client_lenient()),
    }
}

fn cached_fetch_bytes(url: &str, no_cache: bool, cache_dir: &str) -> Result<Vec<u8>, String> {
    let path = url_cache_path(url, cache_dir);
    if !no_cache {
        if let Ok(data) = std::fs::read(&path) {
            return Ok(data);
        }
    }
    let data = fetch_bytes(url)?;
    let _ = std::fs::create_dir_all(cache_dir);
    let _ = std::fs::write(&path, &data);
    Ok(data)
}

fn cached_fetch_text(url: &str, no_cache: bool, cache_dir: &str) -> Result<String, String> {
    let bytes = cached_fetch_bytes(url, no_cache, cache_dir)?;
    Ok(String::from_utf8(bytes.clone()).unwrap_or_else(|_| {
        let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(&bytes);
        cow.into_owned()
    }))
}

fn decode_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    let rest = data_url.strip_prefix("data:").ok_or("not a data URL")?;
    let comma = rest.find(',').ok_or("malformed data URL")?;
    let meta  = &rest[..comma];
    let data  = &rest[comma + 1..];
    if meta.ends_with(";base64") {
        base64_decode(data)
    } else {
        Ok(data.as_bytes().to_vec())
    }
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let table: [u8; 128] = {
        let mut t = [255u8; 128];
        for (i, &c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
            .iter()
            .enumerate()
        {
            t[c as usize] = i as u8;
        }
        t
    };
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=' && b < 128 && table[b as usize] != 255).collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let v: Vec<u8> = chunk.iter().map(|&b| table[b as usize]).collect();
        let n = v.len();
        if n >= 2 { out.push((v[0] << 2) | (v[1] >> 4)); }
        if n >= 3 { out.push((v[1] << 4) | (v[2] >> 2)); }
        if n >= 4 { out.push((v[2] << 6) | v[3]); }
    }
    Ok(out)
}
