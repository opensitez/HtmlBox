//! Phoenix Browser — a full browser demo built on the rhtmledit engine.
//! Features: tabs, back/forward history, URL bar, async page/CSS/image loading,
//! link navigation, per-element scrolling, and zoom.

use std::sync::{mpsc, Arc};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::Window;

use tiny_skia::{Pixmap, PixmapPaint, Transform};

use rhtmledit::{hit_test_link, point_to_hit, parse_html_with_hooks, Document, Renderer};
use rhtmledit::css::apply_cascade_vp;
use rhtmledit::dom::{self, HtmlEventType};
use rhtmledit::platform::Platform;

// ─── Layout constants ─────────────────────────────────────────────────────────

const CHROME_H: f32 = 80.0; // tab-bar (36) + nav-bar (44)
const TAB_H: f32 = 36.0;
const TAB_MAX_W: f32 = 220.0;
const TAB_MIN_W: f32 = 80.0;

// ─── New-tab page ─────────────────────────────────────────────────────────────

const NEW_TAB_URL: &str = "about:newtab";
const NEW_TAB_HTML: &str = r#"<!DOCTYPE html><html><head><style>
*{box-sizing:border-box;margin:0;padding:0}
html,body{height:100%;overflow:hidden}
body{background:#1C1C1F;display:flex;flex-direction:column;align-items:center;
     font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;
     color:#E8EAED;padding-top:72px}
.logo{font-size:50px;font-weight:200;letter-spacing:-2px;color:#E8EAED;margin-bottom:6px}
.logo-accent{color:#5E9CF8}
.tagline{color:#6E7077;font-size:13px;letter-spacing:0.2px;margin-bottom:44px}
.search-hint{width:520px;max-width:90%;height:42px;background:#2A2B2E;
             border:1.5px solid #3A3B3E;border-radius:21px;
             display:flex;align-items:center;padding:0 18px;gap:10px;
             color:#6E7077;font-size:13px;margin-bottom:56px}
.search-icon{font-size:14px}
.section{color:#6E7077;font-size:10px;font-weight:700;text-transform:uppercase;
          letter-spacing:1.2px;margin-bottom:16px}
.tiles{display:flex;gap:10px;flex-wrap:wrap;justify-content:center;max-width:640px}
.tile{background:#26272A;border:1px solid #313235;border-radius:14px;
      padding:20px 14px 16px;width:136px;
      display:flex;flex-direction:column;align-items:center;gap:10px;cursor:pointer}
.fav{width:40px;height:40px;border-radius:10px;
     display:flex;align-items:center;justify-content:center;
     font-size:14px;font-weight:800;color:#fff}
.tile-name{color:#C9CBD0;font-size:12px;font-weight:500}
.tile-domain{color:#4A4C52;font-size:10px}
a{color:inherit;text-decoration:none}
</style></head><body>
<div class="logo">Phoe<span class="logo-accent">nix</span></div>
<div class="tagline">Fast, private, and beautifully simple</div>
<div class="search-hint"><span class="search-icon">🔍</span>Search or type an address&hellip;</div>
<div class="section">Quick Access</div>
<div class="tiles">
  <a href="https://news.ycombinator.com"><div class="tile"><div class="fav" style="background:#FF6600">HN</div><div class="tile-name">Hacker News</div><div class="tile-domain">ycombinator.com</div></div></a>
  <a href="https://github.com"><div class="tile"><div class="fav" style="background:#24292E">GH</div><div class="tile-name">GitHub</div><div class="tile-domain">github.com</div></div></a>
  <a href="https://www.rust-lang.org"><div class="tile"><div class="fav" style="background:#CE422B">Rs</div><div class="tile-name">Rust</div><div class="tile-domain">rust-lang.org</div></div></a>
  <a href="https://crates.io"><div class="tile"><div class="fav" style="background:#7B5EA7">Cr</div><div class="tile-name">crates.io</div><div class="tile-domain">crates.io</div></div></a>
  <a href="https://en.wikipedia.org/wiki/Main_Page"><div class="tile"><div class="fav" style="background:#E8E8E8;color:#202124">W</div><div class="tile-name">Wikipedia</div><div class="tile-domain">wikipedia.org</div></div></a>
  <a href="https://slashdot.org/"><div class="tile"><div class="fav" style="background:#F48024">SL</div><div class="tile-name">Slashdot</div><div class="tile-domain">slashdot.org</div></div></a>
  <a href="https://doc.rust-lang.org/book/"><div class="tile"><div class="fav" style="background:#3B4252">Bk</div><div class="tile-name">Rust Book</div><div class="tile-domain">doc.rust-lang.org</div></div></a>
  <a href="https://www.reddit.com"><div class="tile"><div class="fav" style="background:#FF4500">Re</div><div class="tile-name">Reddit</div><div class="tile-domain">reddit.com</div></div></a>
</div>
</body></html>"#;

// ─── Async resource results ───────────────────────────────────────────────────

/// Freshly-parsed documents have all raw pointer fields set to null, making it
/// sound to move them across a thread boundary exactly once before any events fire.
struct FreshDoc(Document);
// SAFETY: all *const HtmlBox fields in Document are std::ptr::null() immediately
// after parse_html_with_hooks returns.  We never send a Document that has had
// mouse/keyboard events fired on it.
unsafe impl Send for FreshDoc {}

enum LoadResult {
    // Fully parsed + styled document, ready to layout on the main thread
    Page  { tab_id: usize, url: String, doc: FreshDoc, css_sheets: Vec<(String, String)> },
    Image { tab_id: usize, src: String, rgba: Vec<u8>, w: u32, h: u32 },
    BgImage { tab_id: usize, src: String, rgba: Vec<u8>, w: u32, h: u32 },
}

// ─── Tab ──────────────────────────────────────────────────────────────────────

struct Tab {
    id:      usize,
    url:     String,
    title:   String,
    history: Vec<String>,
    hist_i:  usize,
    doc:     Option<Document>,
    loading: bool,
}

impl Tab {
    fn new(id: usize) -> Self {
        Self { id, url: String::new(), title: "New Tab".into(),
               history: vec![], hist_i: 0, doc: None, loading: false }
    }
    fn can_back(&self)    -> bool { self.hist_i > 0 }
    fn can_forward(&self) -> bool { self.hist_i + 1 < self.history.len() }
    fn short_title(&self) -> String {
        let t = self.title.trim();
        let chars: Vec<char> = t.chars().collect();
        if chars.len() > 22 { format!("{}…", chars[..22].iter().collect::<String>()) }
        else { chars.iter().collect() }
    }
}

// ─── Chrome click regions ─────────────────────────────────────────────────────

#[derive(Debug)]
enum ChromeHit { None, Back, Forward, Reload, UrlBar, Tab(usize), CloseTab(usize), NewTab }

// ─── App ──────────────────────────────────────────────────────────────────────

struct BrowserApp {
    window:   Option<Arc<Window>>,
    platform: Option<Platform>,

    renderer:        Renderer,  // page content
    chrome_renderer: Renderer,  // browser chrome

    tabs:    Vec<Tab>,
    active:  usize,
    next_id: usize,

    chrome_doc: Option<Document>,

    // URL bar state
    url_text:    String,
    url_focused: bool,

    mouse_pos: (f32, f32),
    width:     f32,
    height:    f32,

    // Inspector state
    inspect_mode: bool,
    inspect_node: *const rhtmledit::HtmlBox,
    inspect_panel_pct: f32,  // panel WIDTH as fraction of window (0.3 = 30%)
    inspect_dragging: bool,  // true while dragging the vertical splitter
    inspect_tab: u8,         // 0=Styles, 1=Computed, 2=Box Model
    inspect_dom_split: f32,  // fraction of panel height for DOM tree (top), rest for tabs (bottom)
    inspect_dom_scroll: f32, // current scroll offset of the DOM tree

    pending_navigate: std::sync::Arc<std::sync::Mutex<Option<String>>>,

    tx:    mpsc::Sender<LoadResult>,
    rx:    mpsc::Receiver<LoadResult>,
    proxy: EventLoopProxy<()>,
    initial_url: Option<String>,
    cache_dir: Option<String>,  // Some("snapshot_cache") when --cached
}

impl BrowserApp {
    fn new(proxy: EventLoopProxy<()>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            window: None, platform: None,
            renderer: Renderer::new(), chrome_renderer: Renderer::new(),
            tabs: vec![Tab::new(0)], active: 0, next_id: 1,
            chrome_doc: None,
            url_text: String::new(), url_focused: false,
            mouse_pos: (0.0, 0.0), width: 1280.0, height: 800.0,
            inspect_mode: false, inspect_node: std::ptr::null(), inspect_panel_pct: 0.0,
            inspect_dragging: false, inspect_tab: 0, inspect_dom_split: 0.5, inspect_dom_scroll: 0.0,
            pending_navigate: std::sync::Arc::new(std::sync::Mutex::new(None)),
            tx, rx, proxy, initial_url: None, cache_dir: None,
        }
    }

    fn content_h(&self) -> f32 { (self.height - CHROME_H).max(0.0) }
    fn page_width(&self) -> f32 {
        if self.inspect_mode { (self.width * (1.0 - self.inspect_panel_pct)).max(100.0) } else { self.width }
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    fn navigate(&mut self, url: String) {
        let url = normalize_url(url);
        let tab = &mut self.tabs[self.active];
        if !tab.history.is_empty() {
            tab.history.truncate(tab.hist_i + 1);
        }
        tab.history.push(url.clone());
        tab.hist_i = tab.history.len() - 1;
        tab.url = url.clone();
        tab.title = "Loading…".into();
        tab.loading = true;
        tab.doc = None;
        self.url_text = url.clone();
        self.url_focused = false;
        self.rebuild_chrome();
        let id = self.tabs[self.active].id;
        self.start_fetch(id, url);
    }

    fn go_back(&mut self) {
        let tab = &mut self.tabs[self.active];
        if !tab.can_back() { return; }
        tab.hist_i -= 1;
        let url = tab.history[tab.hist_i].clone();
        tab.url = url.clone(); tab.loading = true; tab.doc = None;
        self.url_text = url.clone();
        self.rebuild_chrome();
        let id = self.tabs[self.active].id;
        self.start_fetch(id, url);
    }

    fn go_forward(&mut self) {
        let tab = &mut self.tabs[self.active];
        if !tab.can_forward() { return; }
        tab.hist_i += 1;
        let url = tab.history[tab.hist_i].clone();
        tab.url = url.clone(); tab.loading = true; tab.doc = None;
        self.url_text = url.clone();
        self.rebuild_chrome();
        let id = self.tabs[self.active].id;
        self.start_fetch(id, url);
    }

    fn reload(&mut self) {
        let url = self.tabs[self.active].url.clone();
        if url.is_empty() { return; }
        let id = self.tabs[self.active].id;
        self.tabs[self.active].loading = true;
        self.tabs[self.active].doc = None;
        self.rebuild_chrome();
        self.start_fetch(id, url);
    }

    fn new_tab(&mut self) {
        let id = self.next_id; self.next_id += 1;
        self.tabs.push(Tab::new(id));
        self.active = self.tabs.len() - 1;
        self.navigate(NEW_TAB_URL.to_string());
    }

    fn switch_tab(&mut self, i: usize) {
        if i >= self.tabs.len() { return; }
        self.active = i;
        self.url_text = self.tabs[i].url.clone();
        self.url_focused = false;
        self.rebuild_chrome();
    }

    fn close_tab(&mut self, i: usize) {
        if self.tabs.len() == 1 {
            self.tabs[0] = Tab::new(0);
            self.navigate(NEW_TAB_URL.to_string());
            return;
        }
        self.tabs.remove(i);
        if self.active >= self.tabs.len() { self.active = self.tabs.len() - 1; }
        self.url_text = self.tabs[self.active].url.clone();
        self.rebuild_chrome();
    }

    // ── Async fetch ───────────────────────────────────────────────────────────

    fn start_fetch(&self, tab_id: usize, url: String) {
        let tx = self.tx.clone();
        let proxy = self.proxy.clone();
        let cache_dir = self.cache_dir.clone();
        std::thread::spawn(move || {
            let t0 = std::time::Instant::now();
            let mut final_url = url.clone();
            let html = if url == NEW_TAB_URL || url.starts_with("about:") {
                NEW_TAB_HTML.to_string()
            } else if let Some(path) = url.strip_prefix("file://") {
                std::fs::read_to_string(path)
                    .unwrap_or_else(|e| format!("<h2>File error</h2><p>{e}</p>"))
            } else if let Some(ref cd) = cache_dir {
                // Cache uses the original URL as key; redirect target is lost
                cached_fetch_text(&url, cd).unwrap_or_else(|e| error_page(&url, &e))
            } else {
                match fetch_text_with_url(&url) {
                    Ok((body, redirected_url)) => {
                        final_url = redirected_url;
                        body
                    }
                    Err(e) => error_page(&url, &e),
                }
            };
            eprintln!("[browser] HTML fetch: {:.0}ms ({} bytes)", t0.elapsed().as_millis(), html.len());

            // CSS channel: receives sheets as they finish fetching.
            let (css_tx, css_rx) = std::sync::mpsc::channel::<(usize, String, String)>();
            let base = final_url.clone();
            let css_idx = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

            // parse_html_with_hooks fires our callback for every open tag.
            // When a <link rel="stylesheet"> is seen we immediately spawn a
            // fetch thread — it races against the rest of the HTML body parse,
            // so by the time parse returns, most CSS is already in-flight.
            let css_tx2 = css_tx.clone();
            let css_idx2 = css_idx.clone();
            let cache_dir2 = cache_dir.clone();
            let t1 = std::time::Instant::now();
            let doc = parse_html_with_hooks(&html, &url, move |tag, attrs| {
                if tag == "link"
                    && attrs.get("rel").map(|s| s == "stylesheet").unwrap_or(false)
                {
                    if let Some(href) = attrs.get("href") {
                        let abs = resolve_url(&base, href);
                        let sender = css_tx2.clone();
                        let idx = css_idx2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        let cd = cache_dir2.clone();
                        eprintln!("[browser]   CSS fetch start: {abs}");
                        std::thread::spawn(move || {
                            let t = std::time::Instant::now();
                            let text = if let Some(ref cd) = cd {
                                cached_fetch_text(&abs, cd).unwrap_or_default()
                            } else {
                                fetch_text(&abs).unwrap_or_default()
                            };
                            eprintln!("[browser]   CSS fetch done:  {abs} ({:.0}ms, {} bytes)", t.elapsed().as_millis(), text.len());
                            let _ = sender.send((idx, abs, text));
                        });
                    }
                }
            });
            eprintln!("[browser] Parse: {:.0}ms", t1.elapsed().as_millis());

            // Collect in declaration order.
            drop(css_tx);
            let t2 = std::time::Instant::now();
            let mut css_results: Vec<(usize, String, String)> = css_rx.iter().collect();
            eprintln!("[browser] CSS wait: {:.0}ms ({} sheets)", t2.elapsed().as_millis(), css_results.len());
            css_results.sort_by_key(|(idx, _, _)| *idx);
            let css_sheets: Vec<(String, String)> = css_results.into_iter().map(|(_, url, s)| (url, s)).collect();

            let _ = tx.send(LoadResult::Page {
                tab_id, url: final_url, doc: FreshDoc(doc), css_sheets,
            });
            let _ = proxy.send_event(());
        });
    }

    // ── Process incoming results ───────────────────────────────────────────────

    fn process_results(&mut self) {
        // Collect all pending results, then batch image updates to avoid
        // one full re-layout per image (13 images × 360ms = 5s).
        let mut pending: Vec<LoadResult> = Vec::new();
        while let Ok(res) = self.rx.try_recv() {
            pending.push(res);
        }
        if pending.is_empty() { return; }

        // Track which tabs need an image re-layout.
        let mut tabs_need_relayout: Vec<usize> = Vec::new();

        for res in pending {
            match res {
                LoadResult::Page { tab_id, url, doc: FreshDoc(mut doc), css_sheets } => {
                    let Some(idx) = self.tabs.iter().position(|t| t.id == tab_id) else { continue };

                    // Apply any stylesheets that arrived (fetched in parallel during parse)
                    let t_css = std::time::Instant::now();
                    let mut had_css = false;
                    for (css_url, css) in &css_sheets {
                        if !css.is_empty() {
                            doc.stylesheet.parse_and_add_with_base(css, css_url);
                            had_css = true;
                        }
                    }
                    eprintln!("[browser] CSS parse: {:.0}ms ({} rules)", t_css.elapsed().as_millis(), doc.stylesheet.rules.len());
                    if had_css {
                        let t_casc = std::time::Instant::now();
                        let w = self.width;
                        let ch = self.content_h();
                        doc.stylesheet.rebuild_index();
                        apply_cascade_vp(&mut doc.root, &doc.stylesheet, None, 16.0, w, ch, 0, false);
                        eprintln!("[browser] Cascade: {:.0}ms", t_casc.elapsed().as_millis());
                    }

                    // Update URL (may have changed due to redirects)
                    self.tabs[idx].url = url.clone();
                    self.tabs[idx].title = if doc.title.is_empty() {
                        url.split('/').filter(|s| !s.is_empty()).last()
                            .unwrap_or("Untitled").to_string()
                    } else { doc.title.clone() };
                    self.tabs[idx].loading = false;

                    // Fetch images asynchronously (non-blocking, arrive later)
                    let img_semaphore = Arc::new(Semaphore::new(4));
                    let mut img_srcs: Vec<String> = Vec::new();
                    let mut bg_srcs: Vec<String> = Vec::new();
                    Document::walk_all(&doc.root, &mut |b| {
                        if b.tag == "img" {
                            if let Some(src) = b.attributes.get("src") {
                                let abs = resolve_url(&url, src);
                                if !img_srcs.contains(&abs) { img_srcs.push(abs); }
                            }
                        }
                        if b.bg_image_data.is_none() && !b.style.background_image_url.is_empty() {
                            let bg_url = b.style.background_image_url.clone();
                            if !bg_srcs.contains(&bg_url) { bg_srcs.push(bg_url); }
                        }
                    });
                    for src in img_srcs {
                        let tx = self.tx.clone(); let proxy = self.proxy.clone();
                        let s2 = src.clone();
                        let cd = self.cache_dir.clone();
                        let semaphore = img_semaphore.clone();
                        std::thread::spawn(move || {
                            // Limit concurrent image fetches to avoid 429 rate-limiting
                            let _permit = semaphore.acquire();
                            let bytes_result = if let Some(ref cd) = cd {
                                cached_fetch_bytes(&s2, cd)
                            } else {
                                fetch_bytes_with_retry(&s2)
                            };
                            match bytes_result {
                                Ok(bytes) => {
                                    // Try raster decode, then SVG fallback
                                    let decoded = image::load_from_memory(&bytes).ok()
                                        .map(|img| {
                                            let rgba = img.to_rgba8();
                                            (rgba.width(), rgba.height(), rgba.into_raw())
                                        })
                                        .or_else(|| {
                                            std::str::from_utf8(&bytes).ok()
                                                .and_then(|svg| rhtmledit::html::rasterize_svg_intrinsic(svg))
                                                .map(|(rgba, w, h)| (w, h, rgba))
                                        });
                                    if let Some((w, h, raw)) = decoded {
                                        let _ = tx.send(LoadResult::Image { tab_id, src, rgba: raw, w, h });
                                        let _ = proxy.send_event(());
                                    }
                                }
                                Err(_) => {}
                            }
                        });
                    }

                    // Spawn async background image fetches (skip data: URLs, already inline)
                    for bg_src in bg_srcs {
                        if bg_src.starts_with("data:") { continue; }
                        let tx = self.tx.clone(); let proxy = self.proxy.clone();
                        let cd = self.cache_dir.clone();
                        let semaphore = img_semaphore.clone();
                        std::thread::spawn(move || {
                            let _permit = semaphore.acquire();
                            let bytes_result = if let Some(ref cd) = cd {
                                cached_fetch_bytes(&bg_src, cd)
                            } else {
                                fetch_bytes_with_retry(&bg_src)
                            };
                            if let Ok(bytes) = bytes_result {
                                if let Ok(img) = image::load_from_memory(&bytes) {
                                    let rgba = img.to_rgba8();
                                    let (w, h) = (rgba.width(), rgba.height());
                                    let _ = tx.send(LoadResult::BgImage { tab_id, src: bg_src, rgba: rgba.into_raw(), w, h });
                                    let _ = proxy.send_event(());
                                }
                            }
                        });
                    }

                    let t_layout = std::time::Instant::now();
                    self.layout_doc(&mut doc);
                    eprintln!("[browser] Layout: {:.0}ms", t_layout.elapsed().as_millis());

                    // Wire form events — submit navigates, collecting form data
                    let nav = self.pending_navigate.clone();
                    let proxy = self.proxy.clone();
                    let tab_url = url.clone();
                    doc.on_form_event = Some(Box::new(move |event: &rhtmledit::FormEvent| {
                        if let rhtmledit::FormEventKind::Submit(action) = &event.kind {
                            let target = if action.is_empty() {
                                tab_url.clone()
                            } else {
                                resolve_url(&tab_url, action)
                            };
                            // TODO: collect form data and encode as query string (GET) or body (POST)
                            // For now, just navigate to the action URL
                            eprintln!("[browser] Form submit → {}", target);
                            *nav.lock().unwrap() = Some(target);
                            let _ = proxy.send_event(());
                        }
                    }));
                    self.tabs[idx].doc = Some(doc);
                    if idx == self.active { self.url_text = self.tabs[idx].url.clone(); }
                    self.rebuild_chrome();
                }
                LoadResult::Image { tab_id, src, rgba, w, h } => {
                    let Some(idx) = self.tabs.iter().position(|t| t.id == tab_id) else { continue };
                    let Some(doc) = self.tabs[idx].doc.as_mut() else { continue };
                    let base = doc.base_url.clone();
                    Document::walk_all_mut(&mut doc.root, &mut |b| {
                        if b.tag == "img" {
                            if let Some(s) = b.attributes.get("src") {
                                if resolve_url(&base, s) == src {
                                    b.image_data  = Some(rgba.clone());
                                    b.image_width = w; b.image_height = h;
                                    b.layout.layout_dirty = true;
                                }
                            }
                        }
                    });
                    if !tabs_need_relayout.contains(&idx) {
                        tabs_need_relayout.push(idx);
                    }
                }
                LoadResult::BgImage { tab_id, src, rgba, w, h } => {
                    let Some(idx) = self.tabs.iter().position(|t| t.id == tab_id) else { continue };
                    let Some(doc) = self.tabs[idx].doc.as_mut() else { continue };
                    Document::walk_all_mut(&mut doc.root, &mut |b| {
                        if b.bg_image_data.is_none() && b.style.background_image_url == src {
                            b.bg_image_data   = Some(rgba.clone());
                            b.bg_image_width  = w;
                            b.bg_image_height = h;
                        }
                    });
                    // Background images don't affect layout, just repaint
                    if idx == self.active {
                        let _ = self.proxy.send_event(());
                    }
                }
            }
        }

        // One batched re-layout per tab instead of one per image.
        for idx in tabs_need_relayout {
            let width = self.width; let ch = self.content_h();
            if let Some(doc) = self.tabs[idx].doc.as_mut() {
                // Propagate layout_dirty up from dirty images to ancestors
                // so the subtree pruning in layout_box actually visits them.
                propagate_dirty(&mut doc.root);
                let t_img = std::time::Instant::now();
                let mut eng = self.renderer.layout_engine();
                eng.viewport_h = ch; eng.layout_no_cascade(doc, width);
                eprintln!("[browser] Image batch re-layout: {:.0}ms", t_img.elapsed().as_millis());
            }
        }
    }

    fn layout_doc(&mut self, doc: &mut Document) {
        let w = self.width; let ch = self.content_h();
        let mut eng = self.renderer.layout_engine();
        eng.viewport_h = ch; eng.layout(doc, w);
    }

    fn relayout_active(&mut self) {
        let w = self.width; let ch = self.content_h();
        if let Some(doc) = self.tabs[self.active].doc.as_mut() {
            let mut eng = self.renderer.layout_engine();
            eng.viewport_h = ch; eng.layout(doc, w);
        }
    }

    // ── Chrome ────────────────────────────────────────────────────────────────

    fn rebuild_chrome(&mut self) {
        let html = self.chrome_html();
        let mut doc = parse_html_with_hooks(&html, "", |_, _| {});
        let w = self.width;
        let mut eng = self.chrome_renderer.layout_engine();
        eng.viewport_h = CHROME_H; eng.layout(&mut doc, w);
        self.chrome_doc = Some(doc);
    }

    fn chrome_html(&self) -> String {
        let n = self.tabs.len();
        // Give tabs a fair share, capped at max, but reserve space for new-tab button
        let avail = self.width - 40.0;
        let tab_w = (avail / n as f32).min(TAB_MAX_W).max(TAB_MIN_W);

        let mut tabs_html = String::new();
        for (i, tab) in self.tabs.iter().enumerate() {
            let active = i == self.active;
            let cls     = if active { "tab active" } else { "tab" };
            let x_cls   = if active { "tab-x ax" } else { "tab-x" };
            let title   = escape_html(&tab.short_title());
            let fav_bg  = domain_color(&tab.url);
            let fav_ch  = domain_letter(&tab.url);
            let spinner = if tab.loading { "↻ " } else { "" };
            tabs_html.push_str(&format!(
                r#"<div class="{cls}" id="tab-{i}" style="max-width:{tab_w:.0}px;min-width:{TAB_MIN_W}px"><div class="fav" style="background:{fav_bg}">{fav_ch}</div><span class="tab-t">{spinner}{title}</span><span class="{x_cls}" id="tab-x-{i}">&#215;</span></div>"#
            ));
        }

        let tab     = &self.tabs[self.active];
        let sec     = if tab.url.starts_with("https://") { "<span class='lock'>&#128274;</span>" }
                      else if tab.url.starts_with("http://") { "<span class='warn'>&#9888;</span>" }
                      else { "" };
        let url_txt = if self.url_focused { escape_html(&self.url_text) } else { pretty_url(&tab.url) };
        let caret   = if self.url_focused { "<span class='cur'>|</span>" } else { "" };
        let url_cls = if self.url_focused { "urlbar focused" } else { "urlbar" };
        let bd      = if tab.can_back()    { "btn" } else { "btn dis" };
        let fd      = if tab.can_forward() { "btn" } else { "btn dis" };

        format!(r#"<!DOCTYPE html><html><head><style>
*{{box-sizing:border-box;margin:0;padding:0}}
body{{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;
      background:#1C1C1F;display:flex;flex-direction:column;height:{CHROME_H}px;overflow:hidden}}
#tab-bar{{display:flex;align-items:flex-end;height:36px;background:#141416;
          padding:0 4px 0 4px;gap:1px;overflow:hidden}}
.tab{{display:flex;align-items:center;height:30px;padding:0 10px;
      border-radius:8px 8px 0 0;background:transparent;color:#6E7077;
      font-size:12px;cursor:pointer;gap:6px;flex-shrink:0;
      border:1px solid transparent;border-bottom:none;overflow:hidden}}
.tab.active{{background:#1C1C1F;color:#E8EAED;border-color:#2C2C2F;border-bottom-color:#1C1C1F}}
.fav{{width:15px;height:15px;border-radius:4px;flex-shrink:0;
      font-size:9px;font-weight:800;color:#fff;
      display:flex;align-items:center;justify-content:center}}
.tab-t{{flex:1;overflow:hidden;white-space:nowrap;min-width:0}}
.tab-x{{color:transparent;font-size:13px;width:18px;height:18px;flex-shrink:0;
        border-radius:4px;display:flex;align-items:center;justify-content:center}}
.ax{{color:#6E7077}}
#btn-new-tab{{width:28px;height:28px;border-radius:8px;color:#6E7077;font-size:20px;
              display:flex;align-items:center;justify-content:center;cursor:pointer;
              align-self:flex-end;margin-bottom:1px;flex-shrink:0;margin-left:2px}}
#nav-bar{{display:flex;align-items:center;height:44px;background:#1C1C1F;padding:0 10px;gap:4px;
          border-top:1px solid #2C2C2F}}
.btn{{width:30px;height:30px;border-radius:50%;color:#C9CBD0;font-size:16px;
      display:flex;align-items:center;justify-content:center;cursor:pointer;flex-shrink:0}}
.dis{{color:#3A3B3E;cursor:default}}
.urlbar{{flex:1;height:32px;background:#2A2B2E;border:1.5px solid #3A3B3E;
         border-radius:16px;color:#E8EAED;font-size:13px;padding:0 14px;
         display:flex;align-items:center;overflow:hidden;cursor:text;margin:0 8px;gap:6px}}
.urlbar.focused{{border-color:#5E9CF8;background:#252628}}
.lock{{color:#5CB85C;font-size:11px;flex-shrink:0}}
.warn{{color:#F0AD4E;font-size:11px;flex-shrink:0}}
.url-t{{flex:1;overflow:hidden;white-space:nowrap;color:#C9CBD0}}
.urlbar.focused .url-t{{color:#E8EAED}}
.cur{{color:#5E9CF8}}
#ext-btn{{width:30px;height:30px;border-radius:50%;color:#6E7077;font-size:19px;
          display:flex;align-items:center;justify-content:center;cursor:pointer;flex-shrink:0}}
</style></head><body>
<div id="tab-bar">{tabs_html}<div id="btn-new-tab">+</div></div>
<div id="nav-bar">
  <div class="{bd}" id="btn-back">&#8592;</div>
  <div class="{fd}" id="btn-fwd">&#8594;</div>
  <div class="btn" id="btn-reload">&#8635;</div>
  <div class="{url_cls}" id="url-bar">{sec}<span class="url-t">{url_txt}{caret}</span></div>
  <div id="ext-btn">&#8942;</div>
</div></body></html>"#)
    }

    fn chrome_hit(&self, x: f32, y: f32) -> ChromeHit {
        let Some(doc) = &self.chrome_doc else { return ChromeHit::None };
        let pt_in = |id: &str| -> bool {
            if let Some(b) = dom::query_selector(&doc.root, &format!("#{id}")) {
                let r = &b.layout.border_rect;
                x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
            } else { false }
        };
        // Close buttons first (smaller, inside tab)
        for i in 0..self.tabs.len() {
            if pt_in(&format!("tab-x-{i}")) { return ChromeHit::CloseTab(i); }
        }
        if pt_in("btn-back")   { return ChromeHit::Back; }
        if pt_in("btn-fwd")    { return ChromeHit::Forward; }
        if pt_in("btn-reload") { return ChromeHit::Reload; }
        if pt_in("url-bar")    { return ChromeHit::UrlBar; }
        if pt_in("btn-new-tab"){ return ChromeHit::NewTab; }
        for i in 0..self.tabs.len() {
            if pt_in(&format!("tab-{i}")) { return ChromeHit::Tab(i); }
        }
        ChromeHit::None
    }

    // ── Render ────────────────────────────────────────────────────────────────

    fn draw(&mut self) {
        let Some(platform) = self.platform.as_mut() else { return };

        let renderer        = &mut self.renderer;
        let chrome_renderer = &mut self.chrome_renderer;
        let chrome_doc      = self.chrome_doc.as_mut();
        let active_doc      = self.tabs[self.active].doc.as_mut();

        let t_draw = std::time::Instant::now();
        platform.render(|scale, pixmap| {
            pixmap.fill(tiny_skia::Color::from_rgba8(28, 28, 31, 255));

            let w_px = pixmap.width();
            let h_px = pixmap.height();
            let chrome_px  = ((CHROME_H * scale) as u32).min(h_px);
            let content_px = h_px.saturating_sub(chrome_px);

            // Page content in sub-pixmap, blitted below chrome
            let inspect_on = self.inspect_mode;
            let inspect_pct = self.inspect_panel_pct;
            let inspect_tab = self.inspect_tab;
            let inspect_dom_scroll = &mut self.inspect_dom_scroll;
            let inspect_ptr = self.inspect_node;
            let inspect_dom_split = self.inspect_dom_split;
            let doc_root_ptr: *const rhtmledit::HtmlBox = active_doc.as_ref()
                .map(|d| &d.root as *const rhtmledit::HtmlBox)
                .unwrap_or(std::ptr::null());
            if let Some(doc) = active_doc {
                let page_w = if inspect_on {
                    ((w_px as f32) * (1.0 - inspect_pct)).max(100.0) as u32
                } else { w_px };

                // Page content (left side)
                if let Some(mut pm) = Pixmap::new(page_w.max(1), content_px.max(1)) {
                    pm.fill(tiny_skia::Color::from_rgba8(255, 255, 255, 255));
                    let t_render = std::time::Instant::now();
                    renderer.render(doc, &mut pm, scale);
                    if inspect_on && !inspect_ptr.is_null() {
                        let node = unsafe { &*inspect_ptr };
                        rhtmledit::draw_inspect_overlay(node, &mut pm, doc.scroll_x, doc.scroll_y, scale);
                    }
                    let render_ms = t_render.elapsed().as_millis();
                    if render_ms > 5 {
                        eprintln!("[browser] Render: {render_ms}ms ({}x{})", page_w, content_px);
                    }
                    pixmap.draw_pixmap(0, chrome_px as i32, pm.as_ref(),
                        &PixmapPaint::default(), Transform::identity(), None);
                }

                // Inspector panel (right side): DOM tree on top, tabs on bottom
                if inspect_on {
                    let panel_x = page_w;
                    let panel_w = w_px.saturating_sub(page_w).max(1);
                    let panel_w_logical = panel_w as f32 / scale;
                    // Split: DOM tree top, tabs bottom
                    let dom_h = ((content_px as f32) * inspect_dom_split) as u32;
                    let tabs_h = content_px.saturating_sub(dom_h);

                    // DOM tree
                    let (dom_tree_body, selected_line) = if !doc_root_ptr.is_null() {
                        let root = unsafe { &*doc_root_ptr };
                        build_dom_tree_html(root, inspect_ptr)
                    } else {
                        (String::new(), None)
                    };
                    let dom_html = format!(
                        "<html><head><style>\
                         body{{background:#1e1e1e;margin:0;padding:4px;font:11px monospace}}\
                         </style></head><body>{dom_tree_body}</body></html>"
                    );
                    let mut dom_doc = rhtmledit::parse_html(&dom_html);
                    { let mut eng = chrome_renderer.layout_engine(); eng.layout(&mut dom_doc, panel_w_logical); }
                    // Scroll DOM tree to selected element
                    if let Some(line) = selected_line {
                        let target_y = line as f32 * 16.0;
                        let visible_h = dom_h as f32 / scale;
                        *inspect_dom_scroll = (target_y - visible_h * 0.3).max(0.0);
                    }
                    dom_doc.scroll_y = *inspect_dom_scroll;
                    if let Some(mut pm) = Pixmap::new(panel_w, dom_h.max(1)) {
                        pm.fill(tiny_skia::Color::from_rgba8(30, 30, 30, 255));
                        chrome_renderer.render(&mut dom_doc, &mut pm, scale);
                        pixmap.draw_pixmap(panel_x as i32, chrome_px as i32, pm.as_ref(),
                            &PixmapPaint::default(), Transform::identity(), None);
                    }

                    // Tabs
                    let tabs_html = if !inspect_ptr.is_null() {
                        let node = unsafe { &*inspect_ptr };
                        build_inspect_panel_html(node, inspect_tab, None)
                    } else {
                        "<html><body style='background:#1e1e1e;color:#666;padding:10px;font:11px monospace'>Right-click to inspect</body></html>".into()
                    };
                    let mut tabs_doc = rhtmledit::parse_html(&tabs_html);
                    { let mut eng = chrome_renderer.layout_engine(); eng.layout(&mut tabs_doc, panel_w_logical); }
                    if let Some(mut pm) = Pixmap::new(panel_w, tabs_h.max(1)) {
                        pm.fill(tiny_skia::Color::from_rgba8(30, 30, 30, 255));
                        chrome_renderer.render(&mut tabs_doc, &mut pm, scale);
                        pixmap.draw_pixmap(panel_x as i32, (chrome_px + dom_h) as i32, pm.as_ref(),
                            &PixmapPaint::default(), Transform::identity(), None);
                    }

                    // Vertical divider between page and panel
                    if let Some(r) = tiny_skia::Rect::from_xywh(panel_x as f32 - 1.0, chrome_px as f32, 2.0, content_px as f32) {
                        let mut p = tiny_skia::Paint::default();
                        p.set_color(tiny_skia::Color::from_rgba8(60, 60, 65, 255));
                        pixmap.fill_rect(r, &p, Transform::identity(), None);
                    }
                    // Horizontal divider between DOM and tabs
                    if let Some(r) = tiny_skia::Rect::from_xywh(panel_x as f32, (chrome_px + dom_h) as f32 - 1.0, panel_w as f32, 1.0) {
                        let mut p = tiny_skia::Paint::default();
                        p.set_color(tiny_skia::Color::from_rgba8(50, 50, 55, 255));
                        pixmap.fill_rect(r, &p, Transform::identity(), None);
                    }
                }
            } else {
                // Grey placeholder while loading
                if let Some(r) = tiny_skia::Rect::from_xywh(0.0, chrome_px as f32,
                        w_px as f32, content_px as f32) {
                    let mut p = tiny_skia::Paint::default();
                    p.set_color(tiny_skia::Color::from_rgba8(26, 26, 29, 255));
                    pixmap.fill_rect(r, &p, Transform::identity(), None);
                }
            }

            // Chrome rendered on top
            if let Some(doc) = chrome_doc {
                if let Some(mut pm) = Pixmap::new(w_px, chrome_px.max(1)) {
                    pm.fill(tiny_skia::Color::from_rgba8(28, 28, 31, 255));
                    chrome_renderer.render(doc, &mut pm, scale);
                    pixmap.draw_pixmap(0, 0, pm.as_ref(),
                        &PixmapPaint::default(), Transform::identity(), None);
                }
            }

            // Bottom chrome separator line
            if let Some(r) = tiny_skia::Rect::from_xywh(0.0, chrome_px as f32 - 1.0,
                    w_px as f32, 1.0) {
                let mut p = tiny_skia::Paint::default();
                p.set_color(tiny_skia::Color::from_rgba8(20, 20, 22, 255));
                pixmap.fill_rect(r, &p, Transform::identity(), None);
            }
        });
        let draw_ms = t_draw.elapsed().as_millis();
        if draw_ms > 5 {
            eprintln!("[browser] Draw total: {draw_ms}ms");
        }
    }
}

// ─── winit application handler ────────────────────────────────────────────────

impl ApplicationHandler<()> for BrowserApp {
    fn resumed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        let win = Arc::new(
            el.create_window(Window::default_attributes()
                .with_title("Phoenix Browser")
                .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 800u32))
            ).unwrap()
        );
        let platform = Platform::new_windowed(win.clone());
        self.width  = platform.logical_width();
        self.height = platform.logical_height();
        self.window   = Some(win);
        self.platform = Some(platform);
        let start_url = self.initial_url.take().unwrap_or_else(|| NEW_TAB_URL.to_string());
        self.navigate(start_url);
    }

    fn user_event(&mut self, _el: &winit::event_loop::ActiveEventLoop, _: ()) {
        let t = std::time::Instant::now();
        self.process_results();
        let ms = t.elapsed().as_millis();
        if ms > 0 { eprintln!("[browser] process_results: {:.0}ms", ms); }
        // Check for pending form submit navigation
        let nav_url = self.pending_navigate.lock().unwrap().take();
        if let Some(url) = nav_url {
            self.navigate(url);
        }
        if let Some(w) = &self.window { w.request_redraw(); }
    }

    fn window_event(&mut self, el: &winit::event_loop::ActiveEventLoop,
                    _wid: winit::window::WindowId, event: WindowEvent) {
        let redraw = self.on_event(el, event);
        if redraw { if let Some(w) = &self.window { w.request_redraw(); } }
    }
}

impl BrowserApp {
    fn on_event(&mut self, el: &winit::event_loop::ActiveEventLoop, event: WindowEvent) -> bool {
        // Track modifier keys (Shift, Ctrl) via the renderer
        if matches!(event, WindowEvent::ModifiersChanged(_)) {
            self.renderer.handle_window_event(&event, None);
        }
        match event {
            WindowEvent::CloseRequested => { el.exit(); false }

            WindowEvent::Resized(sz) => {
                if let Some(p) = self.platform.as_mut() { p.resize(sz.width, sz.height); }
                if let Some(p) = self.platform.as_ref() {
                    self.width  = p.logical_width();
                    self.height = p.logical_height();
                }
                self.rebuild_chrome();
                self.relayout_active();
                true
            }

            WindowEvent::CursorMoved { position, .. } => {
                let sf = self.platform.as_ref().map(|p| p.scale_factor()).unwrap_or(1.0);
                let (sx, sy) = (position.x as f32 / sf, position.y as f32 / sf);
                self.mouse_pos = (sx, sy);
                // Inspector vertical splitter drag
                if self.inspect_dragging {
                    self.inspect_panel_pct = (1.0 - sx / self.width).clamp(0.15, 0.7);
                    // Re-layout page at new width
                    let pw = self.page_width();
                    if let Some(doc) = self.tabs[self.active].doc.as_mut() {
                        let mut eng = self.renderer.layout_engine();
                        eng.layout_no_cascade(doc, pw);
                    }
                    return true;
                }
                if sy >= CHROME_H {
                    let csy = sy - CHROME_H;
                    let w = self.width; let ch = self.content_h();
                    if let Some(doc) = self.tabs[self.active].doc.as_mut() {
                        if doc.process_scrollbar_event(HtmlEventType::MouseMove,
                                sx, csy, w, ch) {
                            return true;
                        }
                        // Dispatch MouseMove for hover tracking
                        let doc_pt = (sx, csy + doc.scroll_y);
                        doc.process_mouse_event(HtmlEventType::MouseMove, doc_pt, 0);
                    }
                }
                // Re-layout on hover change (applies :hover cascade for dropdown menus etc.)
                let needs_hover_relayout = self.tabs[self.active].doc.as_ref()
                    .map(|d| d.hover_changed).unwrap_or(false);
                if needs_hover_relayout {
                    let pw = self.page_width();
                    let ch = self.content_h();
                    if let Some(doc) = self.tabs[self.active].doc.as_mut() {
                        let hb = doc.hovered_box;
                        let tag = if hb == 0 { "null".to_string() }
                            else { let p = rhtmledit::types::find_by_node_id(&doc.root, hb); if p.is_null() { "null".to_string() } else { unsafe { &*p }.tag.clone() } };
                        eprintln!("[hover] changed → relayout, hovered={}", tag);
                        let mut eng = self.renderer.layout_engine();
                        eng.viewport_h = ch;
                        eng.layout_no_cascade(doc, pw);
                    }
                    return true;
                }
                // Update cursor icon based on hovered element
                if let Some(doc) = self.tabs[self.active].doc.as_ref() {
                    let ci = self.renderer.cursor_icon(doc);
                    if let Some(w) = &self.window {
                        use winit::window::CursorIcon;
                        let icon = match ci {
                            rhtmledit::CSSCursor::Pointer    => CursorIcon::Pointer,
                            rhtmledit::CSSCursor::Text       => CursorIcon::Text,
                            rhtmledit::CSSCursor::Move       => CursorIcon::Move,
                            rhtmledit::CSSCursor::NotAllowed => CursorIcon::NotAllowed,
                            rhtmledit::CSSCursor::Grab       => CursorIcon::Grab,
                            rhtmledit::CSSCursor::Grabbing   => CursorIcon::Grabbing,
                            rhtmledit::CSSCursor::ColResize  => CursorIcon::ColResize,
                            rhtmledit::CSSCursor::RowResize  => CursorIcon::RowResize,
                            rhtmledit::CSSCursor::Crosshair  => CursorIcon::Crosshair,
                            rhtmledit::CSSCursor::Help       => CursorIcon::Help,
                            rhtmledit::CSSCursor::Wait       => CursorIcon::Wait,
                            _                                => CursorIcon::Default,
                        };
                        w.set_cursor(winit::window::Cursor::Icon(icon));
                    }
                }
                false
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let bt: u8 = match button {
                    MouseButton::Left => 0, MouseButton::Middle => 1,
                    MouseButton::Right => 2, _ => 0,
                };
                let (sx, sy) = self.mouse_pos;

                if state == ElementState::Pressed {
                    // Inspector vertical splitter drag start
                    if self.inspect_mode && bt == 0 {
                        let splitter_x = self.page_width();
                        if (sx - splitter_x).abs() < 5.0 && sy >= CHROME_H {
                            self.inspect_dragging = true;
                            return true;
                        }
                    }
                    if sy < CHROME_H {
                        // Unfocus URL bar if clicking outside it
                        let hit = self.chrome_hit(sx, sy);
                        if !matches!(hit, ChromeHit::UrlBar) && self.url_focused {
                            self.url_focused = false;
                            self.url_text = self.tabs[self.active].url.clone();
                            self.rebuild_chrome();
                        }
                        match hit {
                            ChromeHit::Back          => { self.go_back();    }
                            ChromeHit::Forward       => { self.go_forward(); }
                            ChromeHit::Reload        => { self.reload();     }
                            ChromeHit::NewTab        => { self.new_tab();    }
                            ChromeHit::Tab(i)        => { self.switch_tab(i); }
                            ChromeHit::CloseTab(i)   => { self.close_tab(i); }
                            ChromeHit::UrlBar => {
                                if !self.url_focused {
                                    self.url_focused = true;
                                    self.url_text = self.tabs[self.active].url.clone();
                                    self.rebuild_chrome();
                                }
                            }
                            ChromeHit::None => {}
                        }
                    } else {
                        // Content area
                        if self.url_focused {
                            self.url_focused = false;
                            self.url_text = self.tabs[self.active].url.clone();
                            self.rebuild_chrome();
                        }
                        // If clicking in the inspector panel (right of page)
                        let pw = self.page_width();
                        if self.inspect_mode && sx > pw + 5.0 {
                            let panel_y = sy - CHROME_H;
                            let content_h = self.content_h();
                            let dom_h = content_h * self.inspect_dom_split;
                            let panel_x = sx - pw;

                            if panel_y < dom_h {
                                // Click in DOM tree — select element by line (account for scroll)
                                let line_idx = ((panel_y + self.inspect_dom_scroll) / 16.0) as usize;
                                if let Some(doc) = self.tabs[self.active].doc.as_ref() {
                                    let mut nodes: Vec<*const rhtmledit::HtmlBox> = Vec::new();
                                    collect_dom_nodes(&doc.root, &mut nodes, 0, 20);
                                    if line_idx < nodes.len() {
                                        self.inspect_node = nodes[line_idx];
                                    }
                                }
                            } else {
                                // Click in tabs area
                                let tabs_y = panel_y - dom_h;
                                // Element bar ~28px, then tab bar ~26px
                                if tabs_y >= 28.0 && tabs_y < 56.0 {
                                    if panel_x < 70.0 { self.inspect_tab = 0; }
                                    else if panel_x < 150.0 { self.inspect_tab = 1; }
                                    else { self.inspect_tab = 2; }
                                }
                            }
                            return true;
                        }
                        let csy = sy - CHROME_H;
                        let w = self.width; let ch = self.content_h();
                        // Check link before scrollbar (scrollbar consumed first)
                        let link = {
                            if let Some(doc) = self.tabs[self.active].doc.as_mut() {
                                doc.process_scrollbar_event(HtmlEventType::MouseDown,
                                    sx, csy, w, ch);
                                if bt == 0 {
                                    let doc_pt = (sx + doc.scroll_x, csy + doc.scroll_y);
                                    let href = hit_test_link(&doc.root, doc_pt, 0);
                                    if let Some(ref h) = href { doc.visited_urls.insert(h.clone()); }
                                    href
                                } else { None }
                            } else { None }
                        };
                        if let Some(href) = link {
                            if !href.is_empty() {
                                let resolved = resolve_url(&self.tabs[self.active].url, &href);
                                self.navigate(resolved);
                                return true;
                            }
                        }
                        // Only send mouse events to page for left-click in the page area
                        if bt == 0 {
                            if let Some(doc) = self.tabs[self.active].doc.as_mut() {
                                let doc_pt = (sx + doc.scroll_x, csy + doc.scroll_y);
                                doc.process_mouse_event(HtmlEventType::MouseDown, doc_pt, bt);
                            }
                        }
                        // Deferred inspect setup (right-click)
                        if bt == 2 {
                            // First pass: read-only to get hit target
                            let hit_ptr = {
                                if let Some(doc) = self.tabs[self.active].doc.as_ref() {
                                    let doc_pt = (sx + doc.scroll_x, csy + doc.scroll_y);
                                    point_to_hit(&doc.root, doc_pt, 2).map(|h| {
                                        rhtmledit::types::find_by_node_id(&doc.root, h.node_id)
                                    }).filter(|p| !p.is_null())
                                } else { None }
                            };
                            if let Some(ptr) = hit_ptr {
                                self.inspect_node = ptr;
                                self.inspect_mode = true;
                                if self.inspect_panel_pct < 0.15 { self.inspect_panel_pct = 0.35; }
                                let pw = self.page_width();
                                if let Some(doc) = self.tabs[self.active].doc.as_mut() {
                                    doc.stylesheet.inspect_mode = true;
                                    let mut eng = self.renderer.layout_engine();
                                    eng.layout(doc, pw);
                                }
                            }
                        }
                    }
                    return true;
                } else {
                    // Released
                    if self.inspect_dragging {
                        self.inspect_dragging = false;
                        return true;
                    }
                    if sy >= CHROME_H {
                        let csy = sy - CHROME_H;
                        let w = self.width; let ch = self.content_h();
                        if let Some(doc) = self.tabs[self.active].doc.as_mut() {
                            let doc_pt = (sx + doc.scroll_x, csy + doc.scroll_y);
                            doc.process_scrollbar_event(HtmlEventType::MouseUp,
                                sx, csy, w, ch);
                            doc.process_mouse_event(HtmlEventType::MouseUp, doc_pt, bt);
                        }
                    }
                    return true;
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let (sx, sy) = self.mouse_pos;
                if sy >= CHROME_H {
                    let sf = self.platform.as_ref().map(|p| p.scale_factor()).unwrap_or(1.0);
                    let dy = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => -y * 40.0,
                        winit::event::MouseScrollDelta::PixelDelta(p)   => -(p.y as f32) / sf,
                    };
                    if let Some(doc) = self.tabs[self.active].doc.as_mut() {
                        let doc_pt = (sx + doc.scroll_x, (sy - CHROME_H) + doc.scroll_y);
                        doc.process_wheel_event(doc_pt, dy);
                    }
                    return true;
                }
                false
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed { return false; }
                if self.url_focused {
                    match &event.logical_key {
                        Key::Named(NamedKey::Enter) => {
                            let url = self.url_text.clone();
                            self.navigate(url);
                        }
                        Key::Named(NamedKey::Escape) => {
                            self.url_focused = false;
                            self.url_text = self.tabs[self.active].url.clone();
                            self.rebuild_chrome();
                        }
                        Key::Named(NamedKey::Backspace) => {
                            self.url_text.pop();
                            self.rebuild_chrome();
                        }
                        Key::Character(s) => {
                            self.url_text.push_str(s);
                            self.rebuild_chrome();
                        }
                        _ => {}
                    }
                    return true;
                }
                // Global shortcuts
                match &event.logical_key {
                    Key::Named(NamedKey::BrowserBack)    => { self.go_back();    return true; }
                    Key::Named(NamedKey::BrowserForward) => { self.go_forward(); return true; }
                    Key::Named(NamedKey::BrowserRefresh) => { self.reload();     return true; }
                    Key::Named(NamedKey::F12) => {
                        self.inspect_mode = !self.inspect_mode;
                        self.inspect_panel_pct = if self.inspect_mode { 0.35 } else { 0.0 };
                        if !self.inspect_mode { self.inspect_node = std::ptr::null(); }
                        let pw = self.page_width();
                        if let Some(doc) = self.tabs[self.active].doc.as_mut() {
                            doc.stylesheet.inspect_mode = self.inspect_mode;
                            let mut eng = self.renderer.layout_engine();
                            eng.layout(doc, pw);
                        }
                        return true;
                    }
                    Key::Named(NamedKey::Escape) if self.inspect_mode => {
                        self.inspect_mode = false;
                        self.inspect_panel_pct = 0.0;
                        self.inspect_node = std::ptr::null();
                        return true;
                    }
                    _ => {}
                }
                // Tab / Shift+Tab: focus next/prev form element
                if matches!(&event.logical_key, Key::Named(NamedKey::Tab)) {
                    if let Some(doc) = self.tabs[self.active].doc.as_mut() {
                        // Shift state tracked by renderer's handle_window_event
                        let shifted = self.renderer.is_shift_held();
                        let moved = if shifted { doc.focus_prev() } else { doc.focus_next() };
                        if moved { return true; }
                    }
                }
                // Enter in a focused text input submits the form
                if matches!(&event.logical_key, Key::Named(NamedKey::Enter)) {
                    if let Some(doc) = self.tabs[self.active].doc.as_mut() {
                        if doc.focused_box != 0 {
                            let focused_ptr = rhtmledit::types::find_by_node_id(&doc.root, doc.focused_box);
                            let focused = if focused_ptr.is_null() { None } else { Some(unsafe { &*focused_ptr }) };
                            if focused.map(|f| f.tag.as_str()) == Some("input") {
                                let t: &str = focused.unwrap().attributes.get("type").map(|s| s.as_str()).unwrap_or("text");
                                if matches!(t, "text" | "password" | "email" | "search") {
                                    // Find parent form and submit
                                    let action = rhtmledit::find_parent_form_action(&doc.root, doc.focused_box);
                                    if let Some(ref mut cb) = doc.on_form_event {
                                        cb(&rhtmledit::FormEvent {
                                            tag: "form".into(),
                                            id: String::new(), name: String::new(),
                                            kind: rhtmledit::FormEventKind::Submit(action),
                                            element: doc.focused_box,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                // Route keyboard input through the engine's key event system
                if let Some(doc) = self.tabs[self.active].doc.as_mut() {
                    // Extract character from the key event
                    let ch = match &event.logical_key {
                        Key::Character(s) => s.chars().next(),
                        Key::Named(NamedKey::Space) => Some(' '),
                        Key::Named(NamedKey::Tab) => Some('\t'),
                        _ => None,
                    };
                    let kc = match &event.logical_key {
                        Key::Named(NamedKey::Backspace) => 8,
                        Key::Named(NamedKey::Delete) => 46,
                        Key::Named(NamedKey::Enter) => 13,
                        Key::Named(NamedKey::ArrowLeft) => 37,
                        Key::Named(NamedKey::ArrowRight) => 39,
                        Key::Named(NamedKey::Home) => 36,
                        Key::Named(NamedKey::End) => 35,
                        Key::Named(NamedKey::Space) => 32,
                        Key::Character(_) => 0,
                        _ => 0,
                    };
                    // For character input, use key code 0 and pass char
                    // For special keys, pass key code and no char
                    if kc != 0 || ch.is_some() {
                        let effective_kc = if kc != 0 { kc } else { ch.unwrap_or(' ') as u32 };
                        if doc.process_key_event(rhtmledit::dom::HtmlEventType::KeyDown,
                                effective_kc, ch, false, false, false, false) {
                            return true;
                        }
                    }
                }
                false
            }

            WindowEvent::RedrawRequested => { self.draw(); false }

            _ => false
        }
    }
}

// ─── Helper functions ─────────────────────────────────────────────────────────

/// Return a deterministic accent color for a domain from a curated palette.
fn domain_color(url: &str) -> &'static str {
    const PALETTE: &[&str] = &[
        "#4285F4", "#EA4335", "#34A853", "#FBBC05", "#FF6D00",
        "#7C4DFF", "#00BCD4", "#E91E63", "#795548", "#5E81AC",
        "#3F51B5", "#009688", "#FF5722", "#8BC34A", "#CE422B",
    ];
    let d = extract_domain(url);
    let h = d.bytes().fold(5381usize, |a, b| a.wrapping_mul(33).wrapping_add(b as usize));
    PALETTE[h % PALETTE.len()]
}

/// First letter of the domain, uppercased — used as a favicon substitute.
fn domain_letter(url: &str) -> String {
    if url == NEW_TAB_URL || url.starts_with("about:") || url.is_empty() {
        return "+".to_string();
    }
    extract_domain(url).chars().next()
        .map(|c| c.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// Generate a small color swatch if the value looks like a color.
fn color_swatch(val: &str) -> String {
    let v = val.trim();
    let is_color = v.starts_with('#') && (v.len() == 4 || v.len() == 7 || v.len() == 9)
        || v.starts_with("rgb") || v.starts_with("hsl")
        || matches!(v, "red"|"blue"|"green"|"white"|"black"|"gray"|"grey"|"orange"|"yellow"|"purple"|"pink"|"cyan"|"transparent");
    if is_color && v != "transparent" {
        format!("<span style='display:inline-block;width:10px;height:10px;border:1px solid #555;\
                 background:{};vertical-align:middle;margin-right:3px;border-radius:2px'></span>", escape_html(v))
    } else {
        String::new()
    }
}

/// Collect one pointer per rendered line in the DOM tree.
/// Must exactly match the line output order of `build_dom_tree_html`.
fn collect_dom_nodes(node: &rhtmledit::HtmlBox, out: &mut Vec<*const rhtmledit::HtmlBox>, depth: usize, max_depth: usize) {
    if depth > max_depth { return; }
    if node.tag == "#text" { return; }
    if matches!(node.style.display, rhtmledit::types::Display::None) { return; }
    let has_children = node.children.iter().any(|c| c.tag != "#text"
        && !matches!(c.style.display, rhtmledit::types::Display::None));
    out.push(node as *const rhtmledit::HtmlBox); // opening tag
    for child in &node.children {
        collect_dom_nodes(child, out, depth + 1, max_depth);
    }
    if has_children {
        out.push(node as *const rhtmledit::HtmlBox); // closing tag → same node
    }
}

/// Build DOM tree HTML. Returns (html_string, selected_line_index).
fn build_dom_tree_html(root: &rhtmledit::HtmlBox, selected: *const rhtmledit::HtmlBox) -> (String, Option<usize>) {
    let mut html = String::new();
    let mut line_count = 0usize;
    let mut selected_line: Option<usize> = None;

    fn walk(node: &rhtmledit::HtmlBox, html: &mut String, depth: usize,
            selected: *const rhtmledit::HtmlBox, line: &mut usize, sel_line: &mut Option<usize>) {
        if node.tag == "#text" { return; }
        if matches!(node.style.display, rhtmledit::types::Display::None) { return; }
        if depth > 20 { return; }

        let indent = depth * 14;
        let is_selected = !selected.is_null() && std::ptr::eq(node, unsafe { &*selected });
        if is_selected { *sel_line = Some(*line); }

        let bg = if is_selected { "background:#264f78;" } else { "" };
        let id = node.attributes.get("id")
            .map(|v| format!(" <span style='color:#d7ba7d'>id=\"{}\"</span>", escape_html(v)))
            .unwrap_or_default();
        let cls = node.attributes.get("class")
            .map(|v| format!(" <span style='color:#9cdcfe'>class=\"{}\"</span>",
                escape_html(&v.split_whitespace().take(4).collect::<Vec<_>>().join(" "))))
            .unwrap_or_default();

        let has_children = node.children.iter().any(|c| c.tag != "#text"
            && !matches!(c.style.display, rhtmledit::types::Display::None));
        let arrow = if has_children { "▼ " } else { "  " };

        html.push_str(&format!(
            "<div style='padding:1px 2px 1px {}px;white-space:nowrap;overflow:hidden;line-height:16px;\
             font:11px monospace;cursor:pointer;{bg}'>\
             <span style='color:#888'>{arrow}</span>\
             <span style='color:#569cd6'>&lt;{}</span>{id}{cls}<span style='color:#569cd6'>&gt;</span>\
             </div>\n",
            indent, escape_html(&node.tag)
        ));
        *line += 1;

        for child in &node.children {
            walk(child, html, depth + 1, selected, line, sel_line);
        }

        // Closing tag for elements with children
        if has_children {
            html.push_str(&format!(
                "<div style='padding:1px 2px 1px {}px;line-height:16px;font:11px monospace;color:#569cd6'>\
                 &lt;/{}&gt;</div>\n",
                indent, escape_html(&node.tag)
            ));
            *line += 1;
        }
    }

    walk(root, &mut html, 0, selected, &mut line_count, &mut selected_line);

    (html, selected_line)
}

fn build_inspect_panel_html(node: &rhtmledit::HtmlBox, active_tab: u8, doc_root: Option<&rhtmledit::HtmlBox>) -> String {
    let s = &node.style;
    let id  = node.attributes.get("id").map(|v| format!("#{v}")).unwrap_or_default();
    let cls = node.attributes.get("class")
        .map(|v| format!(".{}", v.split_whitespace().collect::<Vec<_>>().join(".")))
        .unwrap_or_default();

    let tab_style = |idx: u8| -> &'static str {
        if idx == active_tab {
            "color:#fff;border-bottom:2px solid #4fc3f7;padding:6px 12px;font-weight:600"
        } else {
            "color:#888;border-bottom:2px solid transparent;padding:6px 12px;cursor:pointer"
        }
    };

    let mut html = format!(r#"<html><head><style>
        body {{ background: #1e1e1e; color: #d4d4d4; font: 11px -apple-system, sans-serif; padding: 0; margin: 0; }}
        .panel {{ padding: 6px 10px; }}
        .elem-bar {{ background: #2d2d30; padding: 6px 10px; border-bottom: 1px solid #3e3e42;
                     font: 12px monospace; white-space: nowrap; overflow: hidden; }}
        .tab-bar {{ display: flex; background: #252526; border-bottom: 1px solid #3e3e42; font-size: 11px; }}
        .tag {{ color: #569cd6; }}
        .id {{ color: #d7ba7d; }}
        .cls {{ color: #9cdcfe; }}
        .attr-name {{ color: #9cdcfe; }}
        .attr-val {{ color: #ce9178; }}
        h3 {{ color: #ccc; font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px;
             margin: 10px 0 6px 0; padding: 4px 0; border-bottom: 1px solid #3e3e42; }}
        .prop {{ color: #9cdcfe; }}
        .val {{ color: #ce9178; }}
        .computed-row {{ display: flex; padding: 1px 0; }}
        .computed-row .prop {{ min-width: 130px; }}
        .sel {{ color: #dcdcaa; font-weight: 600; }}
        .rule-src {{ color: #858585; font-size: 10px; }}
        .box-model {{ text-align: center; margin: 6px 0; font-size: 10px; }}
        .bm-margin {{ background: rgba(246,178,107,0.15); border: 1px dashed rgba(246,178,107,0.4); padding: 4px; position: relative; }}
        .bm-margin::before {{ content: 'margin'; position: absolute; top: 1px; left: 3px; color: #f6b26b; font-size: 9px; }}
        .bm-padding {{ background: rgba(147,196,125,0.15); border: 1px dashed rgba(147,196,125,0.4); padding: 4px; position: relative; }}
        .bm-padding::before {{ content: 'padding'; position: absolute; top: 1px; left: 3px; color: #93c47d; font-size: 9px; }}
        .bm-content {{ background: rgba(109,158,235,0.2); padding: 6px 4px; color: #6d9eeb; font-weight: 600; }}
        .bm-row {{ display: flex; align-items: center; }}
        .bm-side {{ flex: 0 0 30px; text-align: center; color: #aaa; }}
        .bm-center {{ flex: 1; text-align: center; }}
        .bm-top, .bm-bot {{ text-align: center; color: #aaa; padding: 2px 0; }}
        .rule-block {{ margin-bottom: 6px; padding: 4px 6px; background: #252526; border-radius: 3px; }}
        .rule-block .decl {{ padding-left: 12px; }}
        .rule-block .overridden {{ text-decoration: line-through; color: #666; }}
        .dom-tree {{ font: 11px monospace; padding: 4px 0; }}
        .dom-node {{ padding: 1px 0 1px 12px; white-space: nowrap; overflow: hidden; }}
    </style></head><body>"#);

    // ── Element breadcrumb bar ──
    html.push_str("<div class='elem-bar'>");
    html.push_str(&format!(
        "<span class='tag'>&lt;{}</span><span class='id'>{}</span><span class='cls'>{}</span>",
        escape_html(&node.tag), escape_html(&id), escape_html(&cls)
    ));
    for (k, v) in &node.attributes {
        if k == "id" || k == "class" || k == "style" { continue; }
        let short_v: String = v.chars().take(30).collect();
        html.push_str(&format!(
            " <span class='attr-name'>{}</span>=<span class='attr-val'>\"{}\"</span>",
            escape_html(k), escape_html(&short_v)
        ));
    }
    html.push_str("<span class='tag'>&gt;</span></div>");

    // ── Tab bar ──
    html.push_str(&format!(
        "<div class='tab-bar'>\
         <div style='{}'>Styles</div>\
         <div style='{}'>Computed</div>\
         <div style='{}'>Box Model</div>\
         </div>",
        tab_style(0), tab_style(1), tab_style(2)
    ));

    html.push_str("<div class='panel'>");

    match active_tab {
        0 => {
            // ── Styles tab: matched CSS rules ──
            if !node.matched_rules.is_empty() {
                let mut seen_props: std::collections::HashSet<String> = std::collections::HashSet::new();
                let mut rules_rev: Vec<_> = node.matched_rules.iter().collect();
                rules_rev.reverse();
                for rule in &rules_rev {
                    html.push_str("<div class='rule-block'>");
                    let src_label = if rule.source == "ua" { " (user agent)" } else { "" };
                    html.push_str(&format!(
                        "<div><span class='sel'>{}</span> <span class='rule-src'>sp:{}{}</span></div>",
                        escape_html(&rule.selector), rule.specificity, src_label
                    ));
                    for (prop, val) in &rule.declarations {
                        if prop.starts_with("--") { continue; }
                        let overridden = seen_props.contains(prop);
                        let cls = if overridden { "decl overridden" } else { "decl" };
                        let swatch = color_swatch(val);
                        html.push_str(&format!(
                            "<div class='{cls}'><span class='prop'>{prop}</span>: {swatch}<span class='val'>{}</span>;</div>",
                            escape_html(val)
                        ));
                    }
                    html.push_str("</div>");
                    for (prop, _) in &rule.declarations {
                        if !prop.starts_with("--") { seen_props.insert(prop.clone()); }
                    }
                }
            } else {
                html.push_str("<div style='color:#666;padding:10px'>No matched rules (enable inspect before cascade)</div>");
            }
        }
        1 => {
            // ── Computed tab ──
            let bg = s.background_color;
            let bg_str = if bg.a > 0 {
                format!("#{:02x}{:02x}{:02x}", bg.r, bg.g, bg.b)
            } else { "transparent".into() };
            let props: Vec<(&str, String)> = vec![
                ("display", format!("{:?}", s.display)),
                ("position", format!("{:?}", s.position)),
                ("float", format!("{:?}", s.float)),
                ("box-sizing", format!("{:?}", s.box_sizing)),
                ("width", format!("{:?}", s.width)),
                ("height", format!("{:?}", s.height)),
                ("min-width", format!("{:?}", s.min_width)),
                ("max-width", format!("{:?}", s.max_width)),
                ("overflow", format!("{:?} / {:?}", s.overflow_x, s.overflow_y)),
                ("flex-direction", format!("{:?}", s.flex_direction)),
                ("flex-wrap", format!("{:?}", s.flex_wrap)),
                ("flex", format!("{} {} {:?}", s.flex_grow, s.flex_shrink, s.flex_basis)),
                ("align-items", format!("{:?}", s.align_items)),
                ("align-self", format!("{:?}", s.align_self)),
                ("justify-content", format!("{:?}", s.justify_content)),
                ("vertical-align", format!("{:?}", s.vertical_align)),
                ("font-size", format!("{:.1}px", s.font_size_px(16.0, 16.0))),
                ("line-height", format!("{:?}", s.line_height)),
                ("color", format!("#{:02x}{:02x}{:02x}", s.color.r, s.color.g, s.color.b)),
                ("background", bg_str),
                ("z-index", format!("{}", s.z_index)),
            ];
            for (name, val) in &props {
                html.push_str(&format!(
                    "<div class='computed-row'><span class='prop'>{name}</span><span class='val'>{val}</span></div>"
                ));
            }
            // Children summary
            let elem_children: Vec<_> = node.children.iter()
                .filter(|c| c.tag != "#text" && !matches!(c.style.display, rhtmledit::types::Display::None))
                .collect();
            if !elem_children.is_empty() {
                html.push_str("<h3>Children</h3>");
                html.push_str("<div class='dom-tree'>");
                for child in &elem_children {
                    let cid  = child.attributes.get("id").map(|v| format!("#{v}")).unwrap_or_default();
                    let ccls = child.attributes.get("class")
                        .map(|v| format!(".{}", v.split_whitespace().take(3).collect::<Vec<_>>().join(".")))
                        .unwrap_or_default();
                    html.push_str(&format!(
                        "<div class='dom-node'><span class='tag'>{}</span><span class='id'>{}</span><span class='cls'>{}</span> \
                         <span style='color:#666'>{:?} {:.0}x{:.0}</span></div>",
                        escape_html(&child.tag), escape_html(&cid), escape_html(&ccls),
                        child.style.display, child.layout.content_rect.w, child.layout.content_rect.h
                    ));
                }
                html.push_str("</div>");
            }
        }
        2 => {
            // ── Box Model tab ──
            let c = &node.layout.content_rect;
            let mt = node.layout.resolved_margin_top;
            let mr = node.layout.resolved_margin_right;
            let mb = node.layout.resolved_margin_bottom;
            let ml = node.layout.resolved_margin_left;
            let bt = node.layout.resolved_border_top;
            let bbr = node.layout.resolved_border_right;
            let bb = node.layout.resolved_border_bottom;
            let bbl = node.layout.resolved_border_left;
            let pt = node.layout.resolved_pad_top;
            let pr = node.layout.resolved_pad_right;
            let pb = node.layout.resolved_pad_bottom;
            let pll = node.layout.resolved_pad_left;
            html.push_str(&format!(
                "<div class='box-model'>\
                 <div class='bm-margin'>\
                   <div class='bm-top'>{mt:.0}</div>\
                   <div class='bm-row'><div class='bm-side'>{ml:.0}</div><div class='bm-center'>\
                     <div class='bm-padding'>\
                       <div class='bm-top'>{pt:.0}</div>\
                       <div class='bm-row'><div class='bm-side'>{pll:.0}</div>\
                         <div class='bm-content'>{:.0} x {:.0}</div>\
                       <div class='bm-side'>{pr:.0}</div></div>\
                       <div class='bm-bot'>{pb:.0}</div>\
                     </div>\
                   </div><div class='bm-side'>{mr:.0}</div></div>\
                   <div class='bm-bot'>{mb:.0}</div>\
                 </div></div>",
                c.w, c.h
            ));
            if bt > 0.0 || bbr > 0.0 || bb > 0.0 || bbl > 0.0 {
                html.push_str(&format!(
                    "<div style='color:#888;font-size:10px;text-align:center'>border: {bt:.0} {bbr:.0} {bb:.0} {bbl:.0}</div>"
                ));
            }
            html.push_str(&format!(
                "<div style='color:#666;font-size:10px;text-align:center'>position: ({:.0}, {:.0}) size: {:.0} x {:.0}</div>",
                c.x, c.y, node.layout.margin_rect.w, node.layout.margin_rect.h
            ));
        }
        _ => {}
    }

    html.push_str("</div></body></html>");
    html
}

fn extract_domain(url: &str) -> &str {
    let s = url.trim_start_matches("https://")
               .trim_start_matches("http://")
               .trim_start_matches("file://");
    s.split('/').next().unwrap_or(s)
}

/// Strip scheme for display; return empty string for new-tab / about pages.
fn pretty_url(url: &str) -> String {
    if url == NEW_TAB_URL || url.starts_with("about:") || url.is_empty() {
        return String::new();
    }
    let s = url.trim_start_matches("https://")
               .trim_start_matches("http://");
    escape_html(s.trim_end_matches('/'))
}

/// Normalise a user-typed string into a full URL.
fn normalize_url(s: String) -> String {
    let s = s.trim().to_string();
    if s.is_empty() || s == NEW_TAB_URL || s.starts_with("about:") { return NEW_TAB_URL.to_string(); }
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("file://") { return s; }
    // Looks like a hostname?
    if !s.contains(' ') && (s.contains('.') || s.starts_with("localhost")) {
        return format!("https://{s}");
    }
    // Search query
    let query: String = s.chars().map(|c| match c {
        ' ' => '+', c if c.is_alphanumeric() || "-_.~".contains(c) => c, _ => c,
    }).collect();
    format!("https://duckduckgo.com/?q={query}")
}

/// Propagate layout_dirty upward: if any descendant is dirty, mark the parent dirty too.
/// Returns true if this node or any descendant is dirty.
fn propagate_dirty(node: &mut rhtmledit::HtmlBox) -> bool {
    let mut any_dirty = node.layout.layout_dirty;
    for child in &mut node.children {
        if propagate_dirty(child) {
            any_dirty = true;
        }
    }
    if any_dirty {
        node.layout.layout_dirty = true;
    }
    any_dirty
}

/// Resolve a (possibly relative) `href` against a `base` URL.
fn resolve_url(base: &str, href: &str) -> String {
    rhtmledit::resolve_url(href, base)
}

fn shared_client() -> &'static reqwest::blocking::Client {
    use std::sync::OnceLock;
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| rhtmledit::http_client())
}

fn shared_client_lenient() -> &'static reqwest::blocking::Client {
    use std::sync::OnceLock;
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| rhtmledit::http_client_lenient())
}

/// Fetch URL text, returning (body, final_url_after_redirects).
/// Retries with lenient TLS on cert mismatch (shared-hosting / www subdomain).
fn fetch_text_with_url(url: &str) -> Result<(String, String), String> {
    let do_fetch = |client: &reqwest::blocking::Client| -> Result<(String, String), String> {
        let resp = client.get(url)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header("Sec-Fetch-Dest", "document")
            .header("Sec-Fetch-Mode", "navigate")
            .header("Sec-Fetch-Site", "none")
            .header("Sec-Fetch-User", "?1")
            .header("Upgrade-Insecure-Requests", "1")
            .send().map_err(|e| e.to_string())?;
        let final_url = resp.url().to_string();
        let bytes = resp.bytes().map_err(|e| e.to_string())?;
        let body = String::from_utf8(bytes.to_vec())
            .unwrap_or_else(|_| {
                let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(&bytes);
                cow.into_owned()
            });
        Ok((body, final_url))
    };
    match do_fetch(shared_client()) {
        Ok((body, url)) if !body.is_empty() => Ok((body, url)),
        _ => do_fetch(shared_client_lenient()),
    }
}

fn fetch_text(url: &str) -> Result<String, String> {
    fetch_text_with_url(url).map(|(body, _)| body)
}

fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let do_fetch = |client: &reqwest::blocking::Client| -> Result<Vec<u8>, String> {
        let resp = client.get(url)
            .header("Accept", "image/avif,image/webp,image/apng,image/*,*/*;q=0.8")
            .header("Sec-Fetch-Dest", "image")
            .header("Sec-Fetch-Mode", "no-cors")
            .header("Sec-Fetch-Site", "cross-site")
            .send().map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let bytes = resp.bytes().map_err(|e| e.to_string())?;
        Ok(bytes.to_vec())
    };
    match do_fetch(shared_client()) {
        Ok(b) if !b.is_empty() => Ok(b),
        _ => do_fetch(shared_client_lenient()),
    }
}

fn fetch_bytes_with_retry(url: &str) -> Result<Vec<u8>, String> {
    for attempt in 0..3 {
        match fetch_bytes(url) {
            Ok(bytes) => return Ok(bytes),
            Err(e) if e.contains("429") && attempt < 2 => {
                std::thread::sleep(std::time::Duration::from_millis(500 * (attempt as u64 + 1)));
            }
            Err(e) => return Err(e),
        }
    }
    Err("max retries".to_string())
}

// ─── Simple counting semaphore ───────────────────────────────────────────────

struct Semaphore {
    count: std::sync::Mutex<usize>,
    condvar: std::sync::Condvar,
    max: usize,
}

struct SemaphorePermit<'a>(&'a Semaphore);

impl Semaphore {
    fn new(max: usize) -> Self {
        Self { count: std::sync::Mutex::new(0), condvar: std::sync::Condvar::new(), max }
    }
    fn acquire(&self) -> SemaphorePermit<'_> {
        let mut count = self.count.lock().unwrap();
        while *count >= self.max {
            count = self.condvar.wait(count).unwrap();
        }
        *count += 1;
        SemaphorePermit(self)
    }
}

impl Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        let mut count = self.0.count.lock().unwrap();
        *count -= 1;
        self.0.condvar.notify_one();
    }
}

// ─── Cached fetch (shared cache with snapshot example) ───────────────────────

fn url_cache_path(url: &str, cache_dir: &str) -> std::path::PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    let hash = hasher.finish();
    let suffix: String = url.chars()
        .filter(|c| c.is_alphanumeric() || *c == '.')
        .take(40)
        .collect();
    std::path::PathBuf::from(cache_dir).join(format!("{hash:016x}_{suffix}"))
}

fn cached_fetch_bytes(url: &str, cache_dir: &str) -> Result<Vec<u8>, String> {
    let path = url_cache_path(url, cache_dir);
    if let Ok(data) = std::fs::read(&path) {
        eprintln!("[browser]   [cache] {url}");
        return Ok(data);
    }
    // Legacy fallback: old resolve_url produced broken URLs like "https://./assets/..."
    // Try looking up the old-style URL in cache to avoid re-fetching everything.
    if let Some(scheme_end) = url.find("://") {
        let after = &url[scheme_end + 3..];
        if let Some(slash) = after.find('/') {
            let old_url = format!("{}/.{}", &url[..scheme_end + 3], &after[slash..]);
            let old_path = url_cache_path(&old_url, cache_dir);
            if let Ok(data) = std::fs::read(&old_path) {
                eprintln!("[browser]   [cache-migrated] {url}");
                // Re-save under correct URL key
                let _ = std::fs::write(&path, &data);
                return Ok(data);
            }
        }
    }
    let data = fetch_bytes(url)?;
    let _ = std::fs::create_dir_all(cache_dir);
    let _ = std::fs::write(&path, &data);
    Ok(data)
}

fn cached_fetch_text(url: &str, cache_dir: &str) -> Result<String, String> {
    let bytes = cached_fetch_bytes(url, cache_dir)?;
    Ok(String::from_utf8(bytes.clone())
        .unwrap_or_else(|_| {
            let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(&bytes);
            cow.into_owned()
        }))
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn error_page(url: &str, err: &str) -> String {
    format!(r#"<!DOCTYPE html><html><head><style>
body{{font-family:-apple-system,sans-serif;background:#1a1b1e;color:#f2f3f5;
     display:flex;flex-direction:column;align-items:center;justify-content:center;
     min-height:100vh;margin:0;gap:12px}}
h2{{color:#ed4245;font-size:28px;font-weight:500}}
.url{{color:#5865f2;font-size:15px}}
.err{{color:#949ba4;font-size:13px;max-width:480px;text-align:center}}
</style></head><body>
<h2>Cannot load page</h2>
<div class="url">{}</div>
<div class="err">{}</div>
</body></html>"#, escape_html(url), escape_html(err))
}

// ─── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut initial_url = None;
    let mut cache_dir = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--cached" => {
                cache_dir = Some(String::from("snapshot_cache"));
            }
            "--cache-dir" => {
                i += 1;
                if i < args.len() { cache_dir = Some(args[i].clone()); }
            }
            other => {
                if initial_url.is_none() { initial_url = Some(other.to_string()); }
            }
        }
        i += 1;
    }
    let event_loop = EventLoop::<()>::with_user_event().build().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();
    let mut app = BrowserApp::new(proxy);
    app.initial_url = initial_url;
    app.cache_dir = cache_dir;
    event_loop.run_app(&mut app).unwrap();
}
