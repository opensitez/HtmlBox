//! Canvas value types — `Color`, `LineCap`, `LineJoin`, `Font`, `Image`.
//!
//! Kept in their own module so the `Canvas` trait surface and the impls
//! can each pull only what they need. Every type here is `Clone +
//! Debug + PartialEq` so they can live in `DrawCmd` variants without
//! ceremony.

use std::sync::Arc;

/// 32-bit RGBA colour. Components are 0-255.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Serialize as CSS, the way HTML §4.12.5 says a canvas colour attribute
    /// reads back.
    ///
    /// Lowercase `#rrggbb` when fully opaque, `rgba(r, g, b, a)` when not —
    /// with the alpha as a NUMBER in 0..1, not the 0..255 byte it is stored as.
    /// `fillStyle = "red"` therefore reads back `"#ff0000"` and not `"red"`:
    /// the attribute returns the serialization of the colour, not the text the
    /// page happened to type.
    pub fn to_css(&self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            // Trailing zeros trimmed, because `rgba(0, 0, 0, 0.5)` is the
            // serialization and `0.50000000` is not.
            let alpha = format!("{}", (self.a as f32 / 255.0 * 1000.0).round() / 1000.0);
            format!("rgba({}, {}, {}, {})", self.r, self.g, self.b, alpha)
        }
    }

    pub const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// Construct an opaque RGB colour.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Construct an RGBA colour.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Convert to `tiny_skia::Color` for backend impls.
    pub fn to_tiny_skia(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.r, self.g, self.b, self.a)
    }
}

/// One entry of a gradient's colour ramp — `CanvasGradient.addColorStop`.
///
/// `offset` is 0..=1 in gradient space. The spec throws `IndexSizeError`
/// outside that range; `add_color_stop` clamps instead, because a recording
/// that silently drops a stop is harder to see than one that pins it to the
/// end. Stops are kept in insertion order and sorted at paint time — the spec
/// requires stable ordering for equal offsets, which a sort-on-insert loses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorStop {
    pub offset: f32,
    pub color: Color,
}

/// How a pattern repeats — `createPattern`'s second argument.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Repetition {
    #[default]
    Repeat,
    RepeatX,
    RepeatY,
    NoRepeat,
}

impl Repetition {
    /// Parse the spec's four literals. The empty string is `repeat`, per
    /// §"createPattern" — a null or missing repetition is not an error.
    pub fn parse(value: &str) -> Option<Repetition> {
        match value.trim() {
            "" | "repeat" => Some(Repetition::Repeat),
            "repeat-x" => Some(Repetition::RepeatX),
            "repeat-y" => Some(Repetition::RepeatY),
            "no-repeat" => Some(Repetition::NoRepeat),
            _ => None,
        }
    }
}

/// A gradient's geometry. The colour ramp travels beside it in [`Gradient`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientKind {
    /// `createLinearGradient(x0, y0, x1, y1)` — the ramp runs along the line.
    Linear { x0: f32, y0: f32, x1: f32, y1: f32 },
    /// `createRadialGradient(x0, y0, r0, x1, y1, r1)` — the ramp runs between
    /// two circles, which is what makes off-centre highlights expressible.
    Radial {
        x0: f32,
        y0: f32,
        r0: f32,
        x1: f32,
        y1: f32,
        r1: f32,
    },
    /// `createConicGradient(startAngle, x, y)` — the ramp sweeps around a
    /// point. Note the spec's argument order puts the ANGLE first.
    Conic { start_angle: f32, x: f32, y: f32 },
}

/// `CanvasGradient` — geometry plus an ordered colour ramp.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    pub stops: Vec<ColorStop>,
}

impl Gradient {
    pub fn linear(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self {
            kind: GradientKind::Linear { x0, y0, x1, y1 },
            stops: Vec::new(),
        }
    }

    pub fn radial(x0: f32, y0: f32, r0: f32, x1: f32, y1: f32, r1: f32) -> Self {
        Self {
            kind: GradientKind::Radial {
                x0,
                y0,
                r0,
                x1,
                y1,
                r1,
            },
            stops: Vec::new(),
        }
    }

    pub fn conic(start_angle: f32, x: f32, y: f32) -> Self {
        Self {
            kind: GradientKind::Conic { start_angle, x, y },
            stops: Vec::new(),
        }
    }

    /// `CanvasGradient.addColorStop(offset, color)`.
    pub fn add_color_stop(&mut self, offset: f32, color: Color) {
        self.stops.push(ColorStop {
            offset: offset.clamp(0.0, 1.0),
            color,
        });
    }

    /// The ramp in paint order: by offset, insertion order preserved for ties.
    ///
    /// `sort_by` is stable, which is the half the spec actually pins down —
    /// two stops at the same offset paint in the order they were added, and
    /// that is how a hard colour boundary is expressed.
    pub fn sorted_stops(&self) -> Vec<ColorStop> {
        let mut stops = self.stops.clone();
        stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        stops
    }
}

/// `CanvasPattern` — an image tiled under the shape being painted.
#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub image: Image,
    pub repetition: Repetition,
}

/// What `fillStyle` / `strokeStyle` hold.
///
/// The spec types both as `DOMString | CanvasGradient | CanvasPattern`, so a
/// bare `Color` cannot represent the state: it makes every gradient and every
/// pattern unexpressible, which is why `LinearGradientBrush` and `HatchBrush`
/// had nowhere to land and stayed on the widget factory.
///
/// `Color` is still the default and still the common case — `Paint::Color` is
/// what a plain `set_fill_color` produces, and the backends keep their fast
/// path for it.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Color(Color),
    Gradient(Gradient),
    Pattern(Pattern),
}

