//! Data-driven CSS property definitions.
//!
//! Each CSS property is defined as a `PropertyDef` with:
//! - Identity (PropertyId, name)
//! - Metadata (inherited, shorthand longhands)
//! - Behavior (apply function, copy function)
//!
//! This replaces the giant match in apply_property_by_id with table lookup.
//! Adding a new property = one entry in `get()`.

use super::properties::PropertyId;
use crate::types::ComputedStyle;

/// Function that applies a CSS value string to a ComputedStyle field.
pub type ApplyFn = fn(style: &mut ComputedStyle, value: &str);

/// Function that copies a property from one ComputedStyle to another (for inheritance).
pub type CopyFn = fn(dst: &mut ComputedStyle, src: &ComputedStyle);

/// Definition of a single CSS property.
pub struct PropertyDef {
    pub id: PropertyId,
    pub name: &'static str,
    pub inherited: bool,
    pub apply: ApplyFn,
    pub copy: CopyFn,
    /// For shorthands: the longhand properties this expands to.
    pub longhands: &'static [PropertyId],
}

// ── No-op defaults ──────────────────────────────────────────────────────────

fn apply_noop(_: &mut ComputedStyle, _: &str) {}
fn copy_noop(_: &mut ComputedStyle, _: &ComputedStyle) {}

// ── Unknown default ─────────────────────────────────────────────────────────

static UNKNOWN_DEF: PropertyDef = PropertyDef {
    id: PropertyId::Unknown, name: "", inherited: false,
    apply: apply_noop, copy: copy_noop, longhands: &[],
};

// ── Table lookup ────────────────────────────────────────────────────────────

