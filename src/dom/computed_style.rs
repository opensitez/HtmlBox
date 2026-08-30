//! `getComputedStyle` — CSSOM §6.6.
//!
//! ⛔ ONE unit on purpose, and moved as one. The resolution has an ORDERING
//! dependency that no signature expresses: the layout-rect path must be tried
//! before the cascaded-style path, because a box with a used size answers from
//! its rect and one without answers from its computed value. A mutation
//! proved the order is load-bearing, so splitting these across modules would
//! put a silent constraint across a file boundary.
//!
//! The serializers are free functions BELOW the `impl`, not interleaved with
//! it — which is how they came to be stranded at the bottom of `api.rs` under
//! a second `impl Document` block.

use crate::types::Document;

impl Document {
    /// `getComputedStyle(element).getPropertyValue(property)` — the RESOLVED
    /// value, in used units.
    ///
    /// Reading this FORCES LAYOUT, as it does in a browser: a document built
    /// but never laid out has no geometry, and `0px` is the absence of an
    /// answer rather than an answer. Everything that is not geometry falls back
    /// to the declared value — an honest floor, since fully resolving every
    /// property against the cascade is a larger job than this.
    pub fn computed_style_property(&mut self, id: u32, property: &str) -> String {
        let width = self.viewport_w;
        crate::layout::LayoutEngine::new().layout(self, width);
        let rect = self.get_bounding_client_rect(id);
        let property = property.to_ascii_lowercase();
        // **An inset is the used value only when the element is POSITIONED**
        // (CSSOM §6.6.1). For a static box `top` has no effect at all, so its
        // resolved value is the COMPUTED one — `1em` on a 16px font is `16px`,
        // wherever the box happens to sit on the page.
        //
        // Answering the bounding rect for every element made `top` mean "how
        // far down the page you are", so a static box inside a body with the
        // UA's 8px margin reported `8px` for a declared `1em` — and moving the
        // box changed a value the box never set.
        let inset = matches!(property.as_str(), "left" | "top" | "right" | "bottom");
        let positioned = self
            .get_computed_style(id)
            .map(|s| !matches!(s.position, crate::types::Position::Static))
            .unwrap_or(false);
        // ⛔ RELATIVE is its own case, not "positioned, so use the rect". A
        // relative box's inset is the OFFSET it was shifted by, which is 0
        // when unset — the rect path answers where the box sits on the page
        // instead (measured: Chrome says `0px`, this said `27.2px`). The two
        // edges also mirror: `bottom` is `-top` and `right` is `-left`
        // (CSS 2.1 §9.4.3), so `top: 10px` makes `bottom` answer `-10px`.
        if inset && matches!(
            self.get_computed_style(id).map(|s| s.position),
            Some(crate::types::Position::Relative)
        ) {
            let Some(style) = self.get_computed_style(id) else { return String::new() };
            let font_px = style.font_size.resolve(16.0, 16.0, 16.0);
            let resolve = |l: &crate::types::CssLength| -> Option<f32> {
                match l {
                    crate::types::CssLength::Auto | crate::types::CssLength::None => None,
                    other => Some(other.resolve(font_px, self.viewport_w, self.root_font_px())),
                }
            };
            let vertical = resolve(&style.top)
                .or_else(|| resolve(&style.bottom).map(|v| -v))
                .unwrap_or(0.0);
            let horizontal = resolve(&style.left)
                .or_else(|| resolve(&style.right).map(|v| -v))
                .unwrap_or(0.0);
            return match property.as_str() {
                "top" => px(vertical),
                "bottom" => px(-vertical),
                "left" => px(horizontal),
                _ => px(-horizontal),
            };
        }
        if inset && !positioned {
            let style = match self.get_computed_style(id) {
                Some(style) => style,
                None => return String::new(),
            };
            let declared = match property.as_str() {
                "left" => style.left.clone(),
                "top" => style.top.clone(),
                "right" => style.right.clone(),
                _ => style.bottom.clone(),
            };
            let font_px = style.font_size.resolve(16.0, 16.0, 16.0);
            // `auto` is the initial value and stays the word — it is not a
            // length and serialising it as `0px` would claim the box was
            // placed. Percentages stay percentages, as the spec's computed
            // value for an inset does.
            return match declared {
                crate::types::CssLength::Auto => "auto".to_string(),
                crate::types::CssLength::Percent(p) => format!("{p}%"),
                other => format!(
                    "{}px",
                    other.resolve(font_px, self.viewport_w, self.root_font_px())
                ),
            };
        }
        let resolved = match (property.as_str(), rect) {
            // A positioned box's inset IS its used value — the offset from the
            // containing block, not from the page. Same number whenever that
            // block is the initial one, which is why this went unnoticed.
            ("left", Some(r)) => Some(format!("{}px", r.x - self.containing_origin(id).0)),
            ("top", Some(r)) => Some(format!("{}px", r.y - self.containing_origin(id).1)),
            // ⛔ A box only HAS a used width if it generates one. A
            // non-replaced `display: inline` box does not, and neither does a
            // `display: none` box — Chrome answers `"auto"` for both, and this
            // answered the rect (`"8.8px"` for a bare `<span>`). A replaced
            // inline is the exception: `<img width=30>` is `"30px"`.
            ("width", Some(r)) if self.has_a_used_size(id) => Some(format!("{}px", r.w)),
            ("height", Some(r)) if self.has_a_used_size(id) => Some(format!("{}px", r.h)),
            _ => None,
        };
        match resolved {
            Some(v) => v,
            // The CASCADED value, before the inline fallback. Without this
            // step `getComputedStyle(el).position` answered `""` for a value
            // that came from a stylesheet — the accessor read the inline style
            // and nothing else, while the right answer sat in `ComputedStyle`
            // the whole time.
            None => match self.resolved_from_cascade(id, &property) {
                Some(v) => v,
                None => self.get_style_property(id, &property).unwrap_or_default(),
            },
        }
    }