impl Default for Paint {
    fn default() -> Self {
        Paint::Color(Color::BLACK)
    }
}

impl Paint {
    /// The single colour this paint is, if it is one.
    ///
    /// Backends that cannot build a shader use this to degrade to a flat fill
    /// rather than to nothing: a gradient's FIRST stop is a far better answer
    /// than transparent, and it keeps a shape visible in a capture.
    pub fn as_flat_color(&self) -> Color {
        match self {
            Paint::Color(c) => *c,
            Paint::Gradient(g) => g
                .sorted_stops()
                .first()
                .map(|s| s.color)
                .unwrap_or(Color::TRANSPARENT),
            Paint::Pattern(_) => Color::TRANSPARENT,
        }
    }
}

impl From<Color> for Paint {
    fn from(color: Color) -> Self {
        Paint::Color(color)
    }
}

/// `shadowColor` + `shadowBlur` + `shadowOffsetX` + `shadowOffsetY`.
///
/// The default is a fully transparent colour with no blur and no offset, which
/// is the spec's "no shadow" state — and why [`Shadow::is_visible`] tests the
/// alpha first. A shadow with a transparent colour draws nothing no matter how
/// large the blur.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub color: Color,
    pub blur: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl Default for Shadow {
    fn default() -> Self {
        Self {
            color: Color::TRANSPARENT,
            blur: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }
}

impl Shadow {
    /// Does this state paint anything?
    ///
    /// Per the spec's shadow-drawing preconditions: a shadow is painted when
    /// the colour is not fully transparent AND at least one of blur, offsetX
    /// or offsetY is non-zero. A zero-blur zero-offset shadow sits exactly
    /// under the shape and is invisible, so skipping it is not an
    /// approximation — it is the same output for less work.
    pub fn is_visible(&self) -> bool {
        self.color.a != 0 && (self.blur != 0.0 || self.offset_x != 0.0 || self.offset_y != 0.0)
    }
}

/// Which points count as inside a path — `fill(fillRule)` / `clip(fillRule)`.
///
/// This is not cosmetic: a path with self-intersections or a hole punched by a
/// reversed subpath fills DIFFERENTLY under the two rules, and `evenodd` is the
/// only way to express "hole" without splitting the path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

impl FillRule {
    pub fn parse(value: &str) -> Option<FillRule> {
        match value.trim() {
            "nonzero" => Some(FillRule::NonZero),
            "evenodd" => Some(FillRule::EvenOdd),
            _ => None,
        }
    }

    pub fn to_tiny_skia(self) -> tiny_skia::FillRule {
        match self {
            FillRule::NonZero => tiny_skia::FillRule::Winding,
            FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
        }
    }
}

/// How the ends of stroked lines are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineCap {
    /// Flat edge ending exactly at the line endpoint.
    Butt,
    /// Half-circle extending past the endpoint.
    Round,
    /// Square extending past the endpoint by half the line width.
    Square,
}

impl Default for LineCap {
    fn default() -> Self {
        LineCap::Butt
    }
}

/// How stroked lines join at corners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineJoin {
    /// Sharp corner. Long miters get clipped to `miter_limit`.
    Miter,
    /// Rounded corner.
    Round,
    /// Beveled corner.
    Bevel,
}

impl Default for LineJoin {
    fn default() -> Self {
        LineJoin::Miter
    }
}

/// Font weight (normal vs bold). HTML5 canvas uses numeric weights too,
/// but every backend we care about reduces them to a binary normal/bold
/// distinction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontWeight {
    Normal,
    Bold,
}

impl Default for FontWeight {
    fn default() -> Self {
        FontWeight::Normal
    }
}

/// Font style (upright vs italic).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontStyle {
    Normal,
    Italic,
}

/// `textAlign` — which end of the text the `x` in `fillText(text, x, y)` names.
///
/// `start`/`end` are the logical spellings and equal `left`/`right` in a
/// left-to-right context, which is the only direction this canvas lays out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Left,
    Right,
    Center,
}

impl TextAlign {
    pub fn parse(value: &str) -> Option<TextAlign> {
        match value.trim().to_ascii_lowercase().as_str() {
            "start" => Some(TextAlign::Start),
            "end" => Some(TextAlign::End),
            "left" => Some(TextAlign::Left),
            "right" => Some(TextAlign::Right),
            "center" | "centre" => Some(TextAlign::Center),
            _ => None,
        }
    }
}

/// `textBaseline` — which horizontal line of the text the `y` names.
///
/// **The default is `alphabetic`**, and that is not a detail: it means `y` is
/// the baseline, so the glyphs sit ABOVE it. Treating `y` as the top of the
/// text — which this canvas did, with a comment admitting it — puts every
/// string roughly one ascent too low.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextBaseline {
    Top,
    Hanging,
    Middle,
    #[default]
    Alphabetic,
    Ideographic,
    Bottom,
}

impl TextBaseline {
    pub fn parse(value: &str) -> Option<TextBaseline> {
        match value.trim().to_ascii_lowercase().as_str() {
            "top" => Some(TextBaseline::Top),
            "hanging" => Some(TextBaseline::Hanging),
            "middle" => Some(TextBaseline::Middle),
            "alphabetic" => Some(TextBaseline::Alphabetic),
            "ideographic" => Some(TextBaseline::Ideographic),
            "bottom" => Some(TextBaseline::Bottom),
            _ => None,
        }
    }
}

impl Default for FontStyle {
    fn default() -> Self {
        FontStyle::Normal
    }
}