/// Get the property definition for a PropertyId.
/// Returns a static reference — zero allocation, zero-cost lookup.
/// The compiler optimizes the match to a jump table.
pub fn get(id: PropertyId) -> &'static PropertyDef {
    use PropertyId::*;
    match id {
        // ── Sizing ──
        Width     => &PropertyDef { id: Width,     name: "width",      inherited: false, apply: apply_width,      copy: copy_width,      longhands: &[] },
        Height    => &PropertyDef { id: Height,    name: "height",     inherited: false, apply: apply_height,     copy: copy_height,     longhands: &[] },
        MinWidth  => &PropertyDef { id: MinWidth,  name: "min-width",  inherited: false, apply: apply_min_width,  copy: copy_min_width,  longhands: &[] },
        MinHeight => &PropertyDef { id: MinHeight, name: "min-height", inherited: false, apply: apply_min_height, copy: copy_min_height, longhands: &[] },
        MaxWidth  => &PropertyDef { id: MaxWidth,  name: "max-width",  inherited: false, apply: apply_max_width,  copy: copy_max_width,  longhands: &[] },
        MaxHeight => &PropertyDef { id: MaxHeight, name: "max-height", inherited: false, apply: apply_max_height, copy: copy_max_height, longhands: &[] },

        // ── Margin ──
        MarginTop    => &PropertyDef { id: MarginTop,    name: "margin-top",    inherited: false, apply: apply_margin_top,    copy: copy_margin_top,    longhands: &[] },
        MarginRight  => &PropertyDef { id: MarginRight,  name: "margin-right",  inherited: false, apply: apply_margin_right,  copy: copy_margin_right,  longhands: &[] },
        MarginBottom => &PropertyDef { id: MarginBottom, name: "margin-bottom", inherited: false, apply: apply_margin_bottom, copy: copy_margin_bottom, longhands: &[] },
        MarginLeft   => &PropertyDef { id: MarginLeft,   name: "margin-left",   inherited: false, apply: apply_margin_left,   copy: copy_margin_left,   longhands: &[] },
        Margin       => &PropertyDef { id: Margin,       name: "margin",        inherited: false, apply: apply_margin,        copy: copy_noop,          longhands: &[MarginTop, MarginRight, MarginBottom, MarginLeft] },

        // ── Padding ──
        PaddingTop    => &PropertyDef { id: PaddingTop,    name: "padding-top",    inherited: false, apply: apply_padding_top,    copy: copy_padding_top,    longhands: &[] },
        PaddingRight  => &PropertyDef { id: PaddingRight,  name: "padding-right",  inherited: false, apply: apply_padding_right,  copy: copy_padding_right,  longhands: &[] },
        PaddingBottom => &PropertyDef { id: PaddingBottom, name: "padding-bottom", inherited: false, apply: apply_padding_bottom, copy: copy_padding_bottom, longhands: &[] },
        PaddingLeft   => &PropertyDef { id: PaddingLeft,   name: "padding-left",   inherited: false, apply: apply_padding_left,   copy: copy_padding_left,   longhands: &[] },
        Padding       => &PropertyDef { id: Padding,       name: "padding",        inherited: false, apply: apply_padding,        copy: copy_noop,           longhands: &[PaddingTop, PaddingRight, PaddingBottom, PaddingLeft] },

        // ── Color ──
        Color           => &PropertyDef { id: Color,           name: "color",            inherited: true,  apply: apply_color,            copy: copy_color,            longhands: &[] },
        BackgroundColor => &PropertyDef { id: BackgroundColor, name: "background-color",  inherited: false, apply: apply_background_color, copy: copy_background_color, longhands: &[] },
        Opacity         => &PropertyDef { id: Opacity,         name: "opacity",           inherited: false, apply: apply_opacity,          copy: copy_opacity,          longhands: &[] },

        // ── Font (inherited) ──
        FontSize   => &PropertyDef { id: FontSize,   name: "font-size",   inherited: true, apply: apply_font_size,   copy: copy_font_size,   longhands: &[] },
        FontFamily => &PropertyDef { id: FontFamily, name: "font-family", inherited: true, apply: apply_font_family, copy: copy_font_family, longhands: &[] },
        FontWeight => &PropertyDef { id: FontWeight, name: "font-weight", inherited: true, apply: apply_font_weight, copy: copy_font_weight, longhands: &[] },
        FontStyle  => &PropertyDef { id: FontStyle,  name: "font-style",  inherited: true, apply: apply_font_style,  copy: copy_font_style,  longhands: &[] },
        Font       => &PropertyDef { id: Font,       name: "font",        inherited: true, apply: apply_font,        copy: copy_noop,         longhands: &[FontFamily, FontSize, FontWeight, FontStyle, LineHeight] },
        FontVariationSettings => &PropertyDef { id: FontVariationSettings, name: "font-variation-settings", inherited: true, apply: apply_font_variation_settings, copy: copy_font_variation_settings, longhands: &[] },
        FontFeatureSettings   => &PropertyDef { id: FontFeatureSettings,   name: "font-feature-settings",  inherited: true, apply: apply_font_feature_settings,   copy: copy_font_feature_settings,   longhands: &[] },
        FontVariant => &PropertyDef { id: FontVariant, name: "font-variant", inherited: true, apply: apply_font_variant, copy: copy_font_variant, longhands: &[] },
        FontStretch => &PropertyDef { id: FontStretch, name: "font-stretch", inherited: true, apply: apply_font_stretch, copy: copy_font_stretch, longhands: &[] },
        LineHeight => &PropertyDef { id: LineHeight, name: "line-height", inherited: true, apply: apply_line_height, copy: copy_line_height, longhands: &[] },

        // ── Text (inherited) ──
        TextAlign      => &PropertyDef { id: TextAlign,      name: "text-align",      inherited: true,  apply: apply_text_align,      copy: copy_text_align,      longhands: &[] },
        TextTransform  => &PropertyDef { id: TextTransform,  name: "text-transform",  inherited: true,  apply: apply_text_transform,  copy: copy_text_transform,  longhands: &[] },
        TextIndent     => &PropertyDef { id: TextIndent,     name: "text-indent",     inherited: true,  apply: apply_text_indent,     copy: copy_text_indent,     longhands: &[] },
        LetterSpacing  => &PropertyDef { id: LetterSpacing,  name: "letter-spacing",  inherited: true,  apply: apply_letter_spacing,  copy: copy_letter_spacing,  longhands: &[] },
        WordSpacing    => &PropertyDef { id: WordSpacing,    name: "word-spacing",    inherited: true,  apply: apply_word_spacing,    copy: copy_word_spacing,    longhands: &[] },
        WhiteSpace     => &PropertyDef { id: WhiteSpace,     name: "white-space",     inherited: true,  apply: apply_white_space,     copy: copy_white_space,     longhands: &[] },
        Direction      => &PropertyDef { id: Direction,      name: "direction",       inherited: true,  apply: apply_direction,       copy: copy_direction,       longhands: &[] },
        Visibility     => &PropertyDef { id: Visibility,     name: "visibility",      inherited: true,  apply: apply_visibility,      copy: copy_visibility,      longhands: &[] },
        Cursor         => &PropertyDef { id: Cursor,         name: "cursor",          inherited: true,  apply: apply_cursor,          copy: copy_cursor,          longhands: &[] },

        // ── Display & Layout ──
        Display    => &PropertyDef { id: Display,    name: "display",    inherited: false, apply: apply_display,    copy: copy_display,    longhands: &[] },
        Position   => &PropertyDef { id: Position,   name: "position",   inherited: false, apply: apply_position,   copy: copy_position,   longhands: &[] },
        ZIndex     => &PropertyDef { id: ZIndex,     name: "z-index",    inherited: false, apply: apply_z_index,    copy: copy_z_index,    longhands: &[] },
        Float      => &PropertyDef { id: Float,      name: "float",      inherited: false, apply: apply_float,      copy: copy_float,      longhands: &[] },
        Clear      => &PropertyDef { id: Clear,      name: "clear",      inherited: false, apply: apply_clear,      copy: copy_clear,      longhands: &[] },
        BoxSizing  => &PropertyDef { id: BoxSizing,  name: "box-sizing", inherited: false, apply: apply_box_sizing, copy: copy_box_sizing, longhands: &[] },
        OverflowX  => &PropertyDef { id: OverflowX,  name: "overflow-x", inherited: false, apply: apply_overflow_x, copy: copy_overflow_x, longhands: &[] },
        OverflowY  => &PropertyDef { id: OverflowY,  name: "overflow-y", inherited: false, apply: apply_overflow_y, copy: copy_overflow_y, longhands: &[] },
        Overflow   => &PropertyDef { id: Overflow,   name: "overflow",   inherited: false, apply: apply_overflow,   copy: copy_noop,       longhands: &[OverflowX, OverflowY] },

        // ── Position offsets ──
        Top    => &PropertyDef { id: Top,    name: "top",    inherited: false, apply: apply_top,    copy: copy_top,    longhands: &[] },
        Right  => &PropertyDef { id: Right,  name: "right",  inherited: false, apply: apply_right,  copy: copy_right,  longhands: &[] },
        Bottom => &PropertyDef { id: Bottom, name: "bottom", inherited: false, apply: apply_bottom, copy: copy_bottom, longhands: &[] },
        Left   => &PropertyDef { id: Left,   name: "left",   inherited: false, apply: apply_left,   copy: copy_left,   longhands: &[] },

        // ── Border ──
        Border       => &PropertyDef { id: Border,       name: "border",        inherited: false, apply: apply_border,        copy: copy_noop,             longhands: &[BorderTopWidth, BorderRightWidth, BorderBottomWidth, BorderLeftWidth, BorderTopStyle, BorderRightStyle, BorderBottomStyle, BorderLeftStyle, BorderTopColor, BorderRightColor, BorderBottomColor, BorderLeftColor] },
        BorderWidth  => &PropertyDef { id: BorderWidth,  name: "border-width",  inherited: false, apply: apply_border_width,  copy: copy_noop,             longhands: &[BorderTopWidth, BorderRightWidth, BorderBottomWidth, BorderLeftWidth] },
        BorderStyle  => &PropertyDef { id: BorderStyle,  name: "border-style",  inherited: false, apply: apply_border_style_sh, copy: copy_noop,           longhands: &[BorderTopStyle, BorderRightStyle, BorderBottomStyle, BorderLeftStyle] },
        BorderColor  => &PropertyDef { id: BorderColor,  name: "border-color",  inherited: false, apply: apply_border_color_sh, copy: copy_noop,           longhands: &[BorderTopColor, BorderRightColor, BorderBottomColor, BorderLeftColor] },
        BorderTopWidth    => &PropertyDef { id: BorderTopWidth,    name: "border-top-width",    inherited: false, apply: apply_border_top_width,    copy: copy_border_top_width,    longhands: &[] },
        BorderRightWidth  => &PropertyDef { id: BorderRightWidth,  name: "border-right-width",  inherited: false, apply: apply_border_right_width,  copy: copy_border_right_width,  longhands: &[] },
        BorderBottomWidth => &PropertyDef { id: BorderBottomWidth, name: "border-bottom-width", inherited: false, apply: apply_border_bottom_width, copy: copy_border_bottom_width, longhands: &[] },
        BorderLeftWidth   => &PropertyDef { id: BorderLeftWidth,   name: "border-left-width",   inherited: false, apply: apply_border_left_width,   copy: copy_border_left_width,   longhands: &[] },
        BorderTopStyle    => &PropertyDef { id: BorderTopStyle,    name: "border-top-style",    inherited: false, apply: apply_border_top_style,    copy: copy_border_top_style,    longhands: &[] },
        BorderRightStyle  => &PropertyDef { id: BorderRightStyle,  name: "border-right-style",  inherited: false, apply: apply_border_right_style,  copy: copy_border_right_style,  longhands: &[] },
        BorderBottomStyle => &PropertyDef { id: BorderBottomStyle, name: "border-bottom-style", inherited: false, apply: apply_border_bottom_style, copy: copy_border_bottom_style, longhands: &[] },
        BorderLeftStyle   => &PropertyDef { id: BorderLeftStyle,   name: "border-left-style",   inherited: false, apply: apply_border_left_style,   copy: copy_border_left_style,   longhands: &[] },
        BorderTopColor    => &PropertyDef { id: BorderTopColor,    name: "border-top-color",    inherited: false, apply: apply_border_top_color,    copy: copy_border_top_color,    longhands: &[] },
        BorderRightColor  => &PropertyDef { id: BorderRightColor,  name: "border-right-color",  inherited: false, apply: apply_border_right_color,  copy: copy_border_right_color,  longhands: &[] },
        BorderBottomColor => &PropertyDef { id: BorderBottomColor, name: "border-bottom-color", inherited: false, apply: apply_border_bottom_color, copy: copy_border_bottom_color, longhands: &[] },
        BorderLeftColor   => &PropertyDef { id: BorderLeftColor,   name: "border-left-color",   inherited: false, apply: apply_border_left_color,   copy: copy_border_left_color,   longhands: &[] },
        BorderTop    => &PropertyDef { id: BorderTop,    name: "border-top",    inherited: false, apply: apply_border_top_sh,    copy: copy_noop, longhands: &[BorderTopWidth, BorderTopStyle, BorderTopColor] },
        BorderRight  => &PropertyDef { id: BorderRight,  name: "border-right",  inherited: false, apply: apply_border_right_sh,  copy: copy_noop, longhands: &[BorderRightWidth, BorderRightStyle, BorderRightColor] },
        BorderBottom => &PropertyDef { id: BorderBottom, name: "border-bottom", inherited: false, apply: apply_border_bottom_sh, copy: copy_noop, longhands: &[BorderBottomWidth, BorderBottomStyle, BorderBottomColor] },
        BorderLeft   => &PropertyDef { id: BorderLeft,   name: "border-left",   inherited: false, apply: apply_border_left_sh,   copy: copy_noop, longhands: &[BorderLeftWidth, BorderLeftStyle, BorderLeftColor] },

        // ── Border radius ──
        BorderRadius            => &PropertyDef { id: BorderRadius,            name: "border-radius",              inherited: false, apply: apply_border_radius,              copy: copy_noop, longhands: &[BorderTopLeftRadius, BorderTopRightRadius, BorderBottomRightRadius, BorderBottomLeftRadius] },
        BorderTopLeftRadius     => &PropertyDef { id: BorderTopLeftRadius,     name: "border-top-left-radius",     inherited: false, apply: apply_border_top_left_radius,     copy: copy_border_top_left_radius,     longhands: &[] },
        BorderTopRightRadius    => &PropertyDef { id: BorderTopRightRadius,    name: "border-top-right-radius",    inherited: false, apply: apply_border_top_right_radius,    copy: copy_border_top_right_radius,    longhands: &[] },
        BorderBottomLeftRadius  => &PropertyDef { id: BorderBottomLeftRadius,  name: "border-bottom-left-radius",  inherited: false, apply: apply_border_bottom_left_radius,  copy: copy_border_bottom_left_radius,  longhands: &[] },
        BorderBottomRightRadius => &PropertyDef { id: BorderBottomRightRadius, name: "border-bottom-right-radius", inherited: false, apply: apply_border_bottom_right_radius, copy: copy_border_bottom_right_radius, longhands: &[] },

        // ── Table ──
        BorderCollapse => &PropertyDef { id: BorderCollapse, name: "border-collapse", inherited: true,  apply: apply_border_collapse, copy: copy_border_collapse, longhands: &[] },
        BorderSpacing  => &PropertyDef { id: BorderSpacing,  name: "border-spacing",  inherited: true,  apply: apply_border_spacing,  copy: copy_border_spacing,  longhands: &[] },
        CaptionSide    => &PropertyDef { id: CaptionSide,    name: "caption-side",    inherited: true,  apply: apply_caption_side,    copy: copy_caption_side,    longhands: &[] },
        EmptyCells     => &PropertyDef { id: EmptyCells,     name: "empty-cells",     inherited: true,  apply: apply_empty_cells,     copy: copy_empty_cells,     longhands: &[] },
        TableLayout    => &PropertyDef { id: TableLayout,    name: "table-layout",    inherited: false, apply: apply_table_layout,    copy: copy_table_layout,    longhands: &[] },

        // ── Vertical align ──
        VerticalAlign => &PropertyDef { id: VerticalAlign, name: "vertical-align", inherited: false, apply: apply_vertical_align, copy: copy_vertical_align, longhands: &[] },

        // ── Text decoration ──
        TextDecoration          => &PropertyDef { id: TextDecoration,          name: "text-decoration",           inherited: false, apply: apply_text_decoration,           copy: copy_noop, longhands: &[TextDecorationLine, TextDecorationStyle, TextDecorationColor] },
        TextDecorationLine      => &PropertyDef { id: TextDecorationLine,      name: "text-decoration-line",      inherited: false, apply: apply_text_decoration_line,      copy: copy_text_decoration_line,      longhands: &[] },
        TextDecorationColor     => &PropertyDef { id: TextDecorationColor,     name: "text-decoration-color",     inherited: false, apply: apply_text_decoration_color,     copy: copy_text_decoration_color,     longhands: &[] },
        TextDecorationStyle     => &PropertyDef { id: TextDecorationStyle,     name: "text-decoration-style",     inherited: false, apply: apply_text_decoration_style_fn,  copy: copy_text_decoration_style,     longhands: &[] },
        TextDecorationThickness => &PropertyDef { id: TextDecorationThickness, name: "text-decoration-thickness", inherited: false, apply: apply_text_decoration_thickness, copy: copy_text_decoration_thickness, longhands: &[] },
        TextUnderlineOffset     => &PropertyDef { id: TextUnderlineOffset,     name: "text-underline-offset",     inherited: false, apply: apply_text_underline_offset,     copy: copy_text_underline_offset,     longhands: &[] },
        TextOverflow            => &PropertyDef { id: TextOverflow,            name: "text-overflow",             inherited: false, apply: apply_text_overflow,             copy: copy_text_overflow,             longhands: &[] },
        TextShadow              => &PropertyDef { id: TextShadow,              name: "text-shadow",               inherited: true,  apply: apply_text_shadow,               copy: copy_text_shadow,               longhands: &[] },

        // ── Word / overflow-wrap ──
        WordBreak    => &PropertyDef { id: WordBreak,    name: "word-break",    inherited: true, apply: apply_word_break,    copy: copy_word_break,    longhands: &[] },
        OverflowWrap => &PropertyDef { id: OverflowWrap, name: "overflow-wrap", inherited: true, apply: apply_overflow_wrap, copy: copy_overflow_wrap, longhands: &[] },
        WordWrap     => &PropertyDef { id: WordWrap,     name: "word-wrap",     inherited: true, apply: apply_overflow_wrap, copy: copy_overflow_wrap, longhands: &[] },

        // ── List style ──
        ListStyleType     => &PropertyDef { id: ListStyleType,     name: "list-style-type",     inherited: true, apply: apply_list_style_type,     copy: copy_list_style_type,     longhands: &[] },
        ListStylePosition => &PropertyDef { id: ListStylePosition, name: "list-style-position", inherited: true, apply: apply_list_style_position, copy: copy_list_style_position, longhands: &[] },
        ListStyleImage    => &PropertyDef { id: ListStyleImage,    name: "list-style-image",    inherited: true, apply: apply_list_style_image,    copy: copy_list_style_image,    longhands: &[] },
        ListStyle         => &PropertyDef { id: ListStyle,         name: "list-style",          inherited: true, apply: apply_list_style,          copy: copy_noop,                longhands: &[ListStyleType, ListStylePosition, ListStyleImage] },

        // ── Flexbox ──
        FlexDirection  => &PropertyDef { id: FlexDirection,  name: "flex-direction",  inherited: false, apply: apply_flex_direction,  copy: copy_flex_direction,  longhands: &[] },
        FlexWrap       => &PropertyDef { id: FlexWrap,       name: "flex-wrap",       inherited: false, apply: apply_flex_wrap,       copy: copy_flex_wrap,       longhands: &[] },
        FlexGrow       => &PropertyDef { id: FlexGrow,       name: "flex-grow",       inherited: false, apply: apply_flex_grow,       copy: copy_flex_grow,       longhands: &[] },
        FlexShrink     => &PropertyDef { id: FlexShrink,     name: "flex-shrink",     inherited: false, apply: apply_flex_shrink,     copy: copy_flex_shrink,     longhands: &[] },
        FlexBasis      => &PropertyDef { id: FlexBasis,      name: "flex-basis",      inherited: false, apply: apply_flex_basis,      copy: copy_flex_basis,      longhands: &[] },
        Flex           => &PropertyDef { id: Flex,           name: "flex",            inherited: false, apply: apply_flex,            copy: copy_noop,            longhands: &[FlexGrow, FlexShrink, FlexBasis] },
        FlexFlow       => &PropertyDef { id: FlexFlow,       name: "flex-flow",       inherited: false, apply: apply_flex_flow,       copy: copy_noop,            longhands: &[FlexDirection, FlexWrap] },
        Order          => &PropertyDef { id: Order,          name: "order",           inherited: false, apply: apply_order,           copy: copy_order,           longhands: &[] },
        JustifyContent => &PropertyDef { id: JustifyContent, name: "justify-content", inherited: false, apply: apply_justify_content, copy: copy_justify_content, longhands: &[] },
        AlignItems     => &PropertyDef { id: AlignItems,     name: "align-items",     inherited: false, apply: apply_align_items,     copy: copy_align_items,     longhands: &[] },
        AlignSelf      => &PropertyDef { id: AlignSelf,      name: "align-self",      inherited: false, apply: apply_align_self,      copy: copy_align_self,      longhands: &[] },
        AlignContent   => &PropertyDef { id: AlignContent,   name: "align-content",   inherited: false, apply: apply_align_content,   copy: copy_align_content,   longhands: &[] },
        JustifyItems   => &PropertyDef { id: JustifyItems,   name: "justify-items",   inherited: false, apply: apply_justify_items,   copy: copy_justify_items,   longhands: &[] },
        JustifySelf    => &PropertyDef { id: JustifySelf,    name: "justify-self",    inherited: false, apply: apply_justify_self,    copy: copy_justify_self,    longhands: &[] },
        Gap            => &PropertyDef { id: Gap,            name: "gap",             inherited: false, apply: apply_gap,             copy: copy_noop,            longhands: &[RowGap, ColumnGap] },
        RowGap         => &PropertyDef { id: RowGap,         name: "row-gap",         inherited: false, apply: apply_row_gap,         copy: copy_row_gap,         longhands: &[] },
        ColumnGap      => &PropertyDef { id: ColumnGap,      name: "column-gap",      inherited: false, apply: apply_column_gap,      copy: copy_column_gap,      longhands: &[] },

        // ── Grid ──
        GridTemplateColumns => &PropertyDef { id: GridTemplateColumns, name: "grid-template-columns", inherited: false, apply: apply_grid_template_columns, copy: copy_grid_template_columns, longhands: &[] },
        GridTemplateRows    => &PropertyDef { id: GridTemplateRows,    name: "grid-template-rows",    inherited: false, apply: apply_grid_template_rows,    copy: copy_grid_template_rows,    longhands: &[] },
        GridTemplateAreas   => &PropertyDef { id: GridTemplateAreas,   name: "grid-template-areas",   inherited: false, apply: apply_grid_template_areas,   copy: copy_grid_template_areas,   longhands: &[] },
        GridAutoColumns     => &PropertyDef { id: GridAutoColumns,     name: "grid-auto-columns",     inherited: false, apply: apply_grid_auto_columns,     copy: copy_grid_auto_columns,     longhands: &[] },
        GridAutoRows        => &PropertyDef { id: GridAutoRows,        name: "grid-auto-rows",        inherited: false, apply: apply_grid_auto_rows,        copy: copy_grid_auto_rows,        longhands: &[] },
        GridAutoFlow        => &PropertyDef { id: GridAutoFlow,        name: "grid-auto-flow",        inherited: false, apply: apply_grid_auto_flow,        copy: copy_grid_auto_flow,        longhands: &[] },
        GridColumn          => &PropertyDef { id: GridColumn,          name: "grid-column",           inherited: false, apply: apply_grid_column,           copy: copy_noop,                  longhands: &[GridColumnStart, GridColumnEnd] },
        GridRow             => &PropertyDef { id: GridRow,             name: "grid-row",              inherited: false, apply: apply_grid_row,              copy: copy_noop,                  longhands: &[GridRowStart, GridRowEnd] },
        GridColumnStart     => &PropertyDef { id: GridColumnStart,     name: "grid-column-start",     inherited: false, apply: apply_grid_column_start,     copy: copy_grid_column_start,     longhands: &[] },
        GridColumnEnd       => &PropertyDef { id: GridColumnEnd,       name: "grid-column-end",       inherited: false, apply: apply_grid_column_end,       copy: copy_grid_column_end,       longhands: &[] },
        GridRowStart        => &PropertyDef { id: GridRowStart,        name: "grid-row-start",        inherited: false, apply: apply_grid_row_start,        copy: copy_grid_row_start,        longhands: &[] },
        GridRowEnd          => &PropertyDef { id: GridRowEnd,          name: "grid-row-end",          inherited: false, apply: apply_grid_row_end,          copy: copy_grid_row_end,          longhands: &[] },
        GridArea            => &PropertyDef { id: GridArea,            name: "grid-area",             inherited: false, apply: apply_grid_area,             copy: copy_noop,                  longhands: &[GridRowStart, GridColumnStart, GridRowEnd, GridColumnEnd] },
        GridTemplate        => &PropertyDef { id: GridTemplate,        name: "grid-template",         inherited: false, apply: apply_grid_template,         copy: copy_noop,                  longhands: &[GridTemplateRows, GridTemplateColumns, GridTemplateAreas] },

        // ── Background ──
        Background           => &PropertyDef { id: Background,           name: "background",             inherited: false, apply: apply_background,             copy: copy_noop, longhands: &[BackgroundColor, BackgroundImage, BackgroundPosition, BackgroundSize, BackgroundRepeat, BackgroundAttachment, BackgroundOrigin, BackgroundClip] },
        BackgroundImage      => &PropertyDef { id: BackgroundImage,      name: "background-image",       inherited: false, apply: apply_background_image,      copy: copy_background_image,      longhands: &[] },
        BackgroundSize       => &PropertyDef { id: BackgroundSize,       name: "background-size",        inherited: false, apply: apply_background_size,       copy: copy_background_size,       longhands: &[] },
        BackgroundPosition   => &PropertyDef { id: BackgroundPosition,   name: "background-position",    inherited: false, apply: apply_background_position,   copy: copy_background_position,   longhands: &[] },
        BackgroundRepeat     => &PropertyDef { id: BackgroundRepeat,     name: "background-repeat",      inherited: false, apply: apply_background_repeat,     copy: copy_background_repeat,     longhands: &[] },
        BackgroundClip       => &PropertyDef { id: BackgroundClip,       name: "background-clip",        inherited: false, apply: apply_background_clip,       copy: copy_background_clip,       longhands: &[] },
        BackgroundOrigin     => &PropertyDef { id: BackgroundOrigin,     name: "background-origin",      inherited: false, apply: apply_background_origin,     copy: copy_background_origin,     longhands: &[] },
        BackgroundAttachment => &PropertyDef { id: BackgroundAttachment, name: "background-attachment",   inherited: false, apply: apply_background_attachment, copy: copy_background_attachment, longhands: &[] },

        // ── Mask ──
        MaskImage => &PropertyDef { id: MaskImage, name: "mask-image", inherited: false, apply: apply_mask_image, copy: copy_mask_image, longhands: &[] },

        // ── Outline ──
        Outline       => &PropertyDef { id: Outline,       name: "outline",        inherited: false, apply: apply_outline,        copy: copy_noop, longhands: &[OutlineWidth, OutlineStyle, OutlineColor] },
        OutlineStyle  => &PropertyDef { id: OutlineStyle,  name: "outline-style",  inherited: false, apply: apply_outline_style,  copy: copy_outline_style,  longhands: &[] },
        OutlineColor  => &PropertyDef { id: OutlineColor,  name: "outline-color",  inherited: false, apply: apply_outline_color,  copy: copy_outline_color,  longhands: &[] },
        OutlineWidth  => &PropertyDef { id: OutlineWidth,  name: "outline-width",  inherited: false, apply: apply_outline_width,  copy: copy_outline_width,  longhands: &[] },
        OutlineOffset => &PropertyDef { id: OutlineOffset, name: "outline-offset", inherited: false, apply: apply_outline_offset, copy: copy_outline_offset, longhands: &[] },

        // ── Box shadow ──
        BoxShadow => &PropertyDef { id: BoxShadow, name: "box-shadow", inherited: false, apply: apply_box_shadow, copy: copy_box_shadow, longhands: &[] },

        // ── Pointer events ──
        PointerEvents => &PropertyDef { id: PointerEvents, name: "pointer-events", inherited: true, apply: apply_pointer_events, copy: copy_pointer_events, longhands: &[] },

        // ── User interaction ──
        UserSelect => &PropertyDef { id: UserSelect, name: "user-select", inherited: false, apply: apply_user_select, copy: copy_user_select, longhands: &[] },
        Resize     => &PropertyDef { id: Resize,     name: "resize",      inherited: false, apply: apply_resize,      copy: copy_resize,      longhands: &[] },

        // ── Object fit/position ──
        ObjectFit      => &PropertyDef { id: ObjectFit,      name: "object-fit",      inherited: false, apply: apply_object_fit,      copy: copy_object_fit,      longhands: &[] },
        ObjectPosition => &PropertyDef { id: ObjectPosition, name: "object-position", inherited: false, apply: apply_object_position, copy: copy_object_position, longhands: &[] },
        AspectRatio    => &PropertyDef { id: AspectRatio,    name: "aspect-ratio",    inherited: false, apply: apply_aspect_ratio,    copy: copy_aspect_ratio,    longhands: &[] },

        // ── Transform / filter ──
        Transform       => &PropertyDef { id: Transform,       name: "transform",        inherited: false, apply: apply_transform,        copy: copy_transform,        longhands: &[] },
        TransformOrigin => &PropertyDef { id: TransformOrigin, name: "transform-origin", inherited: false, apply: apply_transform_origin, copy: copy_transform_origin, longhands: &[] },
        Filter          => &PropertyDef { id: Filter,          name: "filter",           inherited: false, apply: apply_filter,           copy: copy_filter,           longhands: &[] },
        BackdropFilter  => &PropertyDef { id: BackdropFilter,  name: "backdrop-filter",  inherited: false, apply: apply_backdrop_filter,  copy: copy_backdrop_filter,  longhands: &[] },
        // Accepted but not implemented
        TransformStyle       => &PropertyDef { id: TransformStyle,       name: "transform-style",       inherited: false, apply: apply_noop, copy: copy_noop, longhands: &[] },
        Perspective          => &PropertyDef { id: Perspective,          name: "perspective",            inherited: false, apply: apply_noop, copy: copy_noop, longhands: &[] },
        PerspectiveOrigin    => &PropertyDef { id: PerspectiveOrigin,    name: "perspective-origin",     inherited: false, apply: apply_noop, copy: copy_noop, longhands: &[] },
        BackfaceVisibility   => &PropertyDef { id: BackfaceVisibility,   name: "backface-visibility",    inherited: false, apply: apply_noop, copy: copy_noop, longhands: &[] },

        // ── Transition / animation ──
        Transition                 => &PropertyDef { id: Transition,                 name: "transition",                  inherited: false, apply: apply_transition,                  copy: copy_transition,                  longhands: &[] },
        TransitionProperty         => &PropertyDef { id: TransitionProperty,         name: "transition-property",         inherited: false, apply: apply_transition_property,         copy: copy_transition,                  longhands: &[] },
        TransitionDuration         => &PropertyDef { id: TransitionDuration,         name: "transition-duration",         inherited: false, apply: apply_transition_duration,         copy: copy_transition,                  longhands: &[] },
        TransitionTimingFunction   => &PropertyDef { id: TransitionTimingFunction,   name: "transition-timing-function",  inherited: false, apply: apply_transition_timing_function,  copy: copy_transition,                  longhands: &[] },
        TransitionDelay            => &PropertyDef { id: TransitionDelay,            name: "transition-delay",            inherited: false, apply: apply_transition_delay,            copy: copy_transition,                  longhands: &[] },
        Animation                  => &PropertyDef { id: Animation,                  name: "animation",                   inherited: false, apply: apply_animation,                   copy: copy_animation,                   longhands: &[] },
        AnimationName              => &PropertyDef { id: AnimationName,              name: "animation-name",              inherited: false, apply: apply_animation_name,              copy: copy_animation,                   longhands: &[] },
        AnimationDuration          => &PropertyDef { id: AnimationDuration,          name: "animation-duration",          inherited: false, apply: apply_animation_duration,          copy: copy_animation,                   longhands: &[] },
        AnimationTimingFunction    => &PropertyDef { id: AnimationTimingFunction,    name: "animation-timing-function",   inherited: false, apply: apply_animation_timing_function,   copy: copy_animation,                   longhands: &[] },
        AnimationDelay             => &PropertyDef { id: AnimationDelay,             name: "animation-delay",             inherited: false, apply: apply_animation_delay,             copy: copy_animation,                   longhands: &[] },
        AnimationIterationCount    => &PropertyDef { id: AnimationIterationCount,    name: "animation-iteration-count",   inherited: false, apply: apply_animation_iteration_count,   copy: copy_animation,                   longhands: &[] },
        AnimationDirection         => &PropertyDef { id: AnimationDirection,         name: "animation-direction",         inherited: false, apply: apply_animation_direction,         copy: copy_animation,                   longhands: &[] },
        AnimationFillMode          => &PropertyDef { id: AnimationFillMode,          name: "animation-fill-mode",         inherited: false, apply: apply_animation_fill_mode,         copy: copy_animation,                   longhands: &[] },
        AnimationPlayState         => &PropertyDef { id: AnimationPlayState,         name: "animation-play-state",        inherited: false, apply: apply_animation_play_state,        copy: copy_animation,                   longhands: &[] },
        WillChange                 => &PropertyDef { id: WillChange,                 name: "will-change",                 inherited: false, apply: apply_will_change,                 copy: copy_will_change,                 longhands: &[] },

        // ── Unicode-bidi & writing ──
        UnicodeBidi => &PropertyDef { id: UnicodeBidi, name: "unicode-bidi", inherited: false, apply: apply_unicode_bidi, copy: copy_unicode_bidi, longhands: &[] },
        WritingMode => &PropertyDef { id: WritingMode, name: "writing-mode", inherited: true,  apply: apply_writing_mode, copy: copy_writing_mode, longhands: &[] },

        // ── Hyphens / tab-size / text extras ──
        TabSize => &PropertyDef { id: TabSize, name: "tab-size", inherited: true, apply: apply_tab_size, copy: copy_tab_size, longhands: &[] },
        Hyphens => &PropertyDef { id: Hyphens, name: "hyphens",  inherited: true, apply: apply_hyphens,  copy: copy_hyphens,  longhands: &[] },
        Widows  => &PropertyDef { id: Widows,  name: "widows",   inherited: true, apply: apply_widows,   copy: copy_widows,   longhands: &[] },
        Orphans => &PropertyDef { id: Orphans, name: "orphans",  inherited: true, apply: apply_orphans,  copy: copy_orphans,  longhands: &[] },

        // ── Scrollbar & caret ──
        ScrollbarColor => &PropertyDef { id: ScrollbarColor, name: "scrollbar-color", inherited: false, apply: apply_scrollbar_color, copy: copy_scrollbar_color, longhands: &[] },
        CaretColor     => &PropertyDef { id: CaretColor,     name: "caret-color",     inherited: true,  apply: apply_caret_color,     copy: copy_caret_color,     longhands: &[] },

        // ── Quotes ──
        Quotes => &PropertyDef { id: Quotes, name: "quotes", inherited: true, apply: apply_quotes, copy: copy_quotes, longhands: &[] },

        // ── Container queries ──
        ContainerType => &PropertyDef { id: ContainerType, name: "container-type", inherited: false, apply: apply_container_type, copy: copy_container_type, longhands: &[] },
        ContainerName => &PropertyDef { id: ContainerName, name: "container-name", inherited: false, apply: apply_container_name, copy: copy_container_name, longhands: &[] },
        Container     => &PropertyDef { id: Container,     name: "container",      inherited: false, apply: apply_container,      copy: copy_noop,           longhands: &[ContainerType, ContainerName] },

        // ── Clip ──
        Clip     => &PropertyDef { id: Clip,     name: "clip",      inherited: false, apply: apply_clip,      copy: copy_clip,      longhands: &[] },
        ClipPath => &PropertyDef { id: ClipPath, name: "clip-path", inherited: false, apply: apply_clip_path, copy: copy_clip_path, longhands: &[] },

        // ── Break / page-break ──
        BreakBefore     => &PropertyDef { id: BreakBefore,     name: "break-before",      inherited: false, apply: apply_break_before,  copy: copy_break_before,  longhands: &[] },
        BreakAfter      => &PropertyDef { id: BreakAfter,      name: "break-after",       inherited: false, apply: apply_break_after,   copy: copy_break_after,   longhands: &[] },
        BreakInside     => &PropertyDef { id: BreakInside,     name: "break-inside",      inherited: false, apply: apply_break_inside,  copy: copy_break_inside,  longhands: &[] },
        PageBreakBefore => &PropertyDef { id: PageBreakBefore, name: "page-break-before", inherited: false, apply: apply_break_before,  copy: copy_break_before,  longhands: &[] },
        PageBreakAfter  => &PropertyDef { id: PageBreakAfter,  name: "page-break-after",  inherited: false, apply: apply_break_after,   copy: copy_break_after,   longhands: &[] },
        PageBreakInside => &PropertyDef { id: PageBreakInside, name: "page-break-inside", inherited: false, apply: apply_break_inside,  copy: copy_break_inside,  longhands: &[] },

        // ── Multi-column ──
        ColumnCount     => &PropertyDef { id: ColumnCount,     name: "column-count",      inherited: false, apply: apply_column_count,      copy: copy_column_count,      longhands: &[] },
        ColumnWidth     => &PropertyDef { id: ColumnWidth,     name: "column-width",      inherited: false, apply: apply_column_width,      copy: copy_column_width,      longhands: &[] },
        Columns         => &PropertyDef { id: Columns,         name: "columns",           inherited: false, apply: apply_columns,           copy: copy_noop,              longhands: &[ColumnCount, ColumnWidth] },
        ColumnRule       => &PropertyDef { id: ColumnRule,       name: "column-rule",       inherited: false, apply: apply_column_rule,       copy: copy_noop,              longhands: &[ColumnRuleWidth, ColumnRuleStyle, ColumnRuleColor] },
        ColumnRuleWidth => &PropertyDef { id: ColumnRuleWidth, name: "column-rule-width", inherited: false, apply: apply_column_rule_width, copy: copy_column_rule_width, longhands: &[] },
        ColumnRuleStyle => &PropertyDef { id: ColumnRuleStyle, name: "column-rule-style", inherited: false, apply: apply_column_rule_style, copy: copy_column_rule_style, longhands: &[] },
        ColumnRuleColor => &PropertyDef { id: ColumnRuleColor, name: "column-rule-color", inherited: false, apply: apply_column_rule_color, copy: copy_column_rule_color, longhands: &[] },
        ColumnFill      => &PropertyDef { id: ColumnFill,      name: "column-fill",       inherited: false, apply: apply_column_fill,      copy: copy_column_fill,      longhands: &[] },
        ColumnSpan      => &PropertyDef { id: ColumnSpan,      name: "column-span",       inherited: false, apply: apply_column_span,      copy: copy_column_span,      longhands: &[] },

        // ── Counter ──
        CounterReset     => &PropertyDef { id: CounterReset,     name: "counter-reset",     inherited: false, apply: apply_counter_reset,     copy: copy_counter_reset,     longhands: &[] },
        CounterIncrement => &PropertyDef { id: CounterIncrement, name: "counter-increment", inherited: false, apply: apply_counter_increment, copy: copy_counter_increment, longhands: &[] },
        CounterSet       => &PropertyDef { id: CounterSet,       name: "counter-set",       inherited: false, apply: apply_counter_set,       copy: copy_counter_reset,     longhands: &[] },

        // ── Misc ──
        ScrollBehavior       => &PropertyDef { id: ScrollBehavior,       name: "scroll-behavior",        inherited: false, apply: apply_scroll_behavior,        copy: copy_scroll_behavior,        longhands: &[] },
        OverscrollBehavior   => &PropertyDef { id: OverscrollBehavior,   name: "overscroll-behavior",    inherited: false, apply: apply_overscroll_behavior,    copy: copy_noop,                   longhands: &[OverscrollBehaviorX, OverscrollBehaviorY] },
        OverscrollBehaviorX  => &PropertyDef { id: OverscrollBehaviorX,  name: "overscroll-behavior-x",  inherited: false, apply: apply_overscroll_behavior_x,  copy: copy_overscroll_behavior_x,  longhands: &[] },
        OverscrollBehaviorY  => &PropertyDef { id: OverscrollBehaviorY,  name: "overscroll-behavior-y",  inherited: false, apply: apply_overscroll_behavior_y,  copy: copy_overscroll_behavior_y,  longhands: &[] },
        Isolation            => &PropertyDef { id: Isolation,            name: "isolation",              inherited: false, apply: apply_isolation,              copy: copy_isolation,              longhands: &[] },
        MixBlendMode         => &PropertyDef { id: MixBlendMode,         name: "mix-blend-mode",         inherited: false, apply: apply_mix_blend_mode,         copy: copy_mix_blend_mode,         longhands: &[] },

        // ── Containment ──
        Contain           => &PropertyDef { id: Contain,           name: "contain",            inherited: false, apply: apply_contain,  copy: copy_contain,  longhands: &[] },
        ContentVisibility => &PropertyDef { id: ContentVisibility, name: "content-visibility", inherited: false, apply: apply_noop,     copy: copy_noop,     longhands: &[] },

        // ── Scroll snap ──
        ScrollSnapType   => &PropertyDef { id: ScrollSnapType,   name: "scroll-snap-type",  inherited: false, apply: apply_scroll_snap_type,  copy: copy_scroll_snap_type,  longhands: &[] },
        ScrollSnapAlign  => &PropertyDef { id: ScrollSnapAlign,  name: "scroll-snap-align", inherited: false, apply: apply_scroll_snap_align, copy: copy_scroll_snap_align, longhands: &[] },
        ScrollPadding       => &PropertyDef { id: ScrollPadding,       name: "scroll-padding",        inherited: false, apply: apply_scroll_padding,        copy: copy_noop, longhands: &[ScrollPaddingTop, ScrollPaddingRight, ScrollPaddingBottom, ScrollPaddingLeft] },
        ScrollPaddingTop    => &PropertyDef { id: ScrollPaddingTop,    name: "scroll-padding-top",    inherited: false, apply: apply_scroll_padding_top,    copy: copy_scroll_padding_top,    longhands: &[] },
        ScrollPaddingRight  => &PropertyDef { id: ScrollPaddingRight,  name: "scroll-padding-right",  inherited: false, apply: apply_scroll_padding_right,  copy: copy_scroll_padding_right,  longhands: &[] },
        ScrollPaddingBottom => &PropertyDef { id: ScrollPaddingBottom, name: "scroll-padding-bottom", inherited: false, apply: apply_scroll_padding_bottom, copy: copy_scroll_padding_bottom, longhands: &[] },
        ScrollPaddingLeft   => &PropertyDef { id: ScrollPaddingLeft,   name: "scroll-padding-left",   inherited: false, apply: apply_scroll_padding_left,   copy: copy_scroll_padding_left,   longhands: &[] },
        // Accepted but not implemented
        ScrollSnapStop | ScrollMargin | ScrollMarginTop | ScrollMarginRight | ScrollMarginBottom | ScrollMarginLeft
            => &UNKNOWN_DEF,

        // ── Logical properties ──
        MarginBlock        => &PropertyDef { id: MarginBlock,        name: "margin-block",         inherited: false, apply: apply_margin_block,         copy: copy_noop, longhands: &[MarginTop, MarginBottom] },
        MarginBlockStart   => &PropertyDef { id: MarginBlockStart,   name: "margin-block-start",   inherited: false, apply: apply_margin_block_start,   copy: copy_margin_top,    longhands: &[] },
        MarginBlockEnd     => &PropertyDef { id: MarginBlockEnd,     name: "margin-block-end",     inherited: false, apply: apply_margin_block_end,     copy: copy_margin_bottom, longhands: &[] },
        MarginInline       => &PropertyDef { id: MarginInline,       name: "margin-inline",        inherited: false, apply: apply_margin_inline,        copy: copy_noop, longhands: &[MarginLeft, MarginRight] },
        MarginInlineStart  => &PropertyDef { id: MarginInlineStart,  name: "margin-inline-start",  inherited: false, apply: apply_margin_inline_start,  copy: copy_margin_left,   longhands: &[] },
        MarginInlineEnd    => &PropertyDef { id: MarginInlineEnd,    name: "margin-inline-end",    inherited: false, apply: apply_margin_inline_end,    copy: copy_margin_right,  longhands: &[] },
        PaddingBlock       => &PropertyDef { id: PaddingBlock,       name: "padding-block",        inherited: false, apply: apply_padding_block,        copy: copy_noop, longhands: &[PaddingTop, PaddingBottom] },
        PaddingBlockStart  => &PropertyDef { id: PaddingBlockStart,  name: "padding-block-start",  inherited: false, apply: apply_padding_block_start,  copy: copy_padding_top,    longhands: &[] },
        PaddingBlockEnd    => &PropertyDef { id: PaddingBlockEnd,    name: "padding-block-end",    inherited: false, apply: apply_padding_block_end,    copy: copy_padding_bottom, longhands: &[] },
        PaddingInline      => &PropertyDef { id: PaddingInline,      name: "padding-inline",       inherited: false, apply: apply_padding_inline,       copy: copy_noop, longhands: &[PaddingLeft, PaddingRight] },
        PaddingInlineStart => &PropertyDef { id: PaddingInlineStart, name: "padding-inline-start", inherited: false, apply: apply_padding_inline_start, copy: copy_padding_left,  longhands: &[] },
        PaddingInlineEnd   => &PropertyDef { id: PaddingInlineEnd,   name: "padding-inline-end",   inherited: false, apply: apply_padding_inline_end,   copy: copy_padding_right, longhands: &[] },
        InsetBlockStart    => &PropertyDef { id: InsetBlockStart,    name: "inset-block-start",    inherited: false, apply: apply_inset_block_start,    copy: copy_top,    longhands: &[] },
        InsetBlockEnd      => &PropertyDef { id: InsetBlockEnd,      name: "inset-block-end",      inherited: false, apply: apply_inset_block_end,      copy: copy_bottom, longhands: &[] },
        InsetInlineStart   => &PropertyDef { id: InsetInlineStart,   name: "inset-inline-start",   inherited: false, apply: apply_inset_inline_start,   copy: copy_left,   longhands: &[] },
        InsetInlineEnd     => &PropertyDef { id: InsetInlineEnd,     name: "inset-inline-end",     inherited: false, apply: apply_inset_inline_end,     copy: copy_right,  longhands: &[] },
        Inset              => &PropertyDef { id: Inset,              name: "inset",                inherited: false, apply: apply_inset,                copy: copy_noop,   longhands: &[Top, Right, Bottom, Left] },
        InsetBlock         => &PropertyDef { id: InsetBlock,         name: "inset-block",          inherited: false, apply: apply_inset_block,          copy: copy_noop,   longhands: &[Top, Bottom] },
        InsetInline        => &PropertyDef { id: InsetInline,        name: "inset-inline",         inherited: false, apply: apply_inset_inline,         copy: copy_noop,   longhands: &[Left, Right] },

        // ── Place shorthands ──
        PlaceSelf    => &PropertyDef { id: PlaceSelf,    name: "place-self",    inherited: false, apply: apply_place_self,    copy: copy_noop, longhands: &[AlignSelf, JustifySelf] },
        PlaceItems   => &PropertyDef { id: PlaceItems,   name: "place-items",   inherited: false, apply: apply_place_items,   copy: copy_noop, longhands: &[AlignItems, JustifyItems] },
        PlaceContent => &PropertyDef { id: PlaceContent, name: "place-content", inherited: false, apply: apply_place_content, copy: copy_noop, longhands: &[AlignContent, JustifyContent] },

        // ── Appearance / color-scheme / accent-color ──
        Appearance         => &PropertyDef { id: Appearance,         name: "appearance",           inherited: false, apply: apply_noop,         copy: copy_noop,              longhands: &[] },
        ColorScheme        => &PropertyDef { id: ColorScheme,        name: "color-scheme",         inherited: true,  apply: apply_noop,         copy: copy_noop,              longhands: &[] },
        ForcedColorAdjust  => &PropertyDef { id: ForcedColorAdjust,  name: "forced-color-adjust",  inherited: true,  apply: apply_noop,         copy: copy_noop,              longhands: &[] },
        ColorInterpolation => &PropertyDef { id: ColorInterpolation, name: "color-interpolation",  inherited: true,  apply: apply_noop,         copy: copy_noop,              longhands: &[] },
        AccentColor        => &PropertyDef { id: AccentColor,        name: "accent-color",         inherited: true,  apply: apply_accent_color, copy: copy_background_color,  longhands: &[] },

        // ── Image rendering ──
        ImageRendering    => &PropertyDef { id: ImageRendering,    name: "image-rendering",    inherited: true,  apply: apply_noop, copy: copy_noop, longhands: &[] },
        ImageOrientation  => &PropertyDef { id: ImageOrientation,  name: "image-orientation",  inherited: true,  apply: apply_noop, copy: copy_noop, longhands: &[] },

        // ── Touch / interaction ──
        TouchAction => &PropertyDef { id: TouchAction, name: "touch-action", inherited: false, apply: apply_noop, copy: copy_noop, longhands: &[] },

        // ── Logical sizing ──
        InlineSize => &PropertyDef { id: InlineSize, name: "inline-size", inherited: false, apply: apply_width,  copy: copy_width,  longhands: &[] },
        BlockSize  => &PropertyDef { id: BlockSize,  name: "block-size",  inherited: false, apply: apply_height, copy: copy_height, longhands: &[] },

        // ── Content (generated) ──
        Content => &PropertyDef { id: Content, name: "content", inherited: false, apply: apply_noop, copy: copy_noop, longhands: &[] },

        // ── Individual transform properties (CSS Transforms Level 2) ──
        Rotate    => &PropertyDef { id: Rotate,    name: "rotate",    inherited: false, apply: apply_noop, copy: copy_noop, longhands: &[] },
        Scale     => &PropertyDef { id: Scale,     name: "scale",     inherited: false, apply: apply_noop, copy: copy_noop, longhands: &[] },
        Translate => &PropertyDef { id: Translate,  name: "translate",  inherited: false, apply: apply_noop, copy: copy_noop, longhands: &[] },

        // ── Default ──
        _ => &UNKNOWN_DEF,
    }
}

