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
    Subgrid,
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
    pub subgrid_columns:       bool,
    pub subgrid_rows:          bool,
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
            subgrid_columns:       false,
            subgrid_rows:          false,
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
    pub fn inherit_from(&mut self, parent: &ComputedStyle) {
        self.color           = parent.color;
        self.font_family             = parent.font_family.clone();
        self.font_size               = parent.font_size;
        self.font_weight             = parent.font_weight;
        self.font_style              = parent.font_style;
        self.font_variation_settings = parent.font_variation_settings.clone();
        self.font_feature_settings   = parent.font_feature_settings.clone();
        self.line_height             = parent.line_height;
        self.letter_spacing  = parent.letter_spacing;
        self.word_spacing    = parent.word_spacing;
        self.text_align      = parent.text_align;
        self.text_decoration           = parent.text_decoration;
        self.text_decoration_color     = parent.text_decoration_color;
        self.text_decoration_style     = parent.text_decoration_style;
        self.text_decoration_thickness = parent.text_decoration_thickness;
        self.text_underline_offset     = parent.text_underline_offset;
        self.text_indent               = parent.text_indent;
        self.white_space     = parent.white_space;
        self.text_transform  = parent.text_transform;
        self.word_break      = parent.word_break;
        self.overflow_wrap   = parent.overflow_wrap;
        self.direction       = parent.direction;
        self.list_style_type     = parent.list_style_type;
        self.list_style_position = parent.list_style_position;
        // list_index is set by the HTML parser (ol counter), not inherited via cascade.
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
        self.text_shadow     = parent.text_shadow.clone();
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
    /// Per-character-boundary x positions relative to `self.x`, in logical pixels.
    /// `char_x[i]` = visual x of the caret at byte offset `text_start + i`.
    /// Length = `text_length + 1` (last entry = position after the final character).
    /// Empty when no FontSystem was available during layout (falls back to approximation).
    pub char_x: Vec<f32>,
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
    /// A per-element scrollbar; the element is identified by a raw pointer.
    /// The pointer is valid as long as the document tree has not been rebuilt.
    Element(*mut HtmlBox),
}

// Safety: we never share these across threads; the pointer is only used when
// the Document is exclusively borrowed.
unsafe impl Send for ScrollbarDragKind {}
unsafe impl Sync for ScrollbarDragKind {}

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
    /// The HtmlBox raw pointer, stored as `usize` for Hash/Eq.
    pub element_id: usize,
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

/// The root document: box tree + stylesheet + metadata.
#[derive(Debug, Clone)]
pub struct Document {
    pub root:            HtmlBox,
    pub stylesheet:      Stylesheet,
    pub title:           String,
    pub base_url:        String,
    /// URLs from `<link rel="stylesheet" href="...">` tags in `<head>`.
    /// Populated by the parser so the host can fetch and merge external CSS.
    pub linked_stylesheets: Vec<String>,
    pub editor:          Editor,
    pub events:          EventListeners,
    /// Viewport scroll position in logical pixels (managed by Renderer::render).
    pub scroll_x:        f32,
    pub scroll_y:        f32,
    /// Active scrollbar drag state (None when not dragging).
    pub scrollbar_drag:  Option<ScrollbarDrag>,
    /// Currently hovered element (raw pointer, null if none).
    pub hovered_box:     *const HtmlBox,
    /// Currently active (pressed) element (raw pointer, null if none).
    pub active_box:      *const HtmlBox,
    /// Currently focused element (raw pointer, null if none).
    pub focused_box:     *const HtmlBox,
    /// Element hit on last MouseDown — used to fire Click on MouseUp if same target.
    pub mousedown_target: *const HtmlBox,
    /// Last click target + time for DblClick detection.
    pub last_click_target: *const HtmlBox,
    pub last_click_time:   Option<std::time::Instant>,
    /// Drag state machine.
    pub drag_source:       *const HtmlBox,
    pub drag_start_doc_pt: (f32, f32),
    pub drag_active:       bool,
    /// Set of link hrefs the user has clicked (for :visited pseudo-class).
    pub visited_urls:    std::collections::HashSet<String>,
    /// Last known logical viewport size — kept in sync by LayoutEngine::layout.
    pub viewport_w:      f32,
    pub viewport_h:      f32,
    /// True when focus was moved by keyboard (Tab/Shift+Tab) — drives :focus-visible.
    pub keyboard_focus:  bool,