    /// Serialize a property from the CASCADED style, Chrome's way.
    ///
    /// `None` means "not in the covered set", and the caller falls back to the
    /// inline style — which is what every property did before this existed.
    /// The set is deliberately explicit rather than open-ended, because a
    /// property that falls through fails SILENTLY: it answers the inline value
    /// or the empty string and looks no different from one that is handled.
    ///
    /// **Not covered, on purpose:**
    ///
    /// * `float` — CSS 2.1 §9.7 computes it to `none` on an absolutely
    ///   positioned box, and Chrome duly answers `"none"` for a `float: left`
    ///   that is also `position: fixed`. That is a COMPUTED-VALUE rule and the
    ///   cascade does not apply it, so answering `"none"` here would leave
    ///   `get_computed_style(id).float` saying `Left` while this said
    ///   `"none"` — two readers of one fact disagreeing. It belongs in the
    ///   cascade.
    /// * the shorthands (`margin`, `border`, `font`, …), which serialize from
    ///   several longhands at once.
    fn resolved_from_cascade(&self, id: u32, property: &str) -> Option<String> {
        use crate::types::*;
        let s = self.get_computed_style(id)?;
        let font_px = s.font_size.resolve(16.0, 16.0, 16.0);
        let vw = self.viewport_w;
        let root_px = self.root_font_px();
        let len = |l: &CssLength| -> String {
            match l {
                CssLength::Auto => "auto".to_string(),
                // The `max-*` initial value is its own variant, and it
                // serializes as the keyword — `resolve` would answer `0px`,
                // which is the opposite of "no limit".
                CssLength::None => "none".to_string(),
                CssLength::Percent(p) => format!("{p}%"),
                other => format!("{}px", other.resolve(font_px, vw, root_px)),
            }
        };
        Some(match property {
            // ⛔ EVERY variant. A `_ => return None` arm here sent the
            // uncovered ones to the inline fallback, so `<button>` — which
            // this crate lays out as a flex box — answered `""` rather than a
            // display at all.
            "display" => match s.display {
                Display::None => "none",
                Display::Block => "block",
                Display::Inline => "inline",
                Display::InlineBlock => "inline-block",
                Display::Flex => "flex",
                Display::InlineFlex => "inline-flex",
                Display::Grid => "grid",
                Display::InlineGrid => "inline-grid",
                Display::Table => "table",
                Display::TableRow => "table-row",
                Display::TableCell => "table-cell",
                Display::TableHeaderCell => "table-cell",
                Display::TableRowGroup => "table-row-group",
                Display::TableHeaderGroup => "table-header-group",
                Display::TableFooterGroup => "table-footer-group",
                Display::TableColumnGroup => "table-column-group",
                Display::TableColumn => "table-column",
                Display::TableCaption => "table-caption",
                Display::ListItem => "list-item",
                Display::Ruby => "ruby",
                Display::RubyText => "ruby-text",
                Display::FlowRoot => "flow-root",
                Display::Contents => "contents",
            }
            .to_string(),
            "position" => match s.position {
                Position::Static => "static",
                Position::Relative => "relative",
                Position::Absolute => "absolute",
                Position::Fixed => "fixed",
                _ => return None,
            }
            .to_string(),
            "color" => serialize_color(s.color),
            "background-color" => serialize_color(s.background_color),
            "font-size" => format!("{font_px}px"),
            // ⛔ A NUMBER, not the keyword: `font-weight: bold` serializes as
            // `"700"` (measured).
            "font-weight" => s.font_weight.value().to_string(),
            "font-style" => match s.font_style {
                FontStyle::Normal => "normal",
                FontStyle::Italic => "italic",
                FontStyle::Oblique => "oblique",
            }
            .to_string(),
            "font-family" => s.font_family.clone(),
            "margin-top" => len(&s.margin_top),
            "margin-right" => len(&s.margin_right),
            "margin-bottom" => len(&s.margin_bottom),
            "margin-left" => len(&s.margin_left),
            "padding-top" => len(&s.padding_top),
            "padding-right" => len(&s.padding_right),
            "padding-bottom" => len(&s.padding_bottom),
            "padding-left" => len(&s.padding_left),
            "border-top-width" => len(&s.border_top_width),
            "border-right-width" => len(&s.border_right_width),
            "border-bottom-width" => len(&s.border_bottom_width),
            "border-left-width" => len(&s.border_left_width),
            "border-top-style" => serialize_border_style(s.border_top_style),
            "border-right-style" => serialize_border_style(s.border_right_style),
            "border-bottom-style" => serialize_border_style(s.border_bottom_style),
            "border-left-style" => serialize_border_style(s.border_left_style),
            "border-top-color" => serialize_color(s.border_top_color),
            "border-right-color" => serialize_color(s.border_right_color),
            "border-bottom-color" => serialize_color(s.border_bottom_color),
            "border-left-color" => serialize_color(s.border_left_color),
            "overflow-x" => serialize_overflow(s.overflow_x),
            "overflow-y" => serialize_overflow(s.overflow_y),
            "box-sizing" => match s.box_sizing {
                BoxSizing::ContentBox => "content-box",
                BoxSizing::BorderBox => "border-box",
            }
            .to_string(),
            "clear" => match s.clear {
                Clear::None => "none",
                Clear::Left => "left",
                Clear::Right => "right",
                Clear::Both => "both",
            }
            .to_string(),
            // ⛔ `min-*` serializes `auto` as `0px` — its initial value is
            // `auto`, which computes to zero outside a flex item, and Chrome
            // answers `"0px"` for both. The crate stores `Px(0)` for one of
            // these and `Auto` for the other, so both roads have to lead here.
            // Reached only when the rect path declined — a non-replaced
            // inline or a `display: none` box, neither of which has a used
            // size. The COMPUTED value is the answer there, and it is `auto`
            // unless the author declared one.
            "width" => len(&s.width),
            "height" => len(&s.height),
            "min-width" => serialize_min(&s.min_width, &len),
            "min-height" => serialize_min(&s.min_height, &len),
            // `max-*` has `none` where the others have `auto`.
            "max-width" => len(&s.max_width),
            "max-height" => len(&s.max_height),
            "opacity" => format!("{}", s.opacity),
            // ⛔ `z_index` is a bare `i32` with no `auto`. Chrome answers
            // `"auto"` when it is unset and the number when it is set, and the
            // cascade stores `0` for both — so a declared `z-index: 0` reads
            // back as `"auto"` here. That single case is wrong; answering the
            // number instead would be wrong for every element that never set
            // one, which is nearly all of them.
            "z-index" => {
                if s.z_index == 0 { "auto".to_string() } else { s.z_index.to_string() }
            }
            _ => return None,
        })
    }
}