/// Collect all inherited property IDs for use in inherit_from.
pub const INHERITED_IDS: &[PropertyId] = &[
    PropertyId::Color, PropertyId::FontSize, PropertyId::FontFamily,
    PropertyId::FontWeight, PropertyId::FontStyle, PropertyId::LineHeight,
    PropertyId::TextAlign, PropertyId::TextTransform, PropertyId::TextIndent,
    PropertyId::LetterSpacing, PropertyId::WordSpacing, PropertyId::WhiteSpace,
    PropertyId::Direction, PropertyId::Visibility, PropertyId::Cursor,
    PropertyId::ListStyleType, PropertyId::ListStylePosition,
    PropertyId::BorderCollapse, PropertyId::BorderSpacing,
    PropertyId::CaptionSide, PropertyId::EmptyCells,
    PropertyId::WordBreak, PropertyId::OverflowWrap,
    PropertyId::WritingMode, PropertyId::TabSize, PropertyId::Hyphens,
    PropertyId::Orphans, PropertyId::Widows,
    PropertyId::PointerEvents, PropertyId::CaretColor,
    PropertyId::Quotes, PropertyId::TextShadow,
    PropertyId::FontVariationSettings, PropertyId::FontFeatureSettings,
    PropertyId::FontVariant, PropertyId::FontStretch,
];

// ═══════════════════════════════════════════════════════════════════════════════
// Apply functions — one per property, extracted from the giant match.
// Each takes `(style, value_str)` and sets the appropriate field.
// ═══════════════════════════════════════════════════════════════════════════════

use crate::types::*;
use super::{parse_length, parse_length_or_none, parse_font_size, parse_color};

// ── Sizing ──────────────────────────────────────────────────────────────────

fn apply_width(s: &mut ComputedStyle, v: &str)      { s.width = parse_length(v); }
fn apply_height(s: &mut ComputedStyle, v: &str)     { s.height = parse_length(v); }
fn apply_min_width(s: &mut ComputedStyle, v: &str)  { s.min_width = parse_length(v); }
fn apply_min_height(s: &mut ComputedStyle, v: &str) { s.min_height = parse_length(v); }
fn apply_max_width(s: &mut ComputedStyle, v: &str)  { s.max_width = parse_length_or_none(v); }
fn apply_max_height(s: &mut ComputedStyle, v: &str) { s.max_height = parse_length_or_none(v); }

fn copy_width(d: &mut ComputedStyle, s: &ComputedStyle)      { d.width = s.width.clone(); }
fn copy_height(d: &mut ComputedStyle, s: &ComputedStyle)     { d.height = s.height.clone(); }
fn copy_min_width(d: &mut ComputedStyle, s: &ComputedStyle)  { d.min_width = s.min_width.clone(); }
fn copy_min_height(d: &mut ComputedStyle, s: &ComputedStyle) { d.min_height = s.min_height.clone(); }
fn copy_max_width(d: &mut ComputedStyle, s: &ComputedStyle)  { d.max_width = s.max_width.clone(); }
fn copy_max_height(d: &mut ComputedStyle, s: &ComputedStyle) { d.max_height = s.max_height.clone(); }

// ── Margin ──────────────────────────────────────────────────────────────────

fn apply_margin_top(s: &mut ComputedStyle, v: &str)    { s.margin_top = parse_length(v); }
fn apply_margin_right(s: &mut ComputedStyle, v: &str)  { s.margin_right = parse_length(v); }
fn apply_margin_bottom(s: &mut ComputedStyle, v: &str) { s.margin_bottom = parse_length(v); }
fn apply_margin_left(s: &mut ComputedStyle, v: &str)   { s.margin_left = parse_length(v); }

fn apply_margin(s: &mut ComputedStyle, v: &str) {
    super::apply_shorthand_4(v,
        &mut s.margin_top, &mut s.margin_right,
        &mut s.margin_bottom, &mut s.margin_left,
        parse_length);
}