/// A font specification — family name + size + weight/style.
#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    pub family: String,
    pub size: f32,
    pub weight: FontWeight,
    pub style: FontStyle,
}

impl Font {
    /// Serialize as a CSS `font` shorthand — what `ctx.font` reads back.
    ///
    /// Order is the shorthand's own: style, then weight, then size, then
    /// family. The optional parts are omitted when they are the initial value,
    /// which is why a plain 48px font reads back `"48px sans-serif"` and not
    /// `"normal normal 48px sans-serif"`.
    pub fn to_css(&self) -> String {
        let mut out = String::new();
        if matches!(self.style, FontStyle::Italic) {
            out.push_str("italic ");
        }
        if matches!(self.weight, FontWeight::Bold) {
            out.push_str("bold ");
        }
        // `{}` rather than `{:?}` so 48.0 is `48px`, not `48px` only by luck —
        // a trailing `.0` is not part of the serialization.
        let size = if self.size.fract() == 0.0 {
            format!("{}", self.size as i64)
        } else {
            format!("{}", self.size)
        };
        out.push_str(&size);
        out.push_str("px ");
        out.push_str(&self.family);
        out
    }
}

impl Paint {
    /// Serialize as CSS, for `fillStyle` / `strokeStyle` read-back.
    ///
    /// A gradient or a pattern answers the EMPTY string. The IDL says those
    /// attributes return the `CanvasGradient` / `CanvasPattern` object itself,
    /// which the page is already holding — an engine cannot hand back a script
    /// object, and inventing a string for one would be a value that looks
    /// authoritative and means nothing.
    pub fn to_css(&self) -> String {
        match self {
            Paint::Color(c) => c.to_css(),
            _ => String::new(),
        }
    }
}

impl Default for Font {
    fn default() -> Self {
        Self {
            family: "sans-serif".to_string(),
            size: 12.0,
            weight: FontWeight::Normal,
            style: FontStyle::Normal,
        }
    }
}

impl Font {
    /// Construct a font with the given family and size.
    pub fn new(family: impl Into<String>, size: f32) -> Self {
        Self {
            family: family.into(),
            size,
            weight: FontWeight::Normal,
            style: FontStyle::Normal,
        }
    }

    pub fn with_bold(mut self, bold: bool) -> Self {
        self.weight = if bold {
            FontWeight::Bold
        } else {
            FontWeight::Normal
        };
        self
    }

    pub fn with_italic(mut self, italic: bool) -> Self {
        self.style = if italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        };
        self
    }
}

/// Pixel-buffer image, used by `draw_image`.
///
/// Pixels are tightly packed RGBA, row-major. Stored in an `Arc` so
/// `DrawCmd::DrawImage` is cheap to clone (the recording stores image
/// references, not copies of pixel data).
#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<Vec<u8>>,
}

impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && Arc::ptr_eq(&self.pixels, &other.pixels)
    }
}

impl Image {
    /// The sub-rectangle `(sx, sy, sw, sh)` as a new image — `drawImage`'s
    /// nine-argument source rect.
    ///
    /// Clamped to the image bounds, so a rectangle that hangs off the edge
    /// yields the part that overlaps rather than reading out of bounds. An
    /// empty or fully outside rectangle answers `None`, which the caller
    /// renders as "draw nothing" — the spec's own handling.
    ///
    /// Negative width or height is normalised, matching how the spec reads a
    /// rectangle with either extent negative.
    pub fn crop(&self, sx: f32, sy: f32, sw: f32, sh: f32) -> Option<Image> {
        let (x0, x1) = if sw < 0.0 { (sx + sw, sx) } else { (sx, sx + sw) };
        let (y0, y1) = if sh < 0.0 { (sy + sh, sy) } else { (sy, sy + sh) };

        let x0 = x0.floor().max(0.0) as u32;
        let y0 = y0.floor().max(0.0) as u32;
        let x1 = (x1.ceil().max(0.0) as u32).min(self.width);
        let y1 = (y1.ceil().max(0.0) as u32).min(self.height);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }

        let (w, h) = (x1 - x0, y1 - y0);
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for row in y0..y1 {
            let start = ((row * self.width + x0) * 4) as usize;
            let end = start + (w * 4) as usize;
            out.extend_from_slice(&self.pixels[start..end]);
        }
        Some(Image::from_rgba(w, h, out))
    }

    /// Construct an image from raw RGBA pixel data.
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        debug_assert_eq!(
            pixels.len(),
            (width * height * 4) as usize,
            "Image::from_rgba: expected width*height*4 bytes",
        );
        Self {
            width,
            height,
            pixels: Arc::new(pixels),
        }
    }

    /// Expand an 8-bit palette-indexed image to RGBA.
    ///
    /// This is the shape a software renderer produces: `indices` holds one
    /// byte per pixel, `palette` is up to 256 entries of `0xRRGGBB` (alpha is
    /// forced opaque — an indexed frame has no alpha channel). Doom's
    /// `I_FinishUpdate` is exactly this: an 8-bit screen buffer plus a 256
    /// colour palette, expanded once per frame.
    ///
    /// Lives here rather than in the host bridge because it is an image-format
    /// concern, and it runs NATIVELY — the guest writes indices, the host does
    /// the per-pixel work, so no part of this crosses into WASM.
    ///
    /// Short `indices` are padded with index 0 and out-of-range indices read as
    /// black, so a guest that miscounts gets a visibly wrong frame rather than
    /// a panic.
    pub fn from_paletted(width: u32, height: u32, indices: &[u8], palette: &[u32]) -> Self {
        let count = (width as usize) * (height as usize);
        let mut rgba = Vec::with_capacity(count * 4);
        for i in 0..count {
            let entry = indices
                .get(i)
                .and_then(|&idx| palette.get(idx as usize))
                .copied()
                .unwrap_or(0);
            rgba.push((entry >> 16) as u8);
            rgba.push((entry >> 8) as u8);
            rgba.push(entry as u8);
            rgba.push(0xFF);
        }
        Self {
            width,
            height,
            pixels: Arc::new(rgba),
        }
    }

    /// Decode an image from a file path. Supports PNG, JPEG, GIF, and BMP.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let img = image::open(path.as_ref())
            .map_err(|e| format!("Image::from_file: {}", e))?
            .to_rgba8();
        let (width, height) = img.dimensions();
        Ok(Self::from_rgba(width, height, img.into_raw()))
    }

    /// Decode an image from in-memory bytes (auto-detects format from
    /// content). Supports PNG, JPEG, GIF, and BMP.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let img = image::load_from_memory(bytes)
            .map_err(|e| format!("Image::from_bytes: {}", e))?
            .to_rgba8();
        let (width, height) = img.dimensions();
        Ok(Self::from_rgba(width, height, img.into_raw()))
    }

    /// Save the image as a PNG file.
    pub fn save_png(&self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        image::save_buffer(
            path.as_ref(),
            &self.pixels,
            self.width,
            self.height,
            image::ColorType::Rgba8,
        )
        .map_err(|e| format!("Image::save_png: {}", e))
    }
}

