//! `ComputedStyle` — the resolved property set for one element.

#![allow(unused_imports)]
use super::*;
use std::collections::{HashMap, HashSet};
use crate::css::*;
use crate::dom::*;
use crate::html::*;

// ─── Computed Style ───────────────────────────────────────────────────────────

/// The rarely-set half of a computed style, kept behind a `Box`.
///
/// Every field is `Vec` or `String`, so the empty value is `const` — which is
/// what lets `ComputedStyle::rare()` hand out a shared static instead of
/// allocating for the 99.3% of elements that set none of them.
#[derive(Clone, Debug, Default)]
pub struct RareStyle {
    pub grid_template_columns: Vec<GridTrackSize>,
    pub grid_template_rows: Vec<GridTrackSize>,
    pub grid_template_areas: Vec<Vec<String>>,
    pub auto_repeat_columns: Vec<GridTrackSize>,
    pub gradient_stops: Vec<GradientStop>,
    pub animations: Vec<ParsedAnimation>,
    pub transitions: Vec<ParsedTransition>,
    pub font_variation_settings: Vec<(String, f32)>,
    pub font_feature_settings: Vec<(String, u32)>,
    pub quotes: Vec<String>,
    pub filter: String,
    pub backdrop_filter: String,
    pub mask_image_url: String,
}

impl RareStyle {
    pub const EMPTY: RareStyle = RareStyle {
        grid_template_columns: Vec::new(),
        grid_template_rows: Vec::new(),
        grid_template_areas: Vec::new(),
        auto_repeat_columns: Vec::new(),
        gradient_stops: Vec::new(),
        animations: Vec::new(),
        transitions: Vec::new(),
        font_variation_settings: Vec::new(),
        font_feature_settings: Vec::new(),
        quotes: Vec::new(),
        filter: String::new(),
        backdrop_filter: String::new(),
        mask_image_url: String::new(),
    };
}

impl ComputedStyle {
    /// Read the rare properties. Never allocates.
    pub fn rare(&self) -> &RareStyle {
        static EMPTY: RareStyle = RareStyle::EMPTY;
        self.rare.as_deref().unwrap_or(&EMPTY)
    }

    /// Write the rare properties, allocating the box on first use.
    pub fn rare_mut(&mut self) -> &mut RareStyle {
        self.rare.get_or_insert_with(|| Box::new(RareStyle::default()))
    }
}

#[derive(Clone, Debug)]
pub struct ComputedStyle {
    /// The properties almost nothing sets — `arenaplan.md` item 2.
    ///
    /// ⛔ MEASURED before building: 8 of 1,132 elements on demo.html set any of
    /// them (0.7%), and they cost 312 B in every `ComputedStyle` regardless.
    /// Boxing them is about 10% of tree memory.
    pub rare: Option<Box<RareStyle>>,

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
    /// `safe` / `unsafe` overflow-position bits (Box Alignment §4.4).
    /// bit 0 `justify-content`, 1 `align-content`, 2 `align-items`,
    /// 3 `align-self`. `safe` makes an alignment fall back to start rather
    /// than overflow the container's start edge; the default is `unsafe`,
    /// which is what a bare `center` / `flex-end` means.
    pub align_safety:    u8,
    pub flex_grow:       f32,
    pub flex_shrink:     f32,
    pub flex_basis:      CssLength,
    pub order:           i32,
    pub gap:             CssLength,
    pub row_gap:         CssLength,
    pub column_gap:      CssLength,

    pub grid_auto_columns:     GridTrackSize,
    pub grid_auto_rows:        GridTrackSize,
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
    // Legacy clip: rect(top, right, bottom, left) — absolute offsets from the element's border box.
    // Stored as [top, right, bottom, left] in px. Only applies to position:absolute/fixed.
    pub clip_rect: Option<[f32; 4]>,

    // Pointer events
    pub pointer_events: PointerEvents,


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

    // Parsed transform / filter (for rendering)
    pub css_transform:        CssTransform,
    pub transform_origin_x:   f32,   // 0.0..1.0, default 0.5
    pub transform_origin_y:   f32,   // 0.0..1.0, default 0.5
    pub css_filter:           CssFilters,

    // Text underline offset
    pub text_underline_offset: CssLength,

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