fn copy_margin_top(d: &mut ComputedStyle, s: &ComputedStyle)    { d.margin_top = s.margin_top.clone(); }
fn copy_margin_right(d: &mut ComputedStyle, s: &ComputedStyle)  { d.margin_right = s.margin_right.clone(); }
fn copy_margin_bottom(d: &mut ComputedStyle, s: &ComputedStyle) { d.margin_bottom = s.margin_bottom.clone(); }
fn copy_margin_left(d: &mut ComputedStyle, s: &ComputedStyle)   { d.margin_left = s.margin_left.clone(); }

// ── Padding ─────────────────────────────────────────────────────────────────

fn apply_padding_top(s: &mut ComputedStyle, v: &str)    { s.padding_top = parse_length(v); }
fn apply_padding_right(s: &mut ComputedStyle, v: &str)  { s.padding_right = parse_length(v); }
fn apply_padding_bottom(s: &mut ComputedStyle, v: &str) { s.padding_bottom = parse_length(v); }
fn apply_padding_left(s: &mut ComputedStyle, v: &str)   { s.padding_left = parse_length(v); }

fn apply_padding(s: &mut ComputedStyle, v: &str) {
    super::apply_shorthand_4(v,
        &mut s.padding_top, &mut s.padding_right,
        &mut s.padding_bottom, &mut s.padding_left,
        parse_length);
}

fn copy_padding_top(d: &mut ComputedStyle, s: &ComputedStyle)    { d.padding_top = s.padding_top.clone(); }
fn copy_padding_right(d: &mut ComputedStyle, s: &ComputedStyle)  { d.padding_right = s.padding_right.clone(); }
fn copy_padding_bottom(d: &mut ComputedStyle, s: &ComputedStyle) { d.padding_bottom = s.padding_bottom.clone(); }
fn copy_padding_left(d: &mut ComputedStyle, s: &ComputedStyle)   { d.padding_left = s.padding_left.clone(); }

// ── Color / Background ──────────────────────────────────────────────────────

fn apply_color(s: &mut ComputedStyle, v: &str) {
    if let Some(c) = parse_color(v) { s.color = c; }
}
fn apply_background_color(s: &mut ComputedStyle, v: &str) {
    if let Some(c) = parse_color(v) { s.background_color = c; }
}
fn apply_opacity(s: &mut ComputedStyle, v: &str) {
    let op: f32 = v.parse().unwrap_or(1.0);
    s.opacity = op.max(0.0).min(1.0);
}

fn copy_color(d: &mut ComputedStyle, s: &ComputedStyle) { d.color = s.color; }
fn copy_background_color(d: &mut ComputedStyle, s: &ComputedStyle) { d.background_color = s.background_color; }
fn copy_opacity(d: &mut ComputedStyle, s: &ComputedStyle) { d.opacity = s.opacity; }

// ── Font ────────────────────────────────────────────────────────────────────

fn apply_font_size(s: &mut ComputedStyle, v: &str) { s.font_size = parse_font_size(v); }
fn apply_font_family(s: &mut ComputedStyle, v: &str) {
    let normalized = super::split_font_families(v)
        .into_iter()
        .map(|name| {
            super::resolve_system_font_keyword(&name)
                .map(|sf| sf.to_string())
                .unwrap_or(name)
        })
        .collect::<Vec<_>>()
        .join(", ");
    s.font_family = normalized;
}
fn apply_font_weight(s: &mut ComputedStyle, v: &str) {
    s.font_weight = match v {
        "normal"  => FontWeight::Normal,
        "bold"    => FontWeight::Bold,
        "bolder"  => FontWeight::Value(700),
        "lighter" => FontWeight::Value(300),
        _         => v.parse::<u16>().map(FontWeight::Value).unwrap_or(FontWeight::Normal),
    };
}
fn apply_font_style(s: &mut ComputedStyle, v: &str) {
    s.font_style = match v {
        "italic"  => FontStyle::Italic,
        "oblique" => FontStyle::Oblique,
        _         => FontStyle::Normal,
    };
}
fn apply_font(s: &mut ComputedStyle, v: &str) {
    super::apply_font_shorthand(s, v);
}
fn apply_font_variation_settings(s: &mut ComputedStyle, v: &str) {
    s.font_variation_settings = super::parse_variation_settings(v);
    for (tag, val) in &s.font_variation_settings {
        if tag == "wght" {
            s.font_weight = FontWeight::Value(*val as u16);
        }
    }
}
fn apply_font_feature_settings(s: &mut ComputedStyle, v: &str) {
    s.font_feature_settings = super::parse_feature_settings(v);
    for (tag, val) in &s.font_feature_settings {
        if tag == "smcp" { s.small_caps = *val != 0; }
    }
}
fn apply_font_variant(s: &mut ComputedStyle, v: &str) {
    s.small_caps = v == "small-caps";
}
fn apply_font_stretch(s: &mut ComputedStyle, v: &str) {
    s.font_stretch = match v {
        "ultra-condensed"  => 50.0,
        "extra-condensed"  => 62.5,
        "condensed"        => 75.0,
        "semi-condensed"   => 87.5,
        "normal"           => 100.0,
        "semi-expanded"    => 112.5,
        "expanded"         => 125.0,
        "extra-expanded"   => 150.0,
        "ultra-expanded"   => 200.0,
        s if s.ends_with('%') => s[..s.len()-1].parse().unwrap_or(100.0),
        _                  => 100.0,
    };
}
fn apply_line_height(s: &mut ComputedStyle, v: &str) {
    s.line_height = super::parse_line_height(v);
}

fn copy_font_size(d: &mut ComputedStyle, s: &ComputedStyle)   { d.font_size = s.font_size.clone(); }
fn copy_font_family(d: &mut ComputedStyle, s: &ComputedStyle) { d.font_family = s.font_family.clone(); }
fn copy_font_weight(d: &mut ComputedStyle, s: &ComputedStyle) { d.font_weight = s.font_weight; }
fn copy_font_style(d: &mut ComputedStyle, s: &ComputedStyle)  { d.font_style = s.font_style; }
fn copy_line_height(d: &mut ComputedStyle, s: &ComputedStyle) { d.line_height = s.line_height.clone(); }
fn copy_font_variation_settings(d: &mut ComputedStyle, s: &ComputedStyle) { d.font_variation_settings = s.font_variation_settings.clone(); }
fn copy_font_feature_settings(d: &mut ComputedStyle, s: &ComputedStyle) { d.font_feature_settings = s.font_feature_settings.clone(); d.small_caps = s.small_caps; }
fn copy_font_variant(d: &mut ComputedStyle, s: &ComputedStyle) { d.small_caps = s.small_caps; }
fn copy_font_stretch(d: &mut ComputedStyle, s: &ComputedStyle) { d.font_stretch = s.font_stretch; }

// ── Text ────────────────────────────────────────────────────────────────────

fn apply_text_align(s: &mut ComputedStyle, v: &str) {
    s.text_align = match v {
        "right"   => TextAlign::Right,
        "center"  => TextAlign::Center,
        "justify" => TextAlign::Justify,
        "end"     => TextAlign::End,
        "start"   => TextAlign::Start,
        _         => TextAlign::Left,
    };
}
fn apply_text_transform(s: &mut ComputedStyle, v: &str) {
    s.text_transform = match v {
        "uppercase"  => TextTransform::Uppercase,
        "lowercase"  => TextTransform::Lowercase,
        "capitalize" => TextTransform::Capitalize,
        _            => TextTransform::None,
    };
}
fn apply_text_indent(s: &mut ComputedStyle, v: &str)     { s.text_indent = parse_length(v); }
fn apply_letter_spacing(s: &mut ComputedStyle, v: &str)  { s.letter_spacing = parse_length(v); }
fn apply_word_spacing(s: &mut ComputedStyle, v: &str)    { s.word_spacing = parse_length(v); }
fn apply_white_space(s: &mut ComputedStyle, v: &str) {
    s.white_space = match v {
        "nowrap"   => WhiteSpace::Nowrap,
        "pre"      => WhiteSpace::Pre,
        "pre-wrap" => WhiteSpace::PreWrap,
        "pre-line" => WhiteSpace::PreLine,
        _          => WhiteSpace::Normal,
    };
}
fn apply_direction(s: &mut ComputedStyle, v: &str) {
    s.direction = match v {
        "rtl" => Direction::RTL,
        _     => Direction::LTR,
    };
}
fn apply_visibility(s: &mut ComputedStyle, v: &str) {
    s.visibility = v != "hidden" && v != "collapse";
}
fn apply_cursor(s: &mut ComputedStyle, v: &str) {
    let keyword = v.split(',').last().unwrap_or(v).trim();
    s.cursor = match keyword {
        "pointer"     => CSSCursor::Pointer,
        "text"        => CSSCursor::Text,
        "default"     => CSSCursor::Default,
        "move"        => CSSCursor::Move,
        "not-allowed" => CSSCursor::NotAllowed,
        "grab"        => CSSCursor::Grab,
        "grabbing"    => CSSCursor::Grabbing,
        "col-resize"  => CSSCursor::ColResize,
        "row-resize"  => CSSCursor::RowResize,
        "crosshair"   => CSSCursor::Crosshair,
        "help"        => CSSCursor::Help,
        "wait"        => CSSCursor::Wait,
        "progress"    => CSSCursor::Wait,
        "none"        => CSSCursor::None,
        "n-resize"    => CSSCursor::NResize,
        "e-resize"    => CSSCursor::EResize,
        "s-resize"    => CSSCursor::SResize,
        "w-resize"    => CSSCursor::WResize,
        _             => CSSCursor::Auto,
    };
}

fn copy_text_align(d: &mut ComputedStyle, s: &ComputedStyle)      { d.text_align = s.text_align; }
fn copy_text_transform(d: &mut ComputedStyle, s: &ComputedStyle)  { d.text_transform = s.text_transform; }
fn copy_text_indent(d: &mut ComputedStyle, s: &ComputedStyle)     { d.text_indent = s.text_indent.clone(); }
fn copy_letter_spacing(d: &mut ComputedStyle, s: &ComputedStyle)  { d.letter_spacing = s.letter_spacing.clone(); }
fn copy_word_spacing(d: &mut ComputedStyle, s: &ComputedStyle)    { d.word_spacing = s.word_spacing.clone(); }
fn copy_white_space(d: &mut ComputedStyle, s: &ComputedStyle)     { d.white_space = s.white_space; }
fn copy_direction(d: &mut ComputedStyle, s: &ComputedStyle)       { d.direction = s.direction; }
fn copy_visibility(d: &mut ComputedStyle, s: &ComputedStyle)      { d.visibility = s.visibility; }
fn copy_cursor(d: &mut ComputedStyle, s: &ComputedStyle)          { d.cursor = s.cursor; }

// ── Display & Layout ────────────────────────────────────────────────────────

fn apply_display(s: &mut ComputedStyle, v: &str) {
    s.display = match v {
        "none"               => Display::None,
        "block"              => Display::Block,
        "inline"             => Display::Inline,
        "inline-block"       => Display::InlineBlock,
        "flex"               => Display::Flex,
        "inline-flex"        => Display::InlineFlex,
        "grid"               => Display::Grid,
        "inline-grid"        => Display::InlineGrid,
        "table"              => Display::Table,
        "table-row"          => Display::TableRow,
        "table-cell"         => Display::TableCell,
        "table-caption"      => Display::TableCaption,
        "table-column"       => Display::TableColumn,
        "table-column-group" => Display::TableColumnGroup,
        "table-header-group" => Display::TableHeaderGroup,
        "table-footer-group" => Display::TableFooterGroup,
        "table-row-group"    => Display::TableRowGroup,
        "list-item"          => Display::ListItem,
        "flow-root"          => Display::FlowRoot,
        "contents"           => Display::Contents,
        "ruby"               => Display::Ruby,
        "ruby-text"          => Display::RubyText,
        _                    => Display::Inline,
    };
}
fn apply_position(s: &mut ComputedStyle, v: &str) {
    s.position = match v {
        "static"   => Position::Static,
        "relative" => Position::Relative,
        "absolute" => Position::Absolute,
        "fixed"    => Position::Fixed,
        "sticky"   => Position::Sticky,
        _          => Position::Static,
    };
}
fn apply_z_index(s: &mut ComputedStyle, v: &str) {
    s.z_index = v.parse().unwrap_or(0);
}
fn apply_float(s: &mut ComputedStyle, v: &str) {
    s.float = match v { "left" => Float::Left, "right" => Float::Right, _ => Float::None };
    if s.float != Float::None {
        s.display = match s.display {
            Display::Inline | Display::InlineBlock | Display::InlineFlex | Display::InlineGrid
                => Display::Block,
            other => other,
        };
    }
}
fn apply_clear(s: &mut ComputedStyle, v: &str) {
    s.clear = match v { "left" => Clear::Left, "right" => Clear::Right, "both" => Clear::Both, _ => Clear::None };
}
fn apply_box_sizing(s: &mut ComputedStyle, v: &str) {
    s.box_sizing = match v {
        "border-box" => BoxSizing::BorderBox,
        _            => BoxSizing::ContentBox,
    };
}
fn apply_overflow_x(s: &mut ComputedStyle, v: &str) {
    s.overflow_x = super::parse_overflow(v);
}
fn apply_overflow_y(s: &mut ComputedStyle, v: &str) {
    s.overflow_y = super::parse_overflow(v);
}
fn apply_overflow(s: &mut ComputedStyle, v: &str) {
    let ov = super::parse_overflow(v);
    s.overflow_x = ov;
    s.overflow_y = ov;
}

fn copy_display(d: &mut ComputedStyle, s: &ComputedStyle)   { d.display = s.display; }
fn copy_position(d: &mut ComputedStyle, s: &ComputedStyle)  { d.position = s.position; }
fn copy_z_index(d: &mut ComputedStyle, s: &ComputedStyle)   { d.z_index = s.z_index; }
fn copy_float(d: &mut ComputedStyle, s: &ComputedStyle)     { d.float = s.float; }
fn copy_clear(d: &mut ComputedStyle, s: &ComputedStyle)     { d.clear = s.clear; }
fn copy_box_sizing(d: &mut ComputedStyle, s: &ComputedStyle) { d.box_sizing = s.box_sizing; }
fn copy_overflow_x(d: &mut ComputedStyle, s: &ComputedStyle) { d.overflow_x = s.overflow_x; }
fn copy_overflow_y(d: &mut ComputedStyle, s: &ComputedStyle) { d.overflow_y = s.overflow_y; }

// ── Position offsets ────────────────────────────────────────────────────────

fn apply_top(s: &mut ComputedStyle, v: &str)    { s.top = parse_length(v); }
fn apply_right(s: &mut ComputedStyle, v: &str)  { s.right = parse_length(v); }
fn apply_bottom(s: &mut ComputedStyle, v: &str) { s.bottom = parse_length(v); }
fn apply_left(s: &mut ComputedStyle, v: &str)   { s.left = parse_length(v); }

fn copy_top(d: &mut ComputedStyle, s: &ComputedStyle)    { d.top = s.top.clone(); }
fn copy_right(d: &mut ComputedStyle, s: &ComputedStyle)  { d.right = s.right.clone(); }
fn copy_bottom(d: &mut ComputedStyle, s: &ComputedStyle) { d.bottom = s.bottom.clone(); }
fn copy_left(d: &mut ComputedStyle, s: &ComputedStyle)   { d.left = s.left.clone(); }

// ── Border ──────────────────────────────────────────────────────────────────

fn apply_border(s: &mut ComputedStyle, v: &str) { super::apply_border_shorthand(s, v); }
fn apply_border_width(s: &mut ComputedStyle, v: &str) {
    super::apply_shorthand_4(v,
        &mut s.border_top_width, &mut s.border_right_width,
        &mut s.border_bottom_width, &mut s.border_left_width,
        parse_length);
}
fn apply_border_style_sh(s: &mut ComputedStyle, v: &str) {
    let parts: Vec<&str> = v.split_whitespace().collect();
    match parts.len() {
        1 => {
            let bs = super::parse_border_style(parts[0]);
            s.border_top_style = bs; s.border_right_style = bs;
            s.border_bottom_style = bs; s.border_left_style = bs;
        }
        2 => {
            let tb = super::parse_border_style(parts[0]);
            let rl = super::parse_border_style(parts[1]);
            s.border_top_style = tb; s.border_bottom_style = tb;
            s.border_right_style = rl; s.border_left_style = rl;
        }
        3 => {
            s.border_top_style = super::parse_border_style(parts[0]);
            let rl = super::parse_border_style(parts[1]);
            s.border_right_style = rl; s.border_left_style = rl;
            s.border_bottom_style = super::parse_border_style(parts[2]);
        }
        4 => {
            s.border_top_style    = super::parse_border_style(parts[0]);
            s.border_right_style  = super::parse_border_style(parts[1]);
            s.border_bottom_style = super::parse_border_style(parts[2]);
            s.border_left_style   = super::parse_border_style(parts[3]);
        }
        _ => {}
    }
}
fn apply_border_color_sh(s: &mut ComputedStyle, v: &str) {
    // Split by whitespace, but be careful with color functions like rgb(...)
    // Use the same comma/space-aware splitting as other shorthands
    let parts: Vec<&str> = super::split_shorthand_values(v);
    match parts.len() {
        1 => {
            let bc = parse_color(parts[0]).unwrap_or(Color::BLACK);
            s.border_top_color = bc; s.border_right_color = bc;
            s.border_bottom_color = bc; s.border_left_color = bc;
        }
        2 => {
            let tb = parse_color(parts[0]).unwrap_or(Color::BLACK);
            let rl = parse_color(parts[1]).unwrap_or(Color::BLACK);
            s.border_top_color = tb; s.border_bottom_color = tb;
            s.border_right_color = rl; s.border_left_color = rl;
        }
        3 => {
            s.border_top_color = parse_color(parts[0]).unwrap_or(Color::BLACK);
            let rl = parse_color(parts[1]).unwrap_or(Color::BLACK);
            s.border_right_color = rl; s.border_left_color = rl;
            s.border_bottom_color = parse_color(parts[2]).unwrap_or(Color::BLACK);
        }
        4 => {
            s.border_top_color    = parse_color(parts[0]).unwrap_or(Color::BLACK);
            s.border_right_color  = parse_color(parts[1]).unwrap_or(Color::BLACK);
            s.border_bottom_color = parse_color(parts[2]).unwrap_or(Color::BLACK);
            s.border_left_color   = parse_color(parts[3]).unwrap_or(Color::BLACK);
        }
        _ => {}
    }
}
fn apply_border_top_width(s: &mut ComputedStyle, v: &str)    { s.border_top_width    = parse_length(v); }
fn apply_border_right_width(s: &mut ComputedStyle, v: &str)  { s.border_right_width  = parse_length(v); }
fn apply_border_bottom_width(s: &mut ComputedStyle, v: &str) { s.border_bottom_width = parse_length(v); }
fn apply_border_left_width(s: &mut ComputedStyle, v: &str)   { s.border_left_width   = parse_length(v); }
fn apply_border_top_style(s: &mut ComputedStyle, v: &str)    { s.border_top_style    = super::parse_border_style(v); }
fn apply_border_right_style(s: &mut ComputedStyle, v: &str)  { s.border_right_style  = super::parse_border_style(v); }
fn apply_border_bottom_style(s: &mut ComputedStyle, v: &str) { s.border_bottom_style = super::parse_border_style(v); }
fn apply_border_left_style(s: &mut ComputedStyle, v: &str)   { s.border_left_style   = super::parse_border_style(v); }
fn apply_border_top_color(s: &mut ComputedStyle, v: &str)    { if let Some(c) = parse_color(v) { s.border_top_color    = c; } }
fn apply_border_right_color(s: &mut ComputedStyle, v: &str)  { if let Some(c) = parse_color(v) { s.border_right_color  = c; } }
fn apply_border_bottom_color(s: &mut ComputedStyle, v: &str) { if let Some(c) = parse_color(v) { s.border_bottom_color = c; } }
fn apply_border_left_color(s: &mut ComputedStyle, v: &str)   { if let Some(c) = parse_color(v) { s.border_left_color   = c; } }