// ─── globalCompositeOperation ───────────────────────────────────────────────

/// `globalCompositeOperation` — how a new drawing combines with what is already
/// on the canvas.
///
/// All 26 values the spec lists, and they are not decoration: the twelve
/// Porter-Duff modes at the top decide whether a draw is masked by, or masks,
/// the existing pixels, and the separable blend modes below are the same set
/// CSS `mix-blend-mode` uses. A canvas that silently treated every one of them
/// as `source-over` would paint plausible-looking wrong output, which is why
/// this parses to an enum rather than being carried as a string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CompositeOp {
    #[default]
    SourceOver,
    SourceIn,
    SourceOut,
    SourceAtop,
    DestinationOver,
    DestinationIn,
    DestinationOut,
    DestinationAtop,
    Lighter,
    Copy,
    Xor,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    ColorBlend,
    Luminosity,
}

impl CompositeOp {
    /// Parse the spec keyword. `None` for anything else — the spec says an
    /// unknown value leaves the attribute UNCHANGED rather than resetting it,
    /// so the caller needs to tell "not recognised" from "recognised", and a
    /// silent fallback to `source-over` would be the wrong answer twice over.
    pub fn parse(value: &str) -> Option<CompositeOp> {
        Some(match value {
            "source-over" => CompositeOp::SourceOver,
            "source-in" => CompositeOp::SourceIn,
            "source-out" => CompositeOp::SourceOut,
            "source-atop" => CompositeOp::SourceAtop,
            "destination-over" => CompositeOp::DestinationOver,
            "destination-in" => CompositeOp::DestinationIn,
            "destination-out" => CompositeOp::DestinationOut,
            "destination-atop" => CompositeOp::DestinationAtop,
            "lighter" => CompositeOp::Lighter,
            "copy" => CompositeOp::Copy,
            "xor" => CompositeOp::Xor,
            "multiply" => CompositeOp::Multiply,
            "screen" => CompositeOp::Screen,
            "overlay" => CompositeOp::Overlay,
            "darken" => CompositeOp::Darken,
            "lighten" => CompositeOp::Lighten,
            "color-dodge" => CompositeOp::ColorDodge,
            "color-burn" => CompositeOp::ColorBurn,
            "hard-light" => CompositeOp::HardLight,
            "soft-light" => CompositeOp::SoftLight,
            "difference" => CompositeOp::Difference,
            "exclusion" => CompositeOp::Exclusion,
            "hue" => CompositeOp::Hue,
            "saturation" => CompositeOp::Saturation,
            "color" => CompositeOp::ColorBlend,
            "luminosity" => CompositeOp::Luminosity,
            _ => return None,
        })
    }

