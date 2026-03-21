//! Phoenix Browser — a full browser demo built on the rhtmledit engine.
//! Features: tabs, back/forward history, URL bar, async page/CSS/image loading,
//! link navigation, per-element scrolling, and zoom.

use std::io::Read;
use std::sync::{mpsc, Arc};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::Window;

use tiny_skia::{Pixmap, PixmapPaint, Transform};

use rhtmledit::{hit_test_link, parse_html_with_hooks, Document, Renderer};
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
    Page  { tab_id: usize, url: String, doc: FreshDoc, css_sheets: Vec<String> },
    Image { tab_id: usize, src: String, rgba: Vec<u8>, w: u32, h: u32 },
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

    tx:    mpsc::Sender<LoadResult>,
    rx:    mpsc::Receiver<LoadResult>,
    proxy: EventLoopProxy<()>,
    initial_url: Option<String>,
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
            tx, rx, proxy, initial_url: None,
        }
    }

    fn content_h(&self) -> f32 { (self.height - CHROME_H).max(0.0) }

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
        std::thread::spawn(move || {
            let t0 = std::time::Instant::now();
            let html = if url == NEW_TAB_URL || url.starts_with("about:") {
                NEW_TAB_HTML.to_string()
            } else if let Some(path) = url.strip_prefix("file://") {
                std::fs::read_to_string(path)
                    .unwrap_or_else(|e| format!("<h2>File error</h2><p>{e}</p>"))
            } else {
                fetch_text(&url).unwrap_or_else(|e| error_page(&url, &e))
            };
            eprintln!("[browser] HTML fetch: {:.0}ms ({} bytes)", t0.elapsed().as_millis(), html.len());

            // CSS channel: receives sheets as they finish fetching.
            let (css_tx, css_rx) = std::sync::mpsc::channel::<(usize, String)>();
            let base = url.clone();
            let css_idx = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

            // parse_html_with_hooks fires our callback for every open tag.
            // When a <link rel="stylesheet"> is seen we immediately spawn a
            // fetch thread — it races against the rest of the HTML body parse,
            // so by the time parse returns, most CSS is already in-flight.
            let css_tx2 = css_tx.clone();
            let css_idx2 = css_idx.clone();
            let t1 = std::time::Instant::now();
            let doc = parse_html_with_hooks(&html, &url, move |tag, attrs| {
                if tag == "link"
                    && attrs.get("rel").map(|s| s == "stylesheet").unwrap_or(false)
                {
                    if let Some(href) = attrs.get("href") {
                        let abs = resolve_url(&base, href);
                        let sender = css_tx2.clone();
                        let idx = css_idx2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        eprintln!("[browser]   CSS fetch start: {abs}");
                        std::thread::spawn(move || {
                            let t = std::time::Instant::now();
                            let text = fetch_text(&abs).unwrap_or_default();
                            eprintln!("[browser]   CSS fetch done:  {abs} ({:.0}ms, {} bytes)", t.elapsed().as_millis(), text.len());
                            let _ = sender.send((idx, text));
                        });
                    }
                }
            });
            eprintln!("[browser] Parse: {:.0}ms", t1.elapsed().as_millis());

            // Collect in declaration order.
            drop(css_tx);
            let t2 = std::time::Instant::now();
            let mut css_results: Vec<(usize, String)> = css_rx.iter().collect();
            eprintln!("[browser] CSS wait: {:.0}ms ({} sheets)", t2.elapsed().as_millis(), css_results.len());
            css_results.sort_by_key(|(idx, _)| *idx);
            let css_sheets: Vec<String> = css_results.into_iter().map(|(_, s)| s).collect();

            let _ = tx.send(LoadResult::Page {
                tab_id, url, doc: FreshDoc(doc), css_sheets,
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
                    for css in &css_sheets {
                        if !css.is_empty() {
                            doc.stylesheet.parse_and_add(css);
                            had_css = true;
                        }
                    }
                    eprintln!("[browser] CSS parse: {:.0}ms ({} rules)", t_css.elapsed().as_millis(), doc.stylesheet.rules.len());
                    if had_css {
                        let t_casc = std::time::Instant::now();
                        let w = self.width;
                        let ch = self.content_h();
                        doc.stylesheet.rebuild_index();
                        apply_cascade_vp(&mut doc.root, &doc.stylesheet, None, 16.0, w, ch, std::ptr::null(), false);
                        eprintln!("[browser] Cascade: {:.0}ms", t_casc.elapsed().as_millis());
                    }

                    self.tabs[idx].title = if doc.title.is_empty() {
                        url.split('/').filter(|s| !s.is_empty()).last()
                            .unwrap_or("Untitled").to_string()
                    } else { doc.title.clone() };
                    self.tabs[idx].loading = false;

                    // Fetch images asynchronously (non-blocking, arrive later)
                    let mut img_srcs: Vec<String> = Vec::new();
                    Document::walk_all(&doc.root, &mut |b| {
                        if b.tag == "img" {
                            if let Some(src) = b.attributes.get("src") {
                                let abs = resolve_url(&url, src);
                                if !img_srcs.contains(&abs) { img_srcs.push(abs); }
                            }
                        }
                    });
                    for src in img_srcs {
                        let tx = self.tx.clone(); let proxy = self.proxy.clone();
                        let s2 = src.clone();
                        std::thread::spawn(move || {
                            if let Ok(bytes) = fetch_bytes(&s2) {
                                if let Ok(img) = image::load_from_memory(&bytes) {
                                    let rgba = img.to_rgba8();
                                    let (w, h) = (rgba.width(), rgba.height());
                                    let _ = tx.send(LoadResult::Image { tab_id, src, rgba: rgba.into_raw(), w, h });
                                    let _ = proxy.send_event(());
                                }
                            }
                        });
                    }

                    let t_layout = std::time::Instant::now();
                    self.layout_doc(&mut doc);
                    eprintln!("[browser] Layout: {:.0}ms", t_layout.elapsed().as_millis());
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
                                    b.layout_dirty = true;
                                }
                            }
                        }
                    });
                    if !tabs_need_relayout.contains(&idx) {
                        tabs_need_relayout.push(idx);
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
                let r = &b.border_rect;
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
            if let Some(doc) = active_doc {
                if let Some(mut pm) = Pixmap::new(w_px, content_px.max(1)) {
                    pm.fill(tiny_skia::Color::from_rgba8(255, 255, 255, 255));
                    let t_render = std::time::Instant::now();
                    renderer.render(doc, &mut pm, scale);
                    let render_ms = t_render.elapsed().as_millis();
                    if render_ms > 5 {
                        eprintln!("[browser] Render: {render_ms}ms ({}x{})", w_px, content_px);
                    }
                    pixmap.draw_pixmap(0, chrome_px as i32, pm.as_ref(),
                        &PixmapPaint::default(), Transform::identity(), None);
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
        eprintln!("[browser] process_results: {:.0}ms", t.elapsed().as_millis());
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
                if sy >= CHROME_H {
                    let csy = sy - CHROME_H;
                    let w = self.width; let ch = self.content_h();
                    if let Some(doc) = self.tabs[self.active].doc.as_mut() {
                        if doc.process_scrollbar_event(HtmlEventType::MouseMove,
                                sx, csy, w, ch) {
                            return true;
                        }
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
                        if let Some(doc) = self.tabs[self.active].doc.as_mut() {
                            let doc_pt = (sx + doc.scroll_x, csy + doc.scroll_y);
                            doc.process_mouse_event(HtmlEventType::MouseDown, doc_pt, bt);
                        }
                    }
                    return true;
                } else {
                    // Released
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
                    _ => {}
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
    let mut any_dirty = node.layout_dirty;
    for child in &mut node.children {
        if propagate_dirty(child) {
            any_dirty = true;
        }
    }
    if any_dirty {
        node.layout_dirty = true;
    }
    any_dirty
}

/// Resolve a (possibly relative) `href` against a `base` URL.
fn resolve_url(base: &str, href: &str) -> String {
    if href.is_empty() { return base.to_string(); }
    if href.starts_with("http://") || href.starts_with("https://") { return href.to_string(); }
    if href.starts_with("//") {
        let scheme = if base.starts_with("https") { "https:" } else { "http:" };
        return format!("{scheme}{href}");
    }
    // Find scheme + authority
    let origin = if let Some(p) = base.find("://") {
        let rest = &base[p+3..];
        let slash = rest.find('/').map(|i| p+3+i).unwrap_or(base.len());
        &base[..slash]
    } else { "" };
    if href.starts_with('/') {
        return format!("{origin}{href}");
    }
    // Relative to current directory
    let dir = if let Some(i) = base.rfind('/') {
        if &base[..i] == "https:" || &base[..i] == "http:" { &base } else { &base[..i+1] }
    } else { base };
    format!("{dir}{href}")
}

fn fetch_text(url: &str) -> Result<String, String> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .map_err(|e| e.to_string())?;
    resp.into_string().map_err(|e| e.to_string())
}

fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    resp.into_reader().read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
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
    let initial_url = std::env::args().nth(1);
    let event_loop = EventLoop::<()>::with_user_event().build().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();
    let mut app = BrowserApp::new(proxy);
    app.initial_url = initial_url;
    event_loop.run_app(&mut app).unwrap();
}