fn apply_border_top_sh(s: &mut ComputedStyle, v: &str)    { super::apply_border_side_shorthand(v, &mut s.border_top_width,    &mut s.border_top_style,    &mut s.border_top_color); }
fn apply_border_right_sh(s: &mut ComputedStyle, v: &str)  { super::apply_border_side_shorthand(v, &mut s.border_right_width,  &mut s.border_right_style,  &mut s.border_right_color); }
fn apply_border_bottom_sh(s: &mut ComputedStyle, v: &str) { super::apply_border_side_shorthand(v, &mut s.border_bottom_width, &mut s.border_bottom_style, &mut s.border_bottom_color); }
fn apply_border_left_sh(s: &mut ComputedStyle, v: &str)   { super::apply_border_side_shorthand(v, &mut s.border_left_width,   &mut s.border_left_style,   &mut s.border_left_color); }

fn copy_border_top_width(d: &mut ComputedStyle, s: &ComputedStyle)    { d.border_top_width    = s.border_top_width.clone(); }
fn copy_border_right_width(d: &mut ComputedStyle, s: &ComputedStyle)  { d.border_right_width  = s.border_right_width.clone(); }
fn copy_border_bottom_width(d: &mut ComputedStyle, s: &ComputedStyle) { d.border_bottom_width = s.border_bottom_width.clone(); }
fn copy_border_left_width(d: &mut ComputedStyle, s: &ComputedStyle)   { d.border_left_width   = s.border_left_width.clone(); }
fn copy_border_top_style(d: &mut ComputedStyle, s: &ComputedStyle)    { d.border_top_style    = s.border_top_style; }
fn copy_border_right_style(d: &mut ComputedStyle, s: &ComputedStyle)  { d.border_right_style  = s.border_right_style; }
fn copy_border_bottom_style(d: &mut ComputedStyle, s: &ComputedStyle) { d.border_bottom_style = s.border_bottom_style; }
fn copy_border_left_style(d: &mut ComputedStyle, s: &ComputedStyle)   { d.border_left_style   = s.border_left_style; }
fn copy_border_top_color(d: &mut ComputedStyle, s: &ComputedStyle)    { d.border_top_color    = s.border_top_color; }
fn copy_border_right_color(d: &mut ComputedStyle, s: &ComputedStyle)  { d.border_right_color  = s.border_right_color; }
fn copy_border_bottom_color(d: &mut ComputedStyle, s: &ComputedStyle) { d.border_bottom_color = s.border_bottom_color; }
fn copy_border_left_color(d: &mut ComputedStyle, s: &ComputedStyle)   { d.border_left_color   = s.border_left_color; }

// ── Border radius ───────────────────────────────────────────────────────────

fn apply_border_radius(s: &mut ComputedStyle, v: &str) {
    let radii = if let Some(slash) = v.find('/') { v[..slash].trim() } else { v };
    let parts: Vec<&str> = radii.split_whitespace().collect();
    let tl = parse_length(parts.first().copied().unwrap_or("0"));
    let tr = parse_length(parts.get(1).copied().unwrap_or(parts.first().copied().unwrap_or("0")));
    let br = parse_length(parts.get(2).copied().unwrap_or(parts.first().copied().unwrap_or("0")));
    let bl = parse_length(parts.get(3).copied().unwrap_or(parts.get(1).copied().unwrap_or(parts.first().copied().unwrap_or("0"))));
    s.border_radius              = tl.clone();
    s.border_top_left_radius     = tl;
    s.border_top_right_radius    = tr;
    s.border_bottom_right_radius = br;
    s.border_bottom_left_radius  = bl;
}
fn apply_border_top_left_radius(s: &mut ComputedStyle, v: &str)     { s.border_top_left_radius     = parse_length(v); s.border_radius = s.border_top_left_radius.clone(); }
fn apply_border_top_right_radius(s: &mut ComputedStyle, v: &str)    { s.border_top_right_radius    = parse_length(v); }
fn apply_border_bottom_left_radius(s: &mut ComputedStyle, v: &str)  { s.border_bottom_left_radius  = parse_length(v); }
fn apply_border_bottom_right_radius(s: &mut ComputedStyle, v: &str) { s.border_bottom_right_radius = parse_length(v); }

fn copy_border_top_left_radius(d: &mut ComputedStyle, s: &ComputedStyle)     { d.border_top_left_radius     = s.border_top_left_radius.clone(); d.border_radius = s.border_radius.clone(); }
fn copy_border_top_right_radius(d: &mut ComputedStyle, s: &ComputedStyle)    { d.border_top_right_radius    = s.border_top_right_radius.clone(); }
fn copy_border_bottom_left_radius(d: &mut ComputedStyle, s: &ComputedStyle)  { d.border_bottom_left_radius  = s.border_bottom_left_radius.clone(); }
fn copy_border_bottom_right_radius(d: &mut ComputedStyle, s: &ComputedStyle) { d.border_bottom_right_radius = s.border_bottom_right_radius.clone(); }

// ── Table ───────────────────────────────────────────────────────────────────

fn apply_border_collapse(s: &mut ComputedStyle, v: &str) { s.border_collapse = v == "collapse"; }
fn apply_border_spacing(s: &mut ComputedStyle, v: &str) {
    let parts: Vec<&str> = v.split_whitespace().collect();
    s.border_spacing_h = parse_length(parts.first().copied().unwrap_or("0"));
    s.border_spacing_v = parse_length(parts.get(1).copied().unwrap_or(parts.first().copied().unwrap_or("0")));
}
fn apply_caption_side(s: &mut ComputedStyle, v: &str) { s.caption_side = if v == "bottom" { CaptionSide::Bottom } else { CaptionSide::Top }; }
fn apply_empty_cells(s: &mut ComputedStyle, v: &str)  { s.empty_cells_hide = v == "hide"; }
fn apply_table_layout(s: &mut ComputedStyle, v: &str) { s.table_layout_fixed = v == "fixed"; }

fn copy_border_collapse(d: &mut ComputedStyle, s: &ComputedStyle) { d.border_collapse = s.border_collapse; }
fn copy_border_spacing(d: &mut ComputedStyle, s: &ComputedStyle)  { d.border_spacing_h = s.border_spacing_h.clone(); d.border_spacing_v = s.border_spacing_v.clone(); }
fn copy_caption_side(d: &mut ComputedStyle, s: &ComputedStyle)    { d.caption_side = s.caption_side; }
fn copy_empty_cells(d: &mut ComputedStyle, s: &ComputedStyle)     { d.empty_cells_hide = s.empty_cells_hide; }
fn copy_table_layout(d: &mut ComputedStyle, s: &ComputedStyle)    { d.table_layout_fixed = s.table_layout_fixed; }

// ── Vertical align ──────────────────────────────────────────────────────────

fn apply_vertical_align(s: &mut ComputedStyle, v: &str) {
    s.vertical_align = match v {
        "top"         => VerticalAlign::Top,
        "middle"      => VerticalAlign::Middle,
        "bottom"      => VerticalAlign::Bottom,
        "text-top"    => VerticalAlign::TextTop,
        "text-bottom" => VerticalAlign::TextBottom,
        "sub"         => VerticalAlign::Sub,
        "super"       => VerticalAlign::Super,
        _             => VerticalAlign::Baseline,
    };
}
fn copy_vertical_align(d: &mut ComputedStyle, s: &ComputedStyle) { d.vertical_align = s.vertical_align; }

// ── Text decoration ─────────────────────────────────────────────────────────

fn apply_text_decoration(s: &mut ComputedStyle, v: &str) {
    s.text_decoration.underline     = v.contains("underline");
    s.text_decoration.overline      = v.contains("overline");
    s.text_decoration.strikethrough = v.contains("line-through");
    s.text_decoration_style = if v.contains("double") {
        TextDecorationStyle::Double
    } else if v.contains("dotted") {
        TextDecorationStyle::Dotted
    } else if v.contains("dashed") {
        TextDecorationStyle::Dashed
    } else if v.contains("wavy") {
        TextDecorationStyle::Wavy
    } else {
        TextDecorationStyle::Solid
    };
    for token in v.split_whitespace() {
        if let Some(c) = parse_color(token) {
            s.text_decoration_color = Some(c);
            break;
        }
    }
}
fn apply_text_decoration_line(s: &mut ComputedStyle, v: &str) {
    s.text_decoration.underline     = v.contains("underline");
    s.text_decoration.overline      = v.contains("overline");
    s.text_decoration.strikethrough = v.contains("line-through");
}
fn apply_text_decoration_color(s: &mut ComputedStyle, v: &str) {
    s.text_decoration_color = parse_color(v);
}
fn apply_text_decoration_style_fn(s: &mut ComputedStyle, v: &str) {
    s.text_decoration_style = match v {
        "double" => TextDecorationStyle::Double,
        "dotted" => TextDecorationStyle::Dotted,
        "dashed" => TextDecorationStyle::Dashed,
        "wavy"   => TextDecorationStyle::Wavy,
        _        => TextDecorationStyle::Solid,
    };
}
fn apply_text_decoration_thickness(s: &mut ComputedStyle, v: &str) {
    s.text_decoration_thickness = parse_length(v);
}
fn apply_text_underline_offset(s: &mut ComputedStyle, v: &str) {
    s.text_underline_offset = parse_length(v);
}
fn apply_text_overflow(s: &mut ComputedStyle, v: &str) {
    s.text_overflow = if v == "ellipsis" { TextOverflow::Ellipsis } else { TextOverflow::Clip };
}
fn apply_text_shadow(s: &mut ComputedStyle, v: &str) {
    if v == "none" {
        s.text_shadow = None;
    } else {
        let ts = super::parse_shadow_value(v);
        s.text_shadow = Some(TextShadow { offset_x: ts.0, offset_y: ts.1, blur: ts.2, color: ts.3 });
    }
}

fn copy_text_decoration_line(d: &mut ComputedStyle, s: &ComputedStyle) { d.text_decoration = s.text_decoration; }
fn copy_text_decoration_color(d: &mut ComputedStyle, s: &ComputedStyle) { d.text_decoration_color = s.text_decoration_color; }
fn copy_text_decoration_style(d: &mut ComputedStyle, s: &ComputedStyle) { d.text_decoration_style = s.text_decoration_style; }
fn copy_text_decoration_thickness(d: &mut ComputedStyle, s: &ComputedStyle) { d.text_decoration_thickness = s.text_decoration_thickness.clone(); }
fn copy_text_underline_offset(d: &mut ComputedStyle, s: &ComputedStyle) { d.text_underline_offset = s.text_underline_offset.clone(); }
fn copy_text_overflow(d: &mut ComputedStyle, s: &ComputedStyle) { d.text_overflow = s.text_overflow; }
fn copy_text_shadow(d: &mut ComputedStyle, s: &ComputedStyle) { d.text_shadow = s.text_shadow.clone(); }

// ── Word / overflow-wrap ────────────────────────────────────────────────────

fn apply_word_break(s: &mut ComputedStyle, v: &str) {
    s.word_break = match v {
        "break-all"  => WordBreak::BreakAll,
        "keep-all"   => WordBreak::KeepAll,
        "break-word" => WordBreak::BreakWord,
        _            => WordBreak::Normal,
    };
}
fn apply_overflow_wrap(s: &mut ComputedStyle, v: &str) {
    s.overflow_wrap = match v {
        "break-word" => OverflowWrap::BreakWord,
        "anywhere"   => OverflowWrap::Anywhere,
        _            => OverflowWrap::Normal,
    };
}

fn copy_word_break(d: &mut ComputedStyle, s: &ComputedStyle)    { d.word_break = s.word_break; }
fn copy_overflow_wrap(d: &mut ComputedStyle, s: &ComputedStyle) { d.overflow_wrap = s.overflow_wrap; }

// ── List style ──────────────────────────────────────────────────────────────

fn apply_list_style_type(s: &mut ComputedStyle, v: &str) {
    s.list_style_type = match v {
        "none"         => ListStyleType::None,
        "disc"         => ListStyleType::Disc,
        "circle"       => ListStyleType::Circle,
        "square"       => ListStyleType::Square,
        "decimal"      => ListStyleType::Decimal,
        "lower-alpha"  => ListStyleType::LowerAlpha,
        "upper-alpha"  => ListStyleType::UpperAlpha,
        "lower-roman"  => ListStyleType::LowerRoman,
        "upper-roman"  => ListStyleType::UpperRoman,
        _              => ListStyleType::None,
    };
}
fn apply_list_style_position(s: &mut ComputedStyle, v: &str) {
    s.list_style_position = if v == "inside" { ListStylePosition::Inside } else { ListStylePosition::Outside };
}
fn apply_list_style_image(s: &mut ComputedStyle, v: &str) {
    if v == "none" {
        s.list_style_image = String::new();
    } else if let Some(url) = super::extract_url(v) {
        s.list_style_image = url;
    }
}
fn apply_list_style(s: &mut ComputedStyle, v: &str) {
    if v.contains("none")          { s.list_style_type = ListStyleType::None; }
    else if v.contains("disc")     { s.list_style_type = ListStyleType::Disc; }
    else if v.contains("circle")   { s.list_style_type = ListStyleType::Circle; }
    else if v.contains("square")   { s.list_style_type = ListStyleType::Square; }
    else if v.contains("decimal")  { s.list_style_type = ListStyleType::Decimal; }
}

fn copy_list_style_type(d: &mut ComputedStyle, s: &ComputedStyle)     { d.list_style_type = s.list_style_type; }
fn copy_list_style_position(d: &mut ComputedStyle, s: &ComputedStyle) { d.list_style_position = s.list_style_position; }
fn copy_list_style_image(d: &mut ComputedStyle, s: &ComputedStyle)    { d.list_style_image = s.list_style_image.clone(); }

// ── Flexbox ─────────────────────────────────────────────────────────────────

fn apply_flex_direction(s: &mut ComputedStyle, v: &str) {
    s.flex_direction = match v {
        "row-reverse"    => FlexDirection::RowReverse,
        "column"         => FlexDirection::Column,
        "column-reverse" => FlexDirection::ColumnReverse,
        _                => FlexDirection::Row,
    };
}
fn apply_flex_wrap(s: &mut ComputedStyle, v: &str) {
    s.flex_wrap = match v {
        "wrap"         => FlexWrap::Wrap,
        "wrap-reverse" => FlexWrap::WrapReverse,
        _              => FlexWrap::Nowrap,
    };
}
fn apply_flex_grow(s: &mut ComputedStyle, v: &str)   { s.flex_grow   = v.parse().unwrap_or(0.0); }
fn apply_flex_shrink(s: &mut ComputedStyle, v: &str) { s.flex_shrink = v.parse().unwrap_or(1.0); }
fn apply_flex_basis(s: &mut ComputedStyle, v: &str)  { s.flex_basis  = parse_length(v); }
fn apply_order(s: &mut ComputedStyle, v: &str)       { s.order       = v.parse().unwrap_or(0); }

fn apply_flex(s: &mut ComputedStyle, v: &str) {
    match v {
        "none" => { s.flex_grow = 0.0; s.flex_shrink = 0.0; s.flex_basis = CssLength::Auto; }
        "auto" => { s.flex_grow = 1.0; s.flex_shrink = 1.0; s.flex_basis = CssLength::Auto; }
        _ => {
            let toks: Vec<&str> = v.split_whitespace().collect();
            if toks.len() == 1 {
                let t = toks[0];
                if t.ends_with('%') || t.ends_with("px") || t.ends_with("em")
                    || t.ends_with("rem") || t.ends_with("vw") || t.ends_with("vh")
                {
                    s.flex_grow = 1.0;
                    s.flex_shrink = 1.0;
                    s.flex_basis = parse_length(t);
                } else {
                    s.flex_grow = t.parse().unwrap_or(0.0);
                    s.flex_shrink = 1.0;
                    s.flex_basis = CssLength::Px(0.0);
                }
            } else {
                if let Some(t0) = toks.first() { s.flex_grow = t0.parse().unwrap_or(0.0); }
                if let Some(t1) = toks.get(1)  { s.flex_shrink = t1.parse().unwrap_or(1.0); }
                else                            { s.flex_shrink = 1.0; s.flex_basis = CssLength::Px(0.0); }
                if let Some(t2) = toks.get(2)  { s.flex_basis = parse_length(t2); }
            }
        }
    }
}
fn apply_flex_flow(s: &mut ComputedStyle, v: &str) {
    for tok in v.split_whitespace() {
        match tok {
            "row"            => { s.flex_direction = FlexDirection::Row; }
            "row-reverse"    => { s.flex_direction = FlexDirection::RowReverse; }
            "column"         => { s.flex_direction = FlexDirection::Column; }
            "column-reverse" => { s.flex_direction = FlexDirection::ColumnReverse; }
            "nowrap"         => { s.flex_wrap = FlexWrap::Nowrap; }
            "wrap"           => { s.flex_wrap = FlexWrap::Wrap; }
            "wrap-reverse"   => { s.flex_wrap = FlexWrap::WrapReverse; }
            _ => {}
        }
    }
}
fn apply_justify_content(s: &mut ComputedStyle, v: &str) {
    s.justify_content = match v {
        "flex-end" | "end"   => JustifyContent::FlexEnd,
        "center"             => JustifyContent::Center,
        "space-between"      => JustifyContent::SpaceBetween,
        "space-around"       => JustifyContent::SpaceAround,
        "space-evenly"       => JustifyContent::SpaceEvenly,
        _                    => JustifyContent::FlexStart,
    };
}
fn apply_align_items(s: &mut ComputedStyle, v: &str) {
    s.align_items = match v {
        "flex-start" | "start" | "self-start" => AlignItems::FlexStart,
        "flex-end"   | "end"   | "self-end"   => AlignItems::FlexEnd,
        "center"     => AlignItems::Center,
        "baseline"   => AlignItems::Baseline,
        _            => AlignItems::Stretch,
    };
}
fn apply_align_self(s: &mut ComputedStyle, v: &str) {
    s.align_self = match v {
        "flex-start" | "start" | "self-start" => AlignSelf::FlexStart,
        "flex-end"   | "end"   | "self-end"   => AlignSelf::FlexEnd,
        "center"     => AlignSelf::Center,
        "baseline"   => AlignSelf::Baseline,
        "stretch"    => AlignSelf::Stretch,
        _            => AlignSelf::Auto,
    };
}
fn apply_align_content(s: &mut ComputedStyle, v: &str) {
    s.align_content = match v {
        "flex-start"   => AlignContent::FlexStart,
        "flex-end"     => AlignContent::FlexEnd,
        "center"       => AlignContent::Center,
        "space-between"=> AlignContent::SpaceBetween,
        "space-around" => AlignContent::SpaceAround,
        "space-evenly" => AlignContent::SpaceEvenly,
        _              => AlignContent::Stretch,
    };
}
fn apply_justify_items(s: &mut ComputedStyle, v: &str) {
    s.justify_items = match v {
        "flex-start" | "start" => AlignItems::FlexStart,
        "flex-end"   | "end"   => AlignItems::FlexEnd,
        "center"               => AlignItems::Center,
        "baseline"             => AlignItems::Baseline,
        _                      => AlignItems::Stretch,
    };
}
fn apply_justify_self(s: &mut ComputedStyle, v: &str) {
    s.justify_self = match v {
        "flex-start" | "start" => AlignSelf::FlexStart,
        "flex-end"   | "end"   => AlignSelf::FlexEnd,
        "center"               => AlignSelf::Center,
        "baseline"             => AlignSelf::Baseline,
        "stretch"              => AlignSelf::Stretch,
        _                      => AlignSelf::Auto,
    };
}
fn apply_gap(s: &mut ComputedStyle, v: &str) {
    let g = parse_length(v);
    s.row_gap = g.clone();
    s.column_gap = g.clone();
    s.gap = g;
}
fn apply_row_gap(s: &mut ComputedStyle, v: &str)    { s.row_gap    = parse_length(v); }
fn apply_column_gap(s: &mut ComputedStyle, v: &str) { s.column_gap = parse_length(v); }

