use std::collections::HashMap;
use std::sync::Arc;

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

pub type ComponentMeasureFn = Arc<dyn Fn(&HtmlBox, f32) -> (f32, f32) + Send + Sync>;
pub type ComponentPaintFn   = Arc<dyn Fn(&HtmlBox, &mut tiny_skia::Pixmap, f32, f32, f32, f32, f32) + Send + Sync>;

#[derive(Clone)]
pub struct ComponentCallbacks {
    pub measure: ComponentMeasureFn,
    pub paint:   ComponentPaintFn,
}

#[derive(Default, Clone)]
pub struct ComponentRegistry {
    pub map: HashMap<String, ComponentCallbacks>,
}

impl ComponentRegistry {
    pub fn new() -> Self { Self::default() }
    pub fn register(&mut self, tag: &str, measure: ComponentMeasureFn, paint: ComponentPaintFn) {
        self.map.insert(tag.to_string(), ComponentCallbacks { measure, paint });
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

// ─── CSS Length ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CssLength {
    Px(f32),
    Em(f32),
    Rem(f32),
    Percent(f32),
    /// Viewport-width percentage (1vw = 1% of viewport width).
    Vw(f32),
    /// Viewport-height percentage (1vh = 1% of viewport height).
    Vh(f32),
    Auto,
    Zero,
    None,
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
            CssLength::Auto       => 0.0,
            CssLength::Zero       => 0.0,
            CssLength::None       => 0.0,
        }
    }

    pub fn is_auto(&self) -> bool { matches!(self, CssLength::Auto) }
    pub fn is_none(&self) -> bool { matches!(self, CssLength::None | CssLength::Zero) }
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridTrackSize {
    pub kind:      GridTrackKind,
    pub value:     f32,
    pub min_kind:  GridTrackKind,
    pub min_value: f32,
    pub max_kind:  GridTrackKind,
    pub max_value: f32,
}

impl Default for GridTrackSize {
    fn default() -> Self {
        Self {
            kind: GridTrackKind::Auto, value: 0.0,
            min_kind: GridTrackKind::Auto, min_value: 0.0,
            max_kind: GridTrackKind::Auto, max_value: 0.0,
        }
    }
}

impl GridTrackSize {
    pub fn fixed(px: f32) -> Self { Self { kind: GridTrackKind::Fixed, value: px, ..Default::default() } }
    pub fn percent(pct: f32) -> Self { Self { kind: GridTrackKind::Percent, value: pct, ..Default::default() } }
    pub fn fr(fr: f32) -> Self { Self { kind: GridTrackKind::Fractional, value: fr, ..Default::default() } }
    pub fn auto() -> Self { Self::default() }
    pub fn is_auto(&self) -> bool { self.kind == GridTrackKind::Auto }
    pub fn is_none(&self) -> bool { self.kind == GridTrackKind::Auto && self.value == 0.0 }
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
    pub font_family:       String,
    pub font_size:         CssLength,
    pub font_weight:       FontWeight,
    pub font_style:        FontStyle,
    pub line_height:       CssLength,
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
    pub grid_column_start:     i32,  // 0=auto, >0=line number (1-based), <0=span
    pub grid_column_end:       i32,
    pub grid_row_start:        i32,
    pub grid_row_end:          i32,
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

    // Pointer events
    pub pointer_events: PointerEvents,

    // Quotes
    pub quotes: Vec<String>,

    // Container queries
    pub container_name: String,
    pub container_type: ContainerType,

    // Hover colors
    pub hover_color:            Option<Color>,
    pub hover_background_color: Option<Color>,

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

    // Transform / filter (stored raw; actual matrix math not implemented)
    pub transform:        String,
    pub filter:           String,
    pub backdrop_filter:  String,