    // ── CSS animation / transition runtime ────────────────────────────────────
    /// All currently running CSS animations (one entry per animation per element).
    pub active_animations: Vec<AnimState>,
    /// Per-element active transitions, keyed by HtmlBox pointer (as usize).
    pub(crate) transition_states: HashMap<usize, Vec<TransitionState>>,
    /// Previous transitionable style values per element, for change detection.
    pub(crate) prev_styles: HashMap<usize, HashMap<String, String>>,
    /// Interpolated CSS property overrides produced by `tick_animations`.
    /// Applied on top of the cascade result before geometry runs.
    pub(crate) animation_overrides: HashMap<usize, Vec<(String, String)>>,
    /// Set by `tick_animations`; tells the host to request another render frame.
    pub needs_animation_frame: bool,

    // ── aria-live region machinery ─────────────────────────────────────────────
    /// Announcements queued since the last call to `take_announcements()`.
    pub pending_announcements: Vec<Announcement>,
    /// Text-content snapshots for each aria-live region, keyed by HtmlBox pointer.
    /// Updated every layout pass to detect content changes.
    pub(crate) live_region_snapshots: HashMap<usize, String>,
    /// `false` until the first `check_live_regions()` call.
    /// On the very first pass, only assertive regions announce their initial content;
    /// polite regions are silently initialised so they don't flood the user on load.
    pub(crate) live_regions_initialized: bool,
}

impl Document {
    pub fn new() -> Self {
        Self {
            root:            HtmlBox::new("html"),
            stylesheet:      Stylesheet::default(),
            title:           String::new(),
            base_url:        String::new(),
            linked_stylesheets: Vec::new(),
            editor:          Editor::new(),
            events:          EventListeners::new(),
            scroll_x:        0.0,
            scroll_y:        0.0,
            scrollbar_drag:  None,
            hovered_box:       std::ptr::null(),
            active_box:        std::ptr::null(),
            focused_box:       std::ptr::null(),
            mousedown_target:  std::ptr::null(),
            last_click_target: std::ptr::null(),
            last_click_time:   None,
            drag_source:       std::ptr::null(),
            drag_start_doc_pt: (0.0, 0.0),
            drag_active:       false,
            visited_urls:      std::collections::HashSet::new(),
            viewport_w:        0.0,
            viewport_h:        0.0,
            keyboard_focus:    false,
            active_animations:     Vec::new(),
            transition_states:     HashMap::new(),
            prev_styles:           HashMap::new(),
            animation_overrides:   HashMap::new(),
            needs_animation_frame: false,
            pending_announcements:    Vec::new(),
            live_region_snapshots:    HashMap::new(),
            live_regions_initialized: false,
        }
    }

    /// Re-apply the CSS cascade to the entire document tree.
    /// Call this after mutating class attributes (e.g. toggling dark mode) so
    /// that `ComputedStyle` on every box is updated before the next layout pass.
    /// Resets hover/active pointers since box addresses may change after re-layout.
    pub fn recascade(&mut self) {
        // Invalidate hover/active pointers — raw pointers may alias differently
        // after HtmlBox trees are rebuilt or re-allocated during parsing.
        self.hovered_box = std::ptr::null();
        self.active_box  = std::ptr::null();
        let ss = self.stylesheet.clone();
        let focused = self.focused_box;
        crate::css::apply_cascade_vp(
            &mut self.root, &ss, None, 16.0,
            self.viewport_w, self.viewport_h, focused, self.keyboard_focus,
        );
    }

    /// Re-apply cascade with an explicit focused element pointer.
    pub fn recascade_with_focus(&mut self, focused: *const HtmlBox) {
        self.focused_box = focused;
        self.hovered_box = std::ptr::null();
        self.active_box  = std::ptr::null();
        let ss = self.stylesheet.clone();
        crate::css::apply_cascade_vp(
            &mut self.root, &ss, None, 16.0,
            self.viewport_w, self.viewport_h, focused, self.keyboard_focus,
        );
    }