fn copy_flex_direction(d: &mut ComputedStyle, s: &ComputedStyle)  { d.flex_direction = s.flex_direction; }
fn copy_flex_wrap(d: &mut ComputedStyle, s: &ComputedStyle)       { d.flex_wrap = s.flex_wrap; }
fn copy_flex_grow(d: &mut ComputedStyle, s: &ComputedStyle)       { d.flex_grow = s.flex_grow; }
fn copy_flex_shrink(d: &mut ComputedStyle, s: &ComputedStyle)     { d.flex_shrink = s.flex_shrink; }
fn copy_flex_basis(d: &mut ComputedStyle, s: &ComputedStyle)      { d.flex_basis = s.flex_basis.clone(); }
fn copy_order(d: &mut ComputedStyle, s: &ComputedStyle)           { d.order = s.order; }
fn copy_justify_content(d: &mut ComputedStyle, s: &ComputedStyle) { d.justify_content = s.justify_content; }
fn copy_align_items(d: &mut ComputedStyle, s: &ComputedStyle)     { d.align_items = s.align_items; }
fn copy_align_self(d: &mut ComputedStyle, s: &ComputedStyle)      { d.align_self = s.align_self; }
fn copy_align_content(d: &mut ComputedStyle, s: &ComputedStyle)   { d.align_content = s.align_content; }
fn copy_justify_items(d: &mut ComputedStyle, s: &ComputedStyle)   { d.justify_items = s.justify_items; }
fn copy_justify_self(d: &mut ComputedStyle, s: &ComputedStyle)    { d.justify_self = s.justify_self; }
fn copy_row_gap(d: &mut ComputedStyle, s: &ComputedStyle)         { d.row_gap = s.row_gap.clone(); d.gap = s.gap.clone(); }
fn copy_column_gap(d: &mut ComputedStyle, s: &ComputedStyle)      { d.column_gap = s.column_gap.clone(); }

// ── Grid ────────────────────────────────────────────────────────────────────

fn apply_grid_template_columns(s: &mut ComputedStyle, v: &str) {
    let mut names = std::collections::HashMap::new();
    let tracks = super::parse_track_list_with_names(v, &mut s.auto_repeat_columns, &mut names);
    if tracks.first().map(|t| t.is_subgrid()).unwrap_or(false) {
        s.subgrid_columns = true;
        s.grid_template_columns = Vec::new();
    } else {
        s.subgrid_columns = false;
        s.grid_template_columns = tracks;
    }
    s.grid_col_line_names = names;
}
fn apply_grid_template_rows(s: &mut ComputedStyle, v: &str) {
    let mut dummy = Vec::new();
    let mut names = std::collections::HashMap::new();
    let tracks = super::parse_track_list_with_names(v, &mut dummy, &mut names);
    if tracks.first().map(|t| t.is_subgrid()).unwrap_or(false) {
        s.subgrid_rows = true;
        s.grid_template_rows = Vec::new();
    } else {
        s.subgrid_rows = false;
        s.grid_template_rows = tracks;
    }
    s.grid_row_line_names = names;
}
fn apply_grid_template_areas(s: &mut ComputedStyle, v: &str) {
    s.grid_template_areas = super::parse_grid_template_areas(v);
}
fn apply_grid_auto_columns(s: &mut ComputedStyle, v: &str) {
    s.grid_auto_columns = super::parse_single_track(v);
}
fn apply_grid_auto_rows(s: &mut ComputedStyle, v: &str) {
    s.grid_auto_rows = super::parse_single_track(v);
}
fn apply_grid_auto_flow(s: &mut ComputedStyle, v: &str) {
    s.grid_auto_flow = match v {
        "column"        => GridAutoFlow::Column,
        "row dense"     => GridAutoFlow::RowDense,
        "column dense"  => GridAutoFlow::ColumnDense,
        _               => GridAutoFlow::Row,
    };
}
fn apply_grid_column(s: &mut ComputedStyle, v: &str) {
    if let Some(slash) = v.find('/') {
        let (sv, sn) = super::parse_grid_line_named(v[..slash].trim());
        let (ev, en) = super::parse_grid_line_named(v[slash+1..].trim());
        s.grid_column_start = sv;
        s.grid_column_end   = ev;
        s.grid_column_start_name = sn;
        s.grid_column_end_name   = en;
    } else {
        let (val, name) = super::parse_grid_line_named(v);
        s.grid_column_start = val;
        s.grid_column_end   = 0;
        if !name.is_empty() {
            s.grid_column_start_name = format!("{}-start", name);
            s.grid_column_end_name   = format!("{}-end", name);
        } else {
            s.grid_column_start_name = String::new();
            s.grid_column_end_name   = String::new();
        }
    }
}
fn apply_grid_row(s: &mut ComputedStyle, v: &str) {
    if let Some(slash) = v.find('/') {
        let (sv, sn) = super::parse_grid_line_named(v[..slash].trim());
        let (ev, en) = super::parse_grid_line_named(v[slash+1..].trim());
        s.grid_row_start = sv;
        s.grid_row_end   = ev;
        s.grid_row_start_name = sn;
        s.grid_row_end_name   = en;
    } else {
        let (val, name) = super::parse_grid_line_named(v);
        s.grid_row_start = val;
        s.grid_row_end   = 0;
        if !name.is_empty() {
            s.grid_row_start_name = format!("{}-start", name);
            s.grid_row_end_name   = format!("{}-end", name);
        } else {
            s.grid_row_start_name = String::new();
            s.grid_row_end_name   = String::new();
        }
    }
}
fn apply_grid_column_start(s: &mut ComputedStyle, v: &str) {
    let (val, name) = super::parse_grid_line_named(v);
    s.grid_column_start = val;
    s.grid_column_start_name = name;
}
fn apply_grid_column_end(s: &mut ComputedStyle, v: &str) {
    let (val, name) = super::parse_grid_line_named(v);
    s.grid_column_end = val;
    s.grid_column_end_name = name;
}
fn apply_grid_row_start(s: &mut ComputedStyle, v: &str) {
    let (val, name) = super::parse_grid_line_named(v);
    s.grid_row_start = val;
    s.grid_row_start_name = name;
}
fn apply_grid_row_end(s: &mut ComputedStyle, v: &str) {
    let (val, name) = super::parse_grid_line_named(v);
    s.grid_row_end = val;
    s.grid_row_end_name = name;
}
fn apply_grid_area(s: &mut ComputedStyle, v: &str) {
    let parts: Vec<&str> = v.splitn(4, '/').collect();
    if parts.len() == 4 {
        let rs = super::parse_grid_line(parts[0].trim());
        let cs = super::parse_grid_line(parts[1].trim());
        let re = super::parse_grid_line(parts[2].trim());
        let ce = super::parse_grid_line(parts[3].trim());
        if rs != 0 || cs != 0 || re != 0 || ce != 0 {
            s.grid_row_start    = rs;
            s.grid_column_start = cs;
            s.grid_row_end      = re;
            s.grid_column_end   = ce;
        } else {
            s.grid_area = v.to_string();
        }
    } else {
        s.grid_area = v.to_string();
    }
}
fn apply_grid_template(s: &mut ComputedStyle, v: &str) {
    if v == "none" {
        s.grid_template_rows.clear(); s.grid_template_columns.clear();
        s.subgrid_columns = false; s.subgrid_rows = false;
    } else if let Some(slash) = v.find('/') {
        let rows_part = v[..slash].trim();
        let cols_part = v[slash+1..].trim();
        apply_grid_template_rows(s, rows_part);
        apply_grid_template_columns(s, cols_part);
    }
}

fn copy_grid_template_columns(d: &mut ComputedStyle, s: &ComputedStyle) { d.grid_template_columns = s.grid_template_columns.clone(); d.auto_repeat_columns = s.auto_repeat_columns.clone(); d.grid_col_line_names = s.grid_col_line_names.clone(); d.subgrid_columns = s.subgrid_columns; }
fn copy_grid_template_rows(d: &mut ComputedStyle, s: &ComputedStyle)    { d.grid_template_rows = s.grid_template_rows.clone(); d.grid_row_line_names = s.grid_row_line_names.clone(); d.subgrid_rows = s.subgrid_rows; }
fn copy_grid_template_areas(d: &mut ComputedStyle, s: &ComputedStyle)   { d.grid_template_areas = s.grid_template_areas.clone(); }
fn copy_grid_auto_columns(d: &mut ComputedStyle, s: &ComputedStyle)     { d.grid_auto_columns = s.grid_auto_columns.clone(); }
fn copy_grid_auto_rows(d: &mut ComputedStyle, s: &ComputedStyle)        { d.grid_auto_rows = s.grid_auto_rows.clone(); }
fn copy_grid_auto_flow(d: &mut ComputedStyle, s: &ComputedStyle)        { d.grid_auto_flow = s.grid_auto_flow; }
fn copy_grid_column_start(d: &mut ComputedStyle, s: &ComputedStyle)     { d.grid_column_start = s.grid_column_start; d.grid_column_start_name = s.grid_column_start_name.clone(); }
fn copy_grid_column_end(d: &mut ComputedStyle, s: &ComputedStyle)       { d.grid_column_end = s.grid_column_end; d.grid_column_end_name = s.grid_column_end_name.clone(); }
fn copy_grid_row_start(d: &mut ComputedStyle, s: &ComputedStyle)        { d.grid_row_start = s.grid_row_start; d.grid_row_start_name = s.grid_row_start_name.clone(); }
fn copy_grid_row_end(d: &mut ComputedStyle, s: &ComputedStyle)          { d.grid_row_end = s.grid_row_end; d.grid_row_end_name = s.grid_row_end_name.clone(); }

// ── Background ──────────────────────────────────────────────────────────────

fn apply_background(s: &mut ComputedStyle, v: &str) {
    // Handle gradient functions first
    if v.contains("gradient") {
        super::apply_gradient(s, v);
        return;
    }
    // Extract url() first
    if let Some(url) = super::extract_url(v) {
        s.background_image_url = url;
    }
    // Strip url(...) from value before splitting
    let v_no_url = if let Some(start) = v.find("url(") {
        let depth_start = start;
        let mut depth = 0;
        let mut end = v.len();
        for (i, ch) in v[depth_start..].char_indices() {
            if ch == '(' { depth += 1; }
            if ch == ')' { depth -= 1; if depth == 0 { end = depth_start + i + 1; break; } }
        }
        format!("{} {}", &v[..depth_start], &v[end..])
    } else {
        v.to_string()
    };
    let v_rest = v_no_url.trim();
    let (pos_part, size_part) = if let Some(slash) = v_rest.find(" / ") {
        (&v_rest[..slash], Some(&v_rest[slash+3..]))
    } else {
        (v_rest, None)
    };
    if let Some(size_str) = size_part {
        let size_tok: &str = size_str.split_whitespace().next().unwrap_or("auto");
        match size_tok {
            "cover"   => s.background_size = BackgroundSize::Cover,
            "contain" => s.background_size = BackgroundSize::Contain,
            _         => {
                s.background_size = BackgroundSize::Explicit;
                s.background_size_w = parse_length(size_tok);
                s.background_size_h = CssLength::Auto;
            }
        }
        for tok in size_str.split_whitespace().skip(1) {
            match tok {
                "no-repeat" => s.background_repeat = BackgroundRepeat::NoRepeat,
                "repeat-x"  => s.background_repeat = BackgroundRepeat::RepeatX,
                "repeat-y"  => s.background_repeat = BackgroundRepeat::RepeatY,
                "repeat"    => s.background_repeat = BackgroundRepeat::Repeat,
                _ => {}
            }
        }
    }
    let mut pos_tokens: Vec<&str> = Vec::new();
    for token in pos_part.split_whitespace() {
        match token {
            "none"       => { s.background_image_url.clear(); }
            "no-repeat"  => { s.background_repeat = BackgroundRepeat::NoRepeat; }
            "repeat-x"   => { s.background_repeat = BackgroundRepeat::RepeatX; }
            "repeat-y"   => { s.background_repeat = BackgroundRepeat::RepeatY; }
            "repeat"     => { s.background_repeat = BackgroundRepeat::Repeat; }
            "left" | "center" | "right" | "top" | "bottom" => {
                pos_tokens.push(token);
            }
            _ => {
                if let Some(c) = parse_color(token) {
                    s.background_color = c;
                } else if token.ends_with('%') || token.ends_with("px") || token.ends_with("em") {
                    pos_tokens.push(token);
                }
            }
        }
    }
    if !pos_tokens.is_empty() {
        let mut x_set = false;
        let mut y_set = false;
        for tok in &pos_tokens {
            match *tok {
                "left"   => { s.background_position_x = CssLength::Percent(0.0);   x_set = true; }
                "right"  => { s.background_position_x = CssLength::Percent(100.0); x_set = true; }
                "top"    => { s.background_position_y = CssLength::Percent(0.0);   y_set = true; }
                "bottom" => { s.background_position_y = CssLength::Percent(100.0); y_set = true; }
                "center" => {
                    if !x_set { s.background_position_x = CssLength::Percent(50.0); x_set = true; }
                    else if !y_set { s.background_position_y = CssLength::Percent(50.0); y_set = true; }
                }
                other => {
                    let l = parse_length(other);
                    if !x_set { s.background_position_x = l; x_set = true; }
                    else if !y_set { s.background_position_y = l; }
                }
            }
        }
        if x_set && !y_set { s.background_position_y = CssLength::Percent(50.0); }
    }
}
fn apply_background_image(s: &mut ComputedStyle, v: &str) {
    if v.contains("gradient") {
        super::apply_gradient(s, v);
    } else if v == "none" {
        s.background_image_url.clear();
    } else if let Some(url) = super::extract_url(v) {
        s.background_image_url = url;
    }
}
fn apply_background_size(s: &mut ComputedStyle, v: &str) {
    match v {
        "cover"   => { s.background_size = BackgroundSize::Cover; }
        "contain" => { s.background_size = BackgroundSize::Contain; }
        "auto"    => { s.background_size = BackgroundSize::Auto; }
        _ => {
            s.background_size = BackgroundSize::Explicit;
            let parts: Vec<&str> = v.split_whitespace().collect();
            s.background_size_w = parse_length(parts.first().copied().unwrap_or("auto"));
            s.background_size_h = if parts.len() >= 2 { parse_length(parts[1]) } else { CssLength::Auto };
        }
    }
}
fn apply_background_position(s: &mut ComputedStyle, v: &str) {
    let parts: Vec<&str> = v.split_whitespace().collect();
    let x_str = parts.first().copied().unwrap_or("0%");
    s.background_position_x = match x_str {
        "left"   => CssLength::Percent(0.0),
        "center" => CssLength::Percent(50.0),
        "right"  => CssLength::Percent(100.0),
        _        => parse_length(x_str),
    };
    let y_str = parts.get(1).copied().unwrap_or("center");
    s.background_position_y = match y_str {
        "top"    => CssLength::Percent(0.0),
        "center" => CssLength::Percent(50.0),
        "bottom" => CssLength::Percent(100.0),
        _        => parse_length(y_str),
    };
}
fn apply_background_repeat(s: &mut ComputedStyle, v: &str) {
    s.background_repeat = match v {
        "repeat"    => BackgroundRepeat::Repeat,
        "repeat-x"  => BackgroundRepeat::RepeatX,
        "repeat-y"  => BackgroundRepeat::RepeatY,
        "no-repeat" => BackgroundRepeat::NoRepeat,
        _           => BackgroundRepeat::Repeat,
    };
}
fn apply_background_clip(s: &mut ComputedStyle, v: &str) {
    s.background_clip = match v {
        "padding-box" => BackgroundClip::PaddingBox,
        "content-box" => BackgroundClip::ContentBox,
        "text"        => BackgroundClip::Text,
        _             => BackgroundClip::BorderBox,
    };
}
fn apply_background_origin(s: &mut ComputedStyle, v: &str) {
    s.background_origin = match v {
        "border-box"  => BackgroundClip::BorderBox,
        "content-box" => BackgroundClip::ContentBox,
        _             => BackgroundClip::PaddingBox,
    };
}
fn apply_background_attachment(s: &mut ComputedStyle, v: &str) {
    s.background_attachment = match v {
        "fixed" => BackgroundAttachment::Fixed,
        "local" => BackgroundAttachment::Local,
        _       => BackgroundAttachment::Scroll,
    };
}

