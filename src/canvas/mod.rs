//! Canvas — generic immediate-mode drawing surface for `vybe_widgets`.
//!
//! This module is the **toolkit-level drawing primitive**. It is
//! deliberately VM-agnostic: a Rust user pulling in `vybe_widgets` as a
//! standalone toolkit can build a `Canvas` widget, paint on it via the
//! [`Canvas`] trait, and ship — with no `vybe_host`, `vybe_runtime`, or
//! .NET wrapper layer involved.
//!
//! ## Trait shape
//!
//! [`Canvas`] is HTML5-canvas-shaped. Every operation has a 1:1
//! counterpart in tiny-skia, Cairo, GDI, web canvas, Flutter canvas,
//! etc. — this is the primitive set every drawing backend converges on.
//! Higher-level framework façades that need to expose a different API
//! shape live OUTSIDE this crate; this trait stays the canonical
//! generic surface and never leaks framework-specific concepts (no
//! `Pen`, no `Brush`, no `Graphics`).
//!
//! ## Two impls
//!
//! - **[`RecordingCanvas`]** captures every call as a [`DrawCmd`]. The
//!   data IS the source of truth. Tests inspect it directly; the live
//!   render path replays it onto another canvas backend each frame.
//!
//! - **[`TinySkiaCanvas`]** paints onto a `tiny_skia::Pixmap`. Used by
//!   the form's render loop to turn a recording into pixels.
//!
//! Why two impls? Because the same canvas API has to serve two needs:
//! the live render must produce pixels, and tests must verify that the
//! right calls were made. Recording captures the calls as data,
//! tiny-skia turns them into pixels, and `RecordingCanvas::replay` is
//! the bridge between them.
//!
//! ## Standalone usage
//!
//! ```ignore
//! use vybe_widgets::canvas::{Canvas, TinySkiaCanvas, Color};
//! use tiny_skia::Pixmap;
//!
//! let mut pixmap = Pixmap::new(800, 600).unwrap();
//! pixmap.fill(tiny_skia::Color::WHITE);
//!
//! let mut canvas = TinySkiaCanvas::new(&mut pixmap);
//! canvas.set_fill_color(Color::rgb(255, 0, 0));
//! canvas.fill_rect(10.0, 10.0, 100.0, 100.0);
//!
//! canvas.set_stroke_color(Color::rgb(0, 0, 0));
//! canvas.set_line_width(3.0);
//! canvas.begin_path();
//! canvas.move_to(0.0, 0.0);
//! canvas.line_to(800.0, 600.0);
//! canvas.stroke();
//!
//! pixmap.save_png("out.png").unwrap();
//! ```

pub mod effects;
mod recording;
mod surface;
mod tinyskia;
mod types;

pub use effects::{apply_filter_list, apply_filter_op, blur_pixmap, shadow_layer};

pub use recording::{DrawCmd, RecordingCanvas};
pub use surface::CanvasSurfaces;
/// The drawing state HTML §4.12.5.1.2 defines — every attribute a page can set
/// and read back, in one place.
///
/// Exposed because BOTH `Canvas` implementations hold one, which is what lets
/// the attribute getters below be written once as defaults instead of twice as
/// overrides that could disagree.
pub use tinyskia::PaintState as DrawingState;
pub use tinyskia::{CanvasState, TinySkiaCanvas};

/// Parse a CSS `<color>` the way this engine parses one.
///
/// The entry point for anything outside the module that has canvas colour TEXT
/// and needs a colour — a gradient stop, most of all, since `addColorStop`
/// takes a CSS string. Routed here rather than parsed by the caller so there is
/// one colour grammar per engine and not one per call site.
pub fn parse_color_css(css: &str) -> Option<Color> {
    tinyskia::parse_canvas_color(css)
}
pub use types::{
    Color, ColorStop, CompositeOp, ContextAttributes, Direction, FillRule, Font, FontKerning,
    FontStretch, FontStyle, FontVariantCaps, FontWeight, Gradient, GradientKind, Image, ImageData,
    LineCap, LineJoin, Matrix, Paint, Path2D, PathOp, Pattern, Repetition, Shadow,
    SmoothingQuality, TextAlign, TextBaseline, TextMetrics, TextRendering,
};

/// Apply `setLineDash`'s own rules to a dash list.
///
/// Two of them, both from HTML §4.12.5, and both observable through
/// `getLineDash`:
///
/// 1. A list containing a negative or non-finite value is **rejected whole** —
///   the call does nothing and the previous dash pattern stays in force. That
///   is why this answers `None` rather than filtering the bad entries out:
///   dropping one entry silently re-pairs every dash with the wrong gap.
/// 2. An **odd-length** list is concatenated with itself, because a dash
///   pattern is read in on/off pairs and an odd list has no last gap.
///   `setLineDash([5])` therefore reads back as `[5, 5]`.
///
/// An empty list is valid and means a solid line.
pub fn normalize_dash(intervals: &[f32]) -> Option<Vec<f32>> {
    if intervals.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return None;
    }
    let mut dash = intervals.to_vec();
    if dash.len() % 2 == 1 {
        dash.extend_from_within(..);
    }
    Some(dash)
}

/// Canvas draw-routing tracing: `0` unread, `1` off, `2` on.
///
/// Seeded from `VYBE_DBG_CANVAS` on first read so the environment variable keeps
/// working, then settable at runtime by the debugger's `trace canvas on`. Every
/// trace site reads through [`trace_enabled`], so the switch is global.
static CANVAS_TRACE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// True when canvas draw-routing tracing is on. Zero-cost when off: one relaxed
/// atomic load, no allocation, no environment lookup after the first call.
pub fn trace_enabled() -> bool {
    use std::sync::atomic::Ordering;
    match CANVAS_TRACE.load(Ordering::Relaxed) {
        0 => {
            let on = std::env::var_os("VYBE_DBG_CANVAS").is_some();
            CANVAS_TRACE.store(if on { 2 } else { 1 }, Ordering::Relaxed);
            on
        }
        2 => true,
        _ => false,
    }
}

/// Turn canvas tracing on or off at runtime (the debugger's `trace canvas`).
pub fn set_trace_enabled(on: bool) {
    CANVAS_TRACE.store(if on { 2 } else { 1 }, std::sync::atomic::Ordering::Relaxed);
}

