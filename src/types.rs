use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use crate::dom::arena::DomArena;

// ─── Geometry ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self { Self { x, y, w, h } }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
    pub fn right(&self)  -> f32 { self.x + self.w }
    pub fn bottom(&self) -> f32 { self.y + self.h }
}

// ─── Custom Components ───────────────────────────────────────────────────────

pub type ComponentMeasureFn = Arc<dyn Fn(&WebCore, f32) -> (f32, f32) + Send + Sync>;
pub type ComponentPaintFn   = Arc<dyn Fn(&WebCore, &mut tiny_skia::Pixmap, f32, f32, f32, f32, f32) + Send + Sync>;

#[derive(Clone)]
pub struct ComponentCallbacks {
    pub measure: ComponentMeasureFn,
    pub paint:   ComponentPaintFn,
}

/// A custom component that fully participates in the layout pipeline.
///
/// App developers implement this trait to create custom elements that work
/// as first-class citizens in the engine — like Flutter's `RenderObject`.
///
/// Register with `engine.register_component("my-widget", MyWidget::factory)`.
/// Use in HTML: `<my-widget data-foo="bar" style="width:200px"/>`.
///
/// # Example
/// ```ignore
/// struct ProgressBar;
/// impl Component for ProgressBar {
///     fn measure(&self, node: &WebCore, available_width: f32) -> (f32, f32) {
///         let pct = node.attributes.get("value")
///             .and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
///         (available_width, 24.0) // full width, 24px tall
///     }
///     fn paint(&self, node: &WebCore, pixmap: &mut tiny_skia::Pixmap,
///              x: f32, y: f32, w: f32, h: f32, scale: f32) {
///         // draw track + filled portion
///     }
///     fn intrinsic_width(&self, _node: &WebCore) -> (f32, f32) {
///         (100.0, 300.0) // min 100px, preferred 300px
///     }
/// }
/// ```
pub trait Component: Send + Sync {
    /// Measure: given available width, return (content_width, content_height).
    fn measure(&self, node: &WebCore, available_width: f32) -> (f32, f32);

    /// Paint: draw the component into the pixmap at the given position.
    fn paint(&self, node: &WebCore, pixmap: &mut tiny_skia::Pixmap,
             x: f32, y: f32, w: f32, h: f32, scale: f32);

    /// Intrinsic sizes for parent flex/grid/table sizing.
    /// Returns (min_content_width, max_content_width).
    /// Default: uses measure() with width=0 for min, width=infinity for max.
    fn intrinsic_width(&self, node: &WebCore) -> (f32, f32) {
        let (min_w, _) = self.measure(node, 0.0);
        let (max_w, _) = self.measure(node, f32::MAX);
        (min_w, max_w)
    }

    /// Hit test: is the given point (relative to the component's content rect) inside?
    /// Default: rectangular hit test (always true if within bounds).
    fn hit_test(&self, _node: &WebCore, _x: f32, _y: f32) -> bool { true }

    /// Handle an input event. Return true if the event was consumed.
    fn handle_event(&self, _node: &mut WebCore, _event: &ComponentEvent) -> bool { false }

    /// Accessibility role for this component.
    fn accessibility_role(&self) -> &str { "generic" }

    /// Accessibility label (human-readable name).
    fn accessibility_label(&self, _node: &WebCore) -> Option<String> { None }
}

/// Input events delivered to components.
#[derive(Debug, Clone)]
pub enum ComponentEvent {
    Click { x: f32, y: f32, button: u8 },
    MouseDown { x: f32, y: f32, button: u8 },
    MouseUp { x: f32, y: f32, button: u8 },
    MouseMove { x: f32, y: f32 },
    KeyDown { key: String, modifiers: u8 },
    KeyUp { key: String, modifiers: u8 },
    TextInput { text: String },
    Focus,
    Blur,
}

/// Factory function type for creating Component instances.
pub type ComponentFactory = Arc<dyn Fn() -> Box<dyn Component> + Send + Sync>;

#[derive(Default, Clone)]
pub struct ComponentRegistry {
    /// Legacy callback-based components
    pub map: HashMap<String, ComponentCallbacks>,
    /// Trait-based components (new API)
    pub components: HashMap<String, Arc<dyn Component>>,
}

impl ComponentRegistry {
    pub fn new() -> Self { Self::default() }

    /// Register a legacy callback-based component.
    pub fn register(&mut self, tag: &str, measure: ComponentMeasureFn, paint: ComponentPaintFn) {
        self.map.insert(tag.to_string(), ComponentCallbacks { measure, paint });
    }

    /// Register a trait-based component (new API).
    /// The component instance is shared across all elements with this tag.
    pub fn register_component(&mut self, tag: &str, component: impl Component + 'static) {
        self.components.insert(tag.to_string(), Arc::new(component));
    }

    /// Register a component from an Arc (for sharing across threads).
    pub fn register_component_arc(&mut self, tag: &str, component: Arc<dyn Component>) {
        self.components.insert(tag.to_string(), component);
    }

    /// Look up a component by tag name. Checks trait-based first, then legacy.
    pub fn get_component(&self, tag: &str) -> Option<&Arc<dyn Component>> {
        self.components.get(tag)
    }
}

// ─── Color ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self { Self { r, g, b, a } }
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self { Self { r, g, b, a: 255 } }
    pub const BLACK:       Self = Self::rgb(0, 0, 0);
    pub const WHITE:       Self = Self::rgb(255, 255, 255);
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);

    pub fn to_tiny_skia(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.r, self.g, self.b, self.a)
    }
}

impl Default for Color {
    fn default() -> Self { Self::BLACK }
}

// ─── CSS Value (pre-parsed declaration value) ───────────────────────────────

/// Pre-parsed CSS declaration value. Produced during stylesheet compilation
/// so the cascade never re-parses strings. The `Raw` variant is the fallback
/// for values that haven't been converted to typed form yet, or for values
/// containing `var()` references that must be resolved at cascade time.
#[derive(Clone, Debug)]
pub enum CssValue {
    /// A pre-parsed length value (px, em, %, calc, min, max, clamp, auto, etc.)
    Length(CssLength),
    /// A pre-parsed color value.
    Color(Color),
    /// A numeric value (opacity, flex-grow, flex-shrink, etc.)
    Number(f32),
    /// An integer value (z-index, order, column-count, etc.)
    Integer(i32),
    /// Pre-parsed keyword enums — avoids string matching during cascade.
    Display(Display),
    Position(Position),
    Float(Float),
    Clear(Clear),
    BoxSizing(BoxSizing),
    Overflow(Overflow),
    /// visibility: true=visible, false=hidden
    Visible(bool),
    TextAlign(TextAlign),
    TextTransform(TextTransform),
    WhiteSpace(WhiteSpace),
    FontWeight(FontWeight),
    FontStyle(FontStyle),
    FlexDirection(FlexDirection),
    FlexWrap(FlexWrap),
    AlignItems(AlignItems),
    AlignSelf(AlignSelf),
    AlignContent(AlignContent),
    JustifyContent(JustifyContent),
    ListStyleType(ListStyleType),
    ListStylePosition(ListStylePosition),
    WordBreak(WordBreak),
    BorderStyle(BorderStyleValue),
    VerticalAlign(VerticalAlign),
    /// Global CSS keyword.
    Inherit,
    Initial,
    Unset,
    /// Unparsed string — fallback for complex values, var() references,
    /// and properties that haven't been converted to typed form yet.
    Raw(String),
}

/// Border-style single value (not the shorthand).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BorderStyleValue {
    None, Hidden, Solid, Dashed, Dotted, Double, Groove, Ridge, Inset, Outset,
}

impl CssValue {
    /// Extract the raw string for var() resolution and backward-compat paths.
    /// Returns the string for Raw values, empty string for typed values
    /// (typed values don't contain var() references).
    pub fn raw_str(&self) -> &str {
        match self {
            CssValue::Raw(s) => s.as_str(),
            _ => "",
        }
    }

    /// Check if this value contains a var() reference (only possible in Raw).
    pub fn has_var(&self) -> bool {
        match self {
            CssValue::Raw(s) => s.contains("var("),
            _ => false,
        }
    }
}

// ─── CSS Length ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum CssLength {
    Px(f32),
    Em(f32),
    Rem(f32),
    Percent(f32),
    /// Viewport-width percentage (1vw = 1% of viewport width).
    Vw(f32),
    /// Viewport-height percentage (1vh = 1% of viewport height).
    Vh(f32),
    // ── The four rare variants below are BOXED, and the reason is size ──
    // `CssLength` appears 53 times in `ComputedStyle`, so its width dominates:
    // an inline `Calc([f32; 6])` (24 bytes) or a three-Box `Clamp` (24 bytes)
    // made every length 32 bytes and `ComputedStyle` 3352. Every element owns
    // one, so a 100k-node page carried ~335 MB of style — and the cascade
    // recurses with several of them live per frame, which is what limited
    // nesting depth. Boxing the rare shapes costs one allocation on the few
    // lengths that use them and takes the common ones to 16 bytes.
    /// `calc()` — linear combination [percent, px, em, rem, vw, vh].
    Calc(Box<[f32; 6]>),
    /// `calc()` with non-linear parts (min/max nested inside calc).
    CalcExpr(Box<CalcNode>),
    /// `min()` — resolves to the smallest value.
    Min(Box<Vec<CssLength>>),
    /// `max()` — resolves to the largest value.
    Max(Box<Vec<CssLength>>),
    /// `clamp(min, val, max)` — resolves to val clamped between min and max.
    Clamp(Box<[CssLength; 3]>),
    Auto,
    Zero,
    None,
}

/// Expression node for calc() with nested min/max/clamp.
#[derive(Clone, Debug, PartialEq)]
pub enum CalcNode {
    Value(CssLength),
    Add(Box<CalcNode>, Box<CalcNode>),
    Sub(Box<CalcNode>, Box<CalcNode>),
    Mul(Box<CalcNode>, f32),
    Div(Box<CalcNode>, f32),
}

impl CalcNode {
    pub fn resolve_vp(&self, parent_font_px: f32, containing_px: f32, root_font_px: f32, vw: f32, vh: f32) -> f32 {
        match self {
            CalcNode::Value(v) => v.resolve_vp(parent_font_px, containing_px, root_font_px, vw, vh),
            CalcNode::Add(a, b) => a.resolve_vp(parent_font_px, containing_px, root_font_px, vw, vh)
                                 + b.resolve_vp(parent_font_px, containing_px, root_font_px, vw, vh),
            CalcNode::Sub(a, b) => a.resolve_vp(parent_font_px, containing_px, root_font_px, vw, vh)
                                 - b.resolve_vp(parent_font_px, containing_px, root_font_px, vw, vh),
            CalcNode::Mul(a, f) => a.resolve_vp(parent_font_px, containing_px, root_font_px, vw, vh) * f,
            CalcNode::Div(a, f) => if *f != 0.0 { a.resolve_vp(parent_font_px, containing_px, root_font_px, vw, vh) / f } else { 0.0 },
        }
    }
}

impl Default for CssLength {
    fn default() -> Self { Self::Auto }
}

impl CssLength {
    pub fn resolve(&self, parent_font_px: f32, containing_px: f32, root_font_px: f32) -> f32 {
        self.resolve_vp(parent_font_px, containing_px, root_font_px, 0.0, 0.0)
    }

    /// Resolve with explicit viewport dimensions for `vw`/`vh`.
    pub fn resolve_vp(
        &self,
        parent_font_px: f32,
        containing_px:  f32,
        root_font_px:   f32,
        viewport_w:     f32,
        viewport_h:     f32,
    ) -> f32 {
        match self {
            CssLength::Px(v)      => *v,
            CssLength::Em(v)      => v * parent_font_px,
            CssLength::Rem(v)     => v * root_font_px,
            CssLength::Percent(v) => v / 100.0 * containing_px,
            CssLength::Vw(v)      => v / 100.0 * viewport_w,
            CssLength::Vh(v)      => v / 100.0 * viewport_h,
            CssLength::Calc(c) =>
                c[0] / 100.0 * containing_px + c[1] + c[2] * parent_font_px
                + c[3] * root_font_px + c[4] / 100.0 * viewport_w + c[5] / 100.0 * viewport_h,
            CssLength::CalcExpr(node) =>
                node.resolve_vp(parent_font_px, containing_px, root_font_px, viewport_w, viewport_h),
            CssLength::Min(vals) => vals.iter()
                .map(|v| v.resolve_vp(parent_font_px, containing_px, root_font_px, viewport_w, viewport_h))
                .fold(f32::INFINITY, f32::min),
            CssLength::Max(vals) => vals.iter()
                .map(|v| v.resolve_vp(parent_font_px, containing_px, root_font_px, viewport_w, viewport_h))
                .fold(f32::NEG_INFINITY, f32::max),
            CssLength::Clamp(parts) => {
                let (min, val, max) = (&parts[0], &parts[1], &parts[2]);
                let min_v = min.resolve_vp(parent_font_px, containing_px, root_font_px, viewport_w, viewport_h);
                let val_v = val.resolve_vp(parent_font_px, containing_px, root_font_px, viewport_w, viewport_h);
                let max_v = max.resolve_vp(parent_font_px, containing_px, root_font_px, viewport_w, viewport_h);
                val_v.max(min_v).min(max_v)
            }
            CssLength::Auto       => 0.0,
            CssLength::Zero       => 0.0,
            CssLength::None       => 0.0,
        }
    }

    pub fn is_auto(&self) -> bool { matches!(self, CssLength::Auto) }
    pub fn is_none(&self) -> bool { matches!(self, CssLength::None) }
}

// ─── CSS Enums ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Display {
    None,
    Block,
    Inline,
    InlineBlock,
    Flex,
    InlineFlex,
    Grid,
    InlineGrid,
    Table,
    TableRow,
    TableCell,
    TableHeaderCell,
    TableRowGroup,
    TableHeaderGroup,
    TableFooterGroup,
    TableColumnGroup,
    TableColumn,
    TableCaption,
    ListItem,
    Ruby,
    RubyText,
    FlowRoot,
    Contents,
}