    // Transition / animation (stored for future use)
    pub transition: String,
    pub animation:  String,
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
            font_family:      String::from("sans-serif"),
            font_size:        CssLength::Px(16.0),
            font_weight:      FontWeight::Normal,
            font_style:       FontStyle::Normal,
            line_height:      CssLength::Em(1.2),
            letter_spacing:   CssLength::Zero,
            word_spacing:     CssLength::Zero,
            text_align:       TextAlign::Left,
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
            grid_column_start:     0,
            grid_column_end:       0,
            grid_row_start:        0,
            grid_row_end:          0,
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

            pointer_events: PointerEvents::Auto,

            quotes: Vec::new(),

            container_name: String::new(),
            container_type: ContainerType::Normal,

            hover_color:            None,
            hover_background_color: None,

            list_style_image: String::new(),

            object_position_x: CssLength::Percent(50.0),
            object_position_y: CssLength::Percent(50.0),

            aspect_ratio: None,

            text_decoration_color: None,
            text_decoration_style: TextDecorationStyle::Solid,

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

            transform:       String::new(),
            filter:          String::new(),
            backdrop_filter: String::new(),

            transition:  String::new(),
            animation:   String::new(),
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
    pub fn inherit_from(&mut self, parent: &ComputedStyle) {
        self.color           = parent.color;
        self.font_family     = parent.font_family.clone();
        self.font_size       = parent.font_size;
        self.font_weight     = parent.font_weight;
        self.font_style      = parent.font_style;
        self.line_height     = parent.line_height;
        self.letter_spacing  = parent.letter_spacing;
        self.word_spacing    = parent.word_spacing;
        self.text_align      = parent.text_align;
        self.text_decoration = parent.text_decoration;
        self.text_indent     = parent.text_indent;
        self.white_space     = parent.white_space;
        self.text_transform  = parent.text_transform;
        self.word_break      = parent.word_break;
        self.overflow_wrap   = parent.overflow_wrap;
        self.direction       = parent.direction;
        self.list_style_type     = parent.list_style_type;
        self.list_style_position = parent.list_style_position;
        self.list_index      = parent.list_index;
        self.visibility      = parent.visibility;
        self.direction       = parent.direction;
        self.unicode_bidi    = parent.unicode_bidi;
        self.writing_mode    = parent.writing_mode;
        self.tab_size        = parent.tab_size;
        self.hyphens         = parent.hyphens;
        self.quotes          = parent.quotes.clone();
        self.cursor          = parent.cursor;
        self.pointer_events  = parent.pointer_events;
        self.small_caps      = parent.small_caps;
        self.user_select     = parent.user_select;
        self.font_stretch    = parent.font_stretch;
        self.href            = parent.href.clone();
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

    /// Resolved font size in px (needs parent px for em, root px for rem).
    pub fn font_size_px(&self, parent_px: f32, root_px: f32) -> f32 {
        self.font_size.resolve(parent_px, 0.0, root_px).max(1.0)
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
    /// BiDi visual segments in visual order. Empty = pure LTR, use logical order.
    pub visual_segments: Vec<VisualSegment>,
}

// ─── HTML Box (DOM node) ─────────────────────────────────────────────────────

/// A box/node in the box tree.  Mirrors the C++ `Box` struct.
#[derive(Clone, Debug)]
pub struct HtmlBox {
    pub tag:        String,
    pub style:      ComputedStyle,
    pub attributes: HashMap<String, String>,
    pub text:       String,             // Own text content
    pub children:   Vec<HtmlBox>,

    // Layout results (set by layout pass)
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

    // Image pixel data for <img> and replaced elements (RGBA8, row-major)
    pub image_data:   Option<Vec<u8>>,
    pub image_width:  u32,
    pub image_height: u32,

    // SVG source markup (for round-trip and re-rasterization)
    pub svg_markup: Option<String>,

    // Dirty flag for incremental layout
    pub layout_dirty:          bool,
    pub last_containing_width: f32,

    // Custom data store (arbitrary key/value pairs set by application code)
    pub data: HashMap<String, String>,

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
}

impl HtmlBox {
    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            style: ComputedStyle::default(),
            attributes: HashMap::new(),
            text: String::new(),
            children: Vec::new(),
            content_rect: Rect::default(),
            padding_rect: Rect::default(),
            border_rect:  Rect::default(),
            margin_rect:  Rect::default(),
            baseline:     0.0,
            line_cache:   Vec::new(),
            inline_runs:  Vec::new(),

            image_data:   None,
            image_width:  0,
            image_height: 0,

            svg_markup: None,

            collapsed_margin_top:    0.0,
            collapsed_margin_bottom: 0.0,
            scroll_height: 0.0,
            scroll_width:  0.0,
            scroll_top:    0.0,
            scroll_left:   0.0,
            layout_dirty:          false,
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

            data: HashMap::new(),
        }
    }