fn copy_background_image(d: &mut ComputedStyle, s: &ComputedStyle)      { d.background_image_url = s.background_image_url.clone(); d.gradient_type = s.gradient_type; d.gradient_angle = s.gradient_angle; d.gradient_stops = s.gradient_stops.clone(); }
fn copy_background_size(d: &mut ComputedStyle, s: &ComputedStyle)       { d.background_size = s.background_size; d.background_size_w = s.background_size_w.clone(); d.background_size_h = s.background_size_h.clone(); }
fn copy_background_position(d: &mut ComputedStyle, s: &ComputedStyle)   { d.background_position_x = s.background_position_x.clone(); d.background_position_y = s.background_position_y.clone(); }
fn copy_background_repeat(d: &mut ComputedStyle, s: &ComputedStyle)     { d.background_repeat = s.background_repeat; }
fn copy_background_clip(d: &mut ComputedStyle, s: &ComputedStyle)       { d.background_clip = s.background_clip; }
fn copy_background_origin(d: &mut ComputedStyle, s: &ComputedStyle)     { d.background_origin = s.background_origin; }
fn copy_background_attachment(d: &mut ComputedStyle, s: &ComputedStyle) { d.background_attachment = s.background_attachment; }

fn apply_mask_image(s: &mut ComputedStyle, v: &str) {
    if v == "none" {
        s.mask_image_url.clear();
    } else if let Some(url) = super::extract_url(v) {
        s.mask_image_url = url;
    }
}
fn copy_mask_image(d: &mut ComputedStyle, s: &ComputedStyle) { d.mask_image_url = s.mask_image_url.clone(); }

// ── Outline ─────────────────────────────────────────────────────────────────

fn apply_outline(s: &mut ComputedStyle, v: &str) {
    if v == "none" {
        s.outline_style = BorderStyle::None;
        s.outline_width = 0.0;
    } else {
        for tok in v.split_whitespace() {
            match tok {
                "solid"  => { s.outline_style = BorderStyle::Solid; }
                "dashed" => { s.outline_style = BorderStyle::Dashed; }
                "dotted" => { s.outline_style = BorderStyle::Dotted; }
                "double" => { s.outline_style = BorderStyle::Double; }
                "inset"  => { s.outline_style = BorderStyle::Inset; }
                "outset" => { s.outline_style = BorderStyle::Outset; }
                "groove" => { s.outline_style = BorderStyle::Groove; }
                "ridge"  => { s.outline_style = BorderStyle::Ridge; }
                "none"   => { s.outline_style = BorderStyle::None; }
                _ => {
                    if let CssLength::Px(w) = parse_length(tok) {
                        s.outline_width = w;
                    } else if let Some(c) = parse_color(tok) {
                        s.outline_color = c;
                    }
                }
            }
        }
    }
}
fn apply_outline_style(s: &mut ComputedStyle, v: &str)  { s.outline_style  = super::parse_border_style(v); }
fn apply_outline_color(s: &mut ComputedStyle, v: &str)  { if let Some(c) = parse_color(v) { s.outline_color = c; } }
fn apply_outline_width(s: &mut ComputedStyle, v: &str)  { if let CssLength::Px(w) = parse_length(v) { s.outline_width = w; } }
fn apply_outline_offset(s: &mut ComputedStyle, v: &str) { if let CssLength::Px(w) = parse_length(v) { s.outline_offset = w; } }

fn copy_outline_style(d: &mut ComputedStyle, s: &ComputedStyle)  { d.outline_style  = s.outline_style; }
fn copy_outline_color(d: &mut ComputedStyle, s: &ComputedStyle)  { d.outline_color  = s.outline_color; }
fn copy_outline_width(d: &mut ComputedStyle, s: &ComputedStyle)  { d.outline_width  = s.outline_width; }
fn copy_outline_offset(d: &mut ComputedStyle, s: &ComputedStyle) { d.outline_offset = s.outline_offset; }

// ── Box shadow ──────────────────────────────────────────────────────────────

fn apply_box_shadow(s: &mut ComputedStyle, v: &str) {
    if v == "none" {
        s.box_shadow = None;
    } else {
        let (ox, oy, blur, color) = super::parse_shadow_value(v);
        let toks: Vec<&str> = v.split_whitespace().collect();
        let nums: Vec<f32> = toks.iter()
            .filter_map(|t| { let c = t.trim_start_matches('-').chars().next()?; if c.is_ascii_digit() || c == '.' { t.trim_end_matches("px").parse().ok() } else { None } })
            .collect();
        let spread = nums.get(3).copied().unwrap_or(0.0);
        let inset = v.contains("inset");
        s.box_shadow = Some(BoxShadow { offset_x: ox, offset_y: oy, blur, spread, color, inset });
    }
}
fn copy_box_shadow(d: &mut ComputedStyle, s: &ComputedStyle) { d.box_shadow = s.box_shadow.clone(); }

// ── Pointer events ──────────────────────────────────────────────────────────

fn apply_pointer_events(s: &mut ComputedStyle, v: &str) {
    s.pointer_events = match v {
        "none"           => PointerEvents::None,
        "visiblePainted" => PointerEvents::VisiblePainted,
        "visibleFill"    => PointerEvents::VisibleFill,
        "visibleStroke"  => PointerEvents::VisibleStroke,
        "visible"        => PointerEvents::Visible,
        "painted"        => PointerEvents::Painted,
        "fill"           => PointerEvents::Fill,
        "stroke"         => PointerEvents::Stroke,
        "all"            => PointerEvents::All,
        _                => PointerEvents::Auto,
    };
}
fn copy_pointer_events(d: &mut ComputedStyle, s: &ComputedStyle) { d.pointer_events = s.pointer_events; }

// ── User interaction ────────────────────────────────────────────────────────

fn apply_user_select(s: &mut ComputedStyle, v: &str) {
    s.user_select = match v {
        "none"    => UserSelect::None,
        "text"    => UserSelect::Text,
        "all"     => UserSelect::All,
        "contain" => UserSelect::Contain,
        _         => UserSelect::Auto,
    };
}
fn apply_resize(s: &mut ComputedStyle, v: &str) {
    s.resize = match v {
        "both"       => Resize::Both,
        "horizontal" => Resize::Horizontal,
        "vertical"   => Resize::Vertical,
        _            => Resize::None,
    };
}

fn copy_user_select(d: &mut ComputedStyle, s: &ComputedStyle) { d.user_select = s.user_select; }
fn copy_resize(d: &mut ComputedStyle, s: &ComputedStyle)      { d.resize = s.resize; }

// ── Object fit/position ─────────────────────────────────────────────────────

fn apply_object_fit(s: &mut ComputedStyle, v: &str) {
    s.object_fit = match v {
        "contain"    => ObjectFit::Contain,
        "cover"      => ObjectFit::Cover,
        "none"       => ObjectFit::None,
        "scale-down" => ObjectFit::ScaleDown,
        _            => ObjectFit::Fill,
    };
}
fn apply_object_position(s: &mut ComputedStyle, v: &str) {
    let parts: Vec<&str> = v.split_whitespace().collect();
    s.object_position_x = match parts.first().copied().unwrap_or("50%") {
        "left"   => CssLength::Percent(0.0),
        "center" => CssLength::Percent(50.0),
        "right"  => CssLength::Percent(100.0),
        sv       => parse_length(sv),
    };
    s.object_position_y = match parts.get(1).copied().unwrap_or("50%") {
        "top"    => CssLength::Percent(0.0),
        "center" => CssLength::Percent(50.0),
        "bottom" => CssLength::Percent(100.0),
        sv       => parse_length(sv),
    };
}
fn apply_aspect_ratio(s: &mut ComputedStyle, v: &str) {
    if v == "auto" {
        s.aspect_ratio = None;
    } else if let Some(slash) = v.find('/') {
        let w: f32 = v[..slash].trim().parse().unwrap_or(1.0);
        let h: f32 = v[slash+1..].trim().parse().unwrap_or(1.0);
        if h > 0.0 { s.aspect_ratio = Some(w / h); }
    } else if let Ok(n) = v.trim().parse::<f32>() {
        if n > 0.0 { s.aspect_ratio = Some(n); }
    }
}

fn copy_object_fit(d: &mut ComputedStyle, s: &ComputedStyle)      { d.object_fit = s.object_fit; }
fn copy_object_position(d: &mut ComputedStyle, s: &ComputedStyle) { d.object_position_x = s.object_position_x.clone(); d.object_position_y = s.object_position_y.clone(); }
fn copy_aspect_ratio(d: &mut ComputedStyle, s: &ComputedStyle)    { d.aspect_ratio = s.aspect_ratio; }

// ── Transform / filter ──────────────────────────────────────────────────────

fn apply_transform(s: &mut ComputedStyle, v: &str) {
    s.transform = v.to_string();
    s.css_transform = super::parse_css_transform(v);
}
fn apply_transform_origin(s: &mut ComputedStyle, v: &str) {
    let (ox, oy) = super::parse_transform_origin(v);
    s.transform_origin_x = ox;
    s.transform_origin_y = oy;
}
fn apply_filter(s: &mut ComputedStyle, v: &str) {
    s.filter = v.to_string();
    s.css_filter = super::parse_css_filter(v);
}
fn apply_backdrop_filter(s: &mut ComputedStyle, v: &str) {
    s.backdrop_filter = v.to_string();
}

fn copy_transform(d: &mut ComputedStyle, s: &ComputedStyle)        { d.transform = s.transform.clone(); d.css_transform = s.css_transform.clone(); }
fn copy_transform_origin(d: &mut ComputedStyle, s: &ComputedStyle) { d.transform_origin_x = s.transform_origin_x; d.transform_origin_y = s.transform_origin_y; }
fn copy_filter(d: &mut ComputedStyle, s: &ComputedStyle)           { d.filter = s.filter.clone(); d.css_filter = s.css_filter.clone(); }
fn copy_backdrop_filter(d: &mut ComputedStyle, s: &ComputedStyle)  { d.backdrop_filter = s.backdrop_filter.clone(); }

// ── Transition / animation ──────────────────────────────────────────────────

fn apply_transition(s: &mut ComputedStyle, v: &str) {
    s.transitions = super::parse_transition_shorthand(v);
}
fn ensure_transition(s: &mut ComputedStyle) {
    if s.transitions.is_empty() {
        s.transitions.push(ParsedTransition {
            property: String::new(), duration_ms: 0.0,
            delay_ms: 0.0, timing_fn: EasingFn::Ease,
        });
    }
}
fn apply_transition_property(s: &mut ComputedStyle, v: &str) {
    ensure_transition(s);
    for tr in &mut s.transitions { tr.property = v.to_string(); }
}
fn apply_transition_duration(s: &mut ComputedStyle, v: &str) {
    ensure_transition(s);
    for tr in &mut s.transitions { tr.duration_ms = super::parse_time_ms(v).unwrap_or(0.0); }
}
fn apply_transition_timing_function(s: &mut ComputedStyle, v: &str) {
    ensure_transition(s);
    for tr in &mut s.transitions { tr.timing_fn = super::parse_easing(v); }
}
fn apply_transition_delay(s: &mut ComputedStyle, v: &str) {
    ensure_transition(s);
    for tr in &mut s.transitions { tr.delay_ms = super::parse_time_ms(v).unwrap_or(0.0); }
}

fn apply_animation(s: &mut ComputedStyle, v: &str) {
    s.animations = super::parse_animation_shorthand(v);
}
fn ensure_animation(s: &mut ComputedStyle) {
    if s.animations.is_empty() {
        s.animations.push(ParsedAnimation {
            name: String::new(), duration_ms: 0.0, delay_ms: 0.0,
            timing_fn: EasingFn::Ease, iteration_count: 1.0,
            direction: AnimDirection::Normal, fill_mode: FillMode::None,
            play_state_paused: false,
        });
    }
}
fn apply_animation_name(s: &mut ComputedStyle, v: &str) {
    ensure_animation(s);
    for anim in &mut s.animations { anim.name = v.to_string(); }
}
fn apply_animation_duration(s: &mut ComputedStyle, v: &str) {
    ensure_animation(s);
    for anim in &mut s.animations { anim.duration_ms = super::parse_time_ms(v).unwrap_or(0.0); }
}
fn apply_animation_timing_function(s: &mut ComputedStyle, v: &str) {
    ensure_animation(s);
    for anim in &mut s.animations { anim.timing_fn = super::parse_easing(v); }
}
fn apply_animation_delay(s: &mut ComputedStyle, v: &str) {
    ensure_animation(s);
    for anim in &mut s.animations { anim.delay_ms = super::parse_time_ms(v).unwrap_or(0.0); }
}
fn apply_animation_iteration_count(s: &mut ComputedStyle, v: &str) {
    ensure_animation(s);
    for anim in &mut s.animations { anim.iteration_count = if v == "infinite" { f32::INFINITY } else { v.parse().unwrap_or(1.0) }; }
}
fn apply_animation_direction(s: &mut ComputedStyle, v: &str) {
    ensure_animation(s);
    for anim in &mut s.animations { anim.direction = match v { "reverse" => AnimDirection::Reverse, "alternate" => AnimDirection::Alternate, "alternate-reverse" => AnimDirection::AlternateReverse, _ => AnimDirection::Normal }; }
}
fn apply_animation_fill_mode(s: &mut ComputedStyle, v: &str) {
    ensure_animation(s);
    for anim in &mut s.animations { anim.fill_mode = match v { "forwards" => FillMode::Forwards, "backwards" => FillMode::Backwards, "both" => FillMode::Both, _ => FillMode::None }; }
}
fn apply_animation_play_state(s: &mut ComputedStyle, v: &str) {
    ensure_animation(s);
    for anim in &mut s.animations { anim.play_state_paused = v == "paused"; }
}
fn apply_will_change(s: &mut ComputedStyle, v: &str) {
    s.will_change = v.to_string();
    s.will_change_transform = v.contains("transform");
}

fn copy_transition(d: &mut ComputedStyle, s: &ComputedStyle)  { d.transitions = s.transitions.clone(); }
fn copy_animation(d: &mut ComputedStyle, s: &ComputedStyle)   { d.animations = s.animations.clone(); }
fn copy_will_change(d: &mut ComputedStyle, s: &ComputedStyle) { d.will_change = s.will_change.clone(); d.will_change_transform = s.will_change_transform; }

// ── Unicode-bidi & writing ──────────────────────────────────────────────────

fn apply_unicode_bidi(s: &mut ComputedStyle, v: &str) {
    s.unicode_bidi = match v {
        "normal"           => UnicodeBidi::Normal,
        "embed"            => UnicodeBidi::Embed,
        "bidi-override"    => UnicodeBidi::Override,
        "isolate"          => UnicodeBidi::Isolate,
        "isolate-override" => UnicodeBidi::IsolateOverride,
        "plaintext"        => UnicodeBidi::Plaintext,
        _                  => UnicodeBidi::Normal,
    };
}
fn apply_writing_mode(s: &mut ComputedStyle, v: &str) {
    s.writing_mode = match v {
        "vertical-rl" => WritingMode::VerticalRL,
        "vertical-lr" => WritingMode::VerticalLR,
        _             => WritingMode::HorizontalTB,
    };
}

fn copy_unicode_bidi(d: &mut ComputedStyle, s: &ComputedStyle) { d.unicode_bidi = s.unicode_bidi; }
fn copy_writing_mode(d: &mut ComputedStyle, s: &ComputedStyle) { d.writing_mode = s.writing_mode; }

// ── Hyphens / tab-size / text extras ────────────────────────────────────────

fn apply_tab_size(s: &mut ComputedStyle, v: &str) { s.tab_size = v.parse().unwrap_or(8); }
fn apply_hyphens(s: &mut ComputedStyle, v: &str) {
    s.hyphens = match v {
        "none"   => Hyphens::None,
        "manual" => Hyphens::Manual,
        "auto"   => Hyphens::Auto,
        _        => Hyphens::Manual,
    };
}
fn apply_widows(s: &mut ComputedStyle, v: &str)  { if let Ok(n) = v.parse() { s.widows  = n; } }
fn apply_orphans(s: &mut ComputedStyle, v: &str) { if let Ok(n) = v.parse() { s.orphans = n; } }

fn copy_tab_size(d: &mut ComputedStyle, s: &ComputedStyle) { d.tab_size = s.tab_size; }
fn copy_hyphens(d: &mut ComputedStyle, s: &ComputedStyle)  { d.hyphens = s.hyphens; }
fn copy_widows(d: &mut ComputedStyle, s: &ComputedStyle)   { d.widows = s.widows; }
fn copy_orphans(d: &mut ComputedStyle, s: &ComputedStyle)  { d.orphans = s.orphans; }

// ── Scrollbar & caret ───────────────────────────────────────────────────────

fn apply_scrollbar_color(s: &mut ComputedStyle, v: &str) {
    if v != "auto" {
        let sp = super::find_split_space(v);
        if let Some(idx) = sp {
            let thumb = v[..idx].trim();
            let track = v[idx+1..].trim();
            s.scrollbar_thumb_color = parse_color(thumb);
            s.scrollbar_track_color = parse_color(track);
        }
    }
}
fn apply_caret_color(s: &mut ComputedStyle, v: &str) {
    s.caret_color = if v == "auto" { None } else { parse_color(v) };
}

fn copy_scrollbar_color(d: &mut ComputedStyle, s: &ComputedStyle) { d.scrollbar_thumb_color = s.scrollbar_thumb_color; d.scrollbar_track_color = s.scrollbar_track_color; }
fn copy_caret_color(d: &mut ComputedStyle, s: &ComputedStyle) { d.caret_color = s.caret_color; }

// ── Quotes ──────────────────────────────────────────────────────────────────

fn apply_quotes(s: &mut ComputedStyle, v: &str) {
    s.quotes.clear();
    if v != "none" && v != "auto" {
        let bytes = v.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'"' || bytes[i] == b'\'' {
                let q = bytes[i]; i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != q { i += 1; }
                s.quotes.push(v[start..i].to_string());
                if i < bytes.len() { i += 1; }
            } else { i += 1; }
        }
    }
}

fn copy_quotes(d: &mut ComputedStyle, s: &ComputedStyle) { d.quotes = s.quotes.clone(); }

// ── Container queries ───────────────────────────────────────────────────────

fn apply_container_type(s: &mut ComputedStyle, v: &str) {
    s.container_type = match v {
        "size"        => ContainerType::Size,
        "inline-size" => ContainerType::InlineSize,
        _             => ContainerType::Normal,
    };
}
fn apply_container_name(s: &mut ComputedStyle, v: &str) { s.container_name = v.to_string(); }
fn apply_container(s: &mut ComputedStyle, v: &str) {
    if let Some(slash) = v.find('/') {
        s.container_name = v[..slash].trim().to_string();
        apply_container_type(s, v[slash+1..].trim());
    } else {
        match v {
            "size" | "inline-size" => apply_container_type(s, v),
            _ => s.container_name = v.to_string(),
        }
    }
}

fn copy_container_type(d: &mut ComputedStyle, s: &ComputedStyle) { d.container_type = s.container_type; }
fn copy_container_name(d: &mut ComputedStyle, s: &ComputedStyle) { d.container_name = s.container_name.clone(); }

// ── Clip ────────────────────────────────────────────────────────────────────