    /// High-level mouse event entry point.
    pub fn process_mouse_event(&mut self, etype: crate::dom::HtmlEventType, doc_pt: (f32, f32), button: u8) -> bool {
        use crate::dom::{HtmlEventType, HtmlEvent};
        // client_pos = screen-space logical coordinates (doc coords minus scroll).
        let client_pos = (doc_pt.0, doc_pt.1 - self.scroll_y);

        let mut evt = HtmlEvent::new(etype);
        evt.doc_pos    = doc_pt;
        evt.client_pos = client_pos;
        evt.button     = button;
        let hit_ptr: *const HtmlBox = crate::layout::hit_test::point_to_hit(&self.root, doc_pt, button)
            .map(|h| h.box_ptr)
            .unwrap_or(std::ptr::null());
        evt.target = hit_ptr;

        let mut redraw = false;
        match etype {
            HtmlEventType::MouseMove => {
                if self.hovered_box != hit_ptr {
                    self.hovered_box = hit_ptr;
                    redraw = true;
                }
                // Drag: if mouse button held and moved past threshold, fire DragStart/Drag.
                if !self.drag_source.is_null() {
                    let dx = doc_pt.0 - self.drag_start_doc_pt.0;
                    let dy = doc_pt.1 - self.drag_start_doc_pt.1;
                    if !self.drag_active && (dx * dx + dy * dy) > 25.0 {
                        // DragStart
                        self.drag_active = true;
                        let mut e = HtmlEvent::new(HtmlEventType::DragStart);
                        e.target = self.drag_source; e.doc_pos = self.drag_start_doc_pt;
                        e.client_pos = (self.drag_start_doc_pt.0, self.drag_start_doc_pt.1 - self.scroll_y);
                        if self.events.dispatch(&self.root, e) { redraw = true; }
                    }
                    if self.drag_active {
                        let mut e = HtmlEvent::new(HtmlEventType::Drag);
                        e.target = self.drag_source; e.doc_pos = doc_pt; e.client_pos = client_pos;
                        if self.events.dispatch(&self.root, e) { redraw = true; }
                    }
                }
            }
            HtmlEventType::MouseDown | HtmlEventType::PointerDown => {
                if self.active_box != hit_ptr {
                    self.active_box = hit_ptr;
                    redraw = true;
                }
                if etype == HtmlEventType::MouseDown {
                    self.mousedown_target  = hit_ptr;
                    // Arm drag state machine.
                    self.drag_source       = hit_ptr;
                    self.drag_start_doc_pt = doc_pt;
                    self.drag_active       = false;
                }
                // Focus change on click.
                // Only interactive (focusable) elements receive focus on click.
                // Clicking a non-focusable element blurs the current focus.
                if etype == HtmlEventType::MouseDown {
                    let click_focusable = !hit_ptr.is_null()
                        && is_focusable_node(unsafe { &*hit_ptr });
                    let new_focus = if click_focusable { hit_ptr } else { std::ptr::null() };
                    if self.focused_box != new_focus {
                        let old_focus = self.focused_box;
                        self.keyboard_focus = false;
                        self.focused_box = new_focus;
                        if !old_focus.is_null() {
                            let mut e = HtmlEvent::new(HtmlEventType::Blur);
                            e.target = old_focus; e.related_target = new_focus;
                            self.events.dispatch(&self.root, e);
                            let mut e = HtmlEvent::new(HtmlEventType::FocusOut);
                            e.target = old_focus; e.related_target = new_focus;
                            self.events.dispatch(&self.root, e);
                        }
                        if !new_focus.is_null() {
                            let mut e = HtmlEvent::new(HtmlEventType::Focus);
                            e.target = new_focus; e.related_target = old_focus;
                            self.events.dispatch(&self.root, e);
                            let mut e = HtmlEvent::new(HtmlEventType::FocusIn);
                            e.target = new_focus; e.related_target = old_focus;
                            self.events.dispatch(&self.root, e);
                        }
                        // Always recascade when focus changes so :focus/:focus-visible update.
                        let ss = self.stylesheet.clone();
                        crate::css::apply_cascade_vp(
                            &mut self.root, &ss, None, 16.0,
                            self.viewport_w, self.viewport_h, self.focused_box, false,
                        );
                        redraw = true;
                    }
                }
            }
            HtmlEventType::MouseUp | HtmlEventType::PointerUp => {
                if !self.active_box.is_null() {
                    self.active_box = std::ptr::null();
                    redraw = true;
                }
                if etype == HtmlEventType::MouseUp {
                    // DragEnd if drag was active; save flag before resetting.
                    let was_dragging = self.drag_active;
                    if was_dragging {
                        let mut e = HtmlEvent::new(HtmlEventType::DragEnd);
                        e.target = self.drag_source; e.doc_pos = doc_pt; e.client_pos = client_pos;
                        if self.events.dispatch(&self.root, e) { redraw = true; }
                    }
                    self.drag_source = std::ptr::null();
                    self.drag_active = false;

                    // Click only if no drag occurred and released on same element as pressed.
                    if !hit_ptr.is_null() && hit_ptr == self.mousedown_target && !was_dragging {
                        let mut click = HtmlEvent::new(HtmlEventType::Click);
                        click.target = hit_ptr; click.doc_pos = doc_pt; click.client_pos = client_pos;
                        click.button = button;
                        if self.events.dispatch(&self.root, click) { redraw = true; }

                        // DblClick: same target within 400 ms.
                        let now = std::time::Instant::now();
                        let is_dbl = self.last_click_target == hit_ptr
                            && self.last_click_time
                                .map(|t| t.elapsed().as_millis() < 400)
                                .unwrap_or(false);
                        if is_dbl {
                            let mut dbl = HtmlEvent::new(HtmlEventType::DblClick);
                            dbl.target = hit_ptr; dbl.doc_pos = doc_pt; dbl.client_pos = client_pos;
                            dbl.button = button;
                            if self.events.dispatch(&self.root, dbl) { redraw = true; }
                            // Reset so triple-click doesn't re-trigger.
                            self.last_click_target = std::ptr::null();
                            self.last_click_time   = None;
                        } else {
                            self.last_click_target = hit_ptr;
                            self.last_click_time   = Some(now);
                        }
                    }
                    self.mousedown_target = std::ptr::null();
                    // Track visited links.
                    if button == 0 {
                        if let Some(href) = crate::layout::hit_test::hit_test_link(&self.root, doc_pt, button) {
                            self.visited_urls.insert(href);
                        }
                    }
                }
            }
            _ => {}
        }

        let (handled, evt) = self.events.dispatch_and_return(&self.root, evt);
        if handled { redraw = true; }

        // Only perform editor/default behavior if not prevented by handlers.
        if !evt.default_prevented {
            if self.editor.handle_mouse_event(&self.root, etype, doc_pt, button) {
                redraw = true;
            }
        }

        // Full cascade + layout only when event handlers or editor logic changed
        // DOM state (class toggles, etc.), not merely for hover/active pointer updates.
        if handled {
            let width = self.root.last_containing_width.max(0.0);
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
        let new_ptr: *const HtmlBox = crate::layout::hit_test::point_to_hit(&self.root, doc_pt, 0)
            .map(|h| h.box_ptr)
            .unwrap_or(std::ptr::null());
        let old_ptr = self.hovered_box;
        if new_ptr == old_ptr { return false; }

        let mut redraw = false;
        macro_rules! ev {
            ($t:expr, $tgt:expr, $rel:expr, $bubble:expr) => {{
                let mut e = HtmlEvent::new($t);
                e.target = $tgt; e.related_target = $rel;
                e.doc_pos = doc_pt; e.client_pos = client_pos;
                if $bubble { self.events.dispatch(&self.root, e) }
                else       { self.events.dispatch_direct(&self.root, e) }
            }};
        }
        if !old_ptr.is_null() {
            if ev!(HtmlEventType::MouseOut,    old_ptr, new_ptr, true)  { redraw = true; }
            if ev!(HtmlEventType::MouseLeave,  old_ptr, new_ptr, false) { redraw = true; }
            ev!(HtmlEventType::PointerOut,   old_ptr, new_ptr, true);
            ev!(HtmlEventType::PointerLeave, old_ptr, new_ptr, false);
        }
        if !new_ptr.is_null() {
            if ev!(HtmlEventType::MouseOver,   new_ptr, old_ptr, true)  { redraw = true; }
            if ev!(HtmlEventType::MouseEnter,  new_ptr, old_ptr, false) { redraw = true; }
            ev!(HtmlEventType::PointerOver,  new_ptr, old_ptr, true);
            ev!(HtmlEventType::PointerEnter, new_ptr, old_ptr, false);
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
            node:         &HtmlBox,
            snapshots:    &mut HashMap<usize, String>,
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
                    let ptr   = node as *const HtmlBox as usize;
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

        let root_ptr = &self.root as *const HtmlBox as *const u8; // borrow-checker anchor
        let _ = root_ptr;
        walk(&self.root, &mut self.live_region_snapshots, &mut new_ann, initialized);

        self.live_regions_initialized = true;
        self.pending_announcements.extend(new_ann);
    }

    // ── CSS Animation / Transition runtime ────────────────────────────────────

    /// Walk the tree and ensure an `AnimState` exists for every element that
    /// currently has an `animation` property.  Call this after each cascade pass.
    pub fn sync_animations(&mut self, now: std::time::Instant) {
        let mut current: Vec<(usize, ParsedAnimation)> = Vec::new();
        fn collect(node: &HtmlBox, out: &mut Vec<(usize, ParsedAnimation)>) {
            let id = node as *const HtmlBox as usize;
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
    /// Call this right after a cascade pass (when computed styles may have changed).
    pub fn sync_transitions(&mut self, now: std::time::Instant) {
        let mut current: Vec<(usize, Vec<ParsedTransition>, HashMap<String, String>)> = Vec::new();
        fn collect(node: &HtmlBox, out: &mut Vec<(usize, Vec<ParsedTransition>, HashMap<String, String>)>) {
            let id = node as *const HtmlBox as usize;
            if !node.style.transitions.is_empty() {
                out.push((id, node.style.transitions.clone(), extract_transitionable(node)));
            }
            for child in &node.children { collect(child, out); }
        }
        collect(&self.root, &mut current);

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
                    if prv == cur { continue; }

                    // Already transitioning to this value?
                    let already = self.transition_states
                        .entry(*elem_id).or_default()
                        .iter().any(|t| t.property == prop && t.to_value == cur);
                    if already { continue; }

                    let entry = self.transition_states.entry(*elem_id).or_default();
                    entry.retain(|t| t.property != prop);
                    entry.push(TransitionState {
                        property:    prop.to_string(),
                        from_value:  prv.to_string(),
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
        let mut empty_elems: Vec<usize> = Vec::new();
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
                if progress >= 1.0 { done_trs.push(i); continue; }

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
                            let doc_h = self.root.margin_rect.h;
                            let max_s = (doc_h - viewport_h).max(0.0);
                            self.scroll_y = new_scroll.min(max_s);
                        }
                        ScrollbarDragKind::Element(ptr) => {
                            let node = unsafe { &mut *ptr };
                            let max_s = (node.scroll_height - node.content_rect.h).max(0.0);
                            node.scroll_top = new_scroll.min(max_s);
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
                let doc_h = self.root.margin_rect.h;
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
        let mut positive: Vec<(*const HtmlBox, i32)> = Vec::new();
        let mut normal:   Vec<*const HtmlBox>         = Vec::new();
        collect_focusable_ordered(&self.root, &mut positive, &mut normal);
        positive.sort_by_key(|&(_, idx)| idx);
        let mut focusable: Vec<*const HtmlBox> =
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
        if !old_focus.is_null() {
            let mut e = HtmlEvent::new(HtmlEventType::Blur);
            e.target = old_focus; e.related_target = new_focus;
            self.events.dispatch(&self.root, e);
            let mut e = HtmlEvent::new(HtmlEventType::FocusOut);
            e.target = old_focus; e.related_target = new_focus;
            self.events.dispatch(&self.root, e);
        }
        if !new_focus.is_null() {
            let mut e = HtmlEvent::new(HtmlEventType::Focus);
            e.target = new_focus; e.related_target = old_focus;
            self.events.dispatch(&self.root, e);
            let mut e = HtmlEvent::new(HtmlEventType::FocusIn);
            e.target = new_focus; e.related_target = old_focus;
            self.events.dispatch(&self.root, e);
        }
        let ss = self.stylesheet.clone();
        self.hovered_box = std::ptr::null();
        self.active_box  = std::ptr::null();
        crate::css::apply_cascade_vp(
            &mut self.root, &ss, None, 16.0,
            self.viewport_w, self.viewport_h, self.focused_box, true,
        );
        true
    }
}

/// Returns true if `node` is a focusable element (native or via tabindex/contenteditable).
/// tabindex=-1 elements return true (focusable by script/click) but are excluded from
/// the *tab* order by `collect_focusable_ordered`.
pub fn is_focusable_node(node: &HtmlBox) -> bool {
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
    node: &HtmlBox,
    positive: &mut Vec<(*const HtmlBox, i32)>,
    normal:   &mut Vec<*const HtmlBox>,
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
        Some(n) if n > 0  => positive.push((node as *const HtmlBox, n)),
        Some(0)           => normal.push(node as *const HtmlBox),
        Some(_)           => {} // tabindex < 0: excluded from tab order
        None if native    => normal.push(node as *const HtmlBox),
        None              => {}
    }

    for child in &node.children {
        collect_focusable_ordered(child, positive, normal);
    }
}

impl Default for Document {
    fn default() -> Self { Self::new() }
}

// ─── aria-live helper ──────────────────────────────────────────────────────────

/// Collect the visible text content of a live region by walking its subtree.
/// Used by `Document::check_live_regions` to compare snapshots.
fn collect_live_text(node: &HtmlBox) -> String {
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

fn collect_live_text_inner(node: &HtmlBox, buf: &mut String) {
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
fn scroll_abs_in(node: &mut HtmlBox, pt: (f32, f32), delta_x: f32, delta_y: f32) -> bool {
    for child in &mut node.children {
        if matches!(child.style.display, Display::None) { continue; }
        if matches!(child.style.position, Position::Absolute | Position::Fixed) {
            let mr = child.margin_rect;
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
fn scroll_box_at(node: &mut HtmlBox, pt: (f32, f32), delta_x: f32, delta_y: f32) -> bool {
    if matches!(node.style.display, Display::None) { return false; }

    // Adjust pt for this node's own scroll so we can test its children,
    // whose margin_rect positions are in layout space (scroll = 0 reference).
    let local_pt = (pt.0 + node.scroll_left, pt.1 + node.scroll_top);

    // Recurse depth-first; innermost hit wins.
    for child in &mut node.children {
        if matches!(child.style.display, Display::None) { continue; }
        if matches!(child.style.position, Position::Absolute | Position::Fixed) {
            // Out-of-flow boxes use the viewport coordinate rather than parent scroll.
            let mr = child.margin_rect;
            if pt.0 >= mr.x && pt.0 < mr.x + mr.w && pt.1 >= mr.y && pt.1 < mr.y + mr.h {
                if scroll_box_at(child, pt, delta_x, delta_y) { return true; }
            }
            continue;
        }
        let mr = child.margin_rect;
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
        && node.scroll_height > node.content_rect.h;
    let can_h = delta_x.abs() > 0.1
        && matches!(node.style.overflow_x, Overflow::Scroll | Overflow::Auto)
        && node.scroll_width > node.content_rect.w;

    let mut scrolled = false;

    if can_v {
        let max_scroll = (node.scroll_height - node.content_rect.h).max(0.0);
        let before = node.scroll_top;
        node.scroll_top = (node.scroll_top - delta_y).clamp(0.0, max_scroll);
        if (node.scroll_top - before).abs() > 1e-3 {
            apply_scroll_snap_y(node);
            scrolled = true;
        }
    }
    if can_h {
        let max_scroll = (node.scroll_width - node.content_rect.w).max(0.0);
        let before = node.scroll_left;
        node.scroll_left = (node.scroll_left - delta_x).clamp(0.0, max_scroll);
        if (node.scroll_left - before).abs() > 1e-3 {
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
fn apply_scroll_snap_y(node: &mut HtmlBox) {
    if !node.style.scroll_snap_type.snaps_y() { return; }
    let content_y = node.content_rect.y;
    let content_h = node.content_rect.h;
    let snap_points = collect_snap_points_y(node, content_y, content_h);
    if snap_points.is_empty() { return; }
    let max_scroll = (node.scroll_height - content_h).max(0.0);
    let target = nearest_snap(node.scroll_top, &snap_points, content_h,
                              node.style.scroll_snap_type.mandatory);
    node.scroll_top = target.clamp(0.0, max_scroll);
}

/// Snap the horizontal scroll position of `node`.
fn apply_scroll_snap_x(node: &mut HtmlBox) {
    if !node.style.scroll_snap_type.snaps_x() { return; }
    let content_x = node.content_rect.x;
    let content_w = node.content_rect.w;
    let snap_points = collect_snap_points_x(node, content_x, content_w);
    if snap_points.is_empty() { return; }
    let max_scroll = (node.scroll_width - content_w).max(0.0);
    let target = nearest_snap(node.scroll_left, &snap_points, content_w,
                              node.style.scroll_snap_type.mandatory);
    node.scroll_left = target.clamp(0.0, max_scroll);
}

fn collect_snap_points_y(node: &HtmlBox, content_y: f32, content_h: f32) -> Vec<f32> {
    let mut pts = Vec::new();
    for child in &node.children {
        if matches!(child.style.display, Display::None) { continue; }
        let mr = child.margin_rect;
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

fn collect_snap_points_x(node: &HtmlBox, content_x: f32, content_w: f32) -> Vec<f32> {
    let mut pts = Vec::new();
    for child in &node.children {
        if matches!(child.style.display, Display::None) { continue; }
        let mr = child.margin_rect;
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
    node:      &mut HtmlBox,
    screen_x:  f32,
    screen_y:  f32,
    sx:        f32,
    sy:        f32,
    sbw:       f32,
    drag_out:  &mut Option<ScrollbarDrag>,
) -> bool {
    if matches!(node.style.display, Display::None) { return false; }

    // Children are rendered with the parent's scroll added.
    let child_sx = sx + node.scroll_left;
    let child_sy = sy + node.scroll_top;

    for child in node.children.iter_mut() {
        if scrollbar_hit_test(child, screen_x, screen_y, child_sx, child_sy, sbw, drag_out) {
            return true;
        }
    }

    let cr = node.content_rect;
    let pr = node.padding_rect;
    let prx = pr.x - sx;
    let cy = cr.y - sy;

    let show_v = node.style.overflow_y == Overflow::Scroll
        || (node.style.overflow_y == Overflow::Auto && node.scroll_height > cr.h);

    if show_v && node.scroll_height > cr.h {
        // Scrollbar is at the right edge of the padding box (matches draw_scrollbars).
        let track_x = prx + pr.w - sbw;
        if screen_x >= track_x && screen_x < prx + pr.w
            && screen_y >= cy && screen_y < cy + cr.h
        {
            let track_h     = cr.h;
            let thumb_h     = (track_h * track_h / node.scroll_height).max(20.0);
            let max_s       = node.scroll_height - cr.h;
            let scroll_per_px = if track_h - thumb_h > 0.0 { max_s / (track_h - thumb_h) } else { 0.0 };
            let thumb_y     = if max_s > 0.0 { node.scroll_top * (track_h - thumb_h) / max_s } else { 0.0 };
            let local_y     = screen_y - cy;

            // Jump-scroll if click is outside the thumb.
            if !(local_y >= thumb_y && local_y < thumb_y + thumb_h) {
                let new_thumb_y = (local_y - thumb_h * 0.5).clamp(0.0, track_h - thumb_h);
                node.scroll_top = (new_thumb_y * scroll_per_px).clamp(0.0, max_s);
            }

            *drag_out = Some(ScrollbarDrag {
                kind:          ScrollbarDragKind::Element(node as *mut HtmlBox),
                start_mouse_y: screen_y,
                start_scroll:  node.scroll_top,
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
pub(crate) fn extract_transitionable(node: &HtmlBox) -> HashMap<String, String> {
    let s = &node.style;
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
    interpolate_numeric(from, to, t)
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