    pub fn get_attr(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(|s| s.as_str())
    }

    pub fn is_text_node(&self) -> bool {
        self.tag == "#text"
    }

    pub fn is_void(&self) -> bool {
        matches!(self.tag.as_str(),
            "br" | "hr" | "img" | "input" | "meta" | "link" | "col" |
            "area" | "base" | "embed" | "param" | "source" | "track" | "wbr")
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
    pub fn query_selector_all<'a>(&'a self, selector: &str) -> Vec<&'a HtmlBox> {
        let mut results = Vec::new();
        self.collect_matching(selector, &mut results);
        results
    }

    fn collect_matching<'a>(&'a self, selector: &str, out: &mut Vec<&'a HtmlBox>) {
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
use crate::dom::{Editor, EventListeners};
use crate::layout::LayoutEngine;

/// The root document: box tree + stylesheet + metadata.
#[derive(Debug, Clone)]
pub struct Document {
    pub root:       HtmlBox,
    pub stylesheet: Stylesheet,
    pub title:      String,
    pub base_url:   String,
    pub editor:     Editor,
    pub events:     EventListeners,
}

impl Document {
    pub fn new() -> Self {
        Self {
            root:       HtmlBox::new("html"),
            stylesheet: Stylesheet::default(),
            title:      String::new(),
            base_url:   String::new(),
            editor:     Editor::new(),
            events:     EventListeners::new(),
        }
    }

    /// High-level mouse event entry point.
    pub fn process_mouse_event(&mut self, etype: crate::dom::HtmlEventType, doc_pt: (f32, f32), button: u8) -> bool {
        // First dispatch to listeners so they can `prevent_default()`.
        let mut evt = crate::dom::HtmlEvent::new(etype);
        evt.doc_pos = doc_pt;
        evt.button  = button;
        if let Some(hit) = crate::layout::hit_test::point_to_hit(&self.root, doc_pt, button) {
            evt.target = hit.box_ptr;
        }

        let (handled, evt) = self.events.dispatch_and_return(&self.root, evt);

        let mut redraw = handled;

        // Only perform editor/default behavior if not prevented by handlers.
        if !evt.default_prevented {
            if self.editor.handle_mouse_event(&self.root, etype, doc_pt, button) {
                redraw = true;
            }
        }

        // If handlers or default behavior indicated a redraw is needed, run
        // a layout pass now so the renderer sees updated line caches/inline
        // runs immediately. Use the root's last known containing width.
        if redraw {
            let width = self.root.last_containing_width.max(0.0);
            LayoutEngine::new().layout(self, width);
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

        let (handled, evt) = self.events.dispatch_and_return(&self.root, evt);

        let mut redraw = handled;

        if !evt.default_prevented {
            if self.editor.handle_key_event(&mut self.root, etype, key_code, ch, ctrl) {
                redraw = true;
            }
        }

        redraw
    }

    /// Walk all boxes in depth-first order.
    pub fn walk_all<F: FnMut(&HtmlBox)>(root: &HtmlBox, f: &mut F) {
        f(root);
        for child in &root.children {
            Self::walk_all(child, f);
        }
    }

    pub fn walk_all_mut<F: FnMut(&mut HtmlBox)>(root: &mut HtmlBox, f: &mut F) {
        f(root);
        for child in &mut root.children {
            Self::walk_all_mut(child, f);
        }
    }
}

impl Default for Document {
    fn default() -> Self { Self::new() }
}