fn apply_clip(s: &mut ComputedStyle, v: &str) {
    if v == "auto" || v == "none" {
        s.clip_rect = None;
    } else if let Some(inner) = v.strip_prefix("rect(").and_then(|sv| sv.strip_suffix(')')) {
        let parts: Vec<&str> = if inner.contains(',') {
            inner.split(',').map(|sv| sv.trim()).collect()
        } else {
            inner.split_whitespace().collect()
        };
        if parts.len() == 4 {
            let parse_val = |sv: &str| -> f32 {
                let sv = sv.trim();
                if sv == "auto" { return f32::MAX; }
                if let Some(px) = sv.strip_suffix("px") { return px.trim().parse().unwrap_or(0.0); }
                sv.parse().unwrap_or(0.0)
            };
            s.clip_rect = Some([
                parse_val(parts[0]),
                parse_val(parts[1]),
                parse_val(parts[2]),
                parse_val(parts[3]),
            ]);
        }
    }
}
fn apply_clip_path(s: &mut ComputedStyle, v: &str) {
    if v == "none" {
        s.clip_path = ClipPath::default();
    } else if v.starts_with("inset(") {
        let inner = v[6..v.len().saturating_sub(1)].trim();
        s.clip_path = ClipPath::default();
        s.clip_path.kind = ClipPathKind::Inset;
        let pts: Vec<&str> = inner.split_whitespace().collect();
        s.clip_path.inset_top    = parse_length(pts.first().copied().unwrap_or("0"));
        s.clip_path.inset_right  = parse_length(pts.get(1).copied().unwrap_or(pts.first().copied().unwrap_or("0")));
        s.clip_path.inset_bottom = parse_length(pts.get(2).copied().unwrap_or(pts.first().copied().unwrap_or("0")));
        s.clip_path.inset_left   = parse_length(pts.get(3).copied().unwrap_or(pts.get(1).copied().unwrap_or(pts.first().copied().unwrap_or("0"))));
    } else if v.starts_with("circle(") {
        let inner = v[7..v.len().saturating_sub(1)].trim();
        s.clip_path = ClipPath::default();
        s.clip_path.kind = ClipPathKind::Circle;
        if let Some(at) = inner.find(" at ") {
            s.clip_path.circle_radius = parse_length(&inner[..at]);
            let center: Vec<&str> = inner[at+4..].split_whitespace().collect();
            s.clip_path.center_x = parse_length(center.first().copied().unwrap_or("50%"));
            s.clip_path.center_y = parse_length(center.get(1).copied().unwrap_or(center.first().copied().unwrap_or("50%")));
        } else {
            s.clip_path.circle_radius = parse_length(inner);
            s.clip_path.center_x = CssLength::Percent(50.0);
            s.clip_path.center_y = CssLength::Percent(50.0);
        }
    } else if v.starts_with("ellipse(") {
        let inner = v[8..v.len().saturating_sub(1)].trim();
        s.clip_path = ClipPath::default();
        s.clip_path.kind = ClipPathKind::Ellipse;
        let (radii, center) = if let Some(at) = inner.find(" at ") {
            (&inner[..at], Some(&inner[at+4..]))
        } else { (inner, None) };
        let rv: Vec<&str> = radii.split_whitespace().collect();
        s.clip_path.ellipse_rx = parse_length(rv.first().copied().unwrap_or("50%"));
        s.clip_path.ellipse_ry = parse_length(rv.get(1).copied().unwrap_or(rv.first().copied().unwrap_or("50%")));
        if let Some(c) = center {
            let cv: Vec<&str> = c.split_whitespace().collect();
            s.clip_path.center_x = parse_length(cv.first().copied().unwrap_or("50%"));
            s.clip_path.center_y = parse_length(cv.get(1).copied().unwrap_or(cv.first().copied().unwrap_or("50%")));
        } else {
            s.clip_path.center_x = CssLength::Percent(50.0);
            s.clip_path.center_y = CssLength::Percent(50.0);
        }
    } else if v.starts_with("polygon(") {
        let inner = v[8..v.len().saturating_sub(1)].trim();
        s.clip_path = ClipPath::default();
        s.clip_path.kind = ClipPathKind::Polygon;
        for pair in inner.split(',') {
            let pts: Vec<&str> = pair.trim().split_whitespace().collect();
            if pts.len() >= 2 {
                s.clip_path.points.push((parse_length(pts[0]), parse_length(pts[1])));
            }
        }
    }
}

fn copy_clip(d: &mut ComputedStyle, s: &ComputedStyle)      { d.clip_rect = s.clip_rect; }
fn copy_clip_path(d: &mut ComputedStyle, s: &ComputedStyle)  { d.clip_path = s.clip_path.clone(); }

// ── Break / page-break ──────────────────────────────────────────────────────

fn apply_break_before(s: &mut ComputedStyle, v: &str) {
    s.break_before = match v {
        "always" | "page" => BreakValue::Always,
        "avoid"           => BreakValue::Avoid,
        "left"            => BreakValue::Left,
        "right"           => BreakValue::Right,
        _                 => BreakValue::Auto,
    };
}
fn apply_break_after(s: &mut ComputedStyle, v: &str) {
    s.break_after = match v {
        "always" | "page" => BreakValue::Always,
        "avoid"           => BreakValue::Avoid,
        "left"            => BreakValue::Left,
        "right"           => BreakValue::Right,
        _                 => BreakValue::Auto,
    };
}
fn apply_break_inside(s: &mut ComputedStyle, v: &str) {
    s.break_inside = if v == "avoid" { BreakInside::Avoid } else { BreakInside::Auto };
}

fn copy_break_before(d: &mut ComputedStyle, s: &ComputedStyle) { d.break_before = s.break_before; }
fn copy_break_after(d: &mut ComputedStyle, s: &ComputedStyle)  { d.break_after = s.break_after; }
fn copy_break_inside(d: &mut ComputedStyle, s: &ComputedStyle) { d.break_inside = s.break_inside; }

// ── Multi-column ────────────────────────────────────────────────────────────

fn apply_column_count(s: &mut ComputedStyle, v: &str) {
    s.column_count = if v == "auto" { None } else { v.parse().ok() };
}
fn apply_column_width(s: &mut ComputedStyle, v: &str) { s.column_width = parse_length(v); }
fn apply_columns(s: &mut ComputedStyle, v: &str) {
    for tok in v.split_whitespace() {
        if let Ok(n) = tok.parse::<i32>() {
            s.column_count = Some(n);
        } else {
            s.column_width = parse_length(tok);
        }
    }
}
fn apply_column_rule(s: &mut ComputedStyle, v: &str) {
    super::apply_border_side_shorthand(v,
        &mut s.column_rule_width,
        &mut s.column_rule_style,
        &mut s.column_rule_color);
}
fn apply_column_rule_width(s: &mut ComputedStyle, v: &str) { s.column_rule_width = parse_length(v); }
fn apply_column_rule_style(s: &mut ComputedStyle, v: &str) { s.column_rule_style = super::parse_border_style(v); }
fn apply_column_rule_color(s: &mut ComputedStyle, v: &str) { if let Some(c) = parse_color(v) { s.column_rule_color = c; } }
fn apply_column_fill(s: &mut ComputedStyle, v: &str) { s.column_fill = v == "balance"; }
fn apply_column_span(s: &mut ComputedStyle, v: &str) { s.column_span_all = v == "all"; }

fn copy_column_count(d: &mut ComputedStyle, s: &ComputedStyle)      { d.column_count = s.column_count; }
fn copy_column_width(d: &mut ComputedStyle, s: &ComputedStyle)      { d.column_width = s.column_width.clone(); }
fn copy_column_rule_width(d: &mut ComputedStyle, s: &ComputedStyle) { d.column_rule_width = s.column_rule_width.clone(); }
fn copy_column_rule_style(d: &mut ComputedStyle, s: &ComputedStyle) { d.column_rule_style = s.column_rule_style; }
fn copy_column_rule_color(d: &mut ComputedStyle, s: &ComputedStyle) { d.column_rule_color = s.column_rule_color; }
fn copy_column_fill(d: &mut ComputedStyle, s: &ComputedStyle)       { d.column_fill = s.column_fill; }
fn copy_column_span(d: &mut ComputedStyle, s: &ComputedStyle)       { d.column_span_all = s.column_span_all; }

// ── Counter ─────────────────────────────────────────────────────────────────

fn apply_counter_reset(s: &mut ComputedStyle, v: &str)     { s.counter_reset = super::parse_counter_list(v); }
fn apply_counter_increment(s: &mut ComputedStyle, v: &str) { s.counter_increment = super::parse_counter_list(v); }
fn apply_counter_set(s: &mut ComputedStyle, v: &str)       { s.counter_reset = super::parse_counter_list(v); }

fn copy_counter_reset(d: &mut ComputedStyle, s: &ComputedStyle)     { d.counter_reset = s.counter_reset.clone(); }
fn copy_counter_increment(d: &mut ComputedStyle, s: &ComputedStyle) { d.counter_increment = s.counter_increment.clone(); }

// ── Misc ────────────────────────────────────────────────────────────────────

fn apply_scroll_behavior(s: &mut ComputedStyle, v: &str) {
    s.scroll_behavior = if v == "smooth" { ScrollBehavior::Smooth } else { ScrollBehavior::Auto };
}
fn apply_overscroll_behavior(s: &mut ComputedStyle, v: &str) {
    let val = super::parse_overscroll(v.split_whitespace().next().unwrap_or("auto"));
    s.overscroll_behavior_x = val;
    s.overscroll_behavior_y = val;
}
fn apply_overscroll_behavior_x(s: &mut ComputedStyle, v: &str) { s.overscroll_behavior_x = super::parse_overscroll(v); }
fn apply_overscroll_behavior_y(s: &mut ComputedStyle, v: &str) { s.overscroll_behavior_y = super::parse_overscroll(v); }
fn apply_isolation(s: &mut ComputedStyle, v: &str)     { s.isolation = v == "isolate"; }
fn apply_mix_blend_mode(s: &mut ComputedStyle, v: &str) {
    s.mix_blend_mode = match v {
        "multiply"    => MixBlendMode::Multiply,
        "screen"      => MixBlendMode::Screen,
        "overlay"     => MixBlendMode::Overlay,
        "darken"      => MixBlendMode::Darken,
        "lighten"     => MixBlendMode::Lighten,
        "color-dodge" => MixBlendMode::ColorDodge,
        "color-burn"  => MixBlendMode::ColorBurn,
        "hard-light"  => MixBlendMode::HardLight,
        "soft-light"  => MixBlendMode::SoftLight,
        "difference"  => MixBlendMode::Difference,
        "exclusion"   => MixBlendMode::Exclusion,
        "hue"         => MixBlendMode::Hue,
        "saturation"  => MixBlendMode::Saturation,
        "color"       => MixBlendMode::Color,
        "luminosity"  => MixBlendMode::Luminosity,
        _             => MixBlendMode::Normal,
    };
}

fn copy_scroll_behavior(d: &mut ComputedStyle, s: &ComputedStyle)       { d.scroll_behavior = s.scroll_behavior; }
fn copy_overscroll_behavior_x(d: &mut ComputedStyle, s: &ComputedStyle) { d.overscroll_behavior_x = s.overscroll_behavior_x; }
fn copy_overscroll_behavior_y(d: &mut ComputedStyle, s: &ComputedStyle) { d.overscroll_behavior_y = s.overscroll_behavior_y; }
fn copy_isolation(d: &mut ComputedStyle, s: &ComputedStyle)             { d.isolation = s.isolation; }
fn copy_mix_blend_mode(d: &mut ComputedStyle, s: &ComputedStyle)        { d.mix_blend_mode = s.mix_blend_mode; }

// ── Containment ─────────────────────────────────────────────────────────────

fn apply_contain(s: &mut ComputedStyle, v: &str) {
    let is_strict  = v == "strict";
    let is_content = v == "content";
    s.contain_layout = v.contains("layout") || is_strict || is_content;
    s.contain_paint  = v.contains("paint")  || is_strict || is_content;
    s.contain_size   = v.contains("size")   || is_strict;
}

fn copy_contain(d: &mut ComputedStyle, s: &ComputedStyle) { d.contain_layout = s.contain_layout; d.contain_paint = s.contain_paint; d.contain_size = s.contain_size; }

// ── Scroll snap ─────────────────────────────────────────────────────────────

fn apply_scroll_snap_type(s: &mut ComputedStyle, v: &str) {
    let mut words = v.split_whitespace();
    let axis = match words.next().unwrap_or("none") {
        "x"      => ScrollSnapAxis::X,
        "y"      => ScrollSnapAxis::Y,
        "both"   => ScrollSnapAxis::Both,
        "block"  => ScrollSnapAxis::Block,
        "inline" => ScrollSnapAxis::Inline,
        _        => ScrollSnapAxis::None,
    };
    let mandatory = matches!(words.next().unwrap_or("proximity"), "mandatory");
    s.scroll_snap_type = ScrollSnapType { axis, mandatory };
}
fn apply_scroll_snap_align(s: &mut ComputedStyle, v: &str) {
    s.scroll_snap_align = match v.split_whitespace().next().unwrap_or("none") {
        "start"  => ScrollSnapAlign::Start,
        "end"    => ScrollSnapAlign::End,
        "center" => ScrollSnapAlign::Center,
        _        => ScrollSnapAlign::None,
    };
}
fn apply_scroll_padding(s: &mut ComputedStyle, v: &str) {
    super::apply_shorthand_4(v,
        &mut s.scroll_padding_top, &mut s.scroll_padding_right,
        &mut s.scroll_padding_bottom, &mut s.scroll_padding_left,
        parse_length);
}
fn apply_scroll_padding_top(s: &mut ComputedStyle, v: &str)    { s.scroll_padding_top    = parse_length(v); }
fn apply_scroll_padding_right(s: &mut ComputedStyle, v: &str)  { s.scroll_padding_right  = parse_length(v); }
fn apply_scroll_padding_bottom(s: &mut ComputedStyle, v: &str) { s.scroll_padding_bottom = parse_length(v); }
fn apply_scroll_padding_left(s: &mut ComputedStyle, v: &str)   { s.scroll_padding_left   = parse_length(v); }

fn copy_scroll_snap_type(d: &mut ComputedStyle, s: &ComputedStyle)     { d.scroll_snap_type = s.scroll_snap_type; }
fn copy_scroll_snap_align(d: &mut ComputedStyle, s: &ComputedStyle)    { d.scroll_snap_align = s.scroll_snap_align; }
fn copy_scroll_padding_top(d: &mut ComputedStyle, s: &ComputedStyle)    { d.scroll_padding_top    = s.scroll_padding_top.clone(); }
fn copy_scroll_padding_right(d: &mut ComputedStyle, s: &ComputedStyle)  { d.scroll_padding_right  = s.scroll_padding_right.clone(); }
fn copy_scroll_padding_bottom(d: &mut ComputedStyle, s: &ComputedStyle) { d.scroll_padding_bottom = s.scroll_padding_bottom.clone(); }
fn copy_scroll_padding_left(d: &mut ComputedStyle, s: &ComputedStyle)   { d.scroll_padding_left   = s.scroll_padding_left.clone(); }

// ── Logical properties ──────────────────────────────────────────────────────

fn apply_margin_block(s: &mut ComputedStyle, v: &str)         { let m = parse_length(v); s.margin_top = m.clone(); s.margin_bottom = m; }
fn apply_margin_block_start(s: &mut ComputedStyle, v: &str)   { s.margin_top    = parse_length(v); }
fn apply_margin_block_end(s: &mut ComputedStyle, v: &str)     { s.margin_bottom = parse_length(v); }
fn apply_margin_inline(s: &mut ComputedStyle, v: &str)        { let m = parse_length(v); s.margin_left = m.clone(); s.margin_right = m; }
fn apply_margin_inline_start(s: &mut ComputedStyle, v: &str)  { s.margin_left   = parse_length(v); }
fn apply_margin_inline_end(s: &mut ComputedStyle, v: &str)    { s.margin_right  = parse_length(v); }
fn apply_padding_block(s: &mut ComputedStyle, v: &str)        { let p = parse_length(v); s.padding_top = p.clone(); s.padding_bottom = p; }
fn apply_padding_block_start(s: &mut ComputedStyle, v: &str)  { s.padding_top    = parse_length(v); }
fn apply_padding_block_end(s: &mut ComputedStyle, v: &str)    { s.padding_bottom = parse_length(v); }
fn apply_padding_inline(s: &mut ComputedStyle, v: &str)       { let p = parse_length(v); s.padding_left = p.clone(); s.padding_right = p; }
fn apply_padding_inline_start(s: &mut ComputedStyle, v: &str) { s.padding_left  = parse_length(v); }
fn apply_padding_inline_end(s: &mut ComputedStyle, v: &str)   { s.padding_right = parse_length(v); }
fn apply_inset_block_start(s: &mut ComputedStyle, v: &str)    { s.top    = parse_length(v); }
fn apply_inset_block_end(s: &mut ComputedStyle, v: &str)      { s.bottom = parse_length(v); }
fn apply_inset_inline_start(s: &mut ComputedStyle, v: &str) {
    if s.direction == Direction::RTL { s.right = parse_length(v); }
    else                             { s.left  = parse_length(v); }
}
fn apply_inset_inline_end(s: &mut ComputedStyle, v: &str) {
    if s.direction == Direction::RTL { s.left  = parse_length(v); }
    else                             { s.right = parse_length(v); }
}
fn apply_inset(s: &mut ComputedStyle, v: &str) {
    super::apply_shorthand_4(v,
        &mut s.top, &mut s.right,
        &mut s.bottom, &mut s.left,
        parse_length);
}
fn apply_inset_block(s: &mut ComputedStyle, v: &str)  { let l = parse_length(v); s.top  = l.clone(); s.bottom = l; }
fn apply_inset_inline(s: &mut ComputedStyle, v: &str) { let l = parse_length(v); s.left = l.clone(); s.right  = l; }

// ── Place shorthands ────────────────────────────────────────────────────────

fn apply_place_self(s: &mut ComputedStyle, v: &str) {
    let parts: Vec<&str> = v.splitn(2, ' ').collect();
    apply_align_self(s, parts.first().copied().unwrap_or(v));
    apply_justify_self(s, parts.get(1).copied().unwrap_or(v));
}
fn apply_place_items(s: &mut ComputedStyle, v: &str) {
    let parts: Vec<&str> = v.splitn(2, ' ').collect();
    apply_align_items(s, parts.first().copied().unwrap_or(v));
    apply_justify_items(s, parts.get(1).copied().unwrap_or(v));
}
fn apply_place_content(s: &mut ComputedStyle, v: &str) {
    let parts: Vec<&str> = v.splitn(2, ' ').collect();
    apply_align_content(s, parts.first().copied().unwrap_or(v));
    apply_justify_content(s, parts.get(1).copied().unwrap_or(v));
}

// ── Accent color ────────────────────────────────────────────────────────────

fn apply_accent_color(s: &mut ComputedStyle, v: &str) {
    if let Some(c) = parse_color(v) { s.background_color = c; }
}