/// HTML5-canvas-shaped immediate-mode drawing API.
///
/// Implementations:
/// - [`RecordingCanvas`] — captures calls as [`DrawCmd`] data.
/// - [`TinySkiaCanvas`] — paints onto a `tiny_skia::Pixmap`.
///
/// All coordinates are in pixels. Angles are in radians (matches HTML5
/// canvas; .NET-shaped wrappers are responsible for degrees → radians
/// conversion).
///
/// The trait is `?Sized`-friendly via dyn dispatch — every helper that
/// accepts a canvas takes `&mut dyn Canvas` so framework wrappers don't
/// need to know which concrete impl is in play.
pub trait Canvas {
    // ─── Paint state ────────────────────────────────────────────────────

    /// Set the colour used by subsequent `fill*` operations.
    fn set_fill_color(&mut self, color: Color);

    /// Set the colour used by subsequent `stroke*` operations.
    fn set_stroke_color(&mut self, color: Color);

    /// Set the line width (in pixels) used by subsequent `stroke*`
    /// operations.
    fn set_line_width(&mut self, width: f32);

    /// Set how the ends of stroked lines are drawn.
    fn set_line_cap(&mut self, cap: LineCap);

    /// Set how stroked lines join at corners.
    fn set_line_join(&mut self, join: LineJoin);

    /// Set the miter limit for sharp `LineJoin::Miter` corners.
    fn set_miter_limit(&mut self, limit: f32);

    /// Set the global alpha multiplier (0.0 .. 1.0) applied to all
    /// subsequent paint operations.
    fn set_global_alpha(&mut self, alpha: f32);

    /// HTML5 canvas `imageSmoothingEnabled`. `true` (the default) filters
    /// scaled images bilinearly; `false` selects nearest-neighbour, which is
    /// what a software-rendered frame upscaled to the window needs — bilinear
    /// blurs it. Defaulted so existing `Canvas` impls need no change.
    fn set_image_smoothing(&mut self, _enabled: bool) {}

    /// Set the font used by `fill_text` / `stroke_text`.
    fn set_font(&mut self, font: &Font);

    /// Set the dash pattern used by subsequent `stroke*` operations.
    /// Each f32 is alternating on/off in pixel units, matching HTML5
    /// canvas's `setLineDash`. Empty slice (the default) means solid.
    fn set_line_dash(&mut self, intervals: &[f32]);

    /// Set the dash phase offset used by subsequent `stroke*` ops.
    /// Default is 0.
    fn set_line_dash_offset(&mut self, offset: f32);

    /// `fillStyle` — a colour, a gradient or a pattern.
    ///
    /// `set_fill_color` is the colour-only spelling and stays the common path;
    /// this is the full one the spec types as
    /// `DOMString | CanvasGradient | CanvasPattern`.
    ///
    /// Defaulted to the flat-colour fallback so an impl that predates gradients
    /// keeps compiling AND keeps painting: a gradient degrades to its first
    /// stop rather than to nothing. An impl that can build a shader overrides
    /// this; `TinySkiaCanvas` does.
    fn set_fill_paint(&mut self, paint: &Paint) {
        self.set_fill_color(paint.as_flat_color());
    }

    /// `strokeStyle` — the stroke half of [`Canvas::set_fill_paint`].
    fn set_stroke_paint(&mut self, paint: &Paint) {
        self.set_stroke_color(paint.as_flat_color());
    }

    /// `createLinearGradient(x0, y0, x1, y1)`.
    ///
    /// The four factories below are **context methods in the IDL**, not
    /// constructors — `new CanvasGradient()` does not exist, and a page can
    /// only obtain one from the context. They are provided here because the
    /// work is entirely in the value type; what the context contributes is
    /// being the only door to it.
    fn create_linear_gradient(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Gradient {
        Gradient::linear(x0, y0, x1, y1)
    }

    /// `createRadialGradient(x0, y0, r0, x1, y1, r1)` — the cone between two
    /// circles, which is why it takes two centres and not one.
    fn create_radial_gradient(
        &self,
        x0: f32,
        y0: f32,
        r0: f32,
        x1: f32,
        y1: f32,
        r1: f32,
    ) -> Gradient {
        Gradient::radial(x0, y0, r0, x1, y1, r1)
    }

    /// `createConicGradient(startAngle, x, y)`. Note the spec's argument order:
    /// the ANGLE comes first, ahead of the centre.
    fn create_conic_gradient(&self, start_angle: f32, x: f32, y: f32) -> Gradient {
        Gradient::conic(start_angle, x, y)
    }

    /// `createPattern(image, repetition)`.
    ///
    /// `None` for a repetition keyword that is not one of the four the spec
    /// lists — which the spec makes a `SyntaxError` rather than a silent
    /// fallback to `repeat`, because the difference between tiling and not
    /// tiling is the whole of what the argument says.
    fn create_pattern(&self, image: &Image, repetition: &str) -> Option<Pattern> {
        let repetition = Repetition::parse(repetition)?;
        Some(Pattern {
            image: image.clone(),
            repetition,
        })
    }

    /// `shadowColor` / `shadowBlur` / `shadowOffsetX` / `shadowOffsetY`.
    ///
    /// One setter rather than four, because the four are meaningless apart:
    /// the spec draws a shadow only when the colour is non-transparent AND at
    /// least one of blur/offset is non-zero, so a backend has to read all four
    /// to answer "is there a shadow". Splitting them into four trait methods
    /// would make every impl reassemble the same tuple.
    ///
    /// Defaulted to a no-op: a backend without shadow support paints the shape
    /// unshadowed, which is a visible, honest degradation.
    fn set_shadow(&mut self, _shadow: &Shadow) {}

    // ─── Path building ──────────────────────────────────────────────────

    /// Reset the current path. Subsequent `move_to`, `line_to`, `arc`,
    /// etc. build a new path; `fill` / `stroke` paint it.
    fn begin_path(&mut self);

    /// Close the current sub-path by drawing a line back to its start.
    fn close_path(&mut self);

    /// Move the current point without drawing.
    fn move_to(&mut self, x: f32, y: f32);

    /// Draw a straight line from the current point to `(x, y)`.
    fn line_to(&mut self, x: f32, y: f32);

    /// Add a quadratic Bézier curve from the current point through
    /// `(cx, cy)` to `(x, y)`.
    fn quadratic_curve_to(&mut self, cx: f32, cy: f32, x: f32, y: f32);

    /// Add a cubic Bézier curve from the current point through
    /// `(cx1, cy1)` and `(cx2, cy2)` to `(x, y)`.
    fn bezier_curve_to(&mut self, cx1: f32, cy1: f32, cx2: f32, cy2: f32, x: f32, y: f32);

    /// Add an arc centred at `(x, y)` with radius `r`, sweeping from
    /// `start` to `end` radians. `ccw = true` reverses the sweep.
    fn arc(&mut self, x: f32, y: f32, r: f32, start: f32, end: f32, ccw: bool);

    /// Add a rectangle to the current path.
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32);