    /// The spec keyword, for reading the attribute back.
    pub fn as_str(self) -> &'static str {
        match self {
            CompositeOp::SourceOver => "source-over",
            CompositeOp::SourceIn => "source-in",
            CompositeOp::SourceOut => "source-out",
            CompositeOp::SourceAtop => "source-atop",
            CompositeOp::DestinationOver => "destination-over",
            CompositeOp::DestinationIn => "destination-in",
            CompositeOp::DestinationOut => "destination-out",
            CompositeOp::DestinationAtop => "destination-atop",
            CompositeOp::Lighter => "lighter",
            CompositeOp::Copy => "copy",
            CompositeOp::Xor => "xor",
            CompositeOp::Multiply => "multiply",
            CompositeOp::Screen => "screen",
            CompositeOp::Overlay => "overlay",
            CompositeOp::Darken => "darken",
            CompositeOp::Lighten => "lighten",
            CompositeOp::ColorDodge => "color-dodge",
            CompositeOp::ColorBurn => "color-burn",
            CompositeOp::HardLight => "hard-light",
            CompositeOp::SoftLight => "soft-light",
            CompositeOp::Difference => "difference",
            CompositeOp::Exclusion => "exclusion",
            CompositeOp::Hue => "hue",
            CompositeOp::Saturation => "saturation",
            CompositeOp::ColorBlend => "color",
            CompositeOp::Luminosity => "luminosity",
        }
    }

    /// The tiny-skia blend mode that means the same thing.
    ///
    /// Every one of the 26 has an exact counterpart, so this is a translation
    /// and not an approximation — worth stating because the reverse would be
    /// invisible in output.
    pub fn to_tiny_skia(self) -> tiny_skia::BlendMode {
        use tiny_skia::BlendMode as B;
        match self {
            CompositeOp::SourceOver => B::SourceOver,
            CompositeOp::SourceIn => B::SourceIn,
            CompositeOp::SourceOut => B::SourceOut,
            CompositeOp::SourceAtop => B::SourceAtop,
            CompositeOp::DestinationOver => B::DestinationOver,
            CompositeOp::DestinationIn => B::DestinationIn,
            CompositeOp::DestinationOut => B::DestinationOut,
            CompositeOp::DestinationAtop => B::DestinationAtop,
            CompositeOp::Lighter => B::Plus,
            CompositeOp::Copy => B::Source,
            CompositeOp::Xor => B::Xor,
            CompositeOp::Multiply => B::Multiply,
            CompositeOp::Screen => B::Screen,
            CompositeOp::Overlay => B::Overlay,
            CompositeOp::Darken => B::Darken,
            CompositeOp::Lighten => B::Lighten,
            CompositeOp::ColorDodge => B::ColorDodge,
            CompositeOp::ColorBurn => B::ColorBurn,
            CompositeOp::HardLight => B::HardLight,
            CompositeOp::SoftLight => B::SoftLight,
            CompositeOp::Difference => B::Difference,
            CompositeOp::Exclusion => B::Exclusion,
            CompositeOp::Hue => B::Hue,
            CompositeOp::Saturation => B::Saturation,
            CompositeOp::ColorBlend => B::Color,
            CompositeOp::Luminosity => B::Luminosity,
        }
    }
}

/// `imageSmoothingQuality`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SmoothingQuality {
    #[default]
    Low,
    Medium,
    High,
}

impl SmoothingQuality {
    pub fn parse(value: &str) -> Option<SmoothingQuality> {
        Some(match value {
            "low" => SmoothingQuality::Low,
            "medium" => SmoothingQuality::Medium,
            "high" => SmoothingQuality::High,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SmoothingQuality::Low => "low",
            SmoothingQuality::Medium => "medium",
            SmoothingQuality::High => "high",
        }
    }

    /// tiny-skia's sampling quality. `medium` and `high` both map to bilinear:
    /// tiny-skia has no third tier, and claiming one would be a lie told in
    /// pixels.
    pub fn to_filter_quality(self) -> tiny_skia::FilterQuality {
        match self {
            SmoothingQuality::Low => tiny_skia::FilterQuality::Nearest,
            SmoothingQuality::Medium | SmoothingQuality::High => {
                tiny_skia::FilterQuality::Bilinear
            }
        }
    }
}

// ─── CanvasTextDrawingStyles keywords ───────────────────────────────────────

/// `direction` — the text direction `textAlign`'s `start`/`end` resolve against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Direction {
    Ltr,
    Rtl,
    /// The spec default: take the direction from the canvas element.
    #[default]
    Inherit,
}

impl Direction {
    pub fn parse(value: &str) -> Option<Direction> {
        Some(match value {
            "ltr" => Direction::Ltr,
            "rtl" => Direction::Rtl,
            "inherit" => Direction::Inherit,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Ltr => "ltr",
            Direction::Rtl => "rtl",
            Direction::Inherit => "inherit",
        }
    }
}

/// `fontKerning`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FontKerning {
    #[default]
    Auto,
    Normal,
    None,
}

