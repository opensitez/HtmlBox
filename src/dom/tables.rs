//! The table interfaces — HTML §4.9.1–§4.9.11.

use crate::types::Document;

// ─── The table interfaces — HTML §4.9.1–§4.9.11 ─────────────────────────────
//
// `rows` is the member that shows why these cannot be a plain child walk: it
// is thead's rows, then every tbody's rows in tree order, then tfoot's —
// whatever order the sections appear in the markup. Chrome on a table whose
// `<tfoot>` is written FIRST still answers `h1,r1,r2,r3,f1`.
//
// The index errors are `Option::None` here. Chrome throws `IndexSizeError`;
// this crate has no exception channel, and an out-of-range insert that
// silently appended would be the one outcome no caller can detect.
impl Document {
    /// `table.rows` — HTML §4.9.1, in the spec's order rather than the
    /// document's.
    pub fn table_rows(&self, table: u32) -> Vec<u32> {
        let mut out = Vec::new();
        if let Some(head) = self.t_head(table) {
            out.extend(self.section_rows(head));
        }
        for body in self.t_bodies(table) {
            out.extend(self.section_rows(body));
        }
        // A `<tr>` that is a direct child of the table — which the parser only
        // produces for a fragment — sits with the bodies.
        for child in self.children(table) {
            if self.tag_name(child) == Some("tr") {
                out.push(child);
            }
        }
        if let Some(foot) = self.t_foot(table) {
            out.extend(self.section_rows(foot));
        }
        out
    }

    /// `table.tBodies`.
    pub fn t_bodies(&self, table: u32) -> Vec<u32> {
        self.children(table)
            .into_iter()
            .filter(|c| self.tag_name(*c) == Some("tbody"))
            .collect()
    }

    /// `table.tHead` — the first `<thead>` child, or none.
    pub fn t_head(&self, table: u32) -> Option<u32> {
        self.children(table)
            .into_iter()
            .find(|c| self.tag_name(*c) == Some("thead"))
    }

    /// `table.tFoot`.
    pub fn t_foot(&self, table: u32) -> Option<u32> {
        self.children(table)
            .into_iter()
            .find(|c| self.tag_name(*c) == Some("tfoot"))
    }

    /// `table.caption`.
    pub fn caption(&self, table: u32) -> Option<u32> {
        self.children(table)
            .into_iter()
            .find(|c| self.tag_name(*c) == Some("caption"))
    }

    /// `section.rows` — the `<tr>` children of one `<thead>`/`<tbody>`/`<tfoot>`.
    pub fn section_rows(&self, section: u32) -> Vec<u32> {
        self.children(section)
            .into_iter()
            .filter(|c| self.tag_name(*c) == Some("tr"))
            .collect()
    }

    /// `tr.cells` — the `<td>` and `<th>` children.
    pub fn row_cells(&self, row: u32) -> Vec<u32> {
        self.children(row)
            .into_iter()
            .filter(|c| matches!(self.tag_name(*c), Some("td") | Some("th")))
            .collect()
    }

    /// `tr.rowIndex` — the row's position in its TABLE's `rows`, or −1 when it
    /// is not in a table.
    pub fn row_index(&self, row: u32) -> i32 {
        let Some(table) = self.owning_table(row) else {
            return -1;
        };
        self.table_rows(table)
            .iter()
            .position(|r| *r == row)
            .map_or(-1, |i| i as i32)
    }

    /// `tr.sectionRowIndex` — the row's position within its own section, which
    /// is a different number from `rowIndex` for every row after the first
    /// section.
    pub fn section_row_index(&self, row: u32) -> i32 {
        let Some(parent) = self.parent_element(row) else {
            return -1;
        };
        self.section_rows(parent)
            .iter()
            .position(|r| *r == row)
            .map_or(-1, |i| i as i32)
    }

    /// `td.cellIndex` / `th.cellIndex`.
    pub fn cell_index(&self, cell: u32) -> i32 {
        let Some(row) = self.parent_element(cell) else {
            return -1;
        };
        if self.tag_name(row) != Some("tr") {
            return -1;
        }
        self.row_cells(row)
            .iter()
            .position(|c| *c == cell)
            .map_or(-1, |i| i as i32)
    }

    /// `td.colSpan` — at least 1, at most 1000 (HTML §4.9.11).
    pub fn col_span(&self, cell: u32) -> u32 {
        self.span_attribute(cell, "colspan", 1, 1000)
    }
    pub fn set_col_span(&mut self, cell: u32, span: u32) {
        self.set_attribute(cell, "colspan", &span.to_string());
    }

    /// `td.rowSpan` — at least 0 (0 means "to the end of the section"), at
    /// most 65534.
    pub fn row_span(&self, cell: u32) -> u32 {
        self.span_attribute(cell, "rowspan", 1, 65534)
    }
    pub fn set_row_span(&mut self, cell: u32, span: u32) {
        self.set_attribute(cell, "rowspan", &span.to_string());
    }

    fn span_attribute(&self, cell: u32, name: &str, default: u32, max: u32) -> u32 {
        match self
            .get_attribute(cell, name)
            .and_then(|v| crate::html::forms::parse_non_negative_integer(&v))
        {
            Some(n) if name == "rowspan" => n.min(max),
            Some(n) => n.clamp(1, max),
            None => default,
        }
    }

    /// `table.createCaption()` — returns the existing one if there is one,
    /// which is why it is not simply "create".
    pub fn create_caption(&mut self, table: u32) -> u32 {
        if let Some(existing) = self.caption(table) {
            return existing;
        }
        let caption = self.create_element("caption");
        let first = self.first_child(table).unwrap_or(0);
        if first == 0 {
            self.append_child(table, caption);
        } else {
            self.insert_before(table, caption, first);
        }
        caption
    }