/// Zero with the sign stripped. Negating `0.0` yields `-0.0`, which formats
/// as `"-0"` — and a relative box with no offsets answered `"-0px"` for its
/// mirrored edge.
fn px(v: f32) -> String {
    format!("{}px", if v == 0.0 { 0.0 } else { v })
}

/// `rgb(r, g, b)` when opaque, `rgba(r, g, b, a)` otherwise — CSSOM's own
/// serialization, and what Chrome answers. A fully transparent colour is
/// `"rgba(0, 0, 0, 0)"`, never the keyword `transparent` (measured).
fn serialize_color(c: crate::types::Color) -> String {
    if c.a == 255 {
        format!("rgb({}, {}, {})", c.r, c.g, c.b)
    } else {
        let alpha = (c.a as f32 / 255.0 * 100.0).round() / 100.0;
        format!("rgba({}, {}, {}, {})", c.r, c.g, c.b, alpha)
    }
}

fn serialize_border_style(v: crate::types::BorderStyle) -> String {
    use crate::types::BorderStyle as B;
    match v {
        B::None => "none",
        B::Hidden => "hidden",
        B::Solid => "solid",
        B::Dashed => "dashed",
        B::Dotted => "dotted",
        B::Double => "double",
        B::Groove => "groove",
        B::Ridge => "ridge",
        B::Inset => "inset",
        B::Outset => "outset",
    }
    .to_string()
}

