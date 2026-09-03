//! Web Platform Tests — the `css-flexbox` suite, check-layout subset.
//!
//! These are the W3C's own tests, not fixtures written here, so they probe
//! behaviour nobody on this side thought to construct. A `check-layout` test
//! carries its expectations in the markup as `data-expected-width`,
//! `data-expected-height`, `data-offset-x` and `data-offset-y` — the values
//! WPT's `check-layout-th.js` would read from `offsetWidth`, `offsetHeight`,
//! `offsetLeft` and `offsetTop`. Nothing needs a script engine: the numbers are
//! attributes, and the DOM already answers all four.
//!
//! The corpus lives outside the crate (`data/wpt/css-flexbox`) and is not
//! vendored. When it is absent the test reports that and passes, so a checkout
//! without it still builds and runs green.

use std::path::PathBuf;

/// Where the corpus is expected, relative to this crate.
fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/wpt")
}

fn corpus_dir() -> PathBuf {
    corpus_root().join("css-flexbox")
}

/// Point WPT's root-relative URLs at the local corpus.
///
/// A test says `href="/fonts/ahem.css"`, meaning the WPT server's root. With a
/// `file://` base that resolves to the filesystem root and the font never
/// loads — and 62 of these 75 tests are written against Ahem, whose exact
/// metrics are the whole reason WPT uses it for baseline and sizing tests.
/// Without it they measure the fallback font and fail for a reason that has
/// nothing to do with layout.
fn resolve_wpt_roots(html: &str, root: &std::path::Path) -> String {
    let base = format!("file://{}", root.display());
    html.replace("href=\"/", &format!("href=\"{base}/"))
        .replace("src=\"/", &format!("src=\"{base}/"))
        .replace("href='/", &format!("href='{base}/"))
        .replace("src='/", &format!("src='{base}/"))
}

/// The viewport WPT assumes.
const VIEWPORT_W: f32 = 800.0;
const VIEWPORT_H: f32 = 600.0;

/// One element's expectations, as the harness would read them.
struct Expect {
    id: u32,
    width:  Option<f32>,
    height: Option<f32>,
    off_x:  Option<f32>,
    off_y:  Option<f32>,
}

fn num(doc: &crate::types::Document, id: u32, attr: &str) -> Option<f32> {
    doc.get_attribute(id, attr)?.trim().parse::<f32>().ok()
}

/// Run one test file. Returns (checks, failures).
fn run_one(path: &std::path::Path) -> (usize, Vec<String>) {
    let Ok(raw) = std::fs::read_to_string(path) else { return (0, Vec::new()) };
    let html = resolve_wpt_roots(&raw, &corpus_root());
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

    let base = format!("file://{}", path.parent().unwrap_or(std::path::Path::new("")).display());
    let mut r = crate::Renderer::new();
    let mut doc = r.load_html_with_base(&html, &base, VIEWPORT_W, VIEWPORT_H);

    // Every element carrying an expectation, in document order.
    let mut wanted: Vec<Expect> = Vec::new();
    for attr in ["[data-expected-width]", "[data-expected-height]",
                 "[data-offset-x]", "[data-offset-y]"] {
        for id in crate::dom::query_selector_all_ids(&doc.root, attr) {
            if wanted.iter().any(|e| e.id == id) { continue; }
            wanted.push(Expect {
                id,
                width:  num(&doc, id, "data-expected-width"),
                height: num(&doc, id, "data-expected-height"),
                off_x:  num(&doc, id, "data-offset-x"),
                off_y:  num(&doc, id, "data-offset-y"),
            });
        }
    }

    // A whole pixel: WPT's expectations are integers and the harness itself
    // compares rounded values.
    const TOL: f32 = 1.0;
    let mut checks = 0usize;
    let mut fails: Vec<String> = Vec::new();
    for e in &wanted {
        let mut check = |what: &str, got: f32, want: Option<f32>, out: &mut Vec<String>| {
            let Some(want) = want else { return };
            checks += 1;
            if (got - want).abs() > TOL {
                out.push(format!("{name}: {what} {} != {want}", got.round()));
            }
        };
        check("width",  doc.offset_width(e.id),  e.width,  &mut fails);
        check("height", doc.offset_height(e.id), e.height, &mut fails);
        let (ox, oy) = (doc.offset_left(e.id), doc.offset_top(e.id));
        check("offset-x", ox, e.off_x, &mut fails);
        check("offset-y", oy, e.off_y, &mut fails);
    }
    (checks, fails)
}

/// Report the suite's standing. This does NOT assert a pass rate — it records
/// one, so a change that moves it shows up as a number in the output rather
/// than as a red build nobody can act on. Tighten it to a ratchet once the
/// rate is known and stable.
#[test]
fn wpt_css_flexbox_check_layout() {
    let dir = corpus_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("WPT corpus absent at {} — skipping.", dir.display());
        return;
    };

    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "html").unwrap_or(false))
        .collect();
    files.sort();

    let mut tests = 0usize;
    let mut passed = 0usize;
    let mut total_checks = 0usize;
    // Per test: how many assertions failed, and the first one, which is
    // usually enough to say WHY without drowning the report.
    let mut failing: Vec<(String, usize, String)> = Vec::new();

    for path in &files {
        let Ok(src) = std::fs::read_to_string(path) else { continue };
        if !src.contains("data-expected-") && !src.contains("data-offset-") { continue }
        let (checks, fails) = run_one(path);
        if checks == 0 { continue }
        tests += 1;
        total_checks += checks;
        match fails.first() {
            None => passed += 1,
            Some(first) => {
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                failing.push((name, fails.len(), first.clone()));
            }
        }
    }

    let failed_checks: usize = failing.iter().map(|(_, n, _)| n).sum();
    let ok_checks = total_checks - failed_checks;
    eprintln!("\nWPT css-flexbox (check-layout): {passed}/{tests} tests, \
               {ok_checks}/{total_checks} assertions ({:.1}%)",
              100.0 * ok_checks as f64 / total_checks.max(1) as f64);
    if !failing.is_empty() {
        // Fewest failures first: the tests closest to passing are the ones
        // whose cause is easiest to isolate.
        failing.sort_by_key(|(_, n, _)| *n);
        eprintln!("failing ({}), nearest first:", failing.len());
        for (name, n, first) in &failing {
            eprintln!("  {n:>4} × {name}  — {}", first.split_once(": ").map(|(_, r)| r).unwrap_or(first));
        }
    }

    assert!(tests > 0, "corpus present but no check-layout tests found in {}", dir.display());

    // A ratchet, not a target. It only ever moves up: fixing a bug raises the
    // floor, and a regression that drops the count fails here with the tests
    // that broke listed above. Raise it whenever the number goes up.
    // Two ratchets, both floors that only move up. The assertion count is the
    // sensitive one: a test needs EVERY assertion to pass, so a real fix often
    // moves hundreds of assertions without flipping a single test.
    const TEST_FLOOR: usize = 11;
    const CHECK_FLOOR: usize = 2000;
    assert!(passed >= TEST_FLOOR,
            "WPT css-flexbox regressed: {passed}/{tests} tests pass, floor is {TEST_FLOOR}");
    assert!(ok_checks >= CHECK_FLOOR,
            "WPT css-flexbox regressed: {ok_checks}/{total_checks} assertions, floor is {CHECK_FLOOR}");
}