    pub fn delete_caption(&mut self, table: u32) {
        if let Some(c) = self.caption(table) {
            self.remove_child(c);
        }
    }

    /// `table.createTHead()` — idempotent, and placed before every section.
    pub fn create_t_head(&mut self, table: u32) -> u32 {
        if let Some(existing) = self.t_head(table) {
            return existing;
        }
        let head = self.create_element("thead");
        // After any `<caption>` and `<colgroup>`, before the first section.
        let anchor = self
            .children(table)
            .into_iter()
            .find(|c| !matches!(self.tag_name(*c), Some("caption") | Some("colgroup")));
        match anchor {
            Some(a) => self.insert_before(table, head, a),
            None => self.append_child(table, head),
        }
        head
    }

    pub fn delete_t_head(&mut self, table: u32) {
        if let Some(h) = self.t_head(table) {
            self.remove_child(h);
        }
    }

    /// `table.createTFoot()` — idempotent, appended last.
    pub fn create_t_foot(&mut self, table: u32) -> u32 {
        if let Some(existing) = self.t_foot(table) {
            return existing;
        }
        let foot = self.create_element("tfoot");
        self.append_child(table, foot);
        foot
    }

    pub fn delete_t_foot(&mut self, table: u32) {
        if let Some(f) = self.t_foot(table) {
            self.remove_child(f);
        }
    }

    /// `table.createTBody()` — always a NEW one, unlike the other three.
    pub fn create_t_body(&mut self, table: u32) -> u32 {
        let body = self.create_element("tbody");
        match self.t_bodies(table).last() {
            Some(last) => {
                let after = self.next_sibling(*last);
                if after == 0 {
                    self.append_child(table, body);
                } else {
                    self.insert_before(table, body, after);
                }
            }
            None => self.append_child(table, body),
        }
        body
    }

    /// `table.insertRow(index)` — `None` is the spec's `IndexSizeError`.
    ///
    /// The placement rule is the interesting part: the new row goes into the
    /// PARENT of the row currently at `index`, so `insertRow(0)` on a table
    /// with a `<thead>` puts a row in the head, and `insertRow()` appends to
    /// whatever section holds the last row — a `<tfoot>` if there is one.
    /// Chrome demonstrates both.
    pub fn insert_row(&mut self, table: u32, index: i32) -> Option<u32> {
        let rows = self.table_rows(table);
        if index < -1 || index > rows.len() as i32 {
            return None;
        }
        let row = self.create_element("tr");

        if rows.is_empty() {
            let body = match self.t_bodies(table).last() {
                Some(b) => *b,
                None => {
                    let b = self.create_element("tbody");
                    self.append_child(table, b);
                    b
                }
            };
            self.append_child(body, row);
            return Some(row);
        }
        if index == -1 || index == rows.len() as i32 {
            let last = *rows.last().unwrap();
            let parent = self.parent_element(last).unwrap_or(table);
            self.append_child(parent, row);
        } else {
            let reference = rows[index as usize];
            let parent = self.parent_element(reference).unwrap_or(table);
            self.insert_before(parent, row, reference);
        }
        Some(row)
    }

    /// `table.deleteRow(index)` — `false` is `IndexSizeError`.
    pub fn delete_row(&mut self, table: u32, index: i32) -> bool {
        let rows = self.table_rows(table);
        let target = match index {
            -1 => match rows.last() {
                Some(r) => *r,
                None => return false,
            },
            i if i < 0 || i >= rows.len() as i32 => return false,
            i => rows[i as usize],
        };
        self.remove_child(target);
        true
    }

    /// `tbody.insertRow(index)` / `thead.insertRow(index)`.
    pub fn section_insert_row(&mut self, section: u32, index: i32) -> Option<u32> {
        let rows = self.section_rows(section);
        if index < -1 || index > rows.len() as i32 {
            return None;
        }
        let row = self.create_element("tr");
        if index == -1 || index == rows.len() as i32 {
            self.append_child(section, row);
        } else {
            self.insert_before(section, row, rows[index as usize]);
        }
        Some(row)
    }

    /// `tbody.deleteRow(index)`.
    pub fn section_delete_row(&mut self, section: u32, index: i32) -> bool {
        let rows = self.section_rows(section);
        let target = match index {
            -1 => match rows.last() {
                Some(r) => *r,
                None => return false,
            },
            i if i < 0 || i >= rows.len() as i32 => return false,
            i => rows[i as usize],
        };
        self.remove_child(target);
        true
    }

    /// `tr.insertCell(index)` — always a `<td>`, never a `<th>`.
    pub fn insert_cell(&mut self, row: u32, index: i32) -> Option<u32> {
        let cells = self.row_cells(row);
        if index < -1 || index > cells.len() as i32 {
            return None;
        }
        let cell = self.create_element("td");
        if index == -1 || index == cells.len() as i32 {
            self.append_child(row, cell);
        } else {
            self.insert_before(row, cell, cells[index as usize]);
        }
        Some(cell)
    }

    /// `tr.deleteCell(index)`.
    pub fn delete_cell(&mut self, row: u32, index: i32) -> bool {
        let cells = self.row_cells(row);
        let target = match index {
            -1 => match cells.last() {
                Some(c) => *c,
                None => return false,
            },
            i if i < 0 || i >= cells.len() as i32 => return false,
            i => cells[i as usize],
        };
        self.remove_child(target);
        true
    }

    /// The `<table>` a row belongs to, through its section if it has one.
    fn owning_table(&self, row: u32) -> Option<u32> {
        let parent = self.parent_element(row)?;
        match self.tag_name(parent) {
            Some("table") => Some(parent),
            Some("thead") | Some("tbody") | Some("tfoot") => self.parent_element(parent),
            _ => None,
        }
    }
}