impl Default for Display {
    fn default() -> Self { Self::Inline }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

impl Default for Position {
    fn default() -> Self { Self::Static }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Float {
    None,
    Left,
    Right,
}

impl Default for Float {
    fn default() -> Self { Self::None }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Clear {
    None,
    Left,
    Right,
    Both,
}

impl Default for Clear {
    fn default() -> Self { Self::None }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    LTR,
    RTL,
}

impl Default for Direction {
    fn default() -> Self { Self::LTR }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BorderStyle {
    None,
    Hidden,
    Solid,
    Dashed,
    Dotted,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

impl Default for BorderStyle {
    fn default() -> Self { Self::None }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Overflow {
    Visible,
    Hidden,
    Scroll,
    Auto,
}

impl Default for Overflow {
    fn default() -> Self { Self::Visible }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WhiteSpace {
    Normal,
    Nowrap,
    Pre,
    PreWrap,
    PreLine,
}

impl Default for WhiteSpace {
    fn default() -> Self { Self::Normal }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WordBreak {
    Normal,
    BreakAll,
    KeepAll,
    BreakWord,
}

impl Default for WordBreak {
    fn default() -> Self { Self::Normal }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OverflowWrap {
    Normal,
    BreakWord,
    Anywhere,
}


impl Default for OverflowWrap {
    fn default() -> Self { Self::Normal }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FontWeight {
    Normal,
    Bold,
    Value(u16),
}

impl Default for FontWeight {
    fn default() -> Self { Self::Normal }
}

impl FontWeight {
    pub fn value(&self) -> u16 {
        match self {
            FontWeight::Normal => 400,
            FontWeight::Bold   => 700,
            FontWeight::Value(v) => *v,
        }
    }
    pub fn is_bold(&self) -> bool { self.value() >= 600 }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

impl Default for FontStyle {
    fn default() -> Self { Self::Normal }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
    Start,
    End,
}

impl Default for TextAlign {
    fn default() -> Self { Self::Left }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VerticalAlign {
    Baseline,
    Top,
    Middle,
    Bottom,
    TextTop,
    TextBottom,
    Sub,
    Super,
}

impl Default for VerticalAlign {
    fn default() -> Self { Self::Baseline }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextTransform {
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

impl Default for TextTransform {
    fn default() -> Self { Self::None }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct TextDecoration {
    pub underline: bool,
    pub overline: bool,
    pub strikethrough: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListStyleType {
    None,
    Disc,
    Circle,
    Square,
    Decimal,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
    Disclosure,
}

impl Default for ListStyleType {
    fn default() -> Self { Self::None }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListStylePosition {
    Outside,
    Inside,
}

impl Default for ListStylePosition {
    fn default() -> Self { Self::Outside }
}

// ─── Flex / Grid ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl Default for FlexDirection {
    fn default() -> Self { Self::Row }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlexWrap {
    Nowrap,
    Wrap,
    WrapReverse,
}

impl Default for FlexWrap {
    fn default() -> Self { Self::Nowrap }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AlignItems {
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

impl Default for AlignItems {
    fn default() -> Self { Self::Stretch }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AlignSelf {
    Auto,
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

impl Default for AlignSelf {
    fn default() -> Self { Self::Auto }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JustifyContent {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl Default for JustifyContent {
    fn default() -> Self { Self::FlexStart }
}

// ─── New types for BoxSizing, AlignContent, Grid ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoxSizing { ContentBox, BorderBox }
impl Default for BoxSizing { fn default() -> Self { Self::ContentBox } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AlignContent {
    Stretch, FlexStart, FlexEnd, Center, SpaceBetween, SpaceAround, SpaceEvenly,
}
impl Default for AlignContent { fn default() -> Self { Self::Stretch } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridAutoFlow { Row, RowDense, Column, ColumnDense }
impl Default for GridAutoFlow { fn default() -> Self { Self::Row } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridTrackKind {
    Fixed, Percent, Fractional, Auto, MinMax, MinContent, MaxContent, FitContent,
    Subgrid, Calc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridTrackSize {
    pub kind:      GridTrackKind,
    pub value:     f32,
    pub min_kind:  GridTrackKind,
    pub min_value: f32,
    pub max_kind:  GridTrackKind,
    pub max_value: f32,
    /// For `Calc` kind: the full CssLength for deferred resolution.
    pub calc_length: Option<CssLength>,
}

impl Default for GridTrackSize {
    fn default() -> Self {
        Self {
            kind: GridTrackKind::Auto, value: 0.0,
            min_kind: GridTrackKind::Auto, min_value: 0.0,
            max_kind: GridTrackKind::Auto, max_value: 0.0,
            calc_length: None,
        }
    }
}

impl GridTrackSize {
    pub fn fixed(px: f32) -> Self { Self { kind: GridTrackKind::Fixed, value: px, ..Default::default() } }
    pub fn percent(pct: f32) -> Self { Self { kind: GridTrackKind::Percent, value: pct, ..Default::default() } }
    pub fn fr(fr: f32) -> Self { Self { kind: GridTrackKind::Fractional, value: fr, ..Default::default() } }
    pub fn auto() -> Self { Self::default() }
    pub fn subgrid() -> Self { Self { kind: GridTrackKind::Subgrid, ..Default::default() } }
    pub fn is_auto(&self) -> bool { self.kind == GridTrackKind::Auto }
    pub fn is_none(&self) -> bool { self.kind == GridTrackKind::Auto && self.value == 0.0 }
    pub fn is_subgrid(&self) -> bool { self.kind == GridTrackKind::Subgrid }
}

// ─── CSS Transform ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct CssTransform {
    pub ops: Vec<TransformOp>,
}

#[derive(Clone, Debug)]
pub enum TransformOp {
    Translate(f32, f32),
    TranslateX(f32),
    TranslateY(f32),
    Scale(f32, f32),
    ScaleX(f32),
    ScaleY(f32),
    Rotate(f32),   // degrees
    SkewX(f32),   // degrees
    SkewY(f32),   // degrees
    Matrix(f32, f32, f32, f32, f32, f32),  // a b c d e f
}

// ─── CSS Filter ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct CssFilters {
    pub ops: Vec<FilterOp>,
}

#[derive(Clone, Debug)]
pub enum FilterOp {
    Blur(f32),
    Brightness(f32),
    Contrast(f32),
    Grayscale(f32),
    HueRotate(f32),
    Invert(f32),
    Opacity(f32),
    Saturate(f32),
    Sepia(f32),
    DropShadow { dx: f32, dy: f32, blur: f32, color: Color },
}

// ─── Computed Style ───────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ComputedStyle {
    // Display & layout model
    pub display:    Display,
    pub position:   Position,
    pub float:      Float,
    pub clear:      Clear,
    pub z_index:    i32,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,

    // Box model
    pub box_sizing: BoxSizing,
    pub width:      CssLength,
    pub height:     CssLength,
    pub min_width:  CssLength,
    pub max_width:  CssLength,
    pub min_height: CssLength,
    pub max_height: CssLength,

    pub margin_top:    CssLength,
    pub margin_right:  CssLength,
    pub margin_bottom: CssLength,
    pub margin_left:   CssLength,

    pub padding_top:    CssLength,
    pub padding_right:  CssLength,
    pub padding_bottom: CssLength,
    pub padding_left:   CssLength,

    pub border_top_width:    CssLength,
    pub border_right_width:  CssLength,
    pub border_bottom_width: CssLength,
    pub border_left_width:   CssLength,

    pub border_top_style:    BorderStyle,
    pub border_right_style:  BorderStyle,
    pub border_bottom_style: BorderStyle,
    pub border_left_style:   BorderStyle,

    pub border_top_color:    Color,
    pub border_right_color:  Color,
    pub border_bottom_color: Color,
    pub border_left_color:   Color,

    pub border_radius: CssLength,

    // Positioning
    pub top:    CssLength,
    pub right:  CssLength,
    pub bottom: CssLength,
    pub left:   CssLength,

    // Typography
    pub color:             Color,
    pub background_color:  Color,
    pub font_family:              String,
    pub font_size:                CssLength,
    pub font_weight:              FontWeight,
    pub font_style:               FontStyle,
    /// Parsed `font-variation-settings` axes, e.g. `[("wght", 700.0), ("wdth", 75.0)]`.
    pub font_variation_settings:  Vec<(String, f32)>,
    /// Parsed `font-feature-settings` tags, e.g. `[("kern", 1), ("liga", 0)]`.
    pub font_feature_settings:    Vec<(String, u32)>,
    pub line_height:              CssLength,
    pub letter_spacing:    CssLength,
    pub word_spacing:      CssLength,
    pub text_align:        TextAlign,
    pub vertical_align:    VerticalAlign,
    pub text_decoration:   TextDecoration,
    pub text_indent:       CssLength,
    pub white_space:       WhiteSpace,
    pub text_transform:    TextTransform,
    pub word_break:        WordBreak,
    pub overflow_wrap:     OverflowWrap,
    pub direction:         Direction,

    // List
    pub list_style_type:     ListStyleType,
    pub list_style_position: ListStylePosition,
    pub list_index:          i32,

    // Flexbox
    pub flex_direction:  FlexDirection,
    pub flex_wrap:       FlexWrap,
    pub justify_content: JustifyContent,
    pub align_items:     AlignItems,
    pub align_self:      AlignSelf,
    pub align_content:   AlignContent,
    pub flex_grow:       f32,
    pub flex_shrink:     f32,
    pub flex_basis:      CssLength,
    pub order:           i32,
    pub gap:             CssLength,
    pub row_gap:         CssLength,
    pub column_gap:      CssLength,

    // Grid
    pub grid_template_columns: Vec<GridTrackSize>,
    pub grid_template_rows:    Vec<GridTrackSize>,
    pub grid_auto_columns:     GridTrackSize,
    pub grid_auto_rows:        GridTrackSize,
    pub grid_template_areas:   Vec<Vec<String>>,
    pub auto_repeat_columns:   Vec<GridTrackSize>,
    /// Named grid column lines: name → list of 0-based line indices
    pub grid_col_line_names:   std::collections::HashMap<String, Vec<usize>>,
    /// Named grid row lines: name → list of 0-based line indices
    pub grid_row_line_names:   std::collections::HashMap<String, Vec<usize>>,
    pub subgrid_columns:       bool,
    pub subgrid_rows:          bool,
    pub grid_column_start:     i32,  // 0=auto, >0=line number (1-based), <0=span, <=-10000=span(encoded)
    pub grid_column_end:       i32,
    pub grid_row_start:        i32,
    pub grid_row_end:          i32,
    pub grid_column_start_name: String,  // Named line reference (e.g. "content-start")
    pub grid_column_end_name:   String,
    pub grid_row_start_name:    String,
    pub grid_row_end_name:      String,
    pub grid_area:             String,
    pub grid_auto_flow:        GridAutoFlow,
    pub justify_items:         AlignItems,
    pub justify_self:          AlignSelf,

    // Visual
    pub opacity:          f32,
    pub visibility:       bool,   // true = visible
    pub box_shadow:       Option<BoxShadow>,

    // Gradient background
    pub gradient_type:  GradientType,
    pub gradient_angle: f32,
    pub gradient_stops: Vec<GradientStop>,

    // Background image / position / repeat / size
    pub background_size:       BackgroundSize,
    pub background_size_w:     CssLength,
    pub background_size_h:     CssLength,
    pub background_position_x: CssLength,
    pub background_position_y: CssLength,
    pub background_repeat:     BackgroundRepeat,

    // Outline
    pub outline_width:  f32,
    pub outline_style:  BorderStyle,
    pub outline_color:  Color,
    pub outline_offset: f32,

    // Text effects
    pub text_overflow: TextOverflow,
    pub text_shadow:   Option<TextShadow>,
    pub small_caps:    bool,

    // Object fit for replaced elements
    pub object_fit: ObjectFit,

    // Pseudo-element content and style
    pub before_content: String,
    pub after_content:  String,
    /// Full computed style for ::before (inherits from element, has its own declarations applied).
    pub before_style:    Option<Box<ComputedStyle>>,
    /// Full computed style for ::after.
    pub after_style:     Option<Box<ComputedStyle>>,
    /// Style for ::selection (background-color / color for selected text).
    pub selection_style: Option<Box<ComputedStyle>>,
    /// Style for ::marker (color / font / content for list markers).
    pub marker_style:    Option<Box<ComputedStyle>>,

    // Caret and scrollbar theming
    pub caret_color:           Option<Color>,
    pub scrollbar_thumb_color: Option<Color>,
    pub scrollbar_track_color: Option<Color>,

    // Table extras
    pub border_collapse:    bool,
    pub border_spacing_h:   CssLength,
    pub border_spacing_v:   CssLength,
    pub caption_side:       CaptionSide,
    pub empty_cells_hide:   bool,
    pub table_layout_fixed: bool,
    pub cell_padding:       CssLength,

    // Per-corner border radius
    pub border_top_left_radius:     CssLength,
    pub border_top_right_radius:    CssLength,
    pub border_bottom_left_radius:  CssLength,
    pub border_bottom_right_radius: CssLength,

    // Background image URL
    pub background_image_url: String,

    // CSS mask-image URL (used for icon masking)
    pub mask_image_url: String,

    // Bidi & writing
    pub unicode_bidi: UnicodeBidi,
    pub writing_mode: WritingMode,

    // Cursor
    pub cursor: CSSCursor,

    // Page breaks
    pub break_before: BreakValue,
    pub break_after:  BreakValue,
    pub break_inside: BreakInside,

    // Text extras
    pub tab_size: i32,
    pub hyphens:  Hyphens,
    pub widows:   i32,
    pub orphans:  i32,

    // Clip-path
    pub clip_path: ClipPath,
    // Legacy clip: rect(top, right, bottom, left) — absolute offsets from the element's border box.
    // Stored as [top, right, bottom, left] in px. Only applies to position:absolute/fixed.
    pub clip_rect: Option<[f32; 4]>,

    // Pointer events
    pub pointer_events: PointerEvents,

    // Quotes
    pub quotes: Vec<String>,

    // Container queries
    pub container_name: String,
    pub container_type: ContainerType,

    // State styles — full computed-style overlays applied at render time (not layout time).
    // None means the element has no rule for that state.
    pub hover_style:   Option<Box<ComputedStyle>>,
    pub active_style:  Option<Box<ComputedStyle>>,
    pub visited_style: Option<Box<ComputedStyle>>,

    // List style image
    pub list_style_image: String,

    // Object position (for replaced elements / images)
    pub object_position_x: CssLength,
    pub object_position_y: CssLength,

    // Aspect ratio (width / height)
    pub aspect_ratio: Option<f32>,

    // Text decoration extras
    pub text_decoration_color: Option<Color>,
    pub text_decoration_style: TextDecorationStyle,
    pub text_decoration_thickness: CssLength,

    // Scroll snap
    pub scroll_snap_type:  ScrollSnapType,
    pub scroll_snap_align: ScrollSnapAlign,

    // Overscroll chaining
    pub overscroll_behavior_x: OverscrollBehavior,
    pub overscroll_behavior_y: OverscrollBehavior,

    // Containment
    pub contain_layout: bool,
    pub contain_paint:  bool,
    pub contain_size:   bool,

    // Will-change hints
    pub will_change_transform: bool,

    // Scroll padding
    pub scroll_padding_top:    CssLength,
    pub scroll_padding_right:  CssLength,
    pub scroll_padding_bottom: CssLength,
    pub scroll_padding_left:   CssLength,

    // User interaction
    pub user_select: UserSelect,
    pub resize: Resize,

    // Background extras
    pub background_clip:       BackgroundClip,
    pub background_origin:     BackgroundClip,
    pub background_attachment: BackgroundAttachment,

    // Multi-column layout
    pub column_count:      Option<i32>,
    pub column_width:      CssLength,
    pub column_rule_width: CssLength,
    pub column_rule_style: BorderStyle,
    pub column_rule_color: Color,
    pub column_fill:       bool,  // true = balance, false = auto
    pub column_span_all:   bool,  // true = span all columns

    // Transform / filter (stored raw; actual matrix math not implemented)
    pub transform:        String,
    pub filter:           String,
    pub backdrop_filter:  String,

    // Parsed transform / filter (for rendering)
    pub css_transform:        CssTransform,
    pub transform_origin_x:   f32,   // 0.0..1.0, default 0.5
    pub transform_origin_y:   f32,   // 0.0..1.0, default 0.5
    pub css_filter:           CssFilters,

    // Text underline offset
    pub text_underline_offset: CssLength,

    // Parsed CSS animations and transitions for this element
    pub animations:  Vec<ParsedAnimation>,
    pub transitions: Vec<ParsedTransition>,
    pub will_change: String,

    // Misc
    pub scroll_behavior: ScrollBehavior,
    pub isolation:       bool,   // true = isolate stacking context
    pub mix_blend_mode:  MixBlendMode,

    // Counters
    pub counter_reset:     Vec<(String, i32)>,
    pub counter_increment: Vec<(String, i32)>,

    // Font extras
    pub font_stretch: f32,   // percentage 100.0 = normal

    // Custom properties (CSS variables)
    pub custom_props: HashMap<String, String>,

    /// Link URL (inherited from `<a>` tags)
    pub href: String,
}

#[derive(Clone, Debug)]
pub struct BoxShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur:     f32,
    pub spread:   f32,
    pub color:    Color,
    pub inset:    bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientType { None, Linear, Radial }
impl Default for GradientType { fn default() -> Self { Self::None } }

#[derive(Clone, Debug)]
pub struct GradientStop {
    pub color:    Color,
    pub position: f32,  // 0.0..1.0
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextOverflow { Clip, Ellipsis }
impl Default for TextOverflow { fn default() -> Self { Self::Clip } }

#[derive(Clone, Debug)]
pub struct TextShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur:     f32,
    pub color:    Color,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackgroundSize { Auto, Cover, Contain, Explicit }
impl Default for BackgroundSize { fn default() -> Self { Self::Auto } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackgroundRepeat { Repeat, RepeatX, RepeatY, NoRepeat }
impl Default for BackgroundRepeat { fn default() -> Self { Self::Repeat } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ObjectFit { Fill, Contain, Cover, None, ScaleDown }
impl Default for ObjectFit { fn default() -> Self { Self::Fill } }

// ─── New CSS types ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UserSelect { Auto, None, Text, All, Contain }
impl Default for UserSelect { fn default() -> Self { Self::Auto } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Resize { None, Both, Horizontal, Vertical }
impl Default for Resize { fn default() -> Self { Self::None } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackgroundClip { BorderBox, PaddingBox, ContentBox, Text }
impl Default for BackgroundClip { fn default() -> Self { Self::BorderBox } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackgroundAttachment { Scroll, Fixed, Local }
impl Default for BackgroundAttachment { fn default() -> Self { Self::Scroll } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollBehavior { Auto, Smooth }
impl Default for ScrollBehavior { fn default() -> Self { Self::Auto } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextDecorationStyle { Solid, Double, Dotted, Dashed, Wavy }
impl Default for TextDecorationStyle { fn default() -> Self { Self::Solid } }

/// Scroll-container axis for `scroll-snap-type`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ScrollSnapAxis {
    #[default] None,
    X, Y, Both, Block, Inline,
}

/// Combined snap-type carrying both axis and strictness.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ScrollSnapType {
    pub axis:      ScrollSnapAxis,
    /// `true` = mandatory (always snap), `false` = proximity (snap only when close).
    pub mandatory: bool,
}

impl ScrollSnapType {
    pub fn none() -> Self { Self { axis: ScrollSnapAxis::None, mandatory: false } }
    pub fn snaps_y(self) -> bool {
        matches!(self.axis, ScrollSnapAxis::Y | ScrollSnapAxis::Both | ScrollSnapAxis::Block)
    }
    pub fn snaps_x(self) -> bool {
        matches!(self.axis, ScrollSnapAxis::X | ScrollSnapAxis::Both | ScrollSnapAxis::Inline)
    }
    pub fn is_enabled(self) -> bool { self.axis != ScrollSnapAxis::None }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ScrollSnapAlign { #[default] None, Start, End, Center }

/// Controls scroll chaining when a scroll container reaches its boundary.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum OverscrollBehavior {
    /// Default: propagate scroll to the parent scroll container.
    #[default] Auto,
    /// Don't chain — swallow the scroll delta at the boundary.
    Contain,
    /// Same as Contain for chaining (no bounce effects either).
    None,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MixBlendMode {
    Normal, Multiply, Screen, Overlay, Darken, Lighten,
    ColorDodge, ColorBurn, HardLight, SoftLight, Difference,
    Exclusion, Hue, Saturation, Color, Luminosity,
}
impl Default for MixBlendMode { fn default() -> Self { Self::Normal } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UnicodeBidi { Normal, Embed, Override, Isolate, IsolateOverride, Plaintext }
impl Default for UnicodeBidi { fn default() -> Self { Self::Normal } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CSSCursor {
    Auto, Default, Pointer, Text, Move, Crosshair, Wait, Help, NotAllowed,
    Grab, Grabbing, ColResize, RowResize,
    NResize, EResize, SResize, WResize, NEResize, NWResize, SEResize, SWResize, None,
}
impl Default for CSSCursor { fn default() -> Self { Self::Auto } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BreakValue { Auto, Always, Avoid, Left, Right }
impl Default for BreakValue { fn default() -> Self { Self::Auto } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BreakInside { Auto, Avoid }
impl Default for BreakInside { fn default() -> Self { Self::Auto } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WritingMode { HorizontalTB, VerticalRL, VerticalLR }
impl Default for WritingMode { fn default() -> Self { Self::HorizontalTB } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CaptionSide { Top, Bottom }
impl Default for CaptionSide { fn default() -> Self { Self::Top } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hyphens { None, Manual, Auto }
impl Default for Hyphens { fn default() -> Self { Self::Manual } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerEvents {
    Auto, None, VisiblePainted, VisibleFill, VisibleStroke, Visible, Painted, Fill, Stroke, All,
}
impl Default for PointerEvents { fn default() -> Self { Self::Auto } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContainerType { Normal, Size, InlineSize }
impl Default for ContainerType { fn default() -> Self { Self::Normal } }

#[derive(Clone, Debug, Default)]
pub struct ClipPath {
    pub kind: ClipPathKind,
    // inset(top right bottom left)
    pub inset_top: CssLength, pub inset_right: CssLength,
    pub inset_bottom: CssLength, pub inset_left: CssLength,
    // circle(r at cx cy) / ellipse(rx ry at cx cy)
    pub circle_radius: CssLength,
    pub ellipse_rx: CssLength, pub ellipse_ry: CssLength,
    pub center_x: CssLength, pub center_y: CssLength,
    // polygon points
    pub points: Vec<(CssLength, CssLength)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ClipPathKind { #[default] None, Inset, Circle, Ellipse, Polygon }

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display:    Display::Inline,
            position:   Position::Static,
            float:      Float::None,
            clear:      Clear::None,
            z_index:    0,
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,

            box_sizing: BoxSizing::ContentBox,
            width:      CssLength::Auto,
            height:     CssLength::Auto,
            min_width:  CssLength::Zero,
            max_width:  CssLength::None,
            min_height: CssLength::Auto,
            max_height: CssLength::None,

            margin_top:    CssLength::Zero,
            margin_right:  CssLength::Zero,
            margin_bottom: CssLength::Zero,
            margin_left:   CssLength::Zero,

            padding_top:    CssLength::Zero,
            padding_right:  CssLength::Zero,
            padding_bottom: CssLength::Zero,
            padding_left:   CssLength::Zero,

            border_top_width:    CssLength::Px(0.0),
            border_right_width:  CssLength::Px(0.0),
            border_bottom_width: CssLength::Px(0.0),
            border_left_width:   CssLength::Px(0.0),

            border_top_style:    BorderStyle::None,
            border_right_style:  BorderStyle::None,
            border_bottom_style: BorderStyle::None,
            border_left_style:   BorderStyle::None,

            border_top_color:    Color::BLACK,
            border_right_color:  Color::BLACK,
            border_bottom_color: Color::BLACK,
            border_left_color:   Color::BLACK,

            border_radius: CssLength::Zero,

            top:    CssLength::Auto,
            right:  CssLength::Auto,
            bottom: CssLength::Auto,
            left:   CssLength::Auto,

            color:            Color::BLACK,
            background_color: Color::TRANSPARENT,
            font_family:             String::from("sans-serif"),
            font_size:               CssLength::Px(16.0),
            font_weight:             FontWeight::Normal,
            font_style:              FontStyle::Normal,
            font_variation_settings: Vec::new(),
            font_feature_settings:   Vec::new(),
            line_height:             CssLength::Em(1.2),
            letter_spacing:   CssLength::Zero,
            word_spacing:     CssLength::Zero,
            text_align:       TextAlign::Start,
            vertical_align:   VerticalAlign::Baseline,
            text_decoration:  TextDecoration::default(),
            text_indent:      CssLength::Zero,
            white_space:      WhiteSpace::Normal,
            text_transform:   TextTransform::None,
            word_break:       WordBreak::Normal,
            overflow_wrap:    OverflowWrap::Normal,
            direction:        Direction::LTR,

            list_style_type:     ListStyleType::None,
            list_style_position: ListStylePosition::Outside,
            list_index:          0,

            flex_direction:  FlexDirection::Row,
            flex_wrap:       FlexWrap::Nowrap,
            justify_content: JustifyContent::FlexStart,
            align_items:     AlignItems::Stretch,
            align_self:      AlignSelf::Auto,
            align_content:   AlignContent::Stretch,
            flex_grow:       0.0,
            flex_shrink:     1.0,
            flex_basis:      CssLength::Auto,
            order:           0,
            gap:             CssLength::Zero,
            row_gap:         CssLength::Zero,
            column_gap:      CssLength::Zero,

            grid_template_columns: Vec::new(),
            grid_template_rows:    Vec::new(),
            grid_auto_columns:     GridTrackSize::default(),
            grid_auto_rows:        GridTrackSize::default(),
            grid_template_areas:   Vec::new(),
            auto_repeat_columns:   Vec::new(),
            grid_col_line_names:   std::collections::HashMap::new(),
            grid_row_line_names:   std::collections::HashMap::new(),
            subgrid_columns:       false,
            subgrid_rows:          false,
            grid_column_start:     0,
            grid_column_end:       0,
            grid_row_start:        0,
            grid_row_end:          0,
            grid_column_start_name: String::new(),
            grid_column_end_name:   String::new(),
            grid_row_start_name:    String::new(),
            grid_row_end_name:      String::new(),
            grid_area:             String::new(),
            grid_auto_flow:        GridAutoFlow::Row,
            justify_items:         AlignItems::Stretch,
            justify_self:          AlignSelf::Auto,

            opacity:    1.0,
            visibility: true,
            box_shadow: None,

            gradient_type:  GradientType::None,
            gradient_angle: 180.0,
            gradient_stops: Vec::new(),

            background_size:       BackgroundSize::Auto,
            background_size_w:     CssLength::Auto,
            background_size_h:     CssLength::Auto,
            background_position_x: CssLength::Zero,
            background_position_y: CssLength::Zero,
            background_repeat:     BackgroundRepeat::Repeat,

            outline_width:  0.0,
            outline_style:  BorderStyle::None,
            outline_color:  Color::BLACK,
            outline_offset: 0.0,

            text_overflow: TextOverflow::Clip,
            text_shadow:   None,
            small_caps:    false,

            object_fit: ObjectFit::Fill,

            before_content: String::new(),
            after_content:  String::new(),
            before_style:    None,
            after_style:     None,
            selection_style: None,
            marker_style:    None,

            caret_color:           None,
            scrollbar_thumb_color: None,
            scrollbar_track_color: None,

            border_collapse:    false,
            border_spacing_h:   CssLength::Zero,
            border_spacing_v:   CssLength::Zero,
            caption_side:       CaptionSide::Top,
            empty_cells_hide:   false,
            table_layout_fixed: false,
            cell_padding:       CssLength::Auto,

            border_top_left_radius:     CssLength::Zero,
            border_top_right_radius:    CssLength::Zero,
            border_bottom_left_radius:  CssLength::Zero,
            border_bottom_right_radius: CssLength::Zero,

            background_image_url: String::new(),
            mask_image_url: String::new(),

            unicode_bidi: UnicodeBidi::Normal,
            writing_mode: WritingMode::HorizontalTB,

            cursor: CSSCursor::Auto,

            break_before: BreakValue::Auto,
            break_after:  BreakValue::Auto,
            break_inside: BreakInside::Auto,

            tab_size: 8,
            hyphens:  Hyphens::Manual,
            widows:   2,
            orphans:  2,

            clip_path: ClipPath::default(),
            clip_rect: None,

            pointer_events: PointerEvents::Auto,

            quotes: Vec::new(),

            container_name: String::new(),
            container_type: ContainerType::Normal,

            hover_style:   None,
            active_style:  None,
            visited_style: None,

            list_style_image: String::new(),

            object_position_x: CssLength::Percent(50.0),
            object_position_y: CssLength::Percent(50.0),

            aspect_ratio: None,

            text_decoration_color: None,
            text_decoration_style: TextDecorationStyle::Solid,
            text_decoration_thickness: CssLength::Auto,

            scroll_snap_type:  ScrollSnapType::none(),
            scroll_snap_align: ScrollSnapAlign::None,
            overscroll_behavior_x: OverscrollBehavior::Auto,
            overscroll_behavior_y: OverscrollBehavior::Auto,

            contain_layout: false,
            contain_paint:  false,
            contain_size:   false,

            will_change_transform: false,

            scroll_padding_top:    CssLength::Zero,
            scroll_padding_right:  CssLength::Zero,
            scroll_padding_bottom: CssLength::Zero,
            scroll_padding_left:   CssLength::Zero,

            user_select: UserSelect::Auto,
            resize:      Resize::None,

            background_clip:       BackgroundClip::BorderBox,
            background_origin:     BackgroundClip::PaddingBox,
            background_attachment: BackgroundAttachment::Scroll,

            column_count:      None,
            column_width:      CssLength::Auto,
            column_rule_width: CssLength::Px(0.0),
            column_rule_style: BorderStyle::None,
            column_rule_color: Color::BLACK,
            column_fill:       true,
            column_span_all:   false,

            transform:       String::new(),
            filter:          String::new(),
            backdrop_filter: String::new(),

            css_transform:      CssTransform::default(),
            transform_origin_x: 0.5,
            transform_origin_y: 0.5,
            css_filter:         CssFilters::default(),

            text_underline_offset: CssLength::Auto,

            animations:  Vec::new(),
            transitions: Vec::new(),
            will_change: String::new(),

            scroll_behavior: ScrollBehavior::Auto,
            isolation:       false,
            mix_blend_mode:  MixBlendMode::Normal,

            counter_reset:     Vec::new(),
            counter_increment: Vec::new(),

            font_stretch: 100.0,

            custom_props: HashMap::new(),

            href: String::new(),
        }
    }
}

impl ComputedStyle {
    /// Inherit inheritable properties from a parent style.
    /// Copy inherited properties from parent using the property table.
    pub fn inherit_from(&mut self, parent: &ComputedStyle) {
        for &id in crate::css::property_defs::INHERITED_IDS {
            let def = crate::css::property_defs::get(id);
            (def.copy)(self, parent);
        }
    }

    pub fn is_block_level(&self) -> bool {
        matches!(self.display, Display::Block | Display::Flex | Display::Grid
            | Display::Table | Display::ListItem | Display::FlowRoot)
    }

    pub fn is_inline_level(&self) -> bool {
        matches!(self.display, Display::Inline | Display::InlineBlock
            | Display::InlineFlex | Display::InlineGrid | Display::Ruby | Display::RubyText)
    }

    pub fn is_positioned(&self) -> bool {
        !matches!(self.position, Position::Static)
    }

    pub fn establishes_bfc(&self) -> bool {
        self.is_positioned()
            || !matches!(self.float, Float::None)
            || matches!(self.overflow_x, Overflow::Hidden | Overflow::Scroll | Overflow::Auto)
            || matches!(self.overflow_y, Overflow::Hidden | Overflow::Scroll | Overflow::Auto)
            || matches!(self.display, Display::Flex | Display::Grid | Display::InlineFlex
                | Display::InlineGrid | Display::InlineBlock | Display::Table)
    }

    /// Resolved font size in px (needs parent px for em/%, root px for rem).
    pub fn font_size_px(&self, parent_px: f32, root_px: f32) -> f32 {
        // For font-size, `%` is relative to the parent font size, not the containing block width.
        // Pass parent_px as both the em base and the percentage base.
        self.font_size.resolve(parent_px, parent_px, root_px).max(1.0)
    }
}

// ─── Visual Segment (BiDi) ───────────────────────────────────────────────────

/// One visual run within a line after BiDi reordering.
/// Stored in visual order (left-to-right screen position).
#[derive(Clone, Debug, Default)]
pub struct VisualSegment {
    /// Byte offset in the full flat text string.
    pub logical_start: usize,
    /// Byte length of this segment.
    pub length: usize,
    /// BiDi embedding level (odd = RTL, even = LTR).
    pub level: u8,
    /// X position filled by renderer after measuring all prior segments.
    pub x: f32,
    /// Width filled by renderer.
    pub width: f32,
}

// ─── Inline Run ───────────────────────────────────────────────────────────────

/// A styled run of text within a box's text content.
#[derive(Clone, Debug)]
pub struct InlineRun {
    pub text_offset: usize,
    pub length:      usize,
    pub style:       ComputedStyle,
}

// ─── Layout Line ──────────────────────────────────────────────────────────────

/// Result of line-breaking for a line in inline content.
#[derive(Clone, Debug, Default)]
pub struct LayoutLine {
    pub text_start:  usize,
    pub text_length: usize,
    pub x:     f32,
    pub y:     f32,
    pub width: f32,
    pub height:  f32,
    pub ascent:  f32,
    pub descent: f32,
    pub extra_space_per_word: f32,  // for text-align: justify
    /// X offset from `self.x` where text content actually starts.
    /// Non-zero when atomic inline items (e.g. checkbox, image) precede text on this line.
    pub text_x_offset: f32,
    /// BiDi visual segments in visual order. Empty = pure LTR, use logical order.
    pub visual_segments: Vec<VisualSegment>,
    /// Per-character-boundary x positions relative to `self.x + text_x_offset`, in logical pixels.
    pub char_x: Vec<f32>,
}

// ─── Layout Box (geometry computed by the layout pass) ───────────────────────

/// Layout-only data for a box. Separated from DOM data so that each pipeline
/// stage owns its own data — better cache behavior, independent invalidation,
/// and the ability to have multiple layout views from one DOM.
#[derive(Clone, Debug)]
pub struct LayoutBox {
    // Box model geometry (set by layout pass)
    pub content_rect: Rect,
    pub padding_rect: Rect,
    pub border_rect:  Rect,
    pub margin_rect:  Rect,
    pub baseline:     f32,

    // Cached line breaks for inline content
    pub line_cache: Vec<LayoutLine>,

    // Inline runs (set by CSS cascade pass)
    pub inline_runs: Vec<InlineRun>,

    // Collapsed margin pass-through (set by block layout)
    pub collapsed_margin_top:    f32,
    pub collapsed_margin_bottom: f32,

    // Scroll extent (set by layout)
    pub scroll_height: f32,
    pub scroll_width:  f32,
    pub scroll_top:    f32,
    pub scroll_left:   f32,

    /// Static y position for absolutely positioned elements (set during parent layout).
    pub abs_static_y: Option<f32>,

    // Dirty flags for incremental layout
    pub layout_dirty:          bool,
    /// Intrinsic sizes need recomputation (propagates up to auto-width parents).
    pub intrinsic_dirty:       bool,
    /// Paint-only change (color/background) — skip layout, just repaint.
    pub paint_dirty:           bool,
    pub last_containing_width: f32,

    // Resolved box-model cache (set by layout, read by parent layout)
    pub resolved_margin_top:    f32,
    pub resolved_margin_right:  f32,
    pub resolved_margin_bottom: f32,
    pub resolved_margin_left:   f32,
    pub resolved_border_top:    f32,
    pub resolved_border_right:  f32,
    pub resolved_border_bottom: f32,
    pub resolved_border_left:   f32,
    pub resolved_pad_top:       f32,
    pub resolved_pad_right:     f32,
    pub resolved_pad_bottom:    f32,
    pub resolved_pad_left:      f32,
    pub resolved_content_width: f32,

    /// Cached intrinsic (max-content) width — `NAN` means not yet computed.
    pub cached_intrinsic_w: std::cell::Cell<f32>,
}

impl Default for LayoutBox {
    fn default() -> Self {
        Self {
            content_rect: Rect::default(),
            padding_rect: Rect::default(),
            border_rect:  Rect::default(),
            margin_rect:  Rect::default(),
            baseline:     0.0,
            line_cache:   Vec::new(),
            inline_runs:  Vec::new(),
            collapsed_margin_top:    0.0,
            collapsed_margin_bottom: 0.0,
            scroll_height: 0.0,
            scroll_width:  0.0,
            scroll_top:    0.0,
            scroll_left:   0.0,
            abs_static_y:  None,
            layout_dirty:          false,
            intrinsic_dirty:       false,
            paint_dirty:           false,
            last_containing_width: 0.0,
            resolved_margin_top:    0.0,
            resolved_margin_right:  0.0,
            resolved_margin_bottom: 0.0,
            resolved_margin_left:   0.0,
            resolved_border_top:    0.0,
            resolved_border_right:  0.0,
            resolved_border_bottom: 0.0,
            resolved_border_left:   0.0,
            resolved_pad_top:       0.0,
            resolved_pad_right:     0.0,
            resolved_pad_bottom:    0.0,
            resolved_pad_left:      0.0,
            resolved_content_width: 0.0,
            cached_intrinsic_w: std::cell::Cell::new(f32::NAN),
        }
    }
}

// ─── HTML Box (DOM node) ─────────────────────────────────────────────────────

/// A box/node in the box tree.  Mirrors the C++ `Box` struct.
#[derive(Clone, Debug)]
pub struct WebCore {
    pub tag:        String,
    pub node_id:    u32,                // Stable identity — index into Document.nodes
    pub style:      ComputedStyle,
    pub attributes: HashMap<String, String>,
    pub text:       String,             // Own text content

    // ── Tree structure (linked-list children, O(1) insert/remove) ────────
    pub parent:       u32,              // 0 = no parent (root)
    pub first_child:  u32,              // 0 = no children
    pub last_child:   u32,              // 0 = no children
    pub next_sibling: u32,              // 0 = last child
    pub prev_sibling: u32,              // 0 = first child

    // DEPRECATED: Vec storage kept during migration. Will be removed.
    pub children:   Vec<WebCore>,

    /// Layout geometry — all layout-computed fields live here.
    pub layout: LayoutBox,

    // Custom component cached dimensions (set once by measure(), stable across relayouts).
    // Like replaced elements, components control their own size — the engine only
    // re-measures when the component is explicitly marked dirty.
    pub component_width:  f32,
    pub component_height: f32,

    // Image pixel data for <img> and replaced elements (RGBA8, row-major)
    /// Absolute URL this element's image was resolved to.
    ///
    /// A FIELD and not an attribute. It used to be stored as `_resolved_src` in
    /// `attributes`, which put an invented attribute on the WHATWG surface:
    /// `img.attributes` listed it, `getAttributeNames()` returned it, and it
    /// was serialized into the markup. Internal state may look however it
    /// likes, but not by pretending to be a content attribute.
    pub resolved_src: String,
    pub image_data:   Option<Vec<u8>>,
    pub image_width:  u32,
    pub image_height: u32,

    // Background image pixel data (RGBA8, row-major)
    pub bg_image_data:   Option<Vec<u8>>,
    pub bg_image_width:  u32,
    pub bg_image_height: u32,

    // CSS mask-image data (SVG rasterized to alpha mask)
    pub mask_image_data:   Option<Vec<u8>>,
    pub mask_image_width:  u32,
    pub mask_image_height: u32,

    // SVG source markup (for round-trip and re-rasterization)
    pub svg_markup: Option<String>,
    /// SVG viewBox intrinsic dimensions (width, height). Used for aspect ratio
    /// sizing in layout and on-demand rasterization at the correct display size.
    pub svg_viewbox_w: f32,
    pub svg_viewbox_h: f32,

    // ── Form input editing state ─────────────────────────────────────────
    /// **Checkedness** — whether the box is ticked RIGHT NOW.
    ///
    /// HTML §4.10.5.3 keeps this apart from the `checked` CONTENT ATTRIBUTE,
    /// which is `defaultChecked` — the value a form reset restores to. They
    /// start equal and diverge the moment anything ticks the box: a user
    /// clicking a checkbox must NOT rewrite the document, and
    /// `getAttribute("checked")` must keep answering what the markup says.
    ///
    /// This used to BE the attribute (`attributes.contains_key("checked")`), so
    /// clicking a box edited the page's own markup and a program reading the
    /// attribute back got the user's last click instead of its own default.
    /// One store cannot answer both questions, which is also why the reset
    /// algorithm was impossible to write.
    pub checkedness: bool,
    /// The **dirty checkedness flag** (HTML §4.10.5.3). Raised by a user
    /// interaction or by setting the `checked` IDL member; while it is false
    /// the content attribute still drives checkedness.
    pub dirty_checked: bool,
    /// **Selectedness** of an `<option>` (HTML §4.10.10) — the same separation
    /// `checkedness` draws, for the same reason. The `selected` CONTENT
    /// ATTRIBUTE is `defaultSelected`, the state a form reset restores to; this
    /// is what is selected right now.
    ///
    /// Selection lived in the parent `<select>`'s `data["_selected_idx"]`, a
    /// single index, so two things were inexpressible: a `multiple` list box
    /// with several rows picked, and a list box with NOTHING picked — which is
    /// not an edge case but the state HTML says a fresh list box is in, since
    /// the selectedness setting algorithm auto-selects only at display size 1.
    pub selectedness: bool,
    /// The **dirtiness** flag of an `<option>` (HTML §4.10.10). Raised by a
    /// user picking or toggling the option, and by the `selected` IDL setter;
    /// while it is false the content attribute still drives selectedness.
    pub dirty_selectedness: bool,
    /// The form control's **value** (HTML §4.10.18.1) when it has diverged from
    /// the `value` content attribute — `None` while they still agree.
    ///
    /// Third instance of the same shape. The `value` attribute is
    /// `defaultValue`; this is the value the control holds. Everything used to
    /// write the attribute, so typing into a field edited the document and a
    /// reset had nowhere to restore FROM — which is why a fictional
    /// `defaultValue` "content attribute" had been invented to hold the
    /// original. There is no such attribute in HTML.
    pub value_state: Option<String>,
    /// The **dirty value flag** (HTML §4.10.18.1). Once raised, the `value`
    /// content attribute no longer drives the value.
    pub dirty_value: bool,
    /// Cursor position (char index) within the input's value string.
    pub input_cursor: usize,
    /// Selection anchor (char index). When equal to input_cursor, no selection.
    pub input_sel_anchor: usize,

    // Custom data store (arbitrary key/value pairs set by application code)
    pub data: HashMap<String, String>,

    /// Matched CSS rules (populated only when inspect mode is enabled).
    /// Each entry records the selector, declarations, and source of a rule
    /// that matched this element during the cascade.
    pub matched_rules: Vec<MatchedRule>,

    /// Shadow DOM root. When present, layout/render use the shadow tree instead
    /// of `children` (which become "light DOM" — slottable content).
    pub shadow_root: Option<Box<ShadowRoot>>,

    /// True when hover_style has been swapped into the active `style` slot.
    /// Used by the fast hover-swap path to avoid full re-cascade on hover changes.
    pub hover_applied: bool,

    /// Set by `mark_hover_dirty()` before incremental cascade.
    /// True means this node's :hover match changed — must re-cascade.
    pub cascade_dirty: bool,
    /// True means a descendant has `cascade_dirty` — must traverse children.
    pub has_dirty_descendant: bool,
    /// True means a descendant has `layout_dirty` — must traverse into children during layout.
    /// Allows skipping entire clean subtrees.
    pub has_dirty_layout_descendant: bool,
}

/// Shadow DOM root — holds a scoped tree and stylesheet.
#[derive(Clone, Debug)]
pub struct ShadowRoot {
    /// The shadow tree nodes (laid out/painted instead of light DOM children).
    pub children: Vec<WebCore>,
    /// Scoped stylesheet — only applies inside this shadow tree.
    pub stylesheet: crate::css::Stylesheet,
    /// Open (inspectable) or closed (opaque).
    pub mode: ShadowMode,
}

/// Which grammar a document was built from.
///
/// The DOM's own distinction, and the only thing it changes here is whether
/// names FOLD: HTML is ASCII-case-insensitive for tag and attribute names, XML
/// is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    Html,
    Xml,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShadowMode {
    Open,
    Closed,
}

// ─── Canvas 2D Context ──────────────────────────────────────────────────────

/// A 2D drawing context for `<canvas>` elements.
/// Provides a subset of the HTML Canvas2D API for drawing shapes, text, and images.
pub struct CanvasContext {
    pub width: u32,
    pub height: u32,
    /// RGBA pixel buffer (premultiplied alpha, row-major).
    pub pixels: Vec<u8>,
    fill_r: u8, fill_g: u8, fill_b: u8, fill_a: u8,
    stroke_r: u8, stroke_g: u8, stroke_b: u8, stroke_a: u8,
    line_width: f32,
    font_size: f32,
}

impl CanvasContext {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width, height,
            pixels: vec![0u8; (width * height * 4) as usize],
            fill_r: 0, fill_g: 0, fill_b: 0, fill_a: 255,
            stroke_r: 0, stroke_g: 0, stroke_b: 0, stroke_a: 255,
            line_width: 1.0,
            font_size: 16.0,
        }
    }

    /// Set fill color from CSS-style string: "#rgb", "#rrggbb", "rgb(r,g,b)", or named colors.
    pub fn set_fill_style(&mut self, color: &str) {
        if let Some(c) = crate::css::parse_color(color) {
            self.fill_r = c.r; self.fill_g = c.g; self.fill_b = c.b; self.fill_a = c.a;
        }
    }

    /// Set stroke color.
    pub fn set_stroke_style(&mut self, color: &str) {
        if let Some(c) = crate::css::parse_color(color) {
            self.stroke_r = c.r; self.stroke_g = c.g; self.stroke_b = c.b; self.stroke_a = c.a;
        }
    }

    pub fn set_line_width(&mut self, w: f32) { self.line_width = w; }
    pub fn set_font_size(&mut self, px: f32) { self.font_size = px; }

    /// Fill a rectangle.
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let x0 = x.max(0.0) as u32;
        let y0 = y.max(0.0) as u32;
        let x1 = ((x + w) as u32).min(self.width);
        let y1 = ((y + h) as u32).min(self.height);
        let (r, g, b, a) = (self.fill_r, self.fill_g, self.fill_b, self.fill_a);
        // Premultiply
        let pr = (r as u16 * a as u16 / 255) as u8;
        let pg = (g as u16 * a as u16 / 255) as u8;
        let pb = (b as u16 * a as u16 / 255) as u8;
        let stride = self.width as usize * 4;
        for py in y0..y1 {
            for px in x0..x1 {
                let i = py as usize * stride + px as usize * 4;
                if i + 3 < self.pixels.len() {
                    if a == 255 {
                        self.pixels[i] = r; self.pixels[i+1] = g; self.pixels[i+2] = b; self.pixels[i+3] = 255;
                    } else {
                        // Alpha blend (premultiplied)
                        let da = self.pixels[i+3] as u16;
                        let ia = 255 - a as u16;
                        self.pixels[i]   = (pr as u16 + self.pixels[i] as u16 * ia / 255) as u8;
                        self.pixels[i+1] = (pg as u16 + self.pixels[i+1] as u16 * ia / 255) as u8;
                        self.pixels[i+2] = (pb as u16 + self.pixels[i+2] as u16 * ia / 255) as u8;
                        self.pixels[i+3] = (a as u16 + da * ia / 255) as u8;
                    }
                }
            }
        }
    }

    /// Clear a rectangle to transparent.
    pub fn clear_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let x0 = x.max(0.0) as u32;
        let y0 = y.max(0.0) as u32;
        let x1 = ((x + w) as u32).min(self.width);
        let y1 = ((y + h) as u32).min(self.height);
        let stride = self.width as usize * 4;
        for py in y0..y1 {
            let row = py as usize * stride;
            for px in x0..x1 {
                let i = row + px as usize * 4;
                if i + 3 < self.pixels.len() {
                    self.pixels[i] = 0; self.pixels[i+1] = 0; self.pixels[i+2] = 0; self.pixels[i+3] = 0;
                }
            }
        }
    }

    /// Stroke a rectangle outline.
    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let lw = self.line_width;
        let saved = (self.fill_r, self.fill_g, self.fill_b, self.fill_a);
        self.fill_r = self.stroke_r; self.fill_g = self.stroke_g;
        self.fill_b = self.stroke_b; self.fill_a = self.stroke_a;
        self.fill_rect(x, y, w, lw);           // top
        self.fill_rect(x, y + h - lw, w, lw);  // bottom
        self.fill_rect(x, y, lw, h);            // left
        self.fill_rect(x + w - lw, y, lw, h);  // right
        self.fill_r = saved.0; self.fill_g = saved.1; self.fill_b = saved.2; self.fill_a = saved.3;
    }

    /// Fill a circle.
    pub fn fill_circle(&mut self, cx: f32, cy: f32, radius: f32) {
        let r2 = radius * radius;
        let x0 = (cx - radius).max(0.0) as i32;
        let y0 = (cy - radius).max(0.0) as i32;
        let x1 = ((cx + radius) as i32 + 1).min(self.width as i32);
        let y1 = ((cy + radius) as i32 + 1).min(self.height as i32);
        for py in y0..y1 {
            for px in x0..x1 {
                let dx = px as f32 + 0.5 - cx;
                let dy = py as f32 + 0.5 - cy;
                if dx * dx + dy * dy <= r2 {
                    let i = (py as usize * self.width as usize + px as usize) * 4;
                    if i + 3 < self.pixels.len() {
                        self.pixels[i] = self.fill_r; self.pixels[i+1] = self.fill_g;
                        self.pixels[i+2] = self.fill_b; self.pixels[i+3] = self.fill_a;
                    }
                }
            }
        }
    }

    /// Draw a line from (x1,y1) to (x2,y2) using Bresenham.
    pub fn stroke_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        let (mut x, mut y) = (x1 as i32, y1 as i32);
        let (ex, ey) = (x2 as i32, y2 as i32);
        let dx = (ex - x).abs();
        let dy = -(ey - y).abs();
        let sx = if x < ex { 1 } else { -1 };
        let sy = if y < ey { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            if x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height {
                let i = (y as usize * self.width as usize + x as usize) * 4;
                if i + 3 < self.pixels.len() {
                    self.pixels[i] = self.stroke_r; self.pixels[i+1] = self.stroke_g;
                    self.pixels[i+2] = self.stroke_b; self.pixels[i+3] = self.stroke_a;
                }
            }
            if x == ex && y == ey { break; }
            let e2 = 2 * err;
            if e2 >= dy { err += dy; x += sx; }
            if e2 <= dx { err += dx; y += sy; }
        }
    }

    /// Copy pixel buffer to an WebCore's image_data for rendering.
    pub fn apply_to_node(&self, node: &mut WebCore) {
        node.image_data = Some(self.pixels.clone());
        node.image_width = self.width;
        node.image_height = self.height;
    }
}

/// Form interaction event — fired by the engine, handled by the host.
#[derive(Debug, Clone)]
pub struct FormEvent {
    /// Element tag (e.g. "input", "select", "textarea")
    pub tag: String,
    /// Element id attribute (empty if none)
    pub id: String,
    /// Element name attribute (empty if none)
    pub name: String,
    /// Event kind
    pub kind: FormEventKind,
    /// Stable node_id of the element.
    pub element: u32,
}

// FormEvent is now Send-safe (uses node_id instead of raw pointers)

#[derive(Debug, Clone)]
pub enum FormEventKind {
    /// Text input value changed (new value)
    Input(String),
    /// Value committed (e.g. Enter in text field, option selected)
    Change(String),
    /// Checkbox/radio toggled (new checked state)
    Toggle(bool),
    /// Button clicked (value attribute)
    Click(String),
    /// Form submitted (form element's action URL)
    Submit(String),
    /// Focus gained
    Focus,
    /// Focus lost
    Blur,
}

/// Callback type for form events. The host sets this to handle form interactions.
pub type FormEventCallback = Box<dyn FnMut(&FormEvent) + Send>;

/// A CSS rule that matched an element, stored for inspector display.
#[derive(Clone, Debug)]
pub struct MatchedRule {
    /// The original CSS selector text (e.g. ".container-fluid")
    pub selector: String,
    /// Property → value pairs from this rule
    pub declarations: Vec<(String, String)>,
    /// Specificity of the selector
    pub specificity: u32,
    /// Source: "ua" for user-agent, or the stylesheet URL/index
    pub source: String,
}

impl WebCore {
    /// Does this element DISPLAY an image — `<img>`, or `<input type=image>`?
    ///
    /// HTML §4.10.5.1.19: the Image Button state "represents an image and a
    /// submit button", and it takes `src`, `alt` and its dimensions exactly as
    /// `<img>` does. So every path that renders an image has to accept both,
    /// and gating on the TAG alone left an image input rendering as a text
    /// field — the parser even resolved its `src` and nothing read it.
    ///
    /// `type` is an ENUMERATED attribute, so its value is ASCII
    /// case-insensitive: `type="IMAGE"` is the same state.
    pub fn is_image_element(&self) -> bool {
        if self.tag == "img" {
            return true;
        }
        self.tag == "input"
            && self
                .attributes
                .get("type")
                .map(|t| t.trim().eq_ignore_ascii_case("image"))
                .unwrap_or(false)
    }

    pub fn new(tag: impl Into<String>) -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT_ID: AtomicU32 = AtomicU32::new(500_000);
        Self {
            tag: tag.into(),
            node_id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            style: ComputedStyle::default(),
            attributes: HashMap::new(),
            text: String::new(),
            parent: 0,
            first_child: 0,
            last_child: 0,
            next_sibling: 0,
            prev_sibling: 0,
            children: Vec::new(),
            layout: LayoutBox::default(),

            component_width:  0.0,
            component_height: 0.0,

            resolved_src: String::new(),
            image_data:   None,
            image_width:  0,
            image_height: 0,

            bg_image_data:   None,
            mask_image_data:   None,
            mask_image_width:  0,
            mask_image_height: 0,
            bg_image_width:  0,
            bg_image_height: 0,

            svg_markup: None,
            svg_viewbox_w: 0.0,
            svg_viewbox_h: 0.0,

            checkedness: false,
            dirty_checked: false,
            selectedness: false,
            dirty_selectedness: false,
            value_state: None,
            dirty_value: false,
            input_cursor: 0,
            input_sel_anchor: 0,

            data: HashMap::new(),
            matched_rules: Vec::new(),
            shadow_root: None,
            hover_applied: false,
            cascade_dirty: false,
            has_dirty_descendant: false,
            has_dirty_layout_descendant: false,
        }
    }

    /// Attach a shadow root to this element. Parses `html` as the shadow tree
    /// and extracts `<style>` blocks into a scoped stylesheet.
    pub fn attach_shadow(&mut self, mode: ShadowMode, html: &str) {
        let doc = crate::html::parse_html(html);
        let mut children = doc.root.children;
        // Move <body> children up: the parser wraps a fragment in
        // `<html><head></head><body>…`, and a shadow tree wants the CONTENT.
        //
        // Found by TAG, not by index. This asked whether body was the only
        // child, which stopped being true the moment the parser started
        // synthesising `<head>` as HTML §13.2.6 requires.
        if let Some(at) = children.iter().position(|c| c.tag == "body") {
            children = std::mem::take(&mut children[at].children);
        }
        // Start with UA stylesheet so shadow tree gets default styles
        let mut stylesheet = crate::css::ua_stylesheet();
        // Extract <style> elements into the scoped stylesheet
        let mut styles_css = String::new();
        children.retain(|c| {
            if c.tag == "style" {
                styles_css.push_str(&c.text);
                for ch in &c.children {
                    if ch.tag == "#text" { styles_css.push_str(&ch.text); }
                }
                false
            } else {
                true
            }
        });
        if !styles_css.is_empty() {
            // Author origin — the shadow root's `<style>` outranks the UA sheet
            // it was seeded with (see `css::AUTHOR_ORIGIN_BOOST`).
            stylesheet.parse_and_add_author(&styles_css);
        }
        self.shadow_root = Some(Box::new(ShadowRoot { children, stylesheet, mode }));
    }

    pub fn get_attr(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(|s| s.as_str())
    }

    pub fn is_text_node(&self) -> bool {
        self.tag == "#text"
    }

    /// Whether this node is an ELEMENT.
    ///
    /// Everything that counts elements — `:nth-child`, `firstElementChild`,
    /// `children`, "does this box have any content" — used to spell the test
    /// `tag != "#text"`, which was exact only while text was the one non-element
    /// node that could appear. It is not: the DOM's non-element nodes all carry
    /// a `#`-prefixed name (`#text`, `#comment`, `#cdata-section`,
    /// `#document-fragment`), and a comment counted as an element would shift
    /// every `:nth-child` index after it and make an empty box non-empty.
    ///
    /// Asking the question once, by the naming rule the DOM already uses, is
    /// what keeps the next node kind from having to be added in fifty places.
    pub fn is_element(&self) -> bool {
        !self.tag.starts_with('#') && !self.is_pseudo_element()
    }

    /// Is this a generated `::before` / `::after` box rather than a DOM node?
    ///
    /// The cascade materialises `content` as a real child box so layout and
    /// paint can treat it like anything else. It is NOT a node: a
    /// pseudo-element has no place in `childNodes`, is not counted by
    /// `:nth-child`, and cannot be serialized — `<::after>!</::after>` is not
    /// markup, and it was reaching the output of `serialize_html`.
    pub fn is_pseudo_element(&self) -> bool {
        self.tag.starts_with("::")
    }

    /// Number of direct children.
    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// Whether this node has any children.
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    pub fn is_void(&self) -> bool {
        matches!(self.tag.as_str(),
            "br" | "hr" | "img" | "input" | "meta" | "link" | "col" |
            "area" | "base" | "embed" | "param" | "source" | "track" | "wbr")
    }

    /// Returns the effective children for layout/render: shadow children if a
    /// shadow root is present, otherwise the normal children.
    pub fn effective_children(&self) -> &[WebCore] {
        if let Some(ref sr) = self.shadow_root {
            &sr.children
        } else {
            &self.children
        }
    }

    /// Mutable version of `effective_children`.
    pub fn effective_children_mut(&mut self) -> &mut Vec<WebCore> {
        if let Some(ref mut sr) = self.shadow_root {
            &mut sr.children
        } else {
            &mut self.children
        }
    }

    /// Resolve `<slot>` elements in the shadow tree by projecting light DOM children.
    /// Must be called before layout when a shadow root is present.
    pub fn resolve_slots(&mut self) {
        if self.shadow_root.is_none() { return; }
        let light_children = self.children.clone();
        let sr = self.shadow_root.as_mut().unwrap();
        resolve_slots_inner(&mut sr.children, &light_children);
    }

    /// Collect all text content recursively.
    pub fn text_content(&self) -> String {
        if self.is_text_node() {
            return self.text.clone();
        }
        let mut out = self.text.clone();
        for child in &self.children {
            out.push_str(&child.text_content());
        }
        out
    }

    /// Find boxes matching a simple CSS selector (tag, .class, #id).
    pub fn query_selector_all<'a>(&'a self, selector: &str) -> Vec<&'a WebCore> {
        let mut results = Vec::new();
        self.collect_matching(selector, &mut results);
        results
    }

    fn collect_matching<'a>(&'a self, selector: &str, out: &mut Vec<&'a WebCore>) {
        if self.matches_simple_selector(selector) {
            out.push(self);
        }
        for child in &self.children {
            child.collect_matching(selector, out);
        }
    }

    fn matches_simple_selector(&self, selector: &str) -> bool {
        if selector.starts_with('#') {
            self.attributes.get("id").map(|s| s.as_str()) == Some(&selector[1..])
        } else if selector.starts_with('.') {
            let cls = &selector[1..];
            self.attributes.get("class")
                .map(|s| s.split_whitespace().any(|c| c == cls))
                .unwrap_or(false)
        } else {
            self.tag == selector
        }
    }
}

// ─── Document ─────────────────────────────────────────────────────────────────

pub use crate::css::Stylesheet;
use crate::dom::{Editor, EventListeners, HtmlEvent, HtmlEventType};
use crate::layout::LayoutEngine;

/// Active scrollbar drag state (set by `process_scrollbar_event`).
#[derive(Debug, Clone)]
pub struct ScrollbarDrag {
    /// Kind of scrollbar being dragged.
    pub kind:           ScrollbarDragKind,
    /// Screen Y at the start of the drag.
    pub start_mouse_y:  f32,
    /// Scroll position at the start of the drag.
    pub start_scroll:   f32,
    /// Pixels of scroll per pixel of mouse movement.
    pub scroll_per_px:  f32,
}

/// Which scrollbar is being dragged.
#[derive(Debug, Clone)]
pub enum ScrollbarDragKind {
    /// The viewport (document-level) vertical scrollbar.
    Viewport,
    /// A per-element scrollbar; the element is identified by its stable node_id.
    Element(u32),
}

// ─── CSS Animation / Transition types ─────────────────────────────────────────

/// A single keyframe stop inside a `@keyframes` block.
#[derive(Clone, Debug)]
pub struct KeyframeStop {
    /// Progress point in the animation (0.0 = `from` / `0%`, 1.0 = `to` / `100%`).
    pub offset: f32,
    /// CSS property/value pairs declared at this stop.
    pub properties: Vec<(String, String)>,
}

/// CSS easing function (timing function).
#[derive(Clone, Debug, PartialEq)]
pub enum EasingFn {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
    StepStart,
    StepEnd,
    Steps(u32, bool),  // (count, jump_start)
}
impl Default for EasingFn { fn default() -> Self { Self::Ease } }

/// CSS `animation-direction` values.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum AnimDirection { #[default] Normal, Reverse, Alternate, AlternateReverse }

/// CSS `animation-fill-mode` values.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum FillMode { #[default] None, Forwards, Backwards, Both }

/// A fully parsed CSS `animation` shorthand or sub-property group.
#[derive(Clone, Debug)]
pub struct ParsedAnimation {
    pub name:              String,
    pub duration_ms:       f32,
    pub delay_ms:          f32,
    pub timing_fn:         EasingFn,
    /// `f32::INFINITY` for `animation-iteration-count: infinite`.
    pub iteration_count:   f32,
    pub direction:         AnimDirection,
    pub fill_mode:         FillMode,
    pub play_state_paused: bool,
}

/// A fully parsed CSS `transition` shorthand or sub-property group.
#[derive(Clone, Debug)]
pub struct ParsedTransition {
    pub property:    String,
    pub duration_ms: f32,
    pub delay_ms:    f32,
    pub timing_fn:   EasingFn,
}

/// Runtime state for one active CSS animation on one element.
#[derive(Clone, Debug)]
pub struct AnimState {
    /// The WebCore raw pointer, stored as `usize` for Hash/Eq.
    pub element_id: u32,
    pub animation:  ParsedAnimation,
    pub start_time: std::time::Instant,
}

/// Runtime state for one active CSS transition on one property of one element.
#[derive(Clone, Debug)]
pub struct TransitionState {
    pub property:    String,
    pub from_value:  String,
    pub to_value:    String,
    pub start_time:  std::time::Instant,
    pub duration_ms: f32,
    pub delay_ms:    f32,
    pub timing_fn:   EasingFn,
}

// ─── aria-live announcement types ─────────────────────────────────────────────

/// How urgently an aria-live announcement should be delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivePoliteness {
    /// No announcement (aria-live="off" or no attribute).
    Off,
    /// Deliver after the user's current action completes (aria-live="polite").
    Polite,
    /// Interrupt the user immediately (aria-live="assertive").
    Assertive,
}

/// An accessibility announcement queued by a change to an `aria-live` region.
#[derive(Debug, Clone)]
pub struct Announcement {
    /// Text content to announce.
    pub text:        String,
    /// Urgency — how the host or AT should prioritise this announcement.
    pub politeness:  LivePoliteness,
    /// `true` when `aria-atomic="true"` was set on the region.
    /// Hosts should announce the full `text`; `false` means only the diff matters.
    pub atomic:      bool,
}

// ─── Node Arena (flat storage for all WebCore nodes) ─────────────────────────

/// Flat storage for all WebCore nodes, indexed by node_id.
/// This is the source of truth for the DOM tree. Tree structure is encoded
/// via linked-list pointers (parent/first_child/last_child/next_sibling/prev_sibling)
/// on each WebCore.
pub struct NodeArena {
    nodes: HashMap<u32, WebCore>,
    pub root_id: u32,
}

impl NodeArena {
    pub fn new() -> Self {
        Self { nodes: HashMap::new(), root_id: 0 }
    }

    /// Insert a node. If a node with this ID already exists, it's replaced.
    pub fn insert(&mut self, node: WebCore) {
        self.nodes.insert(node.node_id, node);
    }

    /// Get an immutable reference to a node.
    #[inline]
    pub fn get(&self, id: u32) -> Option<&WebCore> {
        self.nodes.get(&id)
    }

    /// Get a mutable reference to a node.
    #[inline]
    pub fn get_mut(&mut self, id: u32) -> Option<&mut WebCore> {
        self.nodes.get_mut(&id)
    }

    /// Remove a node from the arena. Returns it if it existed.
    pub fn remove(&mut self, id: u32) -> Option<WebCore> {
        self.nodes.remove(&id)
    }

    /// Check if a node exists.
    pub fn contains(&self, id: u32) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Number of nodes in the arena.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Collect child node_ids of a parent (from linked-list pointers).
    pub fn child_ids(&self, parent_id: u32) -> Vec<u32> {
        let mut ids = Vec::new();
        if let Some(parent) = self.nodes.get(&parent_id) {
            let mut cur = parent.first_child;
            while cur != 0 {
                ids.push(cur);
                cur = self.nodes.get(&cur).map(|n| n.next_sibling).unwrap_or(0);
            }
        }
        ids
    }

    /// Iterate child node_ids without allocation (returns an iterator).
    pub fn children(&self, parent_id: u32) -> ChildIdIter<'_> {
        let first = self.nodes.get(&parent_id).map(|n| n.first_child).unwrap_or(0);
        ChildIdIter { arena: self, next: first }
    }

    /// Count children of a node.
    pub fn child_count(&self, parent_id: u32) -> usize {
        self.children(parent_id).count()
    }

    /// Get the root node.
    pub fn root(&self) -> Option<&WebCore> {
        self.nodes.get(&self.root_id)
    }

    /// Get the root node mutably.
    pub fn root_mut(&mut self) -> Option<&mut WebCore> {
        self.nodes.get_mut(&self.root_id)
    }

    /// Append a child to a parent. Updates linked-list pointers.
    pub fn append_child(&mut self, parent_id: u32, child_id: u32) {
        let old_last = self.nodes.get(&parent_id).map(|p| p.last_child).unwrap_or(0);

        if let Some(child) = self.nodes.get_mut(&child_id) {
            child.parent = parent_id;
            child.prev_sibling = old_last;
            child.next_sibling = 0;
        }

        if old_last != 0 {
            if let Some(prev) = self.nodes.get_mut(&old_last) {
                prev.next_sibling = child_id;
            }
        }

        if let Some(parent) = self.nodes.get_mut(&parent_id) {
            if parent.first_child == 0 {
                parent.first_child = child_id;
            }
            parent.last_child = child_id;
        }
    }

    /// Remove a child from its parent. Updates linked-list pointers.
    /// The node stays in the arena (detached).
    pub fn detach(&mut self, node_id: u32) {
        let (parent_id, prev, next) = match self.nodes.get(&node_id) {
            Some(n) => (n.parent, n.prev_sibling, n.next_sibling),
            None => return,
        };
        if parent_id == 0 { return; }

        if prev != 0 {
            if let Some(p) = self.nodes.get_mut(&prev) { p.next_sibling = next; }
        } else if let Some(parent) = self.nodes.get_mut(&parent_id) {
            parent.first_child = next;
        }

        if next != 0 {
            if let Some(n) = self.nodes.get_mut(&next) { n.prev_sibling = prev; }
        } else if let Some(parent) = self.nodes.get_mut(&parent_id) {
            parent.last_child = prev;
        }

        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.parent = 0;
            node.prev_sibling = 0;
            node.next_sibling = 0;
        }
    }

    /// Build the arena from an existing WebCore tree (migration helper).
    /// Clones all nodes into the flat HashMap. Original tree unchanged.
    pub fn from_tree(root: &WebCore) -> Self {
        let mut arena = Self::new();
        arena.root_id = root.node_id;
        flatten_into_arena(root, &mut arena);
        arena
    }
}

/// Recursively flatten a Vec<WebCore> tree into the arena.
/// Clones each node (with empty children Vec) into the flat store.
/// The original tree is NOT modified.
fn flatten_into_arena(node: &WebCore, arena: &mut NodeArena) {
    // Clone the node with an empty children Vec (arena uses linked-list, not Vec)
    let mut flat_node = node.clone();
    flat_node.children.clear(); // arena nodes don't need Vec children
    arena.insert(flat_node);
    // Recurse into children
    for child in &node.children {
        flatten_into_arena(child, arena);
    }
}

/// Iterator over child node_ids using linked-list pointers.
pub struct ChildIdIter<'a> {
    arena: &'a NodeArena,
    next: u32,
}

impl<'a> Iterator for ChildIdIter<'a> {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.next == 0 { return None; }
        let id = self.next;
        self.next = self.arena.get(id).map(|n| n.next_sibling).unwrap_or(0);
        Some(id)
    }
}

impl std::fmt::Debug for NodeArena {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeArena")
            .field("len", &self.nodes.len())
            .field("root_id", &self.root_id)
            .finish()
    }
}

/// The root document: box tree + stylesheet + metadata.
/// Which popup an element opens on activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerKind {
    Color,
    Calendar,
}

pub struct Document {
    pub root:            WebCore,
    /// Flat node storage — all WebCore nodes indexed by node_id.
    /// Rebuilt lazily on first `get_node()` after layout marks it stale.
    pub nodes:           NodeArena,
    /// True when the tree has changed since last arena rebuild.
    pub nodes_stale:     bool,
    pub stylesheet:      Stylesheet,
    pub title:           String,
    pub base_url:        String,

    // ── Arena-based DOM (bridge period: mirrors WebCore tree) ────────────────
    /// Arena-based DOM tree with stable NodeId identity.
    /// During the bridge period, this mirrors the WebCore tree structure.
    pub arena:           DomArena,
    /// Next node_id to assign (monotonically increasing counter).
    pub next_node_id:    u32,
    /// Bridge lookup: set of known node_ids in the tree.
    /// O(1) node lookup index: node_id → raw pointer into the WebCore tree.
    /// Rebuilt by `rebuild_node_index()` after layout. Pointers are valid only
    /// until the next tree mutation (layout, DOM change).
    pub node_index: HashMap<u32, *const WebCore>,
    /// Which grammar this document was built from — HTML or XML.
    ///
    /// The difference the DOM actually draws is CASE. An HTML document
    /// ASCII-lowercases tag and attribute names, so `createElement("DIV")`
    /// makes a `div` and `getAttribute("HREF")` finds `href`; an XML document
    /// is case-sensitive, where `<Rect>` and `<rect>` are two elements.
    ///
    /// Everything webcore parses is HTML, so this only becomes anything other
    /// than `Html` when a caller asks for an XML document explicitly.
    pub kind: DocumentKind,
    /// Separated layout data indexed by node_id (bridge: duplicates WebCore geometry).
    pub layout_store:    crate::layout::layout_box::LayoutStore,
    /// Nodes created by dom_create_element/dom_create_text that haven't been
    /// inserted into the WebCore tree yet. Consumed by dom_append_child/dom_insert_before.
    pub pending_nodes:   HashMap<u32, WebCore>,
    /// URLs from `<link rel="stylesheet" href="...">` tags in `<head>`.
    /// Populated by the parser so the host can fetch and merge external CSS.
    /// External stylesheets from `<link rel="stylesheet">` tags: (href, media).
    /// The `media` string (e.g. "print", "screen", "") is preserved so that
    /// print-only sheets can be skipped for screen rendering but kept for future
    /// print support.
    pub linked_stylesheets: Vec<(String, String)>,
    pub editor:          Editor,
    /// Drawing state for the document's `<canvas>` elements, keyed by node id.
    /// The pixels stay on the element in `WebCore::image_data`; this is what
    /// persists between two calls from a page. See `canvas::CanvasSurfaces`.
    pub canvas_surfaces: crate::canvas::CanvasSurfaces,
    pub events:          EventListeners,
    /// NodeId-based event system with capture/bubble phases.
    pub event_targets:   crate::dom::events::EventTargetMap,
    /// Viewport scroll position in logical pixels (managed by Renderer::render).
    pub scroll_x:        f32,
    pub scroll_y:        f32,
    /// Active scrollbar drag state (None when not dragging).
    pub scrollbar_drag:  Option<ScrollbarDrag>,
    /// Currently hovered element (node_id, 0 if none).
    pub hovered_box:     u32,
    /// Suppresses the next hover change after a hover-triggered relayout.
    /// Prevents feedback loops: hover opens dropdown → layout changes →
    /// re-hit-test finds different element → dropdown closes → repeat.
    pub hover_suppress_count: u8,
    /// Currently active (pressed) element (node_id, 0 if none).
    pub active_box:      u32,
    /// Currently focused element (node_id, 0 if none).
    pub focused_box:     u32,
    /// Element hit on last MouseDown — used to fire Click on MouseUp if same target.
    pub mousedown_target: u32,
    /// Last click target + time for DblClick detection.
    pub last_click_target: u32,
    pub last_click_time:   Option<std::time::Instant>,
    /// Drag state machine.
    pub drag_source:       u32,
    pub drag_start_doc_pt: (f32, f32),
    pub drag_active:       bool,
    /// Set of link hrefs the user has clicked (for :visited pseudo-class).
    pub visited_urls:    std::collections::HashSet<String>,
    /// Last known logical viewport size — kept in sync by LayoutEngine::layout.
    pub viewport_w:      f32,
    pub viewport_h:      f32,
    /// True when focus was moved by keyboard (Tab/Shift+Tab) — drives :focus-visible.
    pub keyboard_focus:  bool,
    /// Caret blink epoch — reset on each keystroke so caret stays visible while typing.
    pub caret_blink_epoch: std::time::Instant,
    /// Currently open select dropdown (node_id, 0 if none open).
    pub open_select: u32,
    /// The element whose PICKER is open — `<input type=color>` today.
    ///
    /// The same shape `open_select` has: one node, drawn as an overlay after
    /// the page and hit-tested before anything else while it is open. A picker
    /// is user-agent chrome that appears on activation, which is exactly what
    /// the dropdown already is; there is one popup surface here and this is a
    /// second thing on it, not a new mechanism.
    pub open_picker: u32,
    /// The `<input type=range>` whose knob the pointer is holding (0 = none).
    ///
    /// A slider is the one control whose interaction is the pointer's whole
    /// PATH rather than where it landed. HTML says so in the words that
    /// distinguish its two events: "while the user is dragging the control's
    /// knob, input events would fire whenever the position changed, whereas
    /// the change event would only fire when the user let go of the knob,
    /// committing to a specific value."
    ///
    /// Held as the ELEMENT, not a flag, because a drag that has wandered off
    /// the control still belongs to it — a pointer released over the page must
    /// commit the slider it grabbed, not abandon it.
    pub dragging_range: u32,
    /// What the range held when the drag began.
    ///
    /// `change` fires on release "if the value is committed" — a press and
    /// release that moved nothing committed nothing, so this is what release
    /// compares against instead of firing unconditionally.
    pub range_drag_origin: String,
    /// Hovered option index in open dropdown (-1 = none).
    pub dropdown_hover_idx: i32,
    /// Form event callback — set by the host to handle form interactions.
    /// Called when users interact with form elements (click checkbox, type in input, etc.).
    pub on_form_event:   Option<FormEventCallback>,

    // ── Engine callbacks ─────────────────────────────────────────────────────
    /// Called when a link is clicked (href). Return `true` to handle navigation,
    /// `false` to let the engine follow the link.
    pub on_navigate:     Option<Box<dyn FnMut(&str) -> bool + Send>>,
    /// Called when the document title changes (e.g. via `<title>` or DOM mutation).
    pub on_title_change: Option<Box<dyn FnMut(&str) + Send>>,
    /// Called after any DOM mutation (node added/removed/attribute changed).
    /// The argument is the node_id of the mutated node.
    pub on_dom_mutation:  Option<Box<dyn FnMut(u32) + Send>>,
    /// Called when a node becomes visible in the viewport (intersection observer pattern).
    pub on_visibility_change: Option<Box<dyn FnMut(u32, bool) + Send>>,

    // ── CSS animation / transition runtime ────────────────────────────────────
    /// All currently running CSS animations (one entry per animation per element).
    pub active_animations: Vec<AnimState>,
    /// Per-element active transitions, keyed by WebCore pointer (as usize).
    pub(crate) transition_states: HashMap<u32, Vec<TransitionState>>,
    /// Previous transitionable style values per element, for change detection.
    pub(crate) prev_styles: HashMap<u32, HashMap<String, String>>,
    /// Clean cascade-time style snapshot, keyed by element pointer.
    /// Populated when the cascade runs; never mutated by animation overrides.
    /// Used by sync_transitions so hover-out correctly reads the base (not overridden) values.
    pub(crate) cascade_styles: HashMap<u32, HashMap<String, String>>,
    /// Interpolated CSS property overrides produced by `tick_animations`.
    /// Applied on top of the cascade result before geometry runs.
    pub(crate) animation_overrides: HashMap<u32, Vec<(String, String)>>,
    /// Set by `tick_animations`; tells the host to request another render frame.
    pub needs_animation_frame: bool,
    /// Set when `hovered_box` changes; cleared by `layout()` after running `sync_transitions`.
    pub hover_changed: bool,
    /// Node IDs of elements that have hover-dependent CSS rules.
    /// Only these need re-cascade on hover change. Populated during full cascade.
    pub hover_sensitive_nodes: HashSet<u32>,
    /// Set by DOM API mutations to force a full cascade on next layout.
    pub style_dirty: bool,
    /// Previous hover target — used to compute the diff for incremental cascade.
    pub prev_hovered_box: u32,

    // ── aria-live region machinery ─────────────────────────────────────────────
    /// Announcements queued since the last call to `take_announcements()`.
    pub pending_announcements: Vec<Announcement>,
    /// Text-content snapshots for each aria-live region, keyed by WebCore pointer.
    /// Updated every layout pass to detect content changes.
    pub(crate) live_region_snapshots: HashMap<u32, String>,
    /// `false` until the first `check_live_regions()` call.
    /// On the very first pass, only assertive regions announce their initial content;
    /// polite regions are silently initialised so they don't flood the user on load.
    pub(crate) live_regions_initialized: bool,

    /// Monotonically increasing counter bumped after every layout pass.
    /// Used by the Renderer to detect when the display list cache is stale.
    pub layout_generation: u64,

    // ── Async image loading ─────────────────────────────────────────────────
    /// Receiver for images arriving from background fetch threads.
    /// Each message is (node_path, decoded_rgba, width, height).
    pub pending_images: Option<std::sync::mpsc::Receiver<(Vec<usize>, crate::html::DecodedImage)>>,
    /// Number of image fetches still in flight.
    pub images_in_flight: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Document {
    pub fn new() -> Self {
        Self {
            root:            WebCore::new("html"),
            nodes:           NodeArena::new(),
            nodes_stale:     true,
            stylesheet:      Stylesheet::default(),
            title:           String::new(),
            arena:           DomArena::new(),
            next_node_id:    1,  // 0 = NodeId::NONE (reserved)
            node_index:      HashMap::new(),
            kind:            DocumentKind::Html,
            layout_store:    crate::layout::layout_box::LayoutStore::new(),
            pending_nodes:   HashMap::new(),
            base_url:        String::new(),
            linked_stylesheets: Vec::new(),
            editor:          Editor::new(),
            canvas_surfaces: crate::canvas::CanvasSurfaces::default(),
            events:          EventListeners::new(),
            event_targets:   crate::dom::events::EventTargetMap::new(),
            scroll_x:        0.0,
            scroll_y:        0.0,
            scrollbar_drag:  None,
            hovered_box:       0,
            hover_suppress_count: 0,
            active_box:        0,
            focused_box:       0,
            mousedown_target:  0,
            last_click_target: 0,
            last_click_time:   None,
            drag_source:       0,
            drag_start_doc_pt: (0.0, 0.0),
            drag_active:       false,
            visited_urls:      std::collections::HashSet::new(),
            viewport_w:        0.0,
            viewport_h:        0.0,
            keyboard_focus:    false,
            caret_blink_epoch: std::time::Instant::now(), open_select: 0, open_picker: 0, dropdown_hover_idx: -1,
            // Transient interaction state, like the two popups beside it: a
            // fresh document is holding nothing.
            dragging_range: 0, range_drag_origin: String::new(),
            on_form_event:     None, on_navigate: None, on_title_change: None, on_dom_mutation: None, on_visibility_change: None,
            active_animations:     Vec::new(),
            transition_states:     HashMap::new(),
            prev_styles:           HashMap::new(),
            cascade_styles:        HashMap::new(),
            animation_overrides:   HashMap::new(),
            needs_animation_frame: false,
            hover_changed:         false,
            hover_sensitive_nodes: HashSet::new(),
            style_dirty:           false,
            prev_hovered_box:      0,
            pending_announcements:    Vec::new(),
            live_region_snapshots:    HashMap::new(),
            live_regions_initialized: false,
            layout_generation:   0,
            pending_images:      None,
            images_in_flight:    std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Poll for images that arrived from background fetch threads.
    /// Returns `true` if any new images were loaded (caller should re-layout).
    pub fn poll_pending_images(&mut self) -> bool {
        let rx = match self.pending_images.as_ref() {
            Some(rx) => rx,
            None => return false,
        };
        let mut loaded_any = false;
        while let Ok((path, decoded)) = rx.try_recv() {
            if let Some(node) = find_node_by_path_mut(&mut self.root, &path) {
                crate::html::set_decoded_image_on_node(node, decoded);
                loaded_any = true;
            }
        }
        if self.images_in_flight.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            self.pending_images = None;
        }
        loaded_any
    }

    // ── Node index (node_id → pointer for O(1) lookup) ───────────────────────

    /// Rebuild the O(1) node index by walking the tree and storing pointers.
    /// Called after layout (tree structure is stable until next mutation).
    pub fn rebuild_node_index(&mut self) {
        self.node_index.clear();
        fn collect(node: &WebCore, map: &mut HashMap<u32, *const WebCore>) {
            if node.node_id != 0 {
                map.insert(node.node_id, node as *const WebCore);
            }
            for child in &node.children { collect(child, map); }
        }
        collect(&self.root, &mut self.node_index);
    }

    /// Backward-compat alias.
    pub fn rebuild_node_map(&mut self) { self.rebuild_node_index(); }

    /// O(1) node lookup by node_id. Uses the cached pointer index.
    /// Falls back to tree walk if index is empty (not yet built).
    #[inline]
    pub fn get_box_by_id(&self, node_id: u32) -> Option<&WebCore> {
        if node_id == 0 { return None; }
        // Fast path: O(1) index lookup
        if let Some(&ptr) = self.node_index.get(&node_id) {
            // SAFETY: pointer is valid because the tree hasn't been mutated
            // since rebuild_node_index() was called.
            return Some(unsafe { &*ptr });
        }
        // Fallback: tree walk (index not built yet)
        fn walk(node: &WebCore, id: u32) -> Option<&WebCore> {
            if node.node_id == id { return Some(node); }
            for child in &node.children { if let Some(f) = walk(child, id) { return Some(f); } }
            None
        }
        walk(&self.root, node_id)
    }

    /// Same as get_box_by_id — O(1) when index is built.
    #[inline]
    pub fn get_node(&self, node_id: u32) -> Option<&WebCore> {
        self.get_box_by_id(node_id)
    }

    /// Rebuild the flat arena from the tree on demand.
    pub fn sync_arena(&mut self) {
        self.nodes = NodeArena::from_tree(&self.root);
        self.nodes_stale = false;
    }

    /// O(1) mutable node lookup via tree walk (arena stores clones, not references).
    /// For mutable access, we must use the tree since the arena is a snapshot.
    pub fn get_box_by_id_mut(&mut self, node_id: u32) -> Option<&mut WebCore> {
        if node_id == 0 { return None; }
        fn walk(node: &mut WebCore, id: u32) -> Option<&mut WebCore> {
            if node.node_id == id { return Some(node); }
            for child in &mut node.children { if let Some(f) = walk(child, id) { return Some(f); } }
            None
        }
        walk(&mut self.root, node_id)
    }

    /// Allocate the next node_id (for dynamically created nodes outside the parser).
    pub fn alloc_node_id(&mut self) -> u32 {
        let id = self.next_node_id;
        self.next_node_id += 1;
        id
    }

    /// Re-apply the CSS cascade to the entire document tree.
    /// Call this after mutating class attributes (e.g. toggling dark mode) so
    /// that `ComputedStyle` on every box is updated before the next layout pass.
    /// Resets hover/active pointers since box addresses may change after re-layout.
    pub fn recascade(&mut self) {
        // Invalidate hover/active pointers — raw pointers may alias differently
        // after WebCore trees are rebuilt or re-allocated during parsing.
        self.hovered_box = 0;
        self.active_box  = 0;
        self.stylesheet.rebuild_index();
        crate::css::apply_cascade_vp(
            &mut self.root, &self.stylesheet, None, 16.0,
            self.viewport_w, self.viewport_h, self.focused_box, self.keyboard_focus,
        );
    }

    /// Re-apply cascade with an explicit focused element node_id.
    pub fn recascade_with_focus(&mut self, focused: u32) {
        self.focused_box = focused;
        self.hovered_box = 0;
        self.active_box  = 0;
        self.stylesheet.rebuild_index();
        crate::css::apply_cascade_vp(
            &mut self.root, &self.stylesheet, None, 16.0,
            self.viewport_w, self.viewport_h, self.focused_box, self.keyboard_focus,
        );
    }

    /// High-level mouse event entry point.
    /// The palette geometry of an open picker — one place, so the hit test and
    /// the paint cannot disagree about where the swatches are.
    ///
    /// Returns the popup's origin and cell size. `None` when the element has no
    /// laid-out box, which is also when there is nothing to click.
    /// The `<details>` a click belongs to, when the click is on its `<summary>`.
    ///
    /// Walks UP from the hit node, because a click lands on whatever is
    /// innermost — the text inside the summary, or an element the author put
    /// there — and the summary itself is often not what was hit.
    pub(crate) fn summary_details(&self, hit: u32) -> Option<u32> {
        // ⛔ Walks the RENDER TREE, not the arena. A hit id comes from hit
        // testing over boxes, and `Document::parent_node` is the ARENA's
        // parent — it asserts on an id the arena does not hold, which took the
        // whole process down on the first click of a submit button. Every
        // other click-path walk here (`find_form_parent_id`) goes over boxes
        // for the same reason.
        fn walk<'a>(node: &'a WebCore, hit: u32, chain: &mut Vec<&'a WebCore>) -> bool {
            chain.push(node);
            if node.node_id == hit {
                return true;
            }
            for child in &node.children {
                if walk(child, hit, chain) {
                    return true;
                }
            }
            chain.pop();
            false
        }
        let mut chain = Vec::new();
        if !walk(&self.root, hit, &mut chain) {
            return None;
        }
        // Innermost first: the click lands on the text inside the summary, or
        // on whatever the author put there, rather than on the summary itself.
        for i in (0..chain.len()).rev() {
            if chain[i].tag == "summary" {
                return chain
                    .get(i.checked_sub(1)?)
                    .filter(|p| p.tag == "details")
                    .map(|p| p.node_id);
            }
        }
        None
    }

    pub(crate) fn picker_rect(&self, id: u32) -> Option<(f32, f32, f32, f32)> {
        let node = self.find_webcore(id)?;
        let br = node.layout.border_rect;
        // Below the control, as the dropdown opens below its select.
        let (w, h) = match self.picker_kind(id) {
            Some(PickerKind::Calendar) => {
                (crate::widgets::Calendar::width(), crate::widgets::Calendar::height())
            }
            _ => {
                let cols = crate::widgets::PALETTE_COLUMNS as f32;
                let rows = (crate::widgets::PALETTE.len() as f32 / cols).ceil();
                let cell = crate::widgets::PALETTE_CELL;
                (cols * cell, rows * cell)
            }
        };
        Some((br.x, br.y + br.h, w, h))
    }

    /// Which picker an element opens, if any — the one place that decides, so
    /// the geometry, the paint and the hit test cannot disagree.
    pub(crate) fn picker_kind(&self, id: u32) -> Option<PickerKind> {
        let node = self.find_webcore(id)?;
        if node.tag != "input" {
            return None;
        }
        match node.attributes.get("type")?.trim().to_ascii_lowercase().as_str() {
            "color" => Some(PickerKind::Color),
            // `month` and `week` open a calendar too in a browser, but they
            // pick a MONTH and a WEEK, not a day — a day grid would write a
            // value their format cannot hold. Until each has its own grid,
            // only `date` opens one.
            "date" => Some(PickerKind::Calendar),
            _ => None,
        }
    }

    /// A click on a LIST BOX row, at document y `click_y`. Returns whether the
    /// selection moved.
    ///
    /// A list box draws its own rows and has no popup, so this is the whole
    /// interaction — the drop-down's `open_select` state machine is never
    /// involved. Which algorithm runs depends on the control:
    ///
    /// * `multiple` — **toggle** the row (HTML §4.10.7: "the user agent should
    ///   allow the user to toggle the selectedness of the option elements").
    ///   Toggling on a plain click is the only way to reach a multi-selection
    ///   at a seam with no modifier keys, and it is what the
    ///   `CheckedListBox` this renders for does anyway.
    /// * single-select — **pick an option**, the algorithm a drop-down runs.
    ///
    /// `unselect_request` is the third case, and it is the one HTML words as a
    /// request rather than a click: "if the multiple attribute is absent and
    /// the element's display size is greater than 1, then the user agent should
    /// also allow the user to request that the option whose selectedness is
    /// true, if any, be unselected."
    ///
    /// A SINGLE-SELECT list box only. A drop-down has no such affordance (its
    /// display size is 1) and a `multiple` list box already reaches an empty
    /// selection by toggling, so binding it there would be a second way to do
    /// one thing. The gesture is the platform's — ctrl/⌘-click on the row that
    /// is already selected — which is why it arrives as an answered question
    /// rather than being decided here.
    pub(crate) fn click_list_box_row(
        &mut self,
        select_id: u32,
        click_y: f32,
        unselect_request: bool,
    ) -> bool {
        let Some(select) = self.find_webcore(select_id) else { return false };
        if select.attributes.contains_key("disabled") {
            return false;
        }
        let content = select.layout.content_rect;
        let font_px = select.style.font_size_px(16.0, 16.0).max(1.0);
        let options = crate::html::forms::option_ids(select);
        let Some(row) = crate::html::forms::list_box_row_at(
            content.y,
            content.h,
            font_px,
            options.len(),
            click_y,
        ) else {
            return false;
        };
        let option_id = options[row];

        let Some(select_mut) = self.find_webcore_mut(select_id) else { return false };
        let multiple = crate::html::forms::is_multiple(&*select_mut);
        // "Upon this request being conveyed to the user agent, and before the
        // relevant user interaction event is queued (e.g. before the click
        // event), the user agent must set the selectedness of that option
        // element to false, set its dirtiness to true, and then send select
        // update notifications." Only the option that IS selected can be
        // unselected, so a request on any other row is an ordinary pick.
        let already_selected = crate::html::forms::list_of_options(&*select_mut)
            .into_iter()
            .any(|o| o.node_id == option_id && o.selectedness);
        let changed = if unselect_request && !multiple && already_selected {
            crate::html::forms::unselect_option(select_mut, option_id)
        } else if multiple {
            crate::html::forms::toggle_option(select_mut, option_id)
        } else {
            crate::html::forms::pick_option(select_mut, option_id)
        };
        if changed {
            select_mut.layout.layout_dirty = true;
            self.send_select_update_notifications(select_id);
        }
        changed
    }

    /// A click along a RANGE control's track, at document point `doc_pt`.
    /// Returns whether the value moved.
    ///
    /// `widgets::Slider` already owned the inverse of its own paint geometry —
    /// the thumb-radius inset at each end, and the axis a vertical writing mode
    /// turns — in `set_from_pointer`. Nothing had ever called it, so every
    /// trackbar and scrollbar was decorative. Driving the widget rather than
    /// re-deriving the mapping is what keeps the thumb under the pointer.
    ///
    /// The number that comes back is then put through the control's own step
    /// and bounds, because HTML enforces them "even during user input" — a
    /// click three-fifths along a `step=20` control lands on a multiple of 20,
    /// not on 60.4.
    ///
    /// ⛔ A CLICK, not a drag: the track jumps to the point rather than paging
    /// toward it. Both are user-agent choices; this is the one browsers make.
    /// Dragging needs `mouse_move` wired to the same path.
    pub(crate) fn drag_range_to(&mut self, input_id: u32, doc_pt: (f32, f32)) -> bool {
        let Some(input) = self.find_webcore(input_id) else { return false };
        // Mutability (HTML §4.10.18.2). `readonly` does NOT apply to a range,
        // so `disabled` is the whole test.
        if input.attributes.contains_key("disabled") {
            return false;
        }
        let rect = input.layout.content_rect;
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return false;
        }
        let min = crate::html::forms::range_minimum(input);
        let max = crate::html::forms::range_maximum(input);
        let current = input_value(input);
        let current_num = crate::html::forms::parse_floating_point(&current).unwrap_or(min);

        let mut slider = crate::widgets::Slider::new(min as f32, max as f32, current_num as f32);
        slider.width = rect.w;
        slider.height = rect.h;
        slider.vertical = !matches!(input.style.writing_mode, WritingMode::HorizontalTB);
        slider.mouse_down(doc_pt.0 - rect.x, doc_pt.1 - rect.y);
        let picked = slider.actual_value() as f64;

        // "User agents must not allow the user to set the value to a string
        // that is not a valid floating-point number", and the range and step
        // constraints hold throughout — so the pointer's answer goes through
        // the same sanitization the markup's did.
        let mut value = picked;
        if value < min {
            value = min;
        }
        if value > max && max >= min {
            value = max;
        }
        let snapped = crate::html::forms::snap_to_step(input, value);
        let text = crate::html::forms::best_representation(snapped);
        if text == current {
            return false;
        }

        let id = input.attributes.get("id").cloned().unwrap_or_default();
        let name = input.attributes.get("name").cloned().unwrap_or_default();
        if let Some(input_mut) = self.find_webcore_mut(input_id) {
            input_mut.value_state = Some(text.clone());
            input_mut.dirty_value = true;
            input_mut.layout.layout_dirty = true;
        }
        // `input` ALONE. Moving the knob is not committing to a value — that
        // is what release means, and `commit_range_drag` is where `change`
        // fires. Firing both here made every pixel of a drag look like a
        // finished decision.
        if let Some(ref mut cb) = self.on_form_event {
            cb(&FormEvent {
                tag: "input".into(),
                id,
                name,
                kind: FormEventKind::Input(text),
                element: input_id,
            });
        }
        true
    }

    /// Let go of the knob: fire `change` if the drag actually moved the value,
    /// and stop holding the control.
    ///
    /// Guarded on the value rather than on the drag having happened, because
    /// "the change event fires when the value is committed" — a press and
    /// release that moved nothing committed nothing, and a slider that
    /// announced a change every time it was merely touched would be lying to
    /// every handler counting them.
    pub(crate) fn commit_range_drag(&mut self) -> bool {
        let input_id = std::mem::replace(&mut self.dragging_range, 0);
        let origin = std::mem::take(&mut self.range_drag_origin);
        if input_id == 0 {
            return false;
        }
        let Some(input) = self.find_webcore(input_id) else { return false };
        let text = input_value(input);
        if text == origin {
            return false;
        }
        let id = input.attributes.get("id").cloned().unwrap_or_default();
        let name = input.attributes.get("name").cloned().unwrap_or_default();
        if let Some(ref mut cb) = self.on_form_event {
            cb(&FormEvent {
                tag: "input".into(),
                id,
                name,
                kind: FormEventKind::Change(text),
                element: input_id,
            });
        }
        true
    }

    /// Whether a node is an `<input type=range>`.
    pub(crate) fn is_range_input(&self, id: u32) -> bool {
        self.find_webcore(id)
            .map(|n| {
                n.tag == "input"
                    && n.attributes
                        .get("type")
                        .map(|t| t.trim().eq_ignore_ascii_case("range"))
                        .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    /// **Send select update notifications** (HTML §4.10.7): fire `input`, then
    /// `change`.
    ///
    /// ⛔ Both, in that order. The drop-down path fired `change` alone, so a
    /// program listening for `input` — which is what a live-updating handler
    /// listens for — never heard a `<select>` at all.
    pub(crate) fn send_select_update_notifications(&mut self, select_id: u32) {
        let Some(select) = self.find_webcore(select_id) else { return };
        let value = crate::html::forms::select_value(select);
        let id = select.attributes.get("id").cloned().unwrap_or_default();
        let name = select.attributes.get("name").cloned().unwrap_or_default();
        if let Some(ref mut cb) = self.on_form_event {
            for kind in [FormEventKind::Input(value.clone()), FormEventKind::Change(value)] {
                cb(&FormEvent {
                    tag: "select".into(),
                    id: id.clone(),
                    name: name.clone(),
                    kind,
                    element: select_id,
                });
            }
        }
    }

    /// A click on an element that is not a form control.
    ///
    /// UI Events puts `click` on whatever the pointer pressed and released
    /// over. `handle_form_click` only knows the controls, so everything else —
    /// a `<td>`, a `<div>`, an `<li>` — had a listener that could be registered
    /// and never fired. A control composed out of ordinary elements, which is
    /// what a calendar's day grid is, could be built and could not be used.
    ///
    /// The event carries the element's `id` and `name` like any other, and its
    /// text as the value, so a handler reads the same shape whatever it is on.
    pub(crate) fn fire_element_click(&mut self, node_id: u32) {
        // A text box is not an event target. The click belongs to the ELEMENT
        // that owns the text — which is what hitting a word means, and what a
        // browser reports. Returning early instead made a click that landed on
        // a cell's digits fire nothing, so the control worked in the middle of
        // a cell and not on its text.
        let mut node_id = node_id;
        for _ in 0..4 {
            match self.find_webcore(node_id) {
                Some(n) if n.tag.starts_with('#') => node_id = self.parent_node(node_id),
                _ => break,
            }
        }
        let Some(node) = self.find_webcore(node_id) else { return };
        if node.tag.starts_with('#') {
            return;
        }
        let tag = node.tag.clone();
        let id = node.attributes.get("id").cloned().unwrap_or_default();
        let name = node.attributes.get("name").cloned().unwrap_or_default();
        let text = node.text.clone();
        if let Some(ref mut cb) = self.on_form_event {
            cb(&FormEvent {
                tag,
                id,
                name,
                kind: FormEventKind::Click(text),
                element: node_id,
            });
        }
    }

    /// The month a date picker is showing: the element's own value, or the
    /// current month when it has none — which is what a browser opens on.
    pub(crate) fn picker_month(&self, id: u32) -> (i32, u32, Option<u32>) {
        let value = self.find_webcore(id).map(input_value).unwrap_or_default();
        match crate::widgets::parse_date(&value) {
            Some((y, m, d)) => (y, m, Some(d)),
            // No date library here, and none needed: an empty control opens on
            // a fixed, obviously-neutral month rather than pretending to know
            // today. The value it writes is a real date either way.
            None => (2026, 1, None),
        }
    }

    /// Which palette colour a point lands on, if any.
    /// Which day an open calendar's point lands on, and the month it belongs to.
    pub(crate) fn calendar_hit(&self, id: u32, doc_pt: (f32, f32)) -> Option<(i32, u32, u32)> {
        let (x, y, w, h) = self.picker_rect(id)?;
        if doc_pt.0 < x || doc_pt.0 >= x + w || doc_pt.1 < y || doc_pt.1 >= y + h {
            return None;
        }
        let (year, month, _) = self.picker_month(id);
        let day = crate::widgets::Calendar::day_at(
            (doc_pt.0 - x, doc_pt.1 - y),
            crate::widgets::first_weekday(year, month),
            crate::widgets::days_in_month(year, month),
        )?;
        Some((year, month, day))
    }

    pub(crate) fn picker_hit(&self, id: u32, doc_pt: (f32, f32)) -> Option<(u8, u8, u8)> {
        let (x, y, w, h) = self.picker_rect(id)?;
        if doc_pt.0 < x || doc_pt.0 >= x + w || doc_pt.1 < y || doc_pt.1 >= y + h {
            return None;
        }
        let cell = crate::widgets::PALETTE_CELL;
        let col = ((doc_pt.0 - x) / cell) as usize;
        let row = ((doc_pt.1 - y) / cell) as usize;
        crate::widgets::PALETTE
            .get(row * crate::widgets::PALETTE_COLUMNS + col)
            .copied()
    }

    /// **Flush pending style and layout**, so a geometry question is answered
    /// about the tree as it is now.
    ///
    /// CSSOM View defines its geometry on BOXES, and a node inserted or
    /// restyled since the last layout does not have one yet. A browser hides
    /// that by flushing on demand — every geometry attribute is specified to
    /// return a box, and returning one means having laid it out. Here layout
    /// ran only in the paint path, so a program that appended a control and
    /// asked for its rect in the same turn was told 0×0, and the real answer
    /// arrived a frame later with nobody left to receive it.
    ///
    /// The width is the one the document was last laid out against — the
    /// viewport its shell gave it. A document that has never been laid out has
    /// no containing block to measure against, and no box to flush to, so it is
    /// left exactly as it is rather than measured against a guess.
    pub fn flush_layout(&mut self) {
        let width = self.root.layout.last_containing_width;
        if width <= 0.0 {
            return;
        }
        self.recascade();
        LayoutEngine::new().layout(self, width);
    }

    /// A pointer event with no modifier keys held.
    ///
    /// The plain spelling, kept because almost every caller has no modifiers to
    /// report and a `false, false, false, false` tail at each of them says
    /// nothing. `process_mouse_event_with_modifiers` is the full event.
    pub fn process_mouse_event(&mut self, etype: crate::dom::HtmlEventType, doc_pt: (f32, f32), button: u8) -> bool {
        self.process_mouse_event_with_modifiers(etype, doc_pt, button, false, false, false, false)
    }

    /// A pointer event, with the modifier keys that were held.
    ///
    /// Modifiers are part of a pointer event, not decoration: HTML's list box
    /// asks the user agent to "allow the user to request that the option whose
    /// selectedness is true, if any, be unselected", and every platform spells
    /// that request as a modified click. Without them the control had a correct
    /// algorithm and no way to reach it.
    ///
    /// Four bools rather than a struct, matching `process_key_event`, which
    /// already carries its modifiers this way — one convention for both halves
    /// of the input surface.
    pub fn process_mouse_event_with_modifiers(
        &mut self,
        etype: crate::dom::HtmlEventType,
        doc_pt: (f32, f32),
        button: u8,
        ctrl: bool,
        _shift: bool,
        _alt: bool,
        meta: bool,
    ) -> bool {
        // Ctrl on the platforms that use it, ⌘ on the one that does not — the
        // same pair every other modified gesture answers to.
        let unselect_request = ctrl || meta;
        use crate::dom::{HtmlEventType, HtmlEvent};
        // client_pos = screen-space logical coordinates (doc coords minus scroll).
        let client_pos = (doc_pt.0, doc_pt.1 - self.scroll_y);

        let mut evt = HtmlEvent::new(etype);
        evt.doc_pos    = doc_pt;
        evt.client_pos = client_pos;
        evt.button     = button;
        let hit_result = crate::layout::hit_test::point_to_hit(&self.root, doc_pt, button);
        let mut hit_node_id: u32 = hit_result.as_ref().map(|h| h.node_id).unwrap_or(0);
        // For inline links: check if the hit point is inside an inline run
        // with an href. If so, find the ancestor <a> element for hover styling.
        if let Some(ref hr) = hit_result {
            if let Some(hit_box) = self.get_node(hr.node_id) {
                for run in &hit_box.layout.inline_runs {
                    if hr.local_offset >= run.text_offset && hr.local_offset < run.text_offset + run.length {
                        if !run.style.href.is_empty() {
                            if let Some(link_id) = find_link_node_id(&self.root, &run.style.href) {
                                hit_node_id = link_id;
                            }
                        }
                        break;
                    }
                }
            }
        }
        evt.target = hit_node_id;
        

        let mut redraw = false;
        match etype {
            HtmlEventType::MouseMove => {
                // A held knob follows the pointer, wherever it has got to —
                // including outside the control, which is why this is keyed on
                // the element being HELD and not on what the move hit.
                if self.dragging_range != 0 {
                    let range_id = self.dragging_range;
                    if self.drag_range_to(range_id, doc_pt) {
                        redraw = true;
                    }
                }
                // After a hover-triggered relayout (e.g. dropdown opens), the
                // layout changes and re-hit-testing at the same mouse position may
                // find a different element, causing a feedback loop
                // (open → re-hit → close → re-hit → open …).
                // Suppress one hover change after each hover-triggered relayout.
                if self.hover_suppress_count > 0 {
                    self.hover_suppress_count -= 1;
                } else if self.hovered_box != hit_node_id {
                    self.hovered_box = hit_node_id;
                    self.hover_changed = true;
                    redraw = true;
                }
                // Track hover over open dropdown
                if self.open_select != 0 {
                    let open_sel_id = self.open_select;
                    let sel = match self.get_node(open_sel_id) {
                        Some(s) => s,
                        None => { self.open_select = 0; return redraw; }
                    };
                    let dropdown_y = sel.layout.border_rect.y + sel.layout.border_rect.h;
                    let font_px = sel.style.font_size_px(16.0, 16.0);
                    let item_h = font_px * 1.8;
                    let group_h = font_px * 1.5;
                    let mut y_acc = 0.0f32;
                    let mut new_hover: i32 = -1;
                    let mut opt_i = 0usize;
                    let rel_y = doc_pt.1 - dropdown_y - 4.0;
                    for child in &sel.children {
                        if child.tag == "option" {
                            if rel_y >= y_acc && rel_y < y_acc + item_h { new_hover = opt_i as i32; }
                            y_acc += item_h;
                            opt_i += 1;
                        } else if child.tag == "optgroup" {
                            y_acc += group_h;
                            for gc in &child.children {
                                if gc.tag == "option" {
                                    if rel_y >= y_acc && rel_y < y_acc + item_h { new_hover = opt_i as i32; }
                                    y_acc += item_h;
                                    opt_i += 1;
                                }
                            }
                        }
                    }
                    if new_hover != self.dropdown_hover_idx {
                        self.dropdown_hover_idx = new_hover;
                        redraw = true;
                    }
                }
                // Drag: if mouse button held and moved past threshold, fire DragStart/Drag.
                if self.drag_source != 0 {
                    let dx = doc_pt.0 - self.drag_start_doc_pt.0;
                    let dy = doc_pt.1 - self.drag_start_doc_pt.1;
                    if !self.drag_active && (dx * dx + dy * dy) > 25.0 {
                        // DragStart
                        self.drag_active = true;
                        let mut e = HtmlEvent::new(HtmlEventType::DragStart);
                        e.target = self.drag_source; e.doc_pos = self.drag_start_doc_pt;
                        e.client_pos = (self.drag_start_doc_pt.0, self.drag_start_doc_pt.1 - self.scroll_y);
                        if self.events.dispatch(&mut self.root, e) { redraw = true; }
                    }
                    if self.drag_active {
                        let mut e = HtmlEvent::new(HtmlEventType::Drag);
                        e.target = self.drag_source; e.doc_pos = doc_pt; e.client_pos = client_pos;
                        if self.events.dispatch(&mut self.root, e) { redraw = true; }
                    }
                }
            }
            HtmlEventType::MouseDown | HtmlEventType::PointerDown => {
                if self.active_box != hit_node_id {
                    self.active_box = hit_node_id;
                    redraw = true;
                }
                if etype == HtmlEventType::MouseDown {
                    self.mousedown_target  = hit_node_id;
                    // Arm drag state machine.
                    self.drag_source       = hit_node_id;
                    self.drag_start_doc_pt = doc_pt;
                    self.drag_active       = false;
                }
                // Focus change on click.
                // Only interactive (focusable) elements receive focus on click.
                // Clicking a non-focusable element blurs the current focus.
                if etype == HtmlEventType::MouseDown {
                    // Walk up from hit target to find the nearest focusable ancestor
                    let focus_target_id = if hit_node_id != 0 {
                        if let Some(hit) = self.get_node(hit_node_id) {
                            if is_focusable_node(hit) {
                                hit_node_id
                            } else {
                                find_form_parent_id(&self.root, hit_node_id)
                            }
                        } else { 0u32 }
                    } else { 0u32 };
                    let click_focusable = focus_target_id != 0 &&
                        self.get_node(focus_target_id)
                            .map(|fp| is_focusable_node(fp))
                            .unwrap_or(false);
                    let new_focus = if click_focusable { focus_target_id } else { 0u32 };
                    if self.focused_box != new_focus {
                        let old_focus = self.focused_box;
                        self.keyboard_focus = false;
                        self.focused_box = new_focus;
                        if old_focus != 0 {
                            let mut e = HtmlEvent::new(HtmlEventType::Blur);
                            e.target = old_focus; e.related_target = new_focus;
                            self.events.dispatch(&mut self.root, e);
                            let mut e = HtmlEvent::new(HtmlEventType::FocusOut);
                            e.target = old_focus; e.related_target = new_focus;
                            self.events.dispatch(&mut self.root, e);
                        }
                        if new_focus != 0 {
                            let mut e = HtmlEvent::new(HtmlEventType::Focus);
                            e.target = new_focus; e.related_target = old_focus;
                            self.events.dispatch(&mut self.root, e);
                            let mut e = HtmlEvent::new(HtmlEventType::FocusIn);
                            e.target = new_focus; e.related_target = old_focus;
                            self.events.dispatch(&mut self.root, e);
                        }
                        // Always recascade when focus changes so :focus/:focus-visible update.
                        self.stylesheet.rebuild_index();
                        crate::css::apply_cascade_vp(
                            &mut self.root, &self.stylesheet, None, 16.0,
                            self.viewport_w, self.viewport_h, self.focused_box, false,
                        );
                        redraw = true;
                    }
                }
                // **Grabbing a slider's knob.** A range is driven from the
                // PRESS, unlike every other control here, because its
                // interaction continues while the pointer moves — see
                // `Document::dragging_range`. The press itself already moves
                // the value: pressing anywhere on the track jumps the knob
                // there, which is what makes the first `input` fire before any
                // movement at all.
                let range_id = find_form_parent_id(&self.root, hit_node_id);
                if self.is_range_input(range_id)
                    && !self.get_node(range_id)
                        .map(|n| n.attributes.contains_key("disabled"))
                        .unwrap_or(true)
                {
                    self.range_drag_origin = self
                        .find_webcore(range_id)
                        .map(input_value)
                        .unwrap_or_default();
                    self.dragging_range = range_id;
                    if self.drag_range_to(range_id, doc_pt) {
                        redraw = true;
                    }
                }
            }
            HtmlEventType::MouseUp | HtmlEventType::PointerUp => {
                // Letting go of a knob. FIRST, so the last position the
                // pointer reached is the value that gets committed — and
                // before any of the click routing below, which must not see a
                // range at all.
                if self.dragging_range != 0 {
                    let range_id = self.dragging_range;
                    if self.drag_range_to(range_id, doc_pt) {
                        redraw = true;
                    }
                    self.commit_range_drag();
                }
                if self.active_box != 0 {
                    self.active_box = 0;
                    redraw = true;
                }
                if etype == HtmlEventType::MouseUp {
                    // DragEnd if drag was active; save flag before resetting.
                    let was_dragging = self.drag_active;
                    if was_dragging {
                            let mut e = HtmlEvent::new(HtmlEventType::DragEnd);
                        e.target = self.drag_source; e.doc_pos = doc_pt; e.client_pos = client_pos;
                        if self.events.dispatch(&mut self.root, e) { redraw = true; }
                    }
                    self.drag_source = 0;
                    self.drag_active = false;

                    // ⛔ **An open popup takes the click FIRST.**
                    //
                    // It is drawn over the page and is not in the tree, so the
                    // hit test below finds whatever happens to be UNDER it —
                    // and finds nothing at all where no element lies, which is
                    // most of the page. Gating a popup's click on that dropped
                    // every pick that landed past the end of the content.
                    //
                    // Either outcome closes it: a swatch picks, anywhere else
                    // dismisses, and neither reaches the page beneath.
                    if self.open_picker != 0 {
                        let picker_id = self.open_picker;
                        // What a pick MEANS depends on the control: a swatch is
                        // a colour, a cell is a date. Both write a value in the
                        // format that control's spec requires, and both close.
                        let picked = match self.picker_kind(picker_id) {
                            Some(PickerKind::Calendar) => self
                                .calendar_hit(picker_id, doc_pt)
                                .map(|(y, m, d)| crate::widgets::to_date_value(y, m, d)),
                            _ => self
                                .picker_hit(picker_id, doc_pt)
                                .map(crate::widgets::to_simple_colour),
                        };
                        if let Some(value) = picked {
                            self.set_value(picker_id, &value);
                            let (id, name) = self
                                .find_webcore(picker_id)
                                .map(|n| {
                                    (
                                        n.attributes.get("id").cloned().unwrap_or_default(),
                                        n.attributes.get("name").cloned().unwrap_or_default(),
                                    )
                                })
                                .unwrap_or_default();
                            if let Some(ref mut cb) = self.on_form_event {
                                cb(&FormEvent {
                                    tag: "input".to_string(),
                                    id,
                                    name,
                                    kind: FormEventKind::Change(value),
                                    element: picker_id,
                                });
                            }
                        }
                        self.open_picker = 0;
                        self.mousedown_target = 0;
                        return true;
                    }
                    // Click only if no drag occurred and released on same element as pressed.
                    //
                    // ⚠ An OPEN DROPDOWN relaxes the first half, for the reason
                    // the picker above is handled before this gate at all: the
                    // list is drawn over the page and is not in the tree, so a
                    // row that happens to hang past the end of the content had
                    // nothing under it, `hit_node_id` was 0, and the pick was
                    // dropped. It worked only where an element lay beneath.
                    //
                    // The branch it guards reads the click's Y against the
                    // select's own geometry and never consults `hit_node_id`,
                    // so letting it through costs nothing when the list is up.
                    if (hit_node_id != 0 && hit_node_id == self.mousedown_target || self.open_select != 0)
                        && !was_dragging
                    {
                        let mut click = HtmlEvent::new(HtmlEventType::Click);
                        click.target = hit_node_id; click.doc_pos = doc_pt; click.client_pos = client_pos;
                        click.button = button;
                        if self.events.dispatch(&mut self.root, click) { redraw = true; }

                        // Form element interactions
                        // The second half of the popup rule: an OPEN DROPDOWN
                        // is handled in here, and this gate excluded it for the
                        // same reason the outer one did — a row with no element
                        // beneath it has `hit_node_id == 0`. The form-click call
                        // still needs a real node and keeps its own check.
                        if (hit_node_id != 0 || self.open_select != 0) && button == 0 {
                            let form_click = (hit_node_id != 0)
                                .then(|| handle_form_click(&mut self.root, hit_node_id, &mut self.on_form_event))
                                .flatten();
                            // **EVERY element gets a `click`, not just the form
                            // controls.** `handle_form_click` answers `None` for
                            // anything it does not recognise, and that was the
                            // end of the road: a listener on a `<td>`, a `<div>`
                            // or an `<li>` was registered, was reachable, and
                            // never fired — so a composed control could be built
                            // out of ordinary elements and could not be clicked.
                            // UI Events puts `click` on the element the pointer
                            // pressed and released over, whatever it is.
                            if form_click.is_none() && hit_node_id != 0 {
                                self.fire_element_click(hit_node_id);
                            }
                            if let Some(form_redraw) = form_click {
                                if form_redraw { redraw = true; }
                                // `handle_form_click` takes `&mut WebCore`, so it
                                // wrote `checked` to the render tree only. Push it
                                // into the arena before anything reads the DOM
                                // through the WHATWG accessors — see
                                // `Document::sync_form_state_to_arena`.
                                self.sync_form_state_to_arena();
                                // **A state change is a STYLE change.** The
                                // cascade is cached, so `:checked` (and
                                // `:checked + label`, and every rule keyed off
                                // it) keeps whatever it computed BEFORE the
                                // click until something says otherwise. Ticking
                                // a box changed the state, painted the tick and
                                // left the styling on the previous frame's
                                // answer.
                                self.style_dirty = true;
                            }
                            // **Activating a `<summary>` toggles its `<details>`**
                        // (HTML §4.11.1). The summary already draws a pointer
                        // cursor and a disclosure marker, so the control looked
                        // interactive and did nothing — the cursor promised an
                        // interaction that was never wired.
                        if hit_node_id != 0 && button == 0 {
                            if let Some(details_id) = self.summary_details(hit_node_id) {
                                let open = self
                                    .find_webcore(details_id)
                                    .map(|n| n.attributes.contains_key("open"))
                                    .unwrap_or(false);
                                if open {
                                    self.remove_attribute(details_id, "open");
                                } else {
                                    self.set_attribute(details_id, "open", "");
                                }
                                // `details:not([open])` is a SELECTOR, so what
                                // is shown is a cascade decision — the same
                                // reason ticking a checkbox marks style dirty.
                                self.style_dirty = true;
                                redraw = true;
                            }
                        }

                        // Handle select dropdown
                            if self.open_select != 0 {
                                // Collect options from DOM children
                                let sel = self.get_node(self.open_select).unwrap();
                                let font_px = sel.style.font_size_px(16.0, 16.0);
                                let item_h = font_px * 1.8;
                                let group_h = font_px * 1.5;

                                // Count items (options + optgroups) for height
                                let mut opt_texts: Vec<String> = Vec::new();
                                let mut opt_values: Vec<String> = Vec::new();
                                let mut total_h = 8.0f32; // padding
                                for child in &sel.children {
                                    if child.tag == "option" {
                                        let txt: String = child.children.iter().filter(|c| c.tag == "#text").map(|c| c.text.as_str()).collect();
                                        let val = child.attributes.get("value").cloned().unwrap_or_else(|| txt.clone());
                                        opt_texts.push(txt.trim().to_string());
                                        opt_values.push(val.trim().to_string());
                                        total_h += item_h;
                                    } else if child.tag == "optgroup" {
                                        total_h += group_h;
                                        for gc in &child.children {
                                            if gc.tag == "option" {
                                                let txt: String = gc.children.iter().filter(|c| c.tag == "#text").map(|c| c.text.as_str()).collect();
                                                let val = gc.attributes.get("value").cloned().unwrap_or_else(|| txt.clone());
                                                opt_texts.push(txt.trim().to_string());
                                                opt_values.push(val.trim().to_string());
                                                total_h += item_h;
                                            }
                                        }
                                    }
                                }

                                let dropdown_y = sel.layout.border_rect.y + sel.layout.border_rect.h;
                                let popup_w = sel.layout.border_rect.w.max(150.0);
                                let click_y = doc_pt.1;
                                let click_x = doc_pt.0;

                                if click_y >= dropdown_y && click_y < dropdown_y + total_h
                                    && click_x >= sel.layout.border_rect.x && click_x < sel.layout.border_rect.x + popup_w
                                {
                                    // Determine which option was clicked
                                    let rel_y = click_y - dropdown_y - 4.0;
                                    let mut y_acc = 0.0f32;
                                    let mut clicked_opt: Option<usize> = None;
                                    let mut opt_i = 0usize;
                                    for child in &sel.children {
                                        if child.tag == "option" {
                                            if rel_y >= y_acc && rel_y < y_acc + item_h {
                                                clicked_opt = Some(opt_i);
                                                break;
                                            }
                                            y_acc += item_h;
                                            opt_i += 1;
                                        } else if child.tag == "optgroup" {
                                            y_acc += group_h;
                                            for gc in &child.children {
                                                if gc.tag == "option" {
                                                    if rel_y >= y_acc && rel_y < y_acc + item_h {
                                                        clicked_opt = Some(opt_i);
                                                        break;
                                                    }
                                                    y_acc += item_h;
                                                    opt_i += 1;
                                                }
                                            }
                                            if clicked_opt.is_some() { break; }
                                        }
                                    }

                                    if let Some(opt_idx) = clicked_opt {
                                        let sel_id = self.open_select;
                                        let new_text = opt_texts.get(opt_idx).cloned().unwrap_or_default();
                                        // The option's node_id, so the pick runs
                                        // over the spec's own list of options
                                        // rather than this popup's parallel
                                        // walk — the two counted optgroups the
                                        // same way, but only one of them is the
                                        // definition.
                                        let option_id = self
                                            .find_webcore(sel_id)
                                            .map(crate::html::forms::option_ids)
                                            .and_then(|ids| ids.get(opt_idx).copied());
                                        if let (Some(option_id), Some(sel_mut)) =
                                            (option_id, self.find_webcore_mut(sel_id))
                                        {
                                            let changed = crate::html::forms::pick_option(sel_mut, option_id);
                                            // The drop-down's shown text is a
                                            // child text node rather than a
                                            // repaint of the options.
                                            if let Some(tn) = sel_mut.children.iter_mut().rev().find(|c| c.tag == "#text") {
                                                tn.text = new_text;
                                            }
                                            sel_mut.layout.layout_dirty = true;
                                            if changed {
                                                // `option:checked` is a selector.
                                                self.style_dirty = true;
                                                self.send_select_update_notifications(sel_id);
                                            }
                                        }
                                    }
                                    self.open_select = 0;
                                    redraw = true;
                                } else {
                                    self.open_select = 0;
                                    redraw = true;
                                }
                            } else {
                                // Check if clicking a select to open it
                                let effective_id = find_form_parent_id(&self.root, hit_node_id);
                                let is_select = self.get_node(effective_id).map(|n| n.tag == "select").unwrap_or(false);
                                // ⛔ A LIST BOX HAS NO POPUP. Its rows are drawn
                                // inside the control, so a click picks one
                                // directly — routing it through `open_select`
                                // opened a phantom list over the page and
                                // selected nothing, ever.
                                let list_box = is_select
                                    && self.get_node(effective_id)
                                        .map(crate::html::forms::is_list_box)
                                        .unwrap_or(false);
                                if list_box {
                                    // ⛔ NO click event here. Every element gets
                                    // one from `fire_element_click` above —
                                    // `handle_form_click` returns `None` for a
                                    // `<select>`, so the generic path already
                                    // covered it. Firing a second would report
                                    // two clicks for one press.
                                    if self.click_list_box_row(effective_id, doc_pt.1, unselect_request) {
                                        // Selectedness is a SELECTOR
                                        // (`option:checked`), so the cascade has
                                        // to be told, exactly as ticking a
                                        // checkbox does.
                                        self.style_dirty = true;
                                        redraw = true;
                                    }
                                } else if is_select {
                                    self.open_select = effective_id;
                                    redraw = true;
                                } else if self.is_range_input(effective_id) {
                                    // ⛔ NOTHING on release. A range is the one
                                    // control driven from the press, because a
                                    // drag is a press, a path and a release —
                                    // handling it here as well would move the
                                    // knob a second time to wherever the
                                    // pointer happened to end.
                                } else if self.picker_kind(effective_id).is_some() {
                                    // Activating the control opens its picker —
                                    // HTML leaves each picker's FORM to the
                                    // user agent and says only that one is
                                    // offered.
                                    self.open_picker = effective_id;
                                    redraw = true;
                                }
                            }
                        }

                        // DblClick: same target within 400 ms.
                        let now = std::time::Instant::now();
                        let is_dbl = self.last_click_target == hit_node_id
                            && self.last_click_time
                                .map(|t| t.elapsed().as_millis() < 400)
                                .unwrap_or(false);
                        if is_dbl {
                            let mut dbl = HtmlEvent::new(HtmlEventType::DblClick);
                            dbl.target = hit_node_id; dbl.doc_pos = doc_pt; dbl.client_pos = client_pos;
                            dbl.button = button;
                            if self.events.dispatch(&mut self.root, dbl) { redraw = true; }
                            // Reset so triple-click doesn't re-trigger.
                            self.last_click_target = 0;
                            self.last_click_time   = None;
                        } else {
                            self.last_click_target = hit_node_id;
                            self.last_click_time   = Some(now);
                        }
                    }
                    self.mousedown_target = 0;
                    // Track visited links + fire on_navigate callback.
                    if button == 0 {
                        if let Some(href) = crate::layout::hit_test::hit_test_link(&self.root, doc_pt, button) {
                            self.visited_urls.insert(href.clone());
                            if let Some(ref mut cb) = self.on_navigate {
                                cb(&href);
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        let (handled, evt) = self.events.dispatch_and_return(&mut self.root, evt);
        if handled { redraw = true; }

        // Also dispatch through the NodeId-based event system (capture/bubble).
        if evt.target != 0 {
            let mut dom_evt = crate::dom::events::DomEvent::new(
                etype.as_str(), evt.target);
            dom_evt.client_x = client_pos.0;
            dom_evt.client_y = client_pos.1;
            dom_evt.button = button;
            if self.event_targets.dispatch_on_tree(&self.root, &mut dom_evt) {
                redraw = true;
            }
        }

        // Only perform editor/default behavior if not prevented by handlers.
        if !evt.default_prevented {
            if self.editor.handle_mouse_event(&self.root, etype, doc_pt, button) {
                redraw = true;
            }
        }

        // Full cascade + layout only when event handlers or editor logic changed
        // DOM state (class toggles, etc.), not merely for hover/active pointer updates.
        if handled {
            let width = self.root.layout.last_containing_width.max(0.0);
            self.recascade();
            LayoutEngine::new().layout(self, width);
        }

        redraw
    }

    /// Dispatch `MouseOver` on the new hover target and `MouseOut` on the previous one.
    /// Called from the renderer on every `CursorMoved` event.
    /// Returns `true` if listeners were fired (caller should redraw).
    pub fn dispatch_over_out(&mut self, doc_pt: (f32, f32)) -> bool {
        use crate::dom::{HtmlEventType, HtmlEvent};
        let client_pos = (doc_pt.0, doc_pt.1 - self.scroll_y);
        let new_id: u32 = crate::layout::hit_test::point_to_hit(&self.root, doc_pt, 0)
            .map(|h| h.node_id)
            .unwrap_or(0);
        let old_id = self.hovered_box;
        if new_id == old_id { return false; }

        let mut redraw = false;
        macro_rules! ev {
            ($t:expr_2021, $tgt:expr_2021, $rel:expr_2021, $bubble:expr_2021) => {{
                let mut e = HtmlEvent::new($t);
                e.target = $tgt; e.related_target = $rel;
                e.doc_pos = doc_pt; e.client_pos = client_pos;
                if $bubble { self.events.dispatch(&mut self.root, e) }
                else       { self.events.dispatch_direct(&mut self.root, e) }
            }};
        }
        if old_id != 0 {
            if ev!(HtmlEventType::MouseOut,    old_id, new_id, true)  { redraw = true; }
            if ev!(HtmlEventType::MouseLeave,  old_id, new_id, false) { redraw = true; }
            ev!(HtmlEventType::PointerOut,   old_id, new_id, true);
            ev!(HtmlEventType::PointerLeave, old_id, new_id, false);
        }
        if new_id != 0 {
            if ev!(HtmlEventType::MouseOver,   new_id, old_id, true)  { redraw = true; }
            if ev!(HtmlEventType::MouseEnter,  new_id, old_id, false) { redraw = true; }
            ev!(HtmlEventType::PointerOver,  new_id, old_id, true);
            ev!(HtmlEventType::PointerEnter, new_id, old_id, false);
        }

        // Also dispatch through NodeId-based event system
        if old_id != 0 {
            let mut e = crate::dom::events::DomEvent::new("mouseout", old_id);
            e.related_target = new_id;
            e.client_x = client_pos.0; e.client_y = client_pos.1;
            self.event_targets.dispatch_on_tree(&self.root, &mut e);
        }
        if new_id != 0 {
            let mut e = crate::dom::events::DomEvent::new("mouseover", new_id);
            e.related_target = old_id;
            e.client_x = client_pos.0; e.client_y = client_pos.1;
            self.event_targets.dispatch_on_tree(&self.root, &mut e);
        }

        redraw
    }

    /// High-level keyboard event entry point.
    pub fn process_key_event(
        &mut self,
        etype: crate::dom::HtmlEventType,
        key_code: u32,
        ch: Option<char>,
        ctrl: bool,
        shift: bool,
        alt: bool,
        meta: bool,
    ) -> bool {
        // Dispatch to listeners first so they can prevent default handling.
        let mut evt = crate::dom::HtmlEvent::new(etype);
        evt.key_code = key_code;
        evt.char_code = ch;
        evt.ctrl_key = ctrl;
        evt.shift_key = shift;
        evt.alt_key = alt;
        evt.meta_key = meta;

        let (handled, evt) = self.events.dispatch_and_return(&mut self.root, evt);

        // Also dispatch through NodeId-based event system (capture/bubble).
        let target = if self.focused_box != 0 { self.focused_box } else { self.root.node_id };
        {
            let mut dom_evt = crate::dom::events::DomEvent::new(etype.as_str(), target);
            dom_evt.key_code = key_code;
            dom_evt.char_code = ch;
            dom_evt.ctrl_key = ctrl;
            dom_evt.shift_key = shift;
            dom_evt.alt_key = alt;
            dom_evt.meta_key = meta;
            if self.event_targets.dispatch_on_tree(&self.root, &mut dom_evt) {
                // handled via new system
            }
        }

        let mut redraw = handled;

        if !evt.default_prevented {
            // Check if a form input is focused — route keys there first
            let form_handled = if self.focused_box != 0
                && etype == crate::dom::HtmlEventType::KeyDown
            {
                let focused = self.get_node(self.focused_box).unwrap_or(&self.root);
                // Select: arrow up/down changes selected option
                if focused.tag == "select" && (key_code == 38 || key_code == 40) {
                    let fid = self.focused_box;
                    // Arrow keys move to the next or previous option and PICK
                    // it, which is the same algorithm a click runs — HTML lists
                    // "through a menu command, or through any other mechanism"
                    // alongside the click for exactly this reason.
                    let options = self
                        .find_webcore(fid)
                        .map(crate::html::forms::option_ids)
                        .unwrap_or_default();
                    if !options.is_empty() {
                        // With nothing selected — a list box's resting state —
                        // Down starts at the first option and Up at the last.
                        let cur = self.find_webcore(fid).map(crate::html::forms::selected_index).unwrap_or(-1);
                        let new_idx = if cur < 0 {
                            if key_code == 40 { 0 } else { options.len() - 1 }
                        } else if key_code == 40 {
                            ((cur as usize) + 1).min(options.len() - 1)
                        } else {
                            (cur as usize).saturating_sub(1)
                        };
                        let option_id = options[new_idx];
                        let new_text = self
                            .find_webcore(option_id)
                            .map(crate::html::forms::option_label)
                            .unwrap_or_default();
                        if let Some(sel) = self.find_webcore_mut(fid) {
                            let changed = crate::html::forms::pick_option(sel, option_id);
                            if let Some(tn) = sel.children.iter_mut().rev().find(|c| c.tag == "#text") {
                                tn.text = new_text;
                            }
                            sel.layout.layout_dirty = true;
                            if changed {
                                self.style_dirty = true;
                                self.send_select_update_notifications(fid);
                            }
                        }
                    }
                    true
                }
                // Number input: arrow up/down increments/decrements
                else if focused.tag == "input"
                    && focused.attributes.get("type").map(|s| s.as_str()) == Some("number")
                    && (key_code == 38 || key_code == 40)
                {
                    let fid = self.focused_box;
                    fn find_n<'a>(n: &'a mut WebCore, t: u32) -> Option<&'a mut WebCore> {
                        if n.node_id == t { return Some(n); }
                        for c in &mut n.children { if let Some(r) = find_n(c, t) { return Some(r); } }
                        None
                    }
                    if let Some(input) = find_n(&mut self.root, fid) {
                        // Read the VALUE, not the default value — an arrow key
                        // after typing used to step from whatever the markup
                        // said rather than from what the field shows.
                        let val: f64 = crate::html::forms::parse_floating_point(&input_value(input)).unwrap_or(0.0);
                        let step: f64 = input.attributes.get("step").and_then(|s| s.parse().ok()).unwrap_or(1.0);
                        let min: Option<f64> = input.attributes.get("min").and_then(|s| s.parse().ok());
                        let max: Option<f64> = input.attributes.get("max").and_then(|s| s.parse().ok());
                        let new_val = if key_code == 38 { val + step } else { val - step };
                        let new_val = if let Some(mx) = max { new_val.min(mx) } else { new_val };
                        let new_val = if let Some(mn) = min { new_val.max(mn) } else { new_val };
                        if new_val != val {
                            input.value_state = Some(crate::html::forms::best_representation(new_val));
                            input.dirty_value = true;
                            input.layout.layout_dirty = true;
                        }
                    }
                    true
                }
                else if is_text_input(focused) {
                    // Find the focused node mutably and process the key
                    let fid = self.focused_box;
                    fn find_input<'a>(n: &'a mut WebCore, t: u32) -> Option<&'a mut WebCore> {
                        if n.node_id == t { return Some(n); }
                        for c in &mut n.children {
                            if let Some(r) = find_input(c, t) { return Some(r); }
                        }
                        None
                    }
                    if let Some(input) = find_input(&mut self.root, fid) {
                        let changed = process_form_input_key(input, key_code, ch, ctrl, shift);
                        // Reset caret blink so it stays visible while typing
                        self.caret_blink_epoch = std::time::Instant::now();
                        if changed {
                            // Fire form event callback
                            if let Some(ref mut cb) = self.on_form_event {
                                cb(&FormEvent {
                                    tag: input.tag.clone(),
                                    id: input.attributes.get("id").cloned().unwrap_or_default(),
                                    name: input.attributes.get("name").cloned().unwrap_or_default(),
                                    kind: FormEventKind::Input(input_value(input)),
                                    element: fid,
                                });
                            }
                        }
                        changed
                    } else { false }
                } else { false }
            } else { false };

            if form_handled {
                redraw = true;
                // Typing went through `process_form_input_key`, which takes
                // `&mut WebCore` and so updated `value` on the render tree
                // only. Reconcile before the DOM is read. Deferred to here
                // rather than done at the call site because `input` borrows
                // `self.root` for the whole block above.
                self.sync_form_state_to_arena();
                // Typing changes `:in-range`, `:valid` and friends the same way
                // a click changes `:checked` — see the note at the click path.
                self.style_dirty = true;
            } else if self.editor.handle_key_event(&mut self.root, etype, key_code, ch, ctrl) {
                redraw = true;
            }
        }

        redraw
    }

    /// Walk all boxes in depth-first order.
    pub fn walk_all<F: FnMut(&WebCore)>(root: &WebCore, f: &mut F) {
        f(root);
        for child in &root.children {
            Self::walk_all(child, f);
        }
    }

    pub fn walk_all_mut<F: FnMut(&mut WebCore)>(root: &mut WebCore, f: &mut F) {
        f(root);
        for child in &mut root.children {
            Self::walk_all_mut(child, f);
        }
    }

    /// Compute the full scrollable extent of the document.
    /// Walks all elements and returns the maximum bottom/right edge,
    /// ignoring containers with `height: 100vh` or similar constraints.
    pub fn scroll_height(root: &WebCore) -> f32 {
        fn walk_scroll(node: &WebCore, max_bottom: &mut f32) {
            if matches!(node.style.display, Display::None) { return; }
            // Fixed elements don't contribute to scroll height
            if matches!(node.style.position, Position::Fixed) { return; }
            // Skip zero-size nodes (not yet laid out or collapsed)
            if node.layout.margin_rect.w == 0.0 && node.layout.margin_rect.h == 0.0 { return; }
            // Absolute elements contribute only if they're within the document flow area
            // (some abs elements are positioned far off-screen as accessibility hacks)
            if matches!(node.style.position, Position::Absolute) {
                // Only count if within a reasonable range (2x the current max)
                let bottom = node.layout.margin_rect.y + node.layout.margin_rect.h;
                if bottom > 0.0 && bottom < *max_bottom * 3.0 + 2000.0 {
                    if bottom > *max_bottom { *max_bottom = bottom; }
                }
                // Don't recurse into abs children — they position relative to their CB
                return;
            }
            let bottom = node.layout.margin_rect.y + node.layout.margin_rect.h;
            if bottom > *max_bottom { *max_bottom = bottom; }
            for child in &node.children {
                walk_scroll(child, max_bottom);
            }
        }
        let mut max_bottom = root.layout.margin_rect.h;
        walk_scroll(root, &mut max_bottom);
        max_bottom
    }

    // ── aria-live ──────────────────────────────────────────────────────────────

    /// Drain and return all pending aria-live announcements.
    ///
    /// Call this after each layout pass and deliver the announcements to the
    /// platform (e.g. a screen reader via accesskit, a toast notification in a
    /// browser chrome, or a system alert).
    ///
    /// ```ignore
    /// for ann in doc.take_announcements() {
    ///     match ann.politeness {
    ///         LivePoliteness::Assertive => speak_immediately(&ann.text),
    ///         LivePoliteness::Polite    => speak_when_idle(&ann.text),
    ///         LivePoliteness::Off       => {}
    ///     }
    /// }
    /// ```
    pub fn take_announcements(&mut self) -> Vec<Announcement> {
        std::mem::take(&mut self.pending_announcements)
    }

    /// Scan all `aria-live` regions in the document, compare their text content
    /// to the snapshot from the previous call, and queue announcements for any
    /// regions whose content has changed.
    ///
    /// This is called automatically by `LayoutEngine::layout`.  You only need
    /// to call it manually if you modify the DOM outside of a layout pass.
    pub fn check_live_regions(&mut self) {
        let initialized = self.live_regions_initialized;
        let mut new_ann: Vec<Announcement> = Vec::new();

        fn walk(
            node:         &WebCore,
            snapshots:    &mut HashMap<u32, String>,
            out:          &mut Vec<Announcement>,
            initialized:  bool,
        ) {
            let politeness = match node.attributes.get("aria-live").map(|s| s.as_str()) {
                Some("assertive") => LivePoliteness::Assertive,
                Some("polite")    => LivePoliteness::Polite,
                _                 => LivePoliteness::Off,
            };

            if politeness != LivePoliteness::Off {
                // aria-busy: region is being updated, defer announcement
                let busy = node.attributes.get("aria-busy")
                    .map(|v| v == "true").unwrap_or(false);
                if !busy {
                    let ptr   = node.node_id;
                    let text  = collect_live_text(node);
                    let atomic = node.attributes.get("aria-atomic")
                        .map(|v| v == "true").unwrap_or(false);

                    match snapshots.get(&ptr) {
                        None => {
                            // First time seeing this region.
                            snapshots.insert(ptr, text.clone());
                            // Assertive regions announce on page load; polite ones
                            // are silently initialised so they don't flood the user.
                            if !initialized
                                && politeness == LivePoliteness::Assertive
                                && !text.is_empty()
                            {
                                out.push(Announcement { text, politeness, atomic });
                            }
                        }
                        Some(prev) if *prev != text => {
                            // Content changed since last layout pass.
                            let changed = text.clone();
                            snapshots.insert(ptr, text);
                            if !changed.is_empty() {
                                out.push(Announcement { text: changed, politeness, atomic });
                            }
                        }
                        _ => {} // No change, no announcement.
                    }
                }
                // Treat the live region as an atomic unit — don't recurse into it
                // looking for nested live regions (that would produce double announcements).
                return;
            }

            for child in &node.children {
                walk(child, snapshots, out, initialized);
            }
        }

        walk(&self.root, &mut self.live_region_snapshots, &mut new_ann, initialized);

        self.live_regions_initialized = true;
        self.pending_announcements.extend(new_ann);
    }

    // ── CSS Animation / Transition runtime ────────────────────────────────────

    /// Walk the tree and ensure an `AnimState` exists for every element that
    /// currently has an `animation` property.  Call this after each cascade pass.
    pub fn sync_animations(&mut self, now: std::time::Instant) {
        let mut current: Vec<(u32, ParsedAnimation)> = Vec::new();
        fn collect(node: &WebCore, out: &mut Vec<(u32, ParsedAnimation)>) {
            let id = node.node_id;
            for a in &node.style.animations {
                out.push((id, a.clone()));
            }
            for child in &node.children { collect(child, out); }
        }
        collect(&self.root, &mut current);

        // Start animations that aren't tracked yet.
        for (id, anim) in &current {
            let running = self.active_animations.iter()
                .any(|s| s.element_id == *id && s.animation.name == anim.name);
            if !running && !anim.name.is_empty() && anim.name != "none" {
                self.active_animations.push(AnimState {
                    element_id: *id,
                    animation:  anim.clone(),
                    start_time: now,
                });
            }
        }

        // Remove animations whose element no longer carries that animation name.
        self.active_animations.retain(|s| {
            current.iter().any(|(id, a)| *id == s.element_id && a.name == s.animation.name)
        });
    }

    /// Detect CSS property changes caused by the cascade and start transitions.
    /// `cascade_ran`: true when the full cascade just ran (node.style is clean).
    /// When false (hover-only change), base values are read from `cascade_styles`
    /// so animation-overridden node.style values don't pollute change detection.
    pub fn sync_transitions(&mut self, now: std::time::Instant, cascade_ran: bool) {
        let hovered = self.hovered_box;
        let mut current: Vec<(u32, Vec<ParsedTransition>, HashMap<String, String>)> = Vec::new();
        fn collect(
            node: &WebCore,
            hovered: u32,
            cascade_ran: bool,
            cascade_styles: &HashMap<u32, HashMap<String, String>>,
            out: &mut Vec<(u32, Vec<ParsedTransition>, HashMap<String, String>)>,
        ) {
            let id = node.node_id;
            if !node.style.transitions.is_empty() {
                // Base values: use the clean cascade snapshot when available, so that
                // animation_overrides applied to node.style don't corrupt detection.
                let base = if cascade_ran {
                    extract_transitionable(node)
                } else {
                    cascade_styles.get(&id)
                        .cloned()
                        .unwrap_or_else(|| extract_transitionable(node))
                };
                let mut vals = base.clone();
                // When hovered, overlay hover_style to get the "target" state.
                if hovered != 0 && subtree_contains_id(node, hovered) {
                    if let Some(hs) = &node.style.hover_style {
                        let hover_vals = extract_transitionable_style(hs);
                        for (k, v) in hover_vals { vals.insert(k, v); }
                    }
                }
                out.push((id, node.style.transitions.clone(), vals));
            }
            for child in &node.children {
                collect(child, hovered, cascade_ran, cascade_styles, out);
            }
        }
        collect(&self.root, hovered, cascade_ran, &self.cascade_styles, &mut current);

        // When cascade ran, save the clean base styles for hover-only frames.
        if cascade_ran {
            fn snapshot(node: &WebCore, out: &mut HashMap<u32, HashMap<String, String>>) {
                if !node.style.transitions.is_empty() {
                    out.insert(node.node_id, extract_transitionable(node));
                }
                for child in &node.children { snapshot(child, out); }
            }
            snapshot(&self.root, &mut self.cascade_styles);
        }

        for (elem_id, trs, cur_vals) in &current {
            let prev = self.prev_styles.get(elem_id).cloned().unwrap_or_default();

            for tr in trs {
                if tr.duration_ms <= 0.0 { continue; }
                let props: Vec<&str> = if tr.property == "all" {
                    cur_vals.keys().map(|s| s.as_str()).collect()
                } else {
                    vec![tr.property.as_str()]
                };

                for prop in props {
                    let cur = match cur_vals.get(prop) { Some(v) => v.as_str(), None => continue };
                    let prv = match prev.get(prop) { Some(v) => v.as_str(), None => { continue; } };
                    if prv == cur {
                        // Uncomment to debug: eprintln!("[TR-SKIP] {} same={:?}", prop, cur);
                        continue;
                    }

                    // Already transitioning to this value?
                    let already = self.transition_states
                        .entry(*elem_id).or_default()
                        .iter().any(|t| t.property == prop && t.to_value == cur);
                    if already { continue; }

                    // If a transition is already running for this property, start the
                    // new one from the current animated value (not from prev_styles) to
                    // avoid a visual jump to the original from/to endpoint.
                    let from_val = self.animation_overrides
                        .get(elem_id)
                        .and_then(|ov| ov.iter().find(|(p, _)| p == prop))
                        .map(|(_, v)| v.as_str())
                        .unwrap_or(prv);
                    let entry = self.transition_states.entry(*elem_id).or_default();
                    entry.retain(|t| t.property != prop);
                    entry.push(TransitionState {
                        property:    prop.to_string(),
                        from_value:  from_val.to_string(),
                        to_value:    cur.to_string(),
                        start_time:  now,
                        duration_ms: tr.duration_ms,
                        delay_ms:    tr.delay_ms,
                        timing_fn:   tr.timing_fn.clone(),
                    });
                }
            }
            self.prev_styles.insert(*elem_id, cur_vals.clone());
        }
    }

    /// Advance all running animations and transitions to time `now`.
    /// Populates `animation_overrides` with interpolated CSS values.
    /// Sets `needs_animation_frame = true` if any animation/transition is still running.
    pub fn tick_animations(&mut self, now: std::time::Instant) {
        self.animation_overrides.clear();
        let keyframes = self.stylesheet.keyframes.clone();
        let mut still_running = false;

        // ── CSS Animations ───────────────────────────────────────────────────
        let mut done: Vec<usize> = Vec::new();
        for (idx, state) in self.active_animations.iter().enumerate() {
            let elapsed_ms = now.duration_since(state.start_time).as_secs_f32() * 1000.0;
            let delayed_ms = elapsed_ms - state.animation.delay_ms;

            if delayed_ms < 0.0 {
                // Delay phase: apply backwards fill if needed.
                if matches!(state.animation.fill_mode, FillMode::Backwards | FillMode::Both) {
                    if let Some(kf) = keyframes.get(&state.animation.name) {
                        if let Some(first) = kf.first() {
                            let entry = self.animation_overrides.entry(state.element_id).or_default();
                            entry.extend(first.properties.clone());
                        }
                    }
                }
                still_running = true;
                continue;
            }

            let duration = state.animation.duration_ms;
            if duration <= 0.0 { done.push(idx); continue; }

            let total_progress = delayed_ms / duration;
            let iteration      = total_progress.floor();
            let t_frac         = total_progress.fract();

            if iteration >= state.animation.iteration_count {
                // Finished: apply forwards fill if needed.
                if matches!(state.animation.fill_mode, FillMode::Forwards | FillMode::Both) {
                    if let Some(kf) = keyframes.get(&state.animation.name) {
                        let final_t = match state.animation.direction {
                            AnimDirection::Reverse | AnimDirection::AlternateReverse => 0.0,
                            _ => 1.0,
                        };
                        let props = interpolate_keyframe_stops(kf, final_t);
                        let entry = self.animation_overrides.entry(state.element_id).or_default();
                        entry.extend(props);
                    }
                }
                done.push(idx);
                continue;
            }
            still_running = true;

            let effective_t = match state.animation.direction {
                AnimDirection::Normal          => t_frac,
                AnimDirection::Reverse         => 1.0 - t_frac,
                AnimDirection::Alternate       => if (iteration as u32) % 2 == 0 { t_frac } else { 1.0 - t_frac },
                AnimDirection::AlternateReverse => if (iteration as u32) % 2 == 0 { 1.0 - t_frac } else { t_frac },
            };
            let eased = apply_easing(&state.animation.timing_fn, effective_t);

            if let Some(kf) = keyframes.get(&state.animation.name) {
                let props = interpolate_keyframe_stops(kf, eased);
                let entry = self.animation_overrides.entry(state.element_id).or_default();
                entry.extend(props);
            }
        }
        for idx in done.into_iter().rev() { self.active_animations.remove(idx); }

        // ── CSS Transitions ──────────────────────────────────────────────────
        let mut empty_elems: Vec<u32> = Vec::new();
        for (elem_id, trs) in &mut self.transition_states {
            let mut done_trs: Vec<usize> = Vec::new();
            for (i, tr) in trs.iter().enumerate() {
                let elapsed_ms = now.duration_since(tr.start_time).as_secs_f32() * 1000.0;
                let delayed_ms = elapsed_ms - tr.delay_ms;

                if delayed_ms < 0.0 {
                    // Apply "from" value during delay.
                    let entry = self.animation_overrides.entry(*elem_id).or_default();
                    entry.push((tr.property.clone(), tr.from_value.clone()));
                    still_running = true;
                    continue;
                }
                if tr.duration_ms <= 0.0 { done_trs.push(i); continue; }

                let progress = (delayed_ms / tr.duration_ms).min(1.0);
                if progress >= 1.0 {
                    // Write the final value into animation_overrides so that
                    // transitioning_ids still contains this element for the
                    // completion frame.  Without this, has_transition becomes
                    // false while is_hovered may still be true, causing the
                    // renderer to pick hover_style's color instead of the
                    // correctly-reverted base color.
                    let entry = self.animation_overrides.entry(*elem_id).or_default();
                    entry.push((tr.property.clone(), tr.to_value.clone()));
                    done_trs.push(i); continue;
                }

                still_running = true;
                let eased  = apply_easing(&tr.timing_fn, progress);
                let interp = interpolate_value(&tr.from_value, &tr.to_value, eased);
                let entry  = self.animation_overrides.entry(*elem_id).or_default();
                entry.push((tr.property.clone(), interp));
            }
            for idx in done_trs.into_iter().rev() { trs.remove(idx); }
            if trs.is_empty() { empty_elems.push(*elem_id); }
        }
        for eid in empty_elems { self.transition_states.remove(&eid); }

        self.needs_animation_frame = still_running;

        // Mark all elements with active overrides as layout_dirty so the
        // layout cache doesn't return stale geometry for animated elements.
        if !self.animation_overrides.is_empty() {
            fn mark_dirty(node: &mut WebCore, ids: &HashMap<u32, Vec<(String, String)>>) {
                if ids.contains_key(&node.node_id) {
                    node.layout.layout_dirty = true;
                }
                for child in &mut node.children {
                    mark_dirty(child, ids);
                }
            }
            mark_dirty(&mut self.root, &self.animation_overrides);
        }
    }

    /// Handle a mouse event for scrollbars (click, drag, release).
    ///
    /// Call this **before** `process_mouse_event` on every mouse down/move/up.
    /// Coordinates are in **screen-space logical pixels** (physical / scale),
    /// i.e. *without* any scroll offset added — the same values you get from
    /// `(position.x as f32 / scale, position.y as f32 / scale)`.
    ///
    /// `viewport_w` and `viewport_h` are the logical viewport dimensions.
    /// Returns `true` if the event was consumed by a scrollbar (no further
    /// processing needed).
    pub fn process_scrollbar_event(
        &mut self,
        etype:      crate::dom::HtmlEventType,
        screen_x:   f32,
        screen_y:   f32,
        viewport_w: f32,
        viewport_h: f32,
    ) -> bool {
        use crate::dom::HtmlEventType::*;
        const SBW: f32 = 10.0; // must match renderer::SCROLLBAR_WIDTH

        match etype {
            // ── MouseMove: continue drag ──────────────────────────────────────
            MouseMove => {
                if let Some(ref drag) = self.scrollbar_drag {
                    let dy = screen_y - drag.start_mouse_y;
                    let new_scroll = (drag.start_scroll + dy * drag.scroll_per_px).max(0.0);
                    match drag.kind {
                        ScrollbarDragKind::Viewport => {
                            let doc_h = Document::scroll_height(&self.root).max(self.root.layout.margin_rect.h);
                            let max_s = (doc_h - viewport_h).max(0.0);
                            self.scroll_y = new_scroll.min(max_s);
                        }
                        ScrollbarDragKind::Element(nid) => {
                            if let Some(node) = self.get_box_by_id_mut(nid) {
                                let max_s = (node.layout.scroll_height - node.layout.content_rect.h).max(0.0);
                                node.layout.scroll_top = new_scroll.min(max_s);
                            }
                        }
                    }
                    return true;
                }
                false
            }

            // ── MouseUp: end drag ─────────────────────────────────────────────
            MouseUp => {
                let was_dragging = self.scrollbar_drag.is_some();
                self.scrollbar_drag = None;
                was_dragging
            }

            // ── MouseDown: hit-test scrollbars, start drag ────────────────────
            MouseDown => {
                // Viewport scrollbar — right edge of window.
                let doc_h = Document::scroll_height(&self.root).max(self.root.layout.margin_rect.h);
                if doc_h > viewport_h && screen_x >= viewport_w - SBW {
                    let track_h = viewport_h;
                    let thumb_h = (track_h * track_h / doc_h).max(20.0);
                    let max_s   = (doc_h - viewport_h).max(0.0);
                    let scale   = if track_h - thumb_h > 0.0 { max_s / (track_h - thumb_h) } else { 0.0 };
                    let thumb_y = if max_s > 0.0 { self.scroll_y * (track_h - thumb_h) / max_s } else { 0.0 };

                    // Click in track but outside thumb → jump to that position.
                    if !(screen_y >= thumb_y && screen_y < thumb_y + thumb_h) {
                        let new_thumb_y = (screen_y - thumb_h * 0.5).max(0.0).min(track_h - thumb_h);
                        self.scroll_y = (new_thumb_y * scale).min(max_s).max(0.0);
                    }
                    let thumb_y = if max_s > 0.0 { self.scroll_y * (track_h - thumb_h) / max_s } else { 0.0 };
                    self.scrollbar_drag = Some(ScrollbarDrag {
                        kind:          ScrollbarDragKind::Viewport,
                        start_mouse_y: screen_y,
                        start_scroll:  self.scroll_y,
                        scroll_per_px: scale,
                    });
                    let _ = thumb_y;
                    return true;
                }

                // Per-element scrollbars — walk tree looking for scrollbar hit.
                // We pass accumulated offsets (sx, sy) matching the renderer.
                let sy = self.scroll_y;
                let sx = self.scroll_x;
                if scrollbar_hit_test(
                    &mut self.root, screen_x, screen_y, sx, sy,
                    SBW, &mut self.scrollbar_drag,
                ) {
                    return true;
                }

                false
            }

            _ => false,
        }
    }

    /// Handle a wheel/scroll event.
    ///
    /// `doc_pt` is the cursor position in document coordinates.
    /// `delta_y` is the vertical scroll amount in logical pixels (negative = scroll down,
    /// positive = scroll up).  Horizontal scroll is handled internally by the renderer via
    /// `process_wheel_event_xy`.
    ///
    /// Finds the innermost scrollable box under the cursor and scrolls it.
    /// Respects `overscroll-behavior` to control scroll chaining.
    /// Returns `true` if any scroll position changed.
    pub fn process_wheel_event(&mut self, doc_pt: (f32, f32), delta_y: f32) -> bool {
        self.process_wheel_event_xy(doc_pt, 0.0, delta_y)
    }

    /// Like `process_wheel_event` but also accepts a horizontal delta.
    /// Used by the renderer when handling trackpad/horizontal wheel events.
    pub fn process_wheel_event_xy(&mut self, doc_pt: (f32, f32), delta_x: f32, delta_y: f32) -> bool {
        if scroll_box_at(&mut self.root, doc_pt, delta_x, delta_y) {
            return true;
        }
        // Viewport fallback — renderer will clamp on next render.
        let old_x = self.scroll_x;
        let old_y = self.scroll_y;
        self.scroll_x -= delta_x;
        self.scroll_y -= delta_y;
        self.scroll_x != old_x || self.scroll_y != old_y || delta_x != 0.0 || delta_y != 0.0
    }

    /// Move keyboard focus to the next focusable element (Tab key).
    pub fn focus_next(&mut self) -> bool { self.shift_tab_focus(false) }

    /// Move keyboard focus to the previous focusable element (Shift+Tab).
    pub fn focus_prev(&mut self) -> bool { self.shift_tab_focus(true) }

    fn shift_tab_focus(&mut self, reverse: bool) -> bool {
        // Build the tab order: elements with explicit tabindex > 0 come first
        // (sorted ascending), then native-focusable and tabindex=0 in document order.
        // Elements with tabindex=-1 are excluded (focusable by script, not keyboard).
        let mut positive: Vec<(u32, i32)> = Vec::new();
        let mut normal:   Vec<u32>       = Vec::new();
        collect_focusable_ordered(&self.root, &mut positive, &mut normal);
        positive.sort_by_key(|&(_, idx)| idx);
        let focusable: Vec<u32> =
            positive.into_iter().map(|(p, _)| p).chain(normal).collect();
        if focusable.is_empty() { return false; }

        let current = self.focused_box;
        let pos = focusable.iter().position(|&p| p == current);
        let next = match pos {
            None => if reverse { focusable.len() - 1 } else { 0 },
            Some(i) => {
                if reverse {
                    if i == 0 { focusable.len() - 1 } else { i - 1 }
                } else {
                    if i + 1 >= focusable.len() { 0 } else { i + 1 }
                }
            }
        };
        let new_focus = focusable[next];
        let old_focus = self.focused_box;
        if new_focus == old_focus { return false; }

        self.keyboard_focus = true;
        self.focused_box = new_focus;
        if old_focus != 0 {
            let mut e = HtmlEvent::new(HtmlEventType::Blur);
            e.target = old_focus; e.related_target = new_focus;
            self.events.dispatch(&mut self.root, e);
            let mut e = HtmlEvent::new(HtmlEventType::FocusOut);
            e.target = old_focus; e.related_target = new_focus;
            self.events.dispatch(&mut self.root, e);
        }
        if new_focus != 0 {
            let mut e = HtmlEvent::new(HtmlEventType::Focus);
            e.target = new_focus; e.related_target = old_focus;
            self.events.dispatch(&mut self.root, e);
            let mut e = HtmlEvent::new(HtmlEventType::FocusIn);
            e.target = new_focus; e.related_target = old_focus;
            self.events.dispatch(&mut self.root, e);
        }
        self.stylesheet.rebuild_index();
        self.hovered_box = 0;
        self.active_box  = 0;
        crate::css::apply_cascade_vp(
            &mut self.root, &self.stylesheet, None, 16.0,
            self.viewport_w, self.viewport_h, self.focused_box, true,
        );
        true
    }
}

/// Returns true if `node` is a focusable element (native or via tabindex/contenteditable).
/// tabindex=-1 elements return true (focusable by script/click) but are excluded from
/// the *tab* order by `collect_focusable_ordered`.
/// Handle a click on a form element: toggle checkbox, select radio, fire form events.
/// Returns Some(true) if a redraw is needed, Some(false) if handled but no redraw, None if not a form element.
pub fn handle_form_click(root: &mut WebCore, target: u32, callback: &mut Option<FormEventCallback>) -> Option<bool> {
    // Find a node by node_id in the tree (immutable)
    fn find_ref<'a>(node: &'a WebCore, t: u32) -> Option<&'a WebCore> {
        if node.node_id == t { return Some(node); }
        for child in &node.children {
            if let Some(found) = find_ref(child, t) { return Some(found); }
        }
        None
    }
    // Find a node by node_id in the tree (mutable)
    fn find_mut<'a>(node: &'a mut WebCore, t: u32) -> Option<&'a mut WebCore> {
        if node.node_id == t { return Some(node); }
        for child in &mut node.children {
            if let Some(found) = find_mut(child, t) { return Some(found); }
        }
        None
    }

    // If the target is a #text node, find the parent form element instead.
    // This handles clicks on text inside <select>, <button>, etc.
    let effective_target = {
        let node = find_ref(root, target)?;
        if node.tag == "#text" {
            // Walk the tree to find the parent of this text node
            fn find_parent_id(node: &WebCore, child_id: u32) -> Option<u32> {
                for c in &node.children {
                    if c.node_id == child_id {
                        return Some(node.node_id);
                    }
                    if let Some(p) = find_parent_id(c, child_id) { return Some(p); }
                }
                None
            }
            find_parent_id(root, target).unwrap_or(target)
        } else {
            target
        }
    };
    let target = effective_target;

    // Disabled elements don't respond to clicks
    let target_node = find_ref(root, target)?;
    if target_node.attributes.contains_key("disabled") { return None; }

    // Read target info before mutation
    let (tag, input_type, name, id, value) = {
        let tag = target_node.tag.clone();
        let input_type = target_node.attributes.get("type").cloned().unwrap_or_default();
        let name = target_node.attributes.get("name").cloned().unwrap_or_default();
        let id = target_node.attributes.get("id").cloned().unwrap_or_default();
        let value = target_node.attributes.get("value").cloned().unwrap_or_default();
        (tag, input_type, name, id, value)
    };

    match tag.as_str() {
        "input" => {
            match input_type.as_str() {
                "checkbox" => {
                    let node = find_mut(root, target)?;
                    // **A click changes STATE, not markup** (HTML §4.10.5.3).
                    // This used to add and remove the `checked` ATTRIBUTE, so
                    // ticking a box edited the document and
                    // `getAttribute("checked")` answered the user's last click
                    // instead of the author's default.
                    let was_checked = node.checkedness;
                    node.checkedness = !was_checked;
                    // "must be set to true whenever the user interacts with the
                    // control in a way that changes the checkedness."
                    node.dirty_checked = true;
                    let new_checked = !was_checked;
                    if let Some(cb) = callback {
                        cb(&FormEvent {
                            tag: tag.clone(), id, name,
                            kind: FormEventKind::Toggle(new_checked),
                            element: target,
                        });
                    }
                    Some(true)
                }
                "radio" => {
                    // Uncheck other radios with the same name, check this one
                    if !name.is_empty() {
                        fn uncheck_radios(node: &mut WebCore, name: &str, except_id: u32) {
                            if node.tag == "input"
                                && node.attributes.get("type").map(|s| s.as_str()) == Some("radio")
                                && node.attributes.get("name").map(|s| s.as_str()) == Some(name)
                                && node.node_id != except_id
                            {
                                node.checkedness = false;
                                node.dirty_checked = true;
                            }
                            for child in &mut node.children {
                                uncheck_radios(child, name, except_id);
                            }
                        }
                        uncheck_radios(root, &name, target);
                    }
                    let node = find_mut(root, target)?;
                    node.checkedness = true;
                    node.dirty_checked = true;
                    if let Some(cb) = callback {
                        cb(&FormEvent {
                            tag: tag.clone(), id, name,
                            kind: FormEventKind::Change(value),
                            element: target,
                        });
                    }
                    Some(true)
                }
                "submit" | "button" | "reset" => {
                    // Reset button: reset the parent form
                    if input_type == "reset" {
                        let _form_action = find_parent_form_action(root, target);
                        // Find and reset the parent form
                        fn find_form_for_reset(node: &WebCore, target_id: u32) -> Option<u32> {
                            if node.tag == "form" {
                                fn contains(n: &WebCore, t: u32) -> bool {
                                    if n.node_id == t { return true; }
                                    n.children.iter().any(|c| contains(c, t))
                                }
                                if contains(node, target_id) { return Some(node.node_id); }
                            }
                            for child in &node.children {
                                if let Some(f) = find_form_for_reset(child, target_id) { return Some(f); }
                            }
                            None
                        }
                        if let Some(form_id) = find_form_for_reset(root, target) {
                            reset_form(root, form_id);
                        }
                    }
                    if let Some(cb) = callback {
                        cb(&FormEvent {
                            tag: tag.clone(), id, name,
                            kind: FormEventKind::Click(value),
                            element: target,
                        });
                    }
                    Some(false)
                }
                "text" | "password" | "email" | "search" | "url" | "tel" | "number" => {
                    // Text input clicked — set cursor to end of value
                    let node = find_mut(root, target)?;
                    let len = input_value(node).chars().count();
                    node.input_cursor = len;
                    node.input_sel_anchor = len;
                    Some(true)
                }
                _ => None,
            }
        }
        "button" => {
            let target_node2 = find_ref(root, target);
            let btn_type = target_node2.and_then(|n| n.attributes.get("type").cloned())
                .unwrap_or_else(|| "submit".to_string());
            if let Some(cb) = callback {
                let text = target_node2.map(|n| n.text.clone()).unwrap_or_default();
                cb(&FormEvent {
                    tag: tag.clone(), id: id.clone(), name: name.clone(),
                    kind: FormEventKind::Click(if value.is_empty() { text } else { value.clone() }),
                    element: target,
                });
                // Submit buttons trigger form submission
                if btn_type == "submit" {
                    let action = find_parent_form_action(root, target);
                    cb(&FormEvent {
                        tag: "form".into(), id: String::new(), name: String::new(),
                        kind: FormEventKind::Submit(action),
                        element: target,
                    });
                }
            }
            // Reset buttons reset the form
            if btn_type == "reset" {
                fn find_form_id(node: &WebCore, target_id: u32) -> Option<u32> {
                    if node.tag == "form" {
                        fn has(n: &WebCore, t: u32) -> bool {
                            if n.node_id == t { return true; }
                            n.children.iter().any(|c| has(c, t))
                        }
                        if has(node, target_id) { return Some(node.node_id); }
                    }
                    for c in &node.children { if let Some(f) = find_form_id(c, target_id) { return Some(f); } }
                    None
                }
                if let Some(fid) = find_form_id(root, target) {
                    reset_form(root, fid);
                }
            }
            Some(btn_type == "reset") // redraw if reset
        }
        // ⛔ `<select>` and `<input type=range>` are both absent on purpose:
        // where the click LANDED decides what they do, and this function is
        // handed a target without a point. Both live in `process_mouse_event`,
        // which has `doc_pt` — a list box picks a row, a range picks a value
        // along its track, and a drop-down opens its popup.
        "select" => None,
        _ => None,
    }
}

/// Find the form element parent of a target (walks up from #text to select/input/button).
fn find_form_parent_id(root: &WebCore, target_id: u32) -> u32 {
    fn find_ref<'a>(node: &'a WebCore, t: u32) -> Option<&'a WebCore> {
        if node.node_id == t { return Some(node); }
        for child in &node.children { if let Some(f) = find_ref(child, t) { return Some(f); } }
        None
    }
    if let Some(node) = find_ref(root, target_id) {
        if matches!(node.tag.as_str(), "input" | "select" | "textarea" | "button") {
            return target_id;
        }
    }
    // Walk tree to find parent
    fn walk(node: &WebCore, target_id: u32) -> Option<u32> {
        for child in &node.children {
            if child.node_id == target_id {
                if matches!(node.tag.as_str(), "input" | "select" | "textarea" | "button" | "label") {
                    return Some(node.node_id);
                }
            }
            if let Some(p) = walk(child, target_id) { return Some(p); }
        }
        None
    }
    walk(root, target_id).unwrap_or(target_id)
}

/// Find the action URL of the nearest ancestor <form> element.
pub fn find_parent_form_action(root: &WebCore, target_id: u32) -> String {
    fn walk(node: &WebCore, target_id: u32) -> Option<String> {
        for child in &node.children {
            if child.node_id == target_id {
                if node.tag == "form" {
                    return Some(node.attributes.get("action").cloned().unwrap_or_default());
                }
                return None; // found target but parent isn't form — caller keeps looking
            }
            if let Some(action) = walk(child, target_id) {
                return Some(action);
            }
            // Check if child contains target and this node is a form
            if node.tag == "form" {
                fn contains(node: &WebCore, target_id: u32) -> bool {
                    if node.node_id == target_id { return true; }
                    node.children.iter().any(|c| contains(c, target_id))
                }
                if contains(child, target_id) {
                    return Some(node.attributes.get("action").cloned().unwrap_or_default());
                }
            }
        }
        None
    }
    walk(root, target_id).unwrap_or_default()
}

/// **Constructing the entry list** (HTML §4.10.21.4) for a `<form>`.
///
/// A LIST of name/value entries in tree order, not a map. HTML appends one
/// entry per contributing control and never says two entries may not share a
/// name — which is the whole shape of a `multiple` select and of a checkbox
/// group, both of which submit several values under one name. Returned as a
/// map, the last write silently won and every value but one vanished at the
/// point of submission, where nothing downstream could tell it had happened.
///
/// The rules each control follows, and where each one is written, stay exactly
/// where they were; this is only the container being able to hold the answer.
/// - Text/password/hidden/email/…: the control's VALUE, not its attribute
/// - Checkbox / radio: only when checked; `"on"` when no value is given
/// - Select: one entry per selected, non-disabled option
/// - Textarea: its value
/// - Disabled elements, and everything inside them, contribute nothing
/// - Elements without a name contribute nothing
pub fn collect_form_data(form: &WebCore) -> Vec<(String, String)> {
    let mut data = Vec::new();
    collect_form_data_inner(form, &mut data);
    data
}

fn collect_form_data_inner(node: &WebCore, data: &mut Vec<(String, String)>) {
    if node.attributes.contains_key("disabled") { return; }
    let name = match node.attributes.get("name") {
        Some(n) if !n.is_empty() => n.clone(),
        _ => {
            // No name — recurse into children but don't collect this node
            for child in &node.children { collect_form_data_inner(child, data); }
            return;
        }
    };
    match node.tag.as_str() {
        "input" => {
            let input_type = node.attributes.get("type").map(|s| s.as_str()).unwrap_or("text");
            match input_type {
                "checkbox" => {
                    // What gets SUBMITTED is the current checkedness, not the
                    // author's default — a box the user unticked must not be
                    // in the form data because the markup still says `checked`.
                    if node.checkedness {
                        let val = node.attributes.get("value").cloned().unwrap_or_else(|| "on".to_string());
                        data.push((name, val));
                    }
                }
                "radio" => {
                    if node.checkedness {
                        let val = node.attributes.get("value").cloned().unwrap_or_default();
                        data.push((name, val));
                    }
                }
                "submit" | "button" | "reset" | "image" => {
                    // Submit buttons are not included in form data by default
                }
                "file" => {
                    // File inputs would need special handling — skip for now
                }
                _ => {
                    // ⛔ The VALUE. Reading the `value` ATTRIBUTE here meant a
                    // form submitted the author's default instead of what the
                    // user typed — and every existing test passed, because none
                    // of them types before collecting.
                    data.push((name, input_value(node)));
                }
            }
        }
        "select" => {
            // "For each option element ... whose selectedness is true and that
            // is not disabled, append an entry" — SELECTEDNESS, so what is
            // submitted is what the user picked rather than what the markup
            // defaulted to, and a control with nothing selected contributes
            // nothing at all.
            //
            // One entry PER selected option, which is how a `multiple` select
            // submits several values under one name.
            for option in crate::html::forms::list_of_options(node) {
                if option.selectedness && !option.attributes.contains_key("disabled") {
                    data.push((name.clone(), crate::html::forms::option_value(option)));
                }
            }
        }
        "textarea" => {
            let val = input_value(node);
            data.push((name, val));
        }
        _ => {
            for child in &node.children { collect_form_data_inner(child, data); }
        }
    }
}

/// Reset all form fields inside a <form> to their default values.
/// Text inputs reset to their original value attribute (from defaultValue).
/// Checkboxes/radios reset to their initial checked state.
/// Selects reset to the initially selected option.
pub fn reset_form(root: &mut WebCore, form_id: u32) {
    fn find_mut<'a>(n: &'a mut WebCore, t: u32) -> Option<&'a mut WebCore> {
        if n.node_id == t { return Some(n); }
        for c in &mut n.children { if let Some(r) = find_mut(c, t) { return Some(r); } }
        None
    }
    if let Some(form) = find_mut(root, form_id) {
        reset_form_inner(form);
    }
}

/// The **reset algorithm** for one control (HTML §4.10.23).
///
/// Every arm is now the same sentence: drop the STATE, clear its dirty flag,
/// and let the content attribute speak again. Nothing is copied anywhere,
/// because the default was never overwritten in the first place.
fn reset_form_inner(node: &mut WebCore) {
    match node.tag.as_str() {
        "input" => {
            let input_type = node.attributes.get("type").cloned().unwrap_or_else(|| "text".to_string());
            match input_type.as_str() {
                "checkbox" | "radio" => {
                    // Verbatim: "set its ... dirty checkedness flag back to
                    // false, ... set the checkedness of the element to true if
                    // the element has a `checked` content attribute and false
                    // if it does not".
                    node.checkedness = node.attributes.contains_key("checked");
                    node.dirty_checked = false;
                }
                // Buttons and file inputs have no resettable value here; every
                // other state carries one.
                "submit" | "reset" | "button" | "image" | "hidden" | "file" => {}
                _ => {
                    // "Set the dirty value flag to false", after which the
                    // value falls back to the `value` content attribute on its
                    // own — dropping the state IS the reset.
                    //
                    // ⛔ This used to read a `defaultValue` ATTRIBUTE, which is
                    // not a content attribute at all but the IDL name FOR
                    // `value`. Nothing ever wrote it, so the fallback did the
                    // work and reset restored the field to whatever the user
                    // had last typed — the identical bug `defaultChecked` had.
                    node.value_state = None;
                    node.dirty_value = false;
                    node.input_cursor = 0;
                    node.input_sel_anchor = 0;
                    // "Invoke the value sanitization algorithm, if the type
                    // attribute's current state defines one" — the reset
                    // algorithm's own last step, and the reason a range does
                    // not come back holding a step-mismatched default.
                    crate::html::forms::seed_input_value(node);
                }
            }
        }
        "textarea" => {
            // A `<textarea>`'s default value is its CHILD TEXT, so the same
            // move restores it: typing no longer edits those children.
            node.value_state = None;
            node.dirty_value = false;
            node.input_cursor = 0;
            node.input_sel_anchor = 0;
        }
        "select" => {
            // "Set the selectedness of all the option elements ... to true if
            // the option element has a `selected` attribute, and false
            // otherwise; set the dirtiness of all ... to false; and then have
            // the select element run the selectedness setting algorithm."
            crate::html::forms::reset_select(node);
        }
        _ => {
            for child in &mut node.children { reset_form_inner(child); }
        }
    }
}

/// Encode form data as application/x-www-form-urlencoded string.
pub fn encode_form_urlencoded(data: &[(String, String)]) -> String {
    // ⛔ IN ENTRY ORDER, not sorted. HTML runs the serializer over "a list of
    // name-value pairs", and a list's order is part of the answer: a server
    // reading repeated names sees them in the order the controls appear.
    // Sorting was here to make a `HashMap`'s arbitrary iteration order
    // repeatable for tests — the wrong fix for a container that could not hold
    // the data in the first place.
    data.iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

/// Build the submission URL for a form.
/// GET: appends encoded data as query string.
/// POST: returns action URL unchanged (data goes in body).
pub fn build_form_submit_url(action: &str, method: &str, data: &[(String, String)]) -> String {
    if method.eq_ignore_ascii_case("post") {
        action.to_string()
    } else {
        let encoded = encode_form_urlencoded(data);
        if encoded.is_empty() {
            action.to_string()
        } else {
            let sep = if action.contains('?') { "&" } else { "?" };
            format!("{}{}{}", action, sep, encoded)
        }
    }
}

/// Apply autofocus: find the first element with the `autofocus` attribute and focus it.
pub fn apply_autofocus(doc: &mut Document) {
    fn find_autofocus(node: &WebCore) -> Option<u32> {
        if node.attributes.contains_key("autofocus") && is_focusable_node(node) {
            return Some(node.node_id);
        }
        for child in &node.children {
            if let Some(id) = find_autofocus(child) { return Some(id); }
        }
        None
    }
    if let Some(id) = find_autofocus(&doc.root) {
        doc.focused_box = id;
    }
}

/// Resolve `<slot>` elements in a shadow tree by projecting light DOM children into them.
fn resolve_slots_inner(shadow_children: &mut Vec<WebCore>, light_children: &[WebCore]) {
    for child in shadow_children.iter_mut() {
        if child.tag == "slot" {
            let slot_name = child.attributes.get("name").cloned().unwrap_or_default();
            let projected: Vec<WebCore> = if slot_name.is_empty() {
                // Default slot: all light children without a `slot` attribute
                // Slottables are elements and non-blank text. A comment is
                // neither, so it is not projected.
                light_children.iter()
                    .filter(|lc| !lc.attributes.contains_key("slot") && lc.is_element()
                        || (lc.is_text_node() && !lc.text.trim().is_empty() && !lc.attributes.contains_key("slot")))
                    .cloned()
                    .collect()
            } else {
                // Named slot: light children with matching `slot` attribute
                light_children.iter()
                    .filter(|lc| lc.attributes.get("slot").map(|s| s == &slot_name).unwrap_or(false))
                    .cloned()
                    .collect()
            };
            if !projected.is_empty() {
                child.children = projected;
            }
            // If no matches, keep slot's own children as fallback
        } else {
            // Recurse into shadow tree children to find nested slots
            resolve_slots_inner(&mut child.children, light_children);
            // Also recurse into shadow roots of nested shadow hosts
            if let Some(ref mut sr) = child.shadow_root {
                resolve_slots_inner(&mut sr.children, light_children);
            }
        }
    }
}

/// Returns true if this element is a text-editable form input.
pub fn is_text_input(node: &WebCore) -> bool {
    match node.tag.as_str() {
        "textarea" => true,
        "input" => {
            let t = node.attributes.get("type").map(|s| s.as_str()).unwrap_or("text");
            matches!(t, "text" | "password" | "email" | "search" | "url" | "tel" | "number")
        }
        _ => false,
    }
}

/// A form control's **value** (HTML §4.10.18.1).
///
/// The single read point for every consumer — the paint path, form submission,
/// the key handler. It answers the VALUE, which is `value_state` once anything
/// has set it and the `value` content attribute (the default value) until then.
pub fn input_value(node: &WebCore) -> String {
    // The PRESENCE of a state string decides, not `dirty_value`. The two are
    // not the same question: sanitization seeds a value with the dirty flag
    // still down, and a control set to the empty string holds the empty string
    // — `Some("")` must not fall back to the default the way `None` does.
    // `dirty_value` answers a different question, for the reset algorithm.
    if let Some(v) = &node.value_state {
        return v.clone();
    }
    if node.tag == "textarea" {
        // Textarea value is in child text nodes
        node.children.iter()
            .filter(|c| c.tag == "#text")
            .map(|c| c.text.as_str())
            .collect()
    } else {
        node.attributes.get("value").cloned().unwrap_or_default()
    }
}

/// Process a key event on a focused form input. Returns true if the value changed.
pub fn process_form_input_key(node: &mut WebCore, key_code: u32, ch: Option<char>, ctrl: bool, _shift: bool) -> bool {
    if !is_text_input(node) { return false; }
    // Disabled elements don't accept any input
    if node.attributes.contains_key("disabled") { return false; }
    // Readonly elements allow cursor movement but not content changes
    let is_readonly = node.attributes.contains_key("readonly");
    let is_textarea = node.tag == "textarea";

    let mut value = input_value(node);
    let len = value.chars().count();
    let cursor = node.input_cursor.min(len);
    let anchor = node.input_sel_anchor.min(len);
    let has_selection = cursor != anchor;
    let sel_start = cursor.min(anchor);
    let sel_end = cursor.max(anchor);
    let mut new_cursor = cursor;
    let mut changed = false;
    let maxlength: Option<usize> = node.attributes.get("maxlength")
        .and_then(|s| s.parse().ok());

    // Ctrl+A: select all
    if ctrl && (key_code == 65 || ch == Some('a') || ch == Some('A')) {
        node.input_sel_anchor = 0;
        node.input_cursor = len;
        return true; // cursor moved, no content change
    }

    // Helper: delete selected range
    let delete_selection = |value: &mut String, sel_s: usize, sel_e: usize| -> usize {
        let byte_s = value.char_indices().nth(sel_s).map(|(i, _)| i).unwrap_or(value.len());
        let byte_e = value.char_indices().nth(sel_e).map(|(i, _)| i).unwrap_or(value.len());
        value.replace_range(byte_s..byte_e, "");
        sel_s
    };

    match key_code {
        8 => { // Backspace
            if !is_readonly {
                if has_selection {
                    new_cursor = delete_selection(&mut value, sel_start, sel_end);
                    changed = true;
                } else if cursor > 0 {
                    let byte_pos = value.char_indices().nth(cursor - 1).map(|(i, _)| i).unwrap_or(0);
                    let byte_end = value.char_indices().nth(cursor).map(|(i, _)| i).unwrap_or(value.len());
                    value.replace_range(byte_pos..byte_end, "");
                    new_cursor = cursor - 1;
                    changed = true;
                }
            }
        }
        46 => { // Delete
            if !is_readonly {
                if has_selection {
                    new_cursor = delete_selection(&mut value, sel_start, sel_end);
                    changed = true;
                } else if cursor < len {
                    let byte_pos = value.char_indices().nth(cursor).map(|(i, _)| i).unwrap_or(value.len());
                    let byte_end = value.char_indices().nth(cursor + 1).map(|(i, _)| i).unwrap_or(value.len());
                    value.replace_range(byte_pos..byte_end, "");
                    changed = true;
                }
            }
        }
        37 => { // Left arrow
            if cursor > 0 { new_cursor = cursor - 1; }
        }
        39 => { // Right arrow
            if cursor < len { new_cursor = cursor + 1; }
        }
        36 => { // Home
            new_cursor = 0;
        }
        35 => { // End
            new_cursor = len;
        }
        13 => { // Enter
            if is_textarea && !is_readonly {
                if maxlength.map(|m| len < m).unwrap_or(true) {
                    let byte_pos = value.char_indices().nth(cursor).map(|(i, _)| i).unwrap_or(value.len());
                    value.insert(byte_pos, '\n');
                    new_cursor = cursor + 1;
                    changed = true;
                }
            }
        }
        _ => {
            // Character input
            if let Some(c) = ch {
                if !c.is_control() && !is_readonly && !ctrl {
                    // Delete selection first if any
                    if has_selection {
                        new_cursor = delete_selection(&mut value, sel_start, sel_end);
                    }
                    let cur_len = value.chars().count();
                    if maxlength.map(|m| cur_len < m).unwrap_or(true) {
                        let byte_pos = value.char_indices().nth(new_cursor).map(|(i, _)| i).unwrap_or(value.len());
                        value.insert(byte_pos, c);
                        new_cursor += 1;
                        changed = true;
                    }
                }
            }
        }
    }

    node.input_cursor = new_cursor;
    node.input_sel_anchor = new_cursor;

    if changed {
        // Typing sets the VALUE and raises the dirty value flag (HTML
        // §4.10.18.1). It does not touch the `value` attribute, nor a
        // `<textarea>`'s child text — those ARE the default value, which is
        // what a form reset restores and what the serializer round-trips.
        node.value_state = Some(value);
        node.dirty_value = true;
        node.layout.layout_dirty = true;
    }

    changed || key_code == 37 || key_code == 39 || key_code == 36 || key_code == 35
}

pub fn is_focusable_node(node: &WebCore) -> bool {
    if matches!(node.style.display, Display::None) { return false; }
    if !node.style.visibility { return false; }
    let tag = node.tag.as_str();
    matches!(tag, "button" | "input" | "textarea" | "select")
        || (tag == "a" && node.attributes.contains_key("href"))
        || node.attributes.get("tabindex")
            .and_then(|v| v.parse::<i32>().ok())
            .is_some()                          // any explicit tabindex (incl. -1)
        || node.attributes.get("contenteditable")
            .map(|v| v == "true" || v == "")
            .unwrap_or(false)
}

/// Walk the box tree and split focusable elements into two buckets for tab ordering:
/// - `positive`: elements with explicit `tabindex > 0`, paired with their index value
/// - `normal`:   native-focusable elements and `tabindex=0` elements, in document order
///
/// Elements with `tabindex=-1` are excluded (programmatically focusable only).
fn collect_focusable_ordered(
    node: &WebCore,
    positive: &mut Vec<(u32, i32)>,
    normal:   &mut Vec<u32>,
) {
    if matches!(node.style.display, Display::None) { return; }
    if !node.style.visibility { return; }
    let tag = node.tag.as_str();

    let tabindex = node.attributes.get("tabindex")
        .and_then(|v| v.parse::<i32>().ok());

    // Determine whether this element is in the tab order.
    let native = matches!(tag, "button" | "input" | "textarea" | "select")
        || (tag == "a" && node.attributes.contains_key("href"))
        || node.attributes.get("contenteditable")
            .map(|v| v == "true" || v == "")
            .unwrap_or(false);

    match tabindex {
        Some(n) if n > 0  => positive.push((node.node_id, n)),
        Some(0)           => normal.push(node.node_id),
        Some(_)           => {} // tabindex < 0: excluded from tab order
        None if native    => normal.push(node.node_id),
        None              => {}
    }

    for child in &node.children {
        collect_focusable_ordered(child, positive, normal);
    }
}

impl Clone for Document {
    fn clone(&self) -> Self {
        Self {
            root:            self.root.clone(),
            nodes:           NodeArena::new(), // rebuilt on demand
            nodes_stale:     true,
            stylesheet:      self.stylesheet.clone(),
            title:           self.title.clone(),
            base_url:        self.base_url.clone(),
            arena:           DomArena::new(),  // cloned docs get fresh arena (rebuilt on demand)
            next_node_id:    self.next_node_id,
            node_index:      HashMap::new(),   // rebuilt on demand
            // A copy of an XML document is still an XML document — carried
            // over rather than reset, or the copy would start folding names
            // the original does not.
            kind:            self.kind,
            layout_store:    crate::layout::layout_box::LayoutStore::new(),
            pending_nodes:   HashMap::new(),
            linked_stylesheets: self.linked_stylesheets.clone(),
            editor:          self.editor.clone(),
            // The canvas BITMAPS come along inside `root.clone()`, because
            // they live on the elements. The drawing STATE does not: a copy of
            // a document starts from the default context, the same way it
            // starts with no event listeners.
            canvas_surfaces: crate::canvas::CanvasSurfaces::default(),
            events:          self.events.clone(),
            event_targets:   crate::dom::events::EventTargetMap::new(), // listeners not cloned
            scroll_x:        self.scroll_x,
            scroll_y:        self.scroll_y,
            scrollbar_drag:  self.scrollbar_drag.clone(),
            hovered_box:     self.hovered_box,
            hover_suppress_count: self.hover_suppress_count,
            active_box:      self.active_box,
            focused_box:     self.focused_box,
            mousedown_target: self.mousedown_target,
            last_click_target: self.last_click_target,
            last_click_time: self.last_click_time,
            drag_source:     self.drag_source,
            drag_start_doc_pt: self.drag_start_doc_pt,
            drag_active:     self.drag_active,
            visited_urls:    self.visited_urls.clone(),
            viewport_w:      self.viewport_w,
            viewport_h:      self.viewport_h,
            keyboard_focus:  self.keyboard_focus,
            caret_blink_epoch: std::time::Instant::now(), open_select: 0, open_picker: 0, dropdown_hover_idx: -1,
            // Transient interaction state, like the two popups beside it: a
            // fresh document is holding nothing.
            dragging_range: 0, range_drag_origin: String::new(),
            active_animations:     self.active_animations.clone(),
            transition_states:     self.transition_states.clone(),
            prev_styles:           self.prev_styles.clone(),
            cascade_styles:        self.cascade_styles.clone(),
            animation_overrides:   self.animation_overrides.clone(),
            needs_animation_frame: self.needs_animation_frame,
            hover_changed:         self.hover_changed,
            hover_sensitive_nodes: self.hover_sensitive_nodes.clone(),
            style_dirty:           self.style_dirty,
            prev_hovered_box:      self.prev_hovered_box,
            pending_announcements:    self.pending_announcements.clone(),
            live_region_snapshots:    self.live_region_snapshots.clone(),
            live_regions_initialized: self.live_regions_initialized,
            layout_generation:       self.layout_generation,
            // Async image state is not cloned — cloned docs start with no pending fetches.
            pending_images:   None,
            images_in_flight: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            on_form_event: None, on_navigate: None, on_title_change: None, on_dom_mutation: None, on_visibility_change: None, // callbacks not cloned
        }
    }
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("title", &self.title)
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl Default for Document {
    fn default() -> Self { Self::new() }
}

// SAFETY: Document contains raw pointers (hovered_box, active_box, etc.) that are
// only used on the main thread. When sent across threads (e.g. background loading),
// these pointers are always null. The receiver must not dereference them until
// re-established on the owning thread.
unsafe impl Send for Document {}

fn find_node_by_path_mut<'a>(root: &'a mut WebCore, path: &[usize]) -> Option<&'a mut WebCore> {
    let mut node = root;
    for &idx in path {
        if idx >= node.children.len() { return None; }
        node = &mut node.children[idx];
    }
    Some(node)
}

// ─── aria-live helper ──────────────────────────────────────────────────────────

/// Collect the visible text content of a live region by walking its subtree.
/// Used by `Document::check_live_regions` to compare snapshots.
fn collect_live_text(node: &WebCore) -> String {
    let mut buf = String::new();
    collect_live_text_inner(node, &mut buf);
    // Collapse runs of whitespace for stable comparison across minor reflows.
    let mut out = String::with_capacity(buf.len());
    let mut in_ws = false;
    for ch in buf.chars() {
        if ch.is_ascii_whitespace() {
            if !in_ws { out.push(' '); in_ws = true; }
        } else {
            in_ws = false;
            out.push(ch);
        }
    }
    out.trim().to_string()
}

fn collect_live_text_inner(node: &WebCore, buf: &mut String) {
    if !node.text.trim().is_empty() {
        if !buf.is_empty() { buf.push(' '); }
        buf.push_str(node.text.trim());
    }
    for child in &node.children {
        if !matches!(child.style.display, Display::None) && child.style.visibility {
            collect_live_text_inner(child, buf);
        }
    }
}

// ─── Wheel-event scroll dispatch ──────────────────────────────────────────────

/// Search a subtree for absolute/fixed descendants that contain `pt` and can
/// be scrolled. Used when an in-flow ancestor fails the hit test but its
/// absolute children may still be under the cursor.
fn scroll_abs_in(node: &mut WebCore, pt: (f32, f32), delta_x: f32, delta_y: f32) -> bool {
    for child in &mut node.children {
        if matches!(child.style.display, Display::None) { continue; }
        if matches!(child.style.position, Position::Absolute | Position::Fixed) {
            let mr = child.layout.margin_rect;
            if pt.0 >= mr.x && pt.0 < mr.x + mr.w && pt.1 >= mr.y && pt.1 < mr.y + mr.h {
                if scroll_box_at(child, pt, delta_x, delta_y) { return true; }
            }
        } else {
            if scroll_abs_in(child, pt, delta_x, delta_y) { return true; }
        }
    }
    false
}

/// Walk the box tree and scroll the *innermost* scrollable box that contains
/// `pt` (in accumulated-scroll document coordinates).  Returns `true` if a box
/// was scrolled.
///
/// `pt` on entry is already adjusted for all ancestor scroll offsets so that it
/// can be compared directly against children's `margin_rect` (layout-space) positions.
fn scroll_box_at(node: &mut WebCore, pt: (f32, f32), delta_x: f32, delta_y: f32) -> bool {
    if matches!(node.style.display, Display::None) { return false; }

    // Adjust pt for this node's own scroll so we can test its children,
    // whose margin_rect positions are in layout space (scroll = 0 reference).
    let local_pt = (pt.0 + node.layout.scroll_left, pt.1 + node.layout.scroll_top);

    // Recurse depth-first; innermost hit wins.
    for child in &mut node.children {
        if matches!(child.style.display, Display::None) { continue; }
        if matches!(child.style.position, Position::Absolute | Position::Fixed) {
            // Out-of-flow boxes use the viewport coordinate rather than parent scroll.
            let mr = child.layout.margin_rect;
            if pt.0 >= mr.x && pt.0 < mr.x + mr.w && pt.1 >= mr.y && pt.1 < mr.y + mr.h {
                if scroll_box_at(child, pt, delta_x, delta_y) { return true; }
            }
            continue;
        }
        let mr = child.layout.margin_rect;
        if local_pt.0 >= mr.x && local_pt.0 < mr.x + mr.w
            && local_pt.1 >= mr.y && local_pt.1 < mr.y + mr.h
        {
            if scroll_box_at(child, local_pt, delta_x, delta_y) { return true; }
        } else {
            // Even though cursor is outside this in-flow child's bounds, its
            // absolute/fixed descendants may be positioned at the cursor location.
            if scroll_abs_in(child, pt, delta_x, delta_y) { return true; }
        }
    }

    // Check whether *this* node is scrollable.
    let can_v = delta_y.abs() > 0.1
        && matches!(node.style.overflow_y, Overflow::Scroll | Overflow::Auto)
        && node.layout.scroll_height > node.layout.content_rect.h;
    let can_h = delta_x.abs() > 0.1
        && matches!(node.style.overflow_x, Overflow::Scroll | Overflow::Auto)
        && node.layout.scroll_width > node.layout.content_rect.w;

    let mut scrolled = false;

    if can_v {
        let max_scroll = (node.layout.scroll_height - node.layout.content_rect.h).max(0.0);
        let before = node.layout.scroll_top;
        node.layout.scroll_top = (node.layout.scroll_top - delta_y).clamp(0.0, max_scroll);
        if (node.layout.scroll_top - before).abs() > 1e-3 {
            apply_scroll_snap_y(node);
            scrolled = true;
        }
    }
    if can_h {
        let max_scroll = (node.layout.scroll_width - node.layout.content_rect.w).max(0.0);
        let before = node.layout.scroll_left;
        node.layout.scroll_left = (node.layout.scroll_left - delta_x).clamp(0.0, max_scroll);
        if (node.layout.scroll_left - before).abs() > 1e-3 {
            apply_scroll_snap_x(node);
            scrolled = true;
        }
    }

    if scrolled { return true; }

    // overscroll-behavior: if this element is a scroll container but couldn't
    // scroll (already at boundary), check whether it should swallow the event anyway.
    let is_v_container = matches!(node.style.overflow_y, Overflow::Scroll | Overflow::Auto);
    let is_h_container = matches!(node.style.overflow_x, Overflow::Scroll | Overflow::Auto);
    if delta_y.abs() > 0.1 && is_v_container
        && node.style.overscroll_behavior_y != OverscrollBehavior::Auto
    {
        return true; // Contain/None: don't chain to parent.
    }
    if delta_x.abs() > 0.1 && is_h_container
        && node.style.overscroll_behavior_x != OverscrollBehavior::Auto
    {
        return true;
    }

    false
}

/// Snap the vertical scroll position of `node` to the nearest child snap point,
/// if the element has `scroll-snap-type` with a Y/Both axis.
fn apply_scroll_snap_y(node: &mut WebCore) {
    if !node.style.scroll_snap_type.snaps_y() { return; }
    let content_y = node.layout.content_rect.y;
    let content_h = node.layout.content_rect.h;
    let snap_points = collect_snap_points_y(node, content_y, content_h);
    if snap_points.is_empty() { return; }
    let max_scroll = (node.layout.scroll_height - content_h).max(0.0);
    let target = nearest_snap(node.layout.scroll_top, &snap_points, content_h,
                              node.style.scroll_snap_type.mandatory);
    node.layout.scroll_top = target.clamp(0.0, max_scroll);
}

/// Snap the horizontal scroll position of `node`.
fn apply_scroll_snap_x(node: &mut WebCore) {
    if !node.style.scroll_snap_type.snaps_x() { return; }
    let content_x = node.layout.content_rect.x;
    let content_w = node.layout.content_rect.w;
    let snap_points = collect_snap_points_x(node, content_x, content_w);
    if snap_points.is_empty() { return; }
    let max_scroll = (node.layout.scroll_width - content_w).max(0.0);
    let target = nearest_snap(node.layout.scroll_left, &snap_points, content_w,
                              node.style.scroll_snap_type.mandatory);
    node.layout.scroll_left = target.clamp(0.0, max_scroll);
}

fn collect_snap_points_y(node: &WebCore, content_y: f32, content_h: f32) -> Vec<f32> {
    let mut pts = Vec::new();
    for child in &node.children {
        if matches!(child.style.display, Display::None) { continue; }
        let mr = child.layout.margin_rect;
        let pt = match child.style.scroll_snap_align {
            ScrollSnapAlign::Start  => mr.y - content_y,
            ScrollSnapAlign::End    => mr.y + mr.h - content_y - content_h,
            ScrollSnapAlign::Center => mr.y + mr.h * 0.5 - content_y - content_h * 0.5,
            ScrollSnapAlign::None   => continue,
        };
        pts.push(pt);
    }
    pts
}

fn collect_snap_points_x(node: &WebCore, content_x: f32, content_w: f32) -> Vec<f32> {
    let mut pts = Vec::new();
    for child in &node.children {
        if matches!(child.style.display, Display::None) { continue; }
        let mr = child.layout.margin_rect;
        let pt = match child.style.scroll_snap_align {
            ScrollSnapAlign::Start  => mr.x - content_x,
            ScrollSnapAlign::End    => mr.x + mr.w - content_x - content_w,
            ScrollSnapAlign::Center => mr.x + mr.w * 0.5 - content_x - content_w * 0.5,
            ScrollSnapAlign::None   => continue,
        };
        pts.push(pt);
    }
    pts
}

/// Return the snap target closest to `current`.
/// For proximity snapping, only snap if within half the viewport size.
fn nearest_snap(current: f32, pts: &[f32], viewport_size: f32, mandatory: bool) -> f32 {
    pts.iter()
        .copied()
        .min_by(|a, b| (a - current).abs().partial_cmp(&(b - current).abs()).unwrap())
        .map(|nearest| {
            if mandatory || (nearest - current).abs() <= viewport_size * 0.5 {
                nearest
            } else {
                current
            }
        })
        .unwrap_or(current)
}

// ─── Per-element scrollbar hit-test ──────────────────────────────────────────

/// Walk the box tree and hit-test per-element scrollbars.
///
/// `sx`/`sy` are the accumulated scroll offsets for this node's ancestors,
/// matching the renderer's coordinate system (`draw_scrollbars(node, pixmap, sx, sy)`).
/// Screen-space position of a node's content area: `cx = cr.x - sx`, `cy = cr.y - sy`.
///
/// Recurses depth-first (children first) so the innermost scrollable element wins.
/// On hit: optionally jump-scrolls to the click position, then writes a
/// `ScrollbarDrag` with `kind = Element(raw ptr)` into `drag_out`.
fn scrollbar_hit_test(
    node:      &mut WebCore,
    screen_x:  f32,
    screen_y:  f32,
    sx:        f32,
    sy:        f32,
    sbw:       f32,
    drag_out:  &mut Option<ScrollbarDrag>,
) -> bool {
    if matches!(node.style.display, Display::None) { return false; }

    // Children are rendered with the parent's scroll added.
    let child_sx = sx + node.layout.scroll_left;
    let child_sy = sy + node.layout.scroll_top;

    for child in node.children.iter_mut() {
        if scrollbar_hit_test(child, screen_x, screen_y, child_sx, child_sy, sbw, drag_out) {
            return true;
        }
    }

    let cr = node.layout.content_rect;
    let pr = node.layout.padding_rect;
    let prx = pr.x - sx;
    let cy = cr.y - sy;

    let show_v = node.style.overflow_y == Overflow::Scroll
        || (node.style.overflow_y == Overflow::Auto && node.layout.scroll_height > cr.h);

    if show_v && node.layout.scroll_height > cr.h {
        // Scrollbar is at the right edge of the padding box (matches draw_scrollbars).
        let track_x = prx + pr.w - sbw;
        if screen_x >= track_x && screen_x < prx + pr.w
            && screen_y >= cy && screen_y < cy + cr.h
        {
            let track_h     = cr.h;
            let thumb_h     = (track_h * track_h / node.layout.scroll_height).max(20.0);
            let max_s       = node.layout.scroll_height - cr.h;
            let scroll_per_px = if track_h - thumb_h > 0.0 { max_s / (track_h - thumb_h) } else { 0.0 };
            let thumb_y     = if max_s > 0.0 { node.layout.scroll_top * (track_h - thumb_h) / max_s } else { 0.0 };
            let local_y     = screen_y - cy;

            // Jump-scroll if click is outside the thumb.
            if !(local_y >= thumb_y && local_y < thumb_y + thumb_h) {
                let new_thumb_y = (local_y - thumb_h * 0.5).clamp(0.0, track_h - thumb_h);
                node.layout.scroll_top = (new_thumb_y * scroll_per_px).clamp(0.0, max_s);
            }

            *drag_out = Some(ScrollbarDrag {
                kind:          ScrollbarDragKind::Element(node.node_id),
                start_mouse_y: screen_y,
                start_scroll:  node.layout.scroll_top,
                scroll_per_px,
            });
            return true;
        }
    }

    false
}

// ─── CSS Animation helpers ────────────────────────────────────────────────────

/// Extract the CSS properties that can participate in transitions from a style.
/// Values are serialised to `rgba(…)` or `Npx` strings for comparison/interpolation.
/// Find the node_id of the nearest ancestor of `target_id` that has a valid node_id.
/// Used when hit-test returns a node with node_id=0 (e.g. pseudo-elements, post-process nodes).
pub fn find_parent_node_id_by_id(root: &WebCore, target_id: u32) -> u32 {
    fn walk(node: &WebCore, target_id: u32) -> Option<u32> {
        for child in &node.children {
            if child.node_id == target_id {
                return if node.node_id != 0 { Some(node.node_id) } else { None };
            }
            if let Some(id) = walk(child, target_id) { return Some(id); }
        }
        None
    }
    walk(root, target_id).unwrap_or(0)
}

/// Find the node_id of an <a> element with the given href.
pub fn find_link_node_id(root: &WebCore, href: &str) -> Option<u32> {
    if root.tag == "a" && root.node_id != 0 && root.style.href == href {
        return Some(root.node_id);
    }
    for child in &root.children {
        if let Some(id) = find_link_node_id(child, href) {
            return Some(id);
        }
    }
    None
}

pub(crate) fn subtree_contains_id(node: &WebCore, target_id: u32) -> bool {
    if node.node_id == target_id { return true; }
    for child in &node.children {
        if subtree_contains_id(child, target_id) { return true; }
    }
    false
}

pub(crate) fn extract_transitionable(node: &WebCore) -> HashMap<String, String> {
    extract_transitionable_style(&node.style)
}

pub(crate) fn extract_transitionable_style(s: &ComputedStyle) -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("opacity".into(),          format!("{}", s.opacity));
    m.insert("color".into(),            color_to_rgba(s.color));
    m.insert("background-color".into(), color_to_rgba(s.background_color));
    m.insert("border-color".into(),     color_to_rgba(s.border_top_color));
    m.insert("transform".into(),        s.transform.clone());
    m.insert("font-size".into(),        format!("{}px", s.font_size_px(16.0, 16.0)));
    m
}

fn color_to_rgba(c: Color) -> String {
    format!("rgba({},{},{},{:.4})", c.r, c.g, c.b, c.a as f32 / 255.0)
}

/// Find the two surrounding keyframe stops for `t` and return interpolated properties.
pub(crate) fn interpolate_keyframe_stops(stops: &[KeyframeStop], t: f32) -> Vec<(String, String)> {
    if stops.is_empty() { return Vec::new(); }
    if stops.len() == 1 { return stops[0].properties.clone(); }

    // Find surrounding stops.
    let (from, to, local_t) = if t <= stops[0].offset {
        (&stops[0], &stops[0], 0.0f32)
    } else if t >= stops[stops.len() - 1].offset {
        let last = &stops[stops.len() - 1];
        (last, last, 1.0f32)
    } else {
        let mut fi = 0usize;
        for i in 0..stops.len() - 1 {
            if t >= stops[i].offset && t <= stops[i + 1].offset {
                fi = i; break;
            }
        }
        let ti = fi + 1;
        let range = stops[ti].offset - stops[fi].offset;
        let lt = if range > 1e-6 { (t - stops[fi].offset) / range } else { 0.0 };
        (&stops[fi], &stops[ti], lt)
    };

    let mut result = Vec::new();
    for (prop, from_val) in &from.properties {
        let to_val = to.properties.iter()
            .find(|(p, _)| p == prop)
            .map(|(_, v)| v.as_str())
            .unwrap_or(from_val.as_str());
        result.push((prop.clone(), interpolate_value(from_val, to_val, local_t)));
    }
    result
}

/// Interpolate between two CSS value strings.
/// Handles `rgba(…)` colors and strings containing numbers.
pub(crate) fn interpolate_value(from: &str, to: &str, t: f32) -> String {
    if let Some(c) = interpolate_color(from, to, t) { return c; }
    // For transforms, if one side is empty/none, synthesize the identity form.
    let (from, to) = if from.is_empty() || from == "none" {
        (transform_identity(to).into(), to.to_string())
    } else if to.is_empty() || to == "none" {
        (from.to_string(), transform_identity(from).into())
    } else {
        (from.to_string(), to.to_string())
    };
    interpolate_numeric(&from, &to, t)
}

/// Given a CSS transform string like `rotate(180deg)`, return the identity form
/// with the same function and matching zero-ish arguments: `rotate(0deg)`.
fn transform_identity(transform: &str) -> String {
    let s = transform.trim();
    if s.is_empty() || s == "none" { return String::new(); }
    // Find the function name and argument count.
    if let Some(open) = s.find('(') {
        let func = &s[..open];
        let inner = s[open+1..].trim_end_matches(')');
        let arg_count = inner.split(',').count();
        // scale identity is 1, everything else is 0.
        let identity_val = if func.starts_with("scale") { "1" } else { "0" };
        // Preserve units from the original arguments.
        let units: Vec<&str> = inner.split(',').map(|a| {
            let a = a.trim();
            // Strip leading minus/digits/dot to find the unit suffix.
            let num_end = a.bytes().position(|b| b.is_ascii_alphabetic())
                .unwrap_or(a.len());
            &a[num_end..]
        }).collect();
        let args: Vec<String> = (0..arg_count).map(|i| {
            format!("{}{}", identity_val, units.get(i).unwrap_or(&""))
        }).collect();
        format!("{}({})", func, args.join(", "))
    } else {
        String::new()
    }
}

fn interpolate_color(from: &str, to: &str, t: f32) -> Option<String> {
    let (fr, fg, fb, fa) = parse_rgba(from)?;
    let (tr, tg, tb, ta) = parse_rgba(to)?;
    let r = lerp(fr, tr, t).round() as u8;
    let g = lerp(fg, tg, t).round() as u8;
    let b = lerp(fb, tb, t).round() as u8;
    let a = lerp(fa, ta, t);
    Some(format!("rgba({},{},{},{:.4})", r, g, b, a))
}

fn parse_rgba(s: &str) -> Option<(f32, f32, f32, f32)> {
    let s = s.trim();
    let inner = s.strip_prefix("rgba(")?.strip_suffix(')')?;
    let mut it = inner.split(',');
    let r = it.next()?.trim().parse::<f32>().ok()?;
    let g = it.next()?.trim().parse::<f32>().ok()?;
    let b = it.next()?.trim().parse::<f32>().ok()?;
    let a = it.next()?.trim().parse::<f32>().ok()?;
    Some((r, g, b, a))
}

/// Interpolate by extracting all decimal numbers from both strings and lerping them.
fn interpolate_numeric(from: &str, to: &str, t: f32) -> String {
    let from_nums = extract_nums(from);
    let to_nums   = extract_nums(to);

    if from_nums.is_empty() || from_nums.len() != to_nums.len() {
        return if t < 0.5 { from.to_string() } else { to.to_string() };
    }

    let mut result = from.to_string();
    // Replace in reverse order so byte offsets remain valid.
    for ((start, end, fv), (_, _, tv)) in from_nums.iter().zip(to_nums.iter()).rev() {
        let v = lerp(*fv, *tv, t);
        let s = if v == v.floor() && v.abs() < 1e9 {
            format!("{}", v as i64)
        } else {
            format!("{:.4}", v)
        };
        result.replace_range(start..end, &s);
    }
    result
}

/// Extract `(start_byte, end_byte, value)` for every number in `s`.
fn extract_nums(s: &str) -> Vec<(usize, usize, f32)> {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let neg = bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit();
        if bytes[i].is_ascii_digit() || neg {
            let start = i;
            if neg { i += 1; }
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') { i += 1; }
            if let Ok(v) = s[start..i].parse::<f32>() {
                result.push((start, i, v));
            }
        } else {
            i += 1;
        }
    }
    result
}

#[inline] fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

/// Apply an easing function to a linear progress value in 0.0..=1.0.
pub(crate) fn apply_easing(easing: &EasingFn, t: f32) -> f32 {
    match easing {
        EasingFn::Linear     => t,
        EasingFn::Ease       => cubic_bezier(0.25, 0.1, 0.25, 1.0, t),
        EasingFn::EaseIn     => cubic_bezier(0.42, 0.0, 1.0,  1.0, t),
        EasingFn::EaseOut    => cubic_bezier(0.0,  0.0, 0.58, 1.0, t),
        EasingFn::EaseInOut  => cubic_bezier(0.42, 0.0, 0.58, 1.0, t),
        EasingFn::CubicBezier(x1, y1, x2, y2) => cubic_bezier(*x1, *y1, *x2, *y2, t),
        EasingFn::StepStart  => if t <= 0.0 { 0.0 } else { 1.0 },
        EasingFn::StepEnd    => if t < 1.0 { 0.0 } else { 1.0 },
        EasingFn::Steps(n, jump_start) => {
            let n = *n as f32;
            if *jump_start { ((t * n).ceil() / n).min(1.0) }
            else           { ((t * n).floor() / n).min(1.0) }
        }
    }
}

/// CSS cubic-bezier evaluation via Newton-Raphson.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let mut u = t;
    for _ in 0..8 {
        let bx = bcoord(x1, x2, u) - t;
        let db = bderiv(x1, x2, u);
        if db.abs() < 1e-6 { break; }
        u = (u - bx / db).clamp(0.0, 1.0);
    }
    bcoord(y1, y2, u)
}

fn bcoord(p1: f32, p2: f32, t: f32) -> f32 {
    let t2 = t * t; let t3 = t2 * t;
    3.0 * (1.0 - t) * (1.0 - t) * t * p1
        + 3.0 * (1.0 - t) * t2 * p2
        + t3
}

fn bderiv(p1: f32, p2: f32, t: f32) -> f32 {
    3.0 * (1.0 - t) * (1.0 - t) * p1
        + 6.0 * (1.0 - t) * t * (p2 - p1)
        + 3.0 * t * t * (1.0 - p2)
}