impl FontKerning {
    pub fn parse(value: &str) -> Option<FontKerning> {
        Some(match value {
            "auto" => FontKerning::Auto,
            "normal" => FontKerning::Normal,
            "none" => FontKerning::None,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FontKerning::Auto => "auto",
            FontKerning::Normal => "normal",
            FontKerning::None => "none",
        }
    }
}

/// `fontStretch` — the nine CSS font-width keywords.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FontStretch {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    #[default]
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

impl FontStretch {
    pub fn parse(value: &str) -> Option<FontStretch> {
        Some(match value {
            "ultra-condensed" => FontStretch::UltraCondensed,
            "extra-condensed" => FontStretch::ExtraCondensed,
            "condensed" => FontStretch::Condensed,
            "semi-condensed" => FontStretch::SemiCondensed,
            "normal" => FontStretch::Normal,
            "semi-expanded" => FontStretch::SemiExpanded,
            "expanded" => FontStretch::Expanded,
            "extra-expanded" => FontStretch::ExtraExpanded,
            "ultra-expanded" => FontStretch::UltraExpanded,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FontStretch::UltraCondensed => "ultra-condensed",
            FontStretch::ExtraCondensed => "extra-condensed",
            FontStretch::Condensed => "condensed",
            FontStretch::SemiCondensed => "semi-condensed",
            FontStretch::Normal => "normal",
            FontStretch::SemiExpanded => "semi-expanded",
            FontStretch::Expanded => "expanded",
            FontStretch::ExtraExpanded => "extra-expanded",
            FontStretch::UltraExpanded => "ultra-expanded",
        }
    }

    /// The cosmic-text width class this keyword names.
    pub fn to_cosmic(self) -> cosmic_text::Stretch {
        use cosmic_text::Stretch as S;
        match self {
            FontStretch::UltraCondensed => S::UltraCondensed,
            FontStretch::ExtraCondensed => S::ExtraCondensed,
            FontStretch::Condensed => S::Condensed,
            FontStretch::SemiCondensed => S::SemiCondensed,
            FontStretch::Normal => S::Normal,
            FontStretch::SemiExpanded => S::SemiExpanded,
            FontStretch::Expanded => S::Expanded,
            FontStretch::ExtraExpanded => S::ExtraExpanded,
            FontStretch::UltraExpanded => S::UltraExpanded,
        }
    }
}

/// `fontVariantCaps`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FontVariantCaps {
    #[default]
    Normal,
    SmallCaps,
    AllSmallCaps,
    PetiteCaps,
    AllPetiteCaps,
    Unicase,
    TitlingCaps,
}

impl FontVariantCaps {
    pub fn parse(value: &str) -> Option<FontVariantCaps> {
        Some(match value {
            "normal" => FontVariantCaps::Normal,
            "small-caps" => FontVariantCaps::SmallCaps,
            "all-small-caps" => FontVariantCaps::AllSmallCaps,
            "petite-caps" => FontVariantCaps::PetiteCaps,
            "all-petite-caps" => FontVariantCaps::AllPetiteCaps,
            "unicase" => FontVariantCaps::Unicase,
            "titling-caps" => FontVariantCaps::TitlingCaps,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FontVariantCaps::Normal => "normal",
            FontVariantCaps::SmallCaps => "small-caps",
            FontVariantCaps::AllSmallCaps => "all-small-caps",
            FontVariantCaps::PetiteCaps => "petite-caps",
            FontVariantCaps::AllPetiteCaps => "all-petite-caps",
            FontVariantCaps::Unicase => "unicase",
            FontVariantCaps::TitlingCaps => "titling-caps",
        }
    }
}

/// `textRendering`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextRendering {
    #[default]
    Auto,
    OptimizeSpeed,
    OptimizeLegibility,
    GeometricPrecision,
}

impl TextRendering {
    pub fn parse(value: &str) -> Option<TextRendering> {
        Some(match value {
            "auto" => TextRendering::Auto,
            "optimizeSpeed" => TextRendering::OptimizeSpeed,
            "optimizeLegibility" => TextRendering::OptimizeLegibility,
            "geometricPrecision" => TextRendering::GeometricPrecision,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TextRendering::Auto => "auto",
            TextRendering::OptimizeSpeed => "optimizeSpeed",
            TextRendering::OptimizeLegibility => "optimizeLegibility",
            TextRendering::GeometricPrecision => "geometricPrecision",
        }
    }
}

// ─── getTransform / DOMMatrix2DInit ─────────────────────────────────────────

/// A 2D affine matrix in the spec's own naming.
///
/// `a`…`f` rather than `m11`/`m12`/… because that is what `setTransform` and
/// `DOMMatrix2DInit` call them, and the six map onto the same six the trait's
/// [`super::Canvas::transform`] already takes. Row-vector convention, like the
/// spec and like tiny-skia: `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Default for Matrix {
    fn default() -> Self {
        Matrix::IDENTITY
    }
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub const fn new(a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) -> Self {
        Matrix { a, b, c, d, e, f }
    }

    /// `self` THEN `other` — the order `transform()` composes in: the new
    /// matrix is applied to points first, in the current space.
    pub fn multiply(self, other: Matrix) -> Matrix {
        Matrix {
            a: other.a * self.a + other.b * self.c,
            b: other.a * self.b + other.b * self.d,
            c: other.c * self.a + other.d * self.c,
            d: other.c * self.b + other.d * self.d,
            e: other.e * self.a + other.f * self.c + self.e,
            f: other.e * self.b + other.f * self.d + self.f,
        }
    }

    /// The inverse, or `None` when the matrix is singular — which is what a
    /// zero scale produces, and is exactly when a hit test has no answer.
    pub fn invert(self) -> Option<Matrix> {
        let det = self.a * self.d - self.b * self.c;
        if det == 0.0 || !det.is_finite() {
            return None;
        }
        let inv = 1.0 / det;
        Some(Matrix {
            a: self.d * inv,
            b: -self.b * inv,
            c: -self.c * inv,
            d: self.a * inv,
            e: (self.c * self.f - self.d * self.e) * inv,
            f: (self.b * self.e - self.a * self.f) * inv,
        })
    }

    /// Map a point through the matrix.
    pub fn apply(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    pub fn to_tiny_skia(self) -> tiny_skia::Transform {
        tiny_skia::Transform::from_row(self.a, self.b, self.c, self.d, self.e, self.f)
    }

    pub fn from_tiny_skia(t: tiny_skia::Transform) -> Matrix {
        Matrix {
            a: t.sx,
            b: t.ky,
            c: t.kx,
            d: t.sy,
            e: t.tx,
            f: t.ty,
        }
    }
}

// ─── TextMetrics ────────────────────────────────────────────────────────────

/// What `measureText` returns — all twelve of the spec's readonly attributes.
///
/// Only `width` is the advance; the other eleven describe the INK and the
/// baselines, and they are what a caller needs to centre a label, box a glyph
/// run, or align two runs in different fonts. Returning a bare width made every
/// one of those a guess.
///
/// The y-direction values follow the spec's sign convention: distances ABOVE
/// the alphabetic baseline are positive, below are negative.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub actual_bounding_box_left: f32,
    pub actual_bounding_box_right: f32,
    pub font_bounding_box_ascent: f32,
    pub font_bounding_box_descent: f32,
    pub actual_bounding_box_ascent: f32,
    pub actual_bounding_box_descent: f32,
    pub em_height_ascent: f32,
    pub em_height_descent: f32,
    pub hanging_baseline: f32,
    pub alphabetic_baseline: f32,
    pub ideographic_baseline: f32,
}

// ─── ImageData ──────────────────────────────────────────────────────────────

/// `ImageData` — width, height and a **non-premultiplied** RGBA buffer.
///
/// Its own type rather than an [`Image`], and the reason is the alpha: `Image`
/// carries whatever its source gave it, while `ImageData.data` is defined as
/// straight RGBA in sRGB. Conflating the two is how a semi-transparent
/// `getImageData` → `putImageData` round trip darkens an image slightly each
/// time — a bug that looks like nothing until it is done in a loop.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes, RGBA, NOT premultiplied.
    pub data: Vec<u8>,
    /// `colorSpace`. Only `srgb` is produced today; carried so the attribute
    /// reads back what it was created with instead of inventing an answer.
    pub color_space: &'static str,
}