    /// Add a full ellipse with centre `(x, y)` and radii `(rx, ry)` to the
    /// current path.
    ///
    /// This is the closed-ellipse shorthand. The spec's `ellipse()` also takes
    /// a rotation and a start/end angle pair — see
    /// [`Canvas::ellipse_arc`], which this delegates to.
    fn ellipse(&mut self, x: f32, y: f32, rx: f32, ry: f32);

    /// `ellipse(x, y, radiusX, radiusY, rotation, startAngle, endAngle, ccw)`.
    ///
    /// **Provided, not required.** An ellipse arc is the unit circle arc under
    /// a scale-then-rotate transform, so it is expressible with `arc` and the
    /// transform stack every canvas already has — which is the spec's own
    /// definition, not an approximation of it.
    ///
    /// The transform is saved and restored around the arc so the path is the
    /// only thing that changes. Note the arc is appended to the CURRENT path:
    /// no `begin_path` here, because `ellipse` is a path-building method and
    /// the caller may be composing a compound path.
    ///
    /// Without this, `ellipse` could only draw a full, axis-aligned ellipse —
    /// a rotated one, or an elliptical wedge, was unexpressible.
    fn ellipse_arc(
        &mut self,
        x: f32,
        y: f32,
        rx: f32,
        ry: f32,
        rotation: f32,
        start: f32,
        end: f32,
        ccw: bool,
    ) {
        // A degenerate radius collapses the ellipse to a point or a segment;
        // the scale below would be singular, so take the closed-form shorthand
        // out and leave the path untouched rather than emit a NaN transform.
        if rx <= 0.0 || ry <= 0.0 {
            return;
        }
        self.save();
        self.translate(x, y);
        self.rotate(rotation);
        self.scale(rx, ry);
        // Radius 1 in the scaled space IS `rx`/`ry` in user space. The centre
        // is the origin because `translate` already moved it there.
        self.arc(0.0, 0.0, 1.0, start, end, ccw);
        self.restore();
    }

    /// `textAlign` — which end of the text the `x` names. Default `start`.
    fn set_text_align(&mut self, _align: TextAlign) {}

    /// `textBaseline` — which line of the text the `y` names. Default
    /// `alphabetic`, so `y` is the BASELINE and the glyphs sit above it.
    fn set_text_baseline(&mut self, _baseline: TextBaseline) {}