/// `min-width` / `min-height`: `auto` is zero outside a flex item.
fn serialize_min(
    l: &crate::types::CssLength,
    len: &dyn Fn(&crate::types::CssLength) -> String,
) -> String {
    match l {
        crate::types::CssLength::Auto | crate::types::CssLength::None => "0px".to_string(),
        other => len(other),
    }
}

fn serialize_overflow(v: crate::types::Overflow) -> String {
    use crate::types::Overflow as O;
    match v {
        O::Visible => "visible",
        O::Hidden => "hidden",
        O::Scroll => "scroll",
        O::Auto => "auto",
    }
    .to_string()
}


impl Document {
    /// Does this element generate a box with a USED width and height?
    ///
    /// A non-replaced `display: inline` box does not — its width is whatever
    /// its content happens to occupy, and CSSOM resolves `width` to the
    /// computed value (`auto`) rather than to a measurement. A `display: none`
    /// box has no measurement at all. Replaced inlines DO have one, which is
    /// why `<img width=30>` answers `"30px"` while a `<span>` answers `"auto"`
    /// (both measured).
    fn has_a_used_size(&self, id: u32) -> bool {
        const REPLACED: &[&str] = &[
            "img", "video", "canvas", "iframe", "embed", "object", "input", "select",
            "textarea", "button", "svg",
        ];
        let Some(style) = self.get_computed_style(id) else { return false };
        match style.display {
            crate::types::Display::None => false,
            crate::types::Display::Inline => {
                self.tag_name(id).is_some_and(|t| REPLACED.contains(&t))
            }
            _ => true,
        }
    }
}