impl ImageData {
    /// `createImageData(sw, sh)` — transparent black, per spec.
    pub fn new(width: u32, height: u32) -> Self {
        ImageData {
            width,
            height,
            data: vec![0u8; (width as usize) * (height as usize) * 4],
            color_space: "srgb",
        }
    }

    /// Wrap an existing straight-RGBA buffer. `None` when the buffer length
    /// disagrees with the dimensions, which the spec makes an `InvalidState`
    /// error rather than something to paper over.
    pub fn from_rgba(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        if data.len() != (width as usize) * (height as usize) * 4 {
            return None;
        }
        Some(ImageData {
            width,
            height,
            data,
            color_space: "srgb",
        })
    }
}

// ─── CanvasRenderingContext2DSettings ───────────────────────────────────────

/// What `getContextAttributes()` returns, and what `getContext("2d", …)` took.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextAttributes {
    pub alpha: bool,
    pub desynchronized: bool,
    pub color_space: &'static str,
    pub color_type: &'static str,
    pub will_read_frequently: bool,
}

impl Default for ContextAttributes {
    fn default() -> Self {
        ContextAttributes {
            alpha: true,
            desynchronized: false,
            color_space: "srgb",
            color_type: "unorm8",
            will_read_frequently: false,
        }
    }
}

// ─── Path2D ─────────────────────────────────────────────────────────────────

/// One entry in a recorded path.
///
/// The `CanvasPath` mixin verbatim. A [`Path2D`] is a list of these, which is
/// what lets the same path be filled, stroked, hit-tested and clipped against
/// without being rebuilt each time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathOp {
    ClosePath,
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadraticCurveTo {
        cx: f32,
        cy: f32,
        x: f32,
        y: f32,
    },
    BezierCurveTo {
        cx1: f32,
        cy1: f32,
        cx2: f32,
        cy2: f32,
        x: f32,
        y: f32,
    },
    ArcTo {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        radius: f32,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    RoundRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radii: [f32; 4],
    },
    Arc {
        x: f32,
        y: f32,
        r: f32,
        start: f32,
        end: f32,
        ccw: bool,
    },
    Ellipse {
        x: f32,
        y: f32,
        rx: f32,
        ry: f32,
        rotation: f32,
        start: f32,
        end: f32,
        ccw: bool,
    },
}

/// `Path2D` — a path built once and used many times.
///
/// It exists because the context's own current path is CONSUMED: `fill()` then
/// `stroke()` on the same shape means describing it twice, and `clip()` throws
/// it away entirely. A `Path2D` is the spec's answer, and it is also the only
/// form in which a path can be handed to `isPointInPath` without disturbing
/// whatever the context was mid-way through building.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path2D {
    pub ops: Vec<PathOp>,
}

impl Path2D {
    pub fn new() -> Self {
        Path2D::default()
    }

    pub fn close_path(&mut self) {
        self.ops.push(PathOp::ClosePath);
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        self.ops.push(PathOp::MoveTo(x, y));
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        self.ops.push(PathOp::LineTo(x, y));
    }