    /// `arcTo(x1, y1, x2, y2, radius)` — the arc that fillets the corner
    /// between the current point, `(x1, y1)` and `(x2, y2)`.
    ///
    /// **Provided, not required**: the spec defines it in terms of a line and
    /// an arc, so composing those is the definition rather than an
    /// approximation of it, and every canvas gets it without an implementation.
    ///
    /// The degenerate cases are the spec's own and are not merely guards — a
    /// zero radius, or three collinear points, is defined to add a straight
    /// line to `(x1, y1)` and stop. Rounding a rectangle whose corner radius is
    /// zero goes through here on every corner.
    fn arc_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, radius: f32) {
        let (x0, y0) = match self.current_point() {
            Some(point) => point,
            // With no subpath the spec says start one at (x1, y1).
            None => {
                self.move_to(x1, y1);
                return;
            }
        };
        let (a1, b1) = (x0 - x1, y0 - y1);
        let (a2, b2) = (x2 - x1, y2 - y1);
        let len1 = (a1 * a1 + b1 * b1).sqrt();
        let len2 = (a2 * a2 + b2 * b2).sqrt();
        // Collinear or coincident, or no radius: a straight line, per spec.
        let cross = a1 * b2 - b1 * a2;
        if radius <= 0.0 || len1 == 0.0 || len2 == 0.0 || cross.abs() < f32::EPSILON {
            self.line_to(x1, y1);
            return;
        }
        let (u1, v1) = (a1 / len1, b1 / len1);
        let (u2, v2) = (a2 / len2, b2 / len2);
        // Half the angle between the two legs decides how far back along each
        // the tangent points sit.
        let cos_half = ((1.0 + (u1 * u2 + v1 * v2)) / 2.0).max(0.0).sqrt();
        let sin_half = ((1.0 - (u1 * u2 + v1 * v2)) / 2.0).max(0.0).sqrt();
        if sin_half <= f32::EPSILON {
            self.line_to(x1, y1);
            return;
        }
        let tangent = radius * cos_half / sin_half;
        let (t1x, t1y) = (x1 + u1 * tangent, y1 + v1 * tangent);
        let (t2x, t2y) = (x1 + u2 * tangent, y1 + v2 * tangent);
        // The centre sits on the bisector, one radius from each leg.
        let (bx, by) = (u1 + u2, v1 + v2);
        let blen = (bx * bx + by * by).sqrt();
        if blen <= f32::EPSILON {
            self.line_to(x1, y1);
            return;
        }
        let centre_distance = radius / sin_half;
        let (cx, cy) = (
            x1 + bx / blen * centre_distance,
            y1 + by / blen * centre_distance,
        );
        let start = (t1y - cy).atan2(t1x - cx);
        let end = (t2y - cy).atan2(t2x - cx);
        self.line_to(t1x, t1y);
        // The sweep follows the turn direction of the two legs.
        self.arc(cx, cy, radius, start, end, cross > 0.0);
    }

    /// `roundRect(x, y, w, h, radius)` — a rectangle with rounded corners.
    ///
    /// Provided for the same reason as [`Canvas::arc_to`]: the spec builds it
    /// out of lines and arcs, so composing them IS the implementation.
    ///
    /// A radius larger than half the shorter side is scaled down rather than
    /// allowed to overlap, which is what the spec requires and what stops a
    /// large radius from turning the shape inside out.
    fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32) {
        self.round_rect_radii(x, y, w, h, [radius; 4]);
    }

    /// `roundRect(x, y, w, h, radii)` — the general form, with a radius per
    /// corner in the spec's order: **top-left, top-right, bottom-right,
    /// bottom-left**.
    ///
    /// The one-radius [`Canvas::round_rect`] is this with the same value four
    /// times. Four is the general case rather than a convenience: a tab, a
    /// grouped button, a speech bubble all round some corners and not others,
    /// and none of them are expressible with a single radius.
    ///
    /// When adjacent radii would overlap along an edge, ALL FOUR are scaled by
    /// the same factor — the spec's rule, and the reason this cannot clamp each
    /// corner independently: clamping separately would change the shape's
    /// proportions, while scaling together preserves them.
    fn round_rect_radii(&mut self, x: f32, y: f32, w: f32, h: f32, radii: [f32; 4]) {
        if w == 0.0 || h == 0.0 {
            return;
        }
        // Negative width or height is legal and means the rectangle extends the
        // other way; normalising keeps the corner maths in one direction.
        let (x, w) = if w < 0.0 { (x + w, -w) } else { (x, w) };
        let (y, h) = if h < 0.0 { (y + h, -h) } else { (y, h) };
        let [mut tl, mut tr, mut br, mut bl] = radii.map(|r| r.max(0.0));
        // Each edge can hold the two radii that meet on it. The tightest of the
        // four ratios is the factor every radius shrinks by.
        let scale = [w / (tl + tr), w / (bl + br), h / (tl + bl), h / (tr + br)]
            .into_iter()
            .filter(|s| s.is_finite())
            .fold(1.0f32, f32::min);
        if scale < 1.0 {
            tl *= scale;
            tr *= scale;
            br *= scale;
            bl *= scale;
        }
        self.move_to(x + tl, y);
        self.line_to(x + w - tr, y);
        self.arc_to(x + w, y, x + w, y + tr, tr);
        self.line_to(x + w, y + h - br);
        self.arc_to(x + w, y + h, x + w - br, y + h, br);
        self.line_to(x + bl, y + h);
        self.arc_to(x, y + h, x, y + h - bl, bl);
        self.line_to(x, y + tl);
        self.arc_to(x, y, x + tl, y, tl);
        self.close_path();
    }

    /// The current point of the path being built, if there is one.
    ///
    /// `arcTo` needs it — the arc is defined relative to where the path
    /// already is — and it is the one piece of path state a composed default
    /// cannot derive for itself. A canvas that does not track it answers
    /// `None`, and `arcTo` then starts a subpath, which is what the spec says
    /// to do in that case anyway.
    fn current_point(&self) -> Option<(f32, f32)> {
        None
    }

    // ─── Drawing ────────────────────────────────────────────────────────

    /// Fill the current path with the current fill colour.
    fn fill(&mut self);

    /// `fill(fillRule)` — fill the current path under an explicit winding rule.
    ///
    /// **Provided**, defaulting to [`Canvas::fill`] so `nonzero` (the spec's
    /// default) costs no impl change. A backend that can select the rule
    /// overrides this; one that cannot fills `evenodd` as `nonzero`, which
    /// differs only for self-intersecting paths and is visible rather than
    /// silent.
    fn fill_with_rule(&mut self, _rule: FillRule) {
        self.fill();
    }

    /// Stroke the current path with the current stroke colour and line
    /// width.
    fn stroke(&mut self);

    /// Fill a rectangle directly (does not modify the current path).
    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32);

    /// Stroke a rectangle directly (does not modify the current path).
    fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32);

    /// Clear a rectangle to fully transparent.
    fn clear_rect(&mut self, x: f32, y: f32, w: f32, h: f32);

    /// Fill `text` at baseline `(x, y)` with the current fill colour
    /// and font.
    fn fill_text(&mut self, text: &str, x: f32, y: f32);

    /// Stroke `text` at baseline `(x, y)` with the current stroke
    /// colour and font.
    fn stroke_text(&mut self, text: &str, x: f32, y: f32);

    /// Draw an image scaled into the rectangle `(x, y, w, h)`.
    fn draw_image(&mut self, img: &Image, x: f32, y: f32, w: f32, h: f32);

    /// `drawImage(image, sx, sy, sw, sh, dx, dy, dw, dh)` — the nine-argument
    /// form, which draws a SOURCE RECTANGLE of the image.
    ///
    /// This is what a sprite sheet needs, and it is not reducible to the
    /// four-argument form: that one always takes the whole image.
    ///
    /// **Provided**, by cropping the source rectangle into a new [`Image`] and
    /// drawing that. Copying is honest here rather than clever — the crop is
    /// the operation, and a backend that can blit a sub-rectangle directly
    /// should override this.
    ///
    /// A source rectangle outside the image is clamped, and an empty one draws
    /// nothing, which is the spec's own handling.
    fn draw_image_rect(
        &mut self,
        img: &Image,
        sx: f32,
        sy: f32,
        sw: f32,
        sh: f32,
        dx: f32,
        dy: f32,
        dw: f32,
        dh: f32,
    ) {
        let Some(cropped) = img.crop(sx, sy, sw, sh) else {
            return;
        };
        self.draw_image(&cropped, dx, dy, dw, dh);
    }

    /// `putImageData(imagedata, dx, dy)` — write pixels **directly** into the
    /// bitmap at `(dx, dy)`, one for one.
    ///
    /// Not a variant of [`Self::draw_image`], and the difference is the whole
    /// point of the operation: HTML §4.12.5 says `putImageData` is **not**
    /// affected by the current transform, the clipping region, `globalAlpha`
    /// or the compositing mode. It is a raw write, and that is why a software
    /// renderer uses it — the frame it computed is the frame that appears.
    ///
    /// It also does not scale: there is no `dw`/`dh`. Scaling raw pixels is
    /// done by putting them into a canvas and drawing THAT canvas, or by
    /// giving the `<canvas>` a CSS box larger than its bitmap. Both are the
    /// browser's answer; neither is a parameter here.
    fn put_image_data(&mut self, img: &Image, dx: f32, dy: f32);

    // ─── Clipping ───────────────────────────────────────────────────────

    /// Use the current path as the clip region for subsequent draw
    /// operations. The path is consumed (matches HTML5 `clip()`).
    /// `save` / `restore` push/pop the clip along with the rest of the
    /// paint state.
    fn clip(&mut self);

    /// `clip(fillRule)` — the clip half of [`Canvas::fill_with_rule`].
    fn clip_with_rule(&mut self, _rule: FillRule) {
        self.clip();
    }

    /// Reset the clip to the entire canvas.
    fn reset_clip(&mut self);

    // ─── State stack ────────────────────────────────────────────────────

    /// Push the current paint state (colours, line width, transform,
    /// etc.) onto an internal stack.
    fn save(&mut self);

    /// Pop the most recently saved paint state.
    fn restore(&mut self);

    // ─── Transforms ─────────────────────────────────────────────────────

    /// Translate the current transform by `(x, y)`.
    fn translate(&mut self, x: f32, y: f32);

    /// Rotate the current transform by `rad` radians.
    fn rotate(&mut self, rad: f32);

    /// Scale the current transform by `(sx, sy)`.
    fn scale(&mut self, sx: f32, sy: f32);

    /// Multiply the current transform by an arbitrary affine matrix.
    fn transform(&mut self, m11: f32, m12: f32, m21: f32, m22: f32, dx: f32, dy: f32);

    /// Reset the current transform to the identity.
    fn reset_transform(&mut self);

    /// `getTransform()` — the current transformation matrix.
    ///
    /// The read half of `setTransform`, and the only way a caller can compose
    /// with a transform it did not set. Defaulted to the identity for a canvas
    /// that does not track one; an impl with a matrix returns it.
    fn get_transform(&self) -> Matrix {
        Matrix::IDENTITY
    }

    /// `setTransform(a, b, c, d, e, f)` — REPLACE the current matrix.
    ///
    /// Provided, because it is exactly `resetTransform` followed by
    /// `transform` — replacing versus composing is the whole difference
    /// between the two, and stating it here means no impl can get it backwards.
    fn set_transform(&mut self, m: Matrix) {
        self.reset_transform();
        self.transform(m.a, m.b, m.c, m.d, m.e, m.f);
    }

    // ─── Context state ──────────────────────────────────────────────────

    /// `reset()` — return the context to its default state.
    ///
    /// Three things, per HTML §4.12.5: the drawing state stack is emptied, the
    /// state is reset to defaults, and **the bitmap is cleared to transparent
    /// black**. The last one is the part that is easy to leave out and the
    /// reason `reset()` is not just a loop of setters.
    fn reset(&mut self);

    /// `isContextLost()` — whether the backing bitmap has been lost.
    ///
    /// A software canvas cannot lose its context: the pixels are ordinary
    /// memory, with no GPU device to be reset out from under them. `false` is
    /// the true answer here rather than a stub, and an impl that acquires a
    /// device is the one that should override it.
    fn is_context_lost(&self) -> bool {
        false
    }

    /// `getContextAttributes()` — the settings this context was created with.
    fn context_attributes(&self) -> ContextAttributes {
        ContextAttributes::default()
    }

    /// `globalCompositeOperation` — how new drawing combines with existing
    /// pixels.
    ///
    /// Defaulted to a no-op, which leaves `source-over` in force. That is a
    /// visible degradation rather than a silent one: the shape still paints,
    /// it simply paints over instead of (say) knocking out.
    fn set_global_composite_operation(&mut self, _op: CompositeOp) {}

    /// `imageSmoothingQuality`. Only consulted when smoothing is enabled.
    fn set_image_smoothing_quality(&mut self, _quality: SmoothingQuality) {}

    /// `filter` — a CSS filter function list, or `"none"`.
    ///
    /// Carried as the source string: the value is a CSS `<filter-value-list>`,
    /// the engine already has a parser for it, and re-spelling it as an enum
    /// here would be a second grammar to keep in step with the first.
    fn set_filter(&mut self, _filter: &str) {}

    /// `getLineDash()` — the dash pattern currently in effect.
    ///
    /// Empty means a solid line. The spec returns the list as set, except that
    /// an odd-length list is doubled — `setLineDash([5])` reads back `[5, 5]` —
    /// so this is not simply the argument handed back.
    fn get_line_dash(&self) -> Vec<f32> {
        Vec::new()
    }

    // ─── CanvasTextDrawingStyles ────────────────────────────────────────

    /// `direction` — which way the text runs, and therefore what `textAlign`'s
    /// `start` and `end` resolve to.
    fn set_direction(&mut self, _direction: Direction) {}

    /// `lang` — the language the text is in, which shapes it: the same
    /// codepoints take different glyphs in Chinese and Japanese.
    fn set_lang(&mut self, _lang: &str) {}

    /// `letterSpacing` — extra space between characters, as a CSS length.
    fn set_letter_spacing(&mut self, _spacing: &str) {}

    /// `wordSpacing` — extra space at each word separator, as a CSS length.
    fn set_word_spacing(&mut self, _spacing: &str) {}

    /// `fontKerning`.
    fn set_font_kerning(&mut self, _kerning: FontKerning) {}

    /// `fontStretch`.
    fn set_font_stretch(&mut self, _stretch: FontStretch) {}

    /// `fontVariantCaps`.
    fn set_font_variant_caps(&mut self, _caps: FontVariantCaps) {}

    /// `textRendering`.
    fn set_text_rendering(&mut self, _rendering: TextRendering) {}

    // ─── Text measurement ───────────────────────────────────────────────

    /// `measureText(text)` — the full [`TextMetrics`], not just an advance.
    ///
    /// `&mut self` because shaping needs the font system, which the impl holds
    /// mutably; measuring does not change what is painted.
    ///
    /// Defaulted to all-zero metrics for a canvas with no text engine. Zero is
    /// the honest answer where nothing can be measured — every other number
    /// would be an invented one that a caller would lay out against.
    fn measure_text(&mut self, _text: &str) -> TextMetrics {
        TextMetrics::default()
    }

    /// `fillText(text, x, y, maxWidth)` — the four-argument form.
    ///
    /// **Provided**, because the spec defines a constraint rather than a new
    /// drawing operation: text wider than `max_width` must be made to fit by
    /// EITHER a more condensed face or a smaller font, whichever the user agent
    /// can do. Scaling about the anchor is the second of those — a backend that
    /// shapes at the transformed size (`TinySkiaCanvas` does) draws the run
    /// proportionally smaller, not horizontally squashed. Both satisfy the
    /// spec; this says which one happens rather than claiming the other.
    ///
    /// The anchor is `(x, y)`, and scaling about it is correct under every
    /// `textAlign`: the alignment offset is applied by `fill_text` relative to
    /// the origin it is passed, which is the anchor after the translate — so
    /// centred text stays centred on `x` and right-aligned text stays flush to
    /// it.
    ///
    /// A text run that already fits is drawn untouched, so this costs one
    /// measurement and nothing else in the common case.
    fn fill_text_constrained(&mut self, text: &str, x: f32, y: f32, max_width: f32) {
        match self.condense_factor(text, max_width) {
            None => self.fill_text(text, x, y),
            Some(factor) => {
                self.save();
                self.translate(x, y);
                self.scale(factor, 1.0);
                self.fill_text(text, 0.0, 0.0);
                self.restore();
            }
        }
    }

    /// `strokeText(text, x, y, maxWidth)` — the stroke half of
    /// [`Canvas::fill_text_constrained`].
    fn stroke_text_constrained(&mut self, text: &str, x: f32, y: f32, max_width: f32) {
        match self.condense_factor(text, max_width) {
            None => self.stroke_text(text, x, y),
            Some(factor) => {
                self.save();
                self.translate(x, y);
                self.scale(factor, 1.0);
                self.stroke_text(text, 0.0, 0.0);
                self.restore();
            }
        }
    }

    /// How much to squeeze `text` to fit `max_width`, or `None` when it already
    /// fits and must be drawn untouched.
    ///
    /// Shared by the two constrained text methods so the rule lives once. A
    /// non-positive `max_width` is the spec's "do nothing" case and reports as
    /// already fitting; a canvas that cannot measure reports width `0`, which
    /// also fits, so no text engine means no accidental squeeze.
    fn condense_factor(&mut self, text: &str, max_width: f32) -> Option<f32> {
        if max_width <= 0.0 {
            return None;
        }
        let width = self.measure_text(text).width;
        if width <= max_width || width <= 0.0 {
            return None;
        }
        Some(max_width / width)
    }

    // ─── Path2D ─────────────────────────────────────────────────────────

    /// Append a [`Path2D`] to the current path.
    ///
    /// **Provided**, and it is the whole of `Path2D` support: a recorded path
    /// is a list of the same calls the trait already takes, so replaying it is
    /// the definition. Every `fill(path)` / `stroke(path)` / `clip(path)`
    /// overload below is this plus the operation it names.
    ///
    /// No `begin_path` — this APPENDS, so a caller can compose a recorded path
    /// with one it is building by hand.
    fn append_path(&mut self, path: &Path2D) {
        for op in &path.ops {
            match *op {
                PathOp::ClosePath => self.close_path(),
                PathOp::MoveTo(x, y) => self.move_to(x, y),
                PathOp::LineTo(x, y) => self.line_to(x, y),
                PathOp::QuadraticCurveTo { cx, cy, x, y } => self.quadratic_curve_to(cx, cy, x, y),
                PathOp::BezierCurveTo {
                    cx1,
                    cy1,
                    cx2,
                    cy2,
                    x,
                    y,
                } => self.bezier_curve_to(cx1, cy1, cx2, cy2, x, y),
                PathOp::ArcTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    radius,
                } => self.arc_to(x1, y1, x2, y2, radius),
                PathOp::Rect { x, y, w, h } => self.rect(x, y, w, h),
                PathOp::RoundRect { x, y, w, h, radii } => self.round_rect_radii(x, y, w, h, radii),
                PathOp::Arc {
                    x,
                    y,
                    r,
                    start,
                    end,
                    ccw,
                } => self.arc(x, y, r, start, end, ccw),
                PathOp::Ellipse {
                    x,
                    y,
                    rx,
                    ry,
                    rotation,
                    start,
                    end,
                    ccw,
                } => self.ellipse_arc(x, y, rx, ry, rotation, start, end, ccw),
            }
        }
    }

    /// `fill(path, fillRule)`.
    ///
    /// The recorded path replaces the current one for the duration, which is
    /// what the overload means: filling a `Path2D` must not disturb whatever
    /// the context was building. `begin_path` first, so nothing already in the
    /// current path is filled along with it.
    fn fill_path(&mut self, path: &Path2D, rule: FillRule) {
        self.begin_path();
        self.append_path(path);
        self.fill_with_rule(rule);
    }

    /// `stroke(path)`.
    fn stroke_path(&mut self, path: &Path2D) {
        self.begin_path();
        self.append_path(path);
        self.stroke();
    }

    /// `clip(path, fillRule)`.
    fn clip_path(&mut self, path: &Path2D, rule: FillRule) {
        self.begin_path();
        self.append_path(path);
        self.clip_with_rule(rule);
    }

    // ─── Hit testing ────────────────────────────────────────────────────

    /// `isPointInPath(x, y, fillRule)` — is `(x, y)` inside the current path?
    ///
    /// The point is in the canvas's own coordinate space, NOT the transformed
    /// space: the spec transforms it by the inverse of the current matrix
    /// before testing, so a hit test keeps working after the caller has scaled
    /// or rotated.
    ///
    /// Defaulted to `false` for a canvas that keeps no geometry — the same
    /// answer it would give for a point outside, which is the safe direction:
    /// a UI built on it responds to nothing rather than to everything.
    fn is_point_in_path(&self, _x: f32, _y: f32, _rule: FillRule) -> bool {
        false
    }

    /// `isPointInStroke(x, y)` — is `(x, y)` on the STROKE of the current path?
    ///
    /// A different question from [`Canvas::is_point_in_path`] and not derivable
    /// from it: an unclosed path has a stroke and no interior, and a thick
    /// stroke on a closed path extends outside the fill.
    fn is_point_in_stroke(&self, _x: f32, _y: f32) -> bool {
        false
    }

    /// `isPointInPath(path, x, y, fillRule)` — the `Path2D` overload.
    fn is_point_in_path2d(&self, _path: &Path2D, _x: f32, _y: f32, _rule: FillRule) -> bool {
        false
    }

    /// `isPointInStroke(path, x, y)` — the `Path2D` overload.
    fn is_point_in_stroke2d(&self, _path: &Path2D, _x: f32, _y: f32) -> bool {
        false
    }

    /// `drawFocusIfNeeded(element)` — draw a focus ring around the current path
    /// when the element the canvas is standing in for has focus.
    ///
    /// The element is the CALLER's to resolve: a canvas has no DOM, and the
    /// question it can answer is "draw the ring or not". Whoever holds the
    /// element passes the answer in.
    ///
    /// Defaulted to a no-op, which is also what a browser does when the element
    /// is not focused — the degradation is that a focused control does not show
    /// its ring, not that something wrong is drawn.
    fn draw_focus_if_needed(&mut self, _focused: bool) {}

    // ─── Pixel access ───────────────────────────────────────────────────

    /// `HTMLCanvasElement.toBlob(callback, type, quality)` — the canvas as an
    /// encoded image file.
    ///
    /// On the canvas rather than the element because the element has no pixels;
    /// it delegates here, the same way `getContext` does. Synchronous for the
    /// same reason [`Canvas::get_image_data`] is: the callback in the IDL is
    /// about not blocking the event loop, which is the caller's concern and not
    /// the rasteriser's.
    ///
    /// `mime` is `image/png` or `image/jpeg`; anything else answers `None`
    /// rather than quietly returning a PNG under the wrong name — a caller that
    /// asked for WebP and got PNG bytes labelled WebP has a corrupt file and no
    /// way to know. `quality` is only read for JPEG, per spec.
    ///
    /// `None` from a canvas with no bitmap, which is a recording.
    fn to_blob(&self, _mime: &str, _quality: Option<f32>) -> Option<Vec<u8>> {
        None
    }

    /// `HTMLCanvasElement.toDataURL(type, quality)`.
    ///
    /// **Provided**, because it is [`Canvas::to_blob`] plus base64 and a
    /// prefix — the spec defines it as exactly that, so a backend that can
    /// encode gets this for free and cannot implement the two inconsistently.
    ///
    /// The spec's fallback for a canvas with no pixels is the string
    /// `"data:,"`, which is a valid empty data URL. That is returned rather
    /// than `None` so the attribute always answers something a page can put in
    /// an `src`, which is what makes it safe to call unconditionally.
    fn to_data_url(&self, mime: &str, quality: Option<f32>) -> String {
        match self.to_blob(mime, quality) {
            Some(bytes) => {
                use base64::Engine as _;
                let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                format!("data:{mime};base64,{encoded}")
            }
            None => "data:,".to_string(),
        }
    }

    /// `createImageData(sw, sh)` — a transparent-black buffer of that size.
    ///
    /// Provided: the spec allocates rather than reading the canvas, so this
    /// needs nothing from the backend.
    fn create_image_data(&self, width: u32, height: u32) -> ImageData {
        ImageData::new(width, height)
    }

    /// `getImageData(sx, sy, sw, sh)` — read pixels back out of the bitmap.
    ///
    /// **The only operation here that a backend can genuinely fail to answer.**
    /// A recording canvas has no pixels — it has the calls that would have made
    /// them — so it returns `None` rather than a plausible block of zeroes that
    /// a caller would treat as a blank canvas.
    ///
    /// Like `putImageData`, this ignores the transform, the clip and
    /// `globalAlpha`: it reads the bitmap, not the drawing.
    fn get_image_data(&self, _sx: i32, _sy: i32, _sw: u32, _sh: u32) -> Option<ImageData> {
        None
    }

    /// `putImageData(imagedata, dx, dy)` in the spec's own type.
    ///
    /// [`Canvas::put_image_data`] takes an [`Image`], which is what the drawing
    /// side of the toolkit passes. This is the `ImageData` spelling, and the
    /// conversion is the alpha: `ImageData` is straight RGBA, so it goes to
    /// `Image::from_rgba` and nowhere else.
    fn put_image_data_spec(&mut self, data: &ImageData, dx: f32, dy: f32) {
        let img = Image::from_rgba(data.width, data.height, data.data.clone());
        self.put_image_data(&img, dx, dy);
    }

    /// `putImageData(imagedata, dx, dy, dirtyX, dirtyY, dirtyWidth,
    /// dirtyHeight)` — write only part of the buffer.
    ///
    /// Provided by cropping to the dirty rectangle and writing that. The dirty
    /// rectangle is in the IMAGE's coordinates and is clipped to it; an empty
    /// one writes nothing, per spec.
    #[allow(clippy::too_many_arguments)]
    fn put_image_data_dirty(
        &mut self,
        data: &ImageData,
        dx: f32,
        dy: f32,
        dirty_x: i32,
        dirty_y: i32,
        dirty_width: i32,
        dirty_height: i32,
    ) {
        // **A negative extent is legal and means the rectangle runs the other
        // way** — the spec normalises it before doing anything else, and these
        // are `long` in the IDL for exactly that reason. Taking them unsigned
        // made `dirtyWidth: -10` unrepresentable; leaving them signed and NOT
        // normalising would make `x1 < x0` and write nothing at all, silently.
        let (dirty_x, dirty_width) = if dirty_width < 0 {
            (dirty_x + dirty_width, -dirty_width)
        } else {
            (dirty_x, dirty_width)
        };
        let (dirty_y, dirty_height) = if dirty_height < 0 {
            (dirty_y + dirty_height, -dirty_height)
        } else {
            (dirty_y, dirty_height)
        };
        // Clip the dirty rect to the buffer. A negative origin moves the
        // destination by the same amount, because the spec writes the
        // INTERSECTION at its position in the source.
        let x0 = dirty_x.max(0);
        let y0 = dirty_y.max(0);
        let x1 = (dirty_x + dirty_width).min(data.width as i32);
        let y1 = (dirty_y + dirty_height).min(data.height as i32);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
        let mut cropped = Vec::with_capacity((w * h * 4) as usize);
        for row in y0..y1 {
            let start = ((row * data.width as i32 + x0) * 4) as usize;
            let end = start + (w * 4) as usize;
            cropped.extend_from_slice(&data.data[start..end]);
        }
        let Some(sub) = ImageData::from_rgba(w, h, cropped) else {
            return;
        };
        self.put_image_data_spec(&sub, dx + x0 as f32, dy + y0 as f32);
    }

    // ─── Reading the drawing state back ─────────────────────────────────
    //
    // **Every attribute in §4.12.5 is readable, not only writable.** A page
    // does `const prev = ctx.fillStyle` before changing it, checks
    // `ctx.textAlign`, or serialises `ctx.font` — and until these existed this
    // trait had 27 setters and no way to ask any of them a question.
    //
    // Written once, as defaults over `drawing_state`, rather than twice as
    // overrides. Both implementations keep the same `DrawingState`, so the
    // recording and the rasteriser cannot drift into disagreeing about what an
    // attribute currently is or what its initial value was.

    // ─── CSS values, as the page wrote them ─────────────────────────────
    //
    // `fillStyle`, `strokeStyle`, `font` and `shadowColor` are CSS values in
    // the IDL, and a page assigns them as text. These take that text and let
    // the ENGINE parse it — with the same parser the engine's stylesheets go
    // through, so `"rebeccapurple"`, `"color-mix(...)"` and every other form
    // the engine understands work here for free and cannot drift.
    //
    // The typed setters above stay: .NET's `System.Drawing` holds a `Color`
    // and a `Font`, not a string, and making it serialize one just so this
    // layer could parse it back would be a round trip that can only lose.

    /// `fillStyle = "<color>"`. A value the engine cannot parse is IGNORED —
    /// §4.12.5 says an unparseable assignment leaves the attribute unchanged
    /// rather than resetting it to a default.
    fn set_fill_style_css(&mut self, css: &str);

    /// `strokeStyle = "<color>"`. Same rule.
    fn set_stroke_style_css(&mut self, css: &str);

    /// `font = "<font shorthand>"`. An unparseable value is ignored.
    fn set_font_css(&mut self, css: &str);

    /// `shadowColor = "<color>"`. Same rule.
    fn set_shadow_color_css(&mut self, css: &str);

    /// The drawing state this canvas is holding — HTML §4.12.5.1.2.
    ///
    /// The one method an implementation has to write; every getter below reads
    /// through it.
    fn drawing_state(&self) -> &DrawingState;

    /// `font`, serialized — §4.12.5.
    ///
    /// The SERIALIZED form, not the text the page wrote: `ctx.font = "48px
    /// serif"` reads back `"48px serif"`, but so does a font set through the
    /// typed `set_font`, which never had a string. Keeping the author's text
    /// instead would make those two disagree for the same font.
    fn current_font_css(&self) -> String {
        self.drawing_state().font.to_css()
    }

    /// The font itself, for a caller that wants the parts rather than a string.
    fn current_font(&self) -> Font {
        self.drawing_state().font.clone()
    }

    /// `fillStyle`, serialized.
    ///
    /// A colour serializes per §4.12.5: lowercase `#rrggbb` when fully opaque,
    /// `rgba(r, g, b, a)` when not. A GRADIENT or PATTERN serializes to nothing
    /// here on purpose — the IDL says the attribute returns the object itself,
    /// and the page is already holding it; an engine has no way to hand back a
    /// JavaScript object and inventing a string for one would be worse than
    /// empty.
    fn fill_style_css(&self) -> String {
        self.drawing_state().fill.to_css()
    }

    /// `strokeStyle`, serialized. Same rules as `fillStyle`.
    fn stroke_style_css(&self) -> String {
        self.drawing_state().stroke.to_css()
    }

    /// `filter` — the CSS `<filter-value-list>` as written, or `"none"`.
    fn filter_css(&self) -> String {
        let f = &self.drawing_state().filter;
        if f.trim().is_empty() {
            "none".to_string()
        } else {
            f.clone()
        }
    }

    /// `shadowColor`, serialized.
    fn shadow_color_css(&self) -> String {
        self.drawing_state().shadow.color.to_css()
    }

    fn shadow_blur(&self) -> f32 {
        self.drawing_state().shadow.blur
    }

    fn shadow_offset_x(&self) -> f32 {
        self.drawing_state().shadow.offset_x
    }

    fn shadow_offset_y(&self) -> f32 {
        self.drawing_state().shadow.offset_y
    }

    /// `letterSpacing` — a CSS length, and it reads back as one.
    ///
    /// Returned as written rather than re-serialized from a number, because
    /// the IDL keeps the string: `"1em"` reads back `"1em"`, not the pixels it
    /// resolves to against whatever the font size happened to be.
    fn letter_spacing_css(&self) -> String {
        self.drawing_state().letter_spacing.clone()
    }

    /// `wordSpacing` — as above.
    fn word_spacing_css(&self) -> String {
        self.drawing_state().word_spacing.clone()
    }

    fn line_width(&self) -> f32 {
        self.drawing_state().line_width
    }

    fn line_cap(&self) -> LineCap {
        self.drawing_state().line_cap
    }

    fn line_join(&self) -> LineJoin {
        self.drawing_state().line_join
    }

    fn miter_limit(&self) -> f32 {
        self.drawing_state().miter_limit
    }

    fn line_dash_offset(&self) -> f32 {
        self.drawing_state().dash_offset
    }

    fn global_alpha(&self) -> f32 {
        self.drawing_state().global_alpha
    }

    fn image_smoothing(&self) -> bool {
        self.drawing_state().image_smoothing
    }

    fn image_smoothing_quality(&self) -> SmoothingQuality {
        self.drawing_state().smoothing_quality
    }

    fn global_composite_operation(&self) -> CompositeOp {
        self.drawing_state().composite
    }

    fn text_align(&self) -> TextAlign {
        self.drawing_state().text_align
    }

    fn text_baseline(&self) -> TextBaseline {
        self.drawing_state().text_baseline
    }

    fn direction(&self) -> Direction {
        self.drawing_state().direction
    }

    fn font_kerning(&self) -> FontKerning {
        self.drawing_state().font_kerning
    }

    fn font_stretch(&self) -> FontStretch {
        self.drawing_state().font_stretch
    }

    fn font_variant_caps(&self) -> FontVariantCaps {
        self.drawing_state().font_variant_caps
    }

    fn text_rendering(&self) -> TextRendering {
        self.drawing_state().text_rendering
    }

    /// `lang` — the empty string means `"inherit"`, which is the IDL default.
    fn lang(&self) -> String {
        self.drawing_state().lang.clone()
    }
}