    pub fn quadratic_curve_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.ops.push(PathOp::QuadraticCurveTo { cx, cy, x, y });
    }

    pub fn bezier_curve_to(&mut self, cx1: f32, cy1: f32, cx2: f32, cy2: f32, x: f32, y: f32) {
        self.ops.push(PathOp::BezierCurveTo {
            cx1,
            cy1,
            cx2,
            cy2,
            x,
            y,
        });
    }

    pub fn arc_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, radius: f32) {
        self.ops.push(PathOp::ArcTo {
            x1,
            y1,
            x2,
            y2,
            radius,
        });
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.ops.push(PathOp::Rect { x, y, w, h });
    }

    pub fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radii: [f32; 4]) {
        self.ops.push(PathOp::RoundRect { x, y, w, h, radii });
    }

    pub fn arc(&mut self, x: f32, y: f32, r: f32, start: f32, end: f32, ccw: bool) {
        self.ops.push(PathOp::Arc {
            x,
            y,
            r,
            start,
            end,
            ccw,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ellipse(
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
        self.ops.push(PathOp::Ellipse {
            x,
            y,
            rx,
            ry,
            rotation,
            start,
            end,
            ccw,
        });
    }

    /// `addPath(path, transform)`.
    ///
    /// The transform is applied to the ADDED path's points, not to this one.
    /// Passing [`Matrix::IDENTITY`] is the one-argument form.
    pub fn add_path(&mut self, other: &Path2D, transform: Matrix) {
        if transform == Matrix::IDENTITY {
            self.ops.extend_from_slice(&other.ops);
            return;
        }
        for op in &other.ops {
            self.ops.push(transform_path_op(*op, transform));
        }
    }
}

/// Map a path op's points through a matrix.
///
/// Radii are scaled by the matrix's average axis scale rather than mapped: an
/// `arc` under a non-uniform transform is an ELLIPSE, which `PathOp::Arc`
/// cannot hold. The lossy cases are converted to `Ellipse`, which can.
fn transform_path_op(op: PathOp, m: Matrix) -> PathOp {
    // Axis lengths of the transformed unit vectors — the scale the matrix
    // applies along x and y, independent of any rotation in it.
    let sx = (m.a * m.a + m.b * m.b).sqrt();
    let sy = (m.c * m.c + m.d * m.d).sqrt();
    // The angle the matrix rotates by, which an ellipse has to carry because
    // its radii are axis-aligned before rotation.
    let rotation = m.b.atan2(m.a);
    match op {
        PathOp::ClosePath => PathOp::ClosePath,
        PathOp::MoveTo(x, y) => {
            let (x, y) = m.apply(x, y);
            PathOp::MoveTo(x, y)
        }
        PathOp::LineTo(x, y) => {
            let (x, y) = m.apply(x, y);
            PathOp::LineTo(x, y)
        }
        PathOp::QuadraticCurveTo { cx, cy, x, y } => {
            let (cx, cy) = m.apply(cx, cy);
            let (x, y) = m.apply(x, y);
            PathOp::QuadraticCurveTo { cx, cy, x, y }
        }
        PathOp::BezierCurveTo {
            cx1,
            cy1,
            cx2,
            cy2,
            x,
            y,
        } => {
            let (cx1, cy1) = m.apply(cx1, cy1);
            let (cx2, cy2) = m.apply(cx2, cy2);
            let (x, y) = m.apply(x, y);
            PathOp::BezierCurveTo {
                cx1,
                cy1,
                cx2,
                cy2,
                x,
                y,
            }
        }
        PathOp::ArcTo {
            x1,
            y1,
            x2,
            y2,
            radius,
        } => {
            let (x1, y1) = m.apply(x1, y1);
            let (x2, y2) = m.apply(x2, y2);
            PathOp::ArcTo {
                x1,
                y1,
                x2,
                y2,
                radius: radius * (sx + sy) / 2.0,
            }
        }
        // A transformed rectangle is only a rectangle again when the matrix has
        // no rotation or skew. It has corners, so it survives as four points
        // either way — but `PathOp::Rect` cannot hold a rotated one, so the
        // caller gets the axis-aligned bounds of the mapped corners and a
        // rotation is not silently dropped from a shape that looks unchanged.
        PathOp::Rect { x, y, w, h } => {
            let (x0, y0) = m.apply(x, y);
            let (x1, y1) = m.apply(x + w, y + h);
            PathOp::Rect {
                x: x0.min(x1),
                y: y0.min(y1),
                w: (x1 - x0).abs(),
                h: (y1 - y0).abs(),
            }
        }
        PathOp::RoundRect { x, y, w, h, radii } => {
            let (x0, y0) = m.apply(x, y);
            let (x1, y1) = m.apply(x + w, y + h);
            let s = (sx + sy) / 2.0;
            PathOp::RoundRect {
                x: x0.min(x1),
                y: y0.min(y1),
                w: (x1 - x0).abs(),
                h: (y1 - y0).abs(),
                radii: [
                    radii[0] * s,
                    radii[1] * s,
                    radii[2] * s,
                    radii[3] * s,
                ],
            }
        }
        // A circle under a non-uniform or rotated matrix is an ellipse, so it
        // becomes one rather than losing the distortion.
        PathOp::Arc {
            x,
            y,
            r,
            start,
            end,
            ccw,
        } => {
            let (cx, cy) = m.apply(x, y);
            PathOp::Ellipse {
                x: cx,
                y: cy,
                rx: r * sx,
                ry: r * sy,
                rotation,
                start,
                end,
                ccw,
            }
        }
        PathOp::Ellipse {
            x,
            y,
            rx,
            ry,
            rotation: r0,
            start,
            end,
            ccw,
        } => {
            let (cx, cy) = m.apply(x, y);
            PathOp::Ellipse {
                x: cx,
                y: cy,
                rx: rx * sx,
                ry: ry * sy,
                rotation: r0 + rotation,
                start,
                end,
                ccw,
            }
        }
    }
}

// ─── Keyword serialization ──────────────────────────────────────────────────
//
// Every canvas attribute whose IDL type is an enumerated string has to read
// back as the keyword that was set — `ctx.textAlign` after `ctx.textAlign =
// "center"` is `"center"`. These are the four that had `parse` and no way back;
// the rest already round-trip.

impl TextAlign {
    pub fn as_str(&self) -> &'static str {
        match self {
            TextAlign::Start => "start",
            TextAlign::End => "end",
            TextAlign::Left => "left",
            TextAlign::Right => "right",
            TextAlign::Center => "center",
        }
    }
}

impl TextBaseline {
    pub fn as_str(&self) -> &'static str {
        match self {
            TextBaseline::Top => "top",
            TextBaseline::Hanging => "hanging",
            TextBaseline::Middle => "middle",
            TextBaseline::Alphabetic => "alphabetic",
            TextBaseline::Ideographic => "ideographic",
            TextBaseline::Bottom => "bottom",
        }
    }
}

impl LineCap {
    pub fn as_str(&self) -> &'static str {
        match self {
            LineCap::Butt => "butt",
            LineCap::Round => "round",
            LineCap::Square => "square",
        }
    }
}

impl LineJoin {
    pub fn as_str(&self) -> &'static str {
        match self {
            LineJoin::Miter => "miter",
            LineJoin::Round => "round",
            LineJoin::Bevel => "bevel",
        }
    }
}
