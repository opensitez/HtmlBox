//! WOFF2 → sfnt.
//!
//! WOFF1 wraps each table in zlib and is a thin container; WOFF2 re-encodes the
//! font. Three things differ and all three have to be undone here:
//!
//!   * one Brotli stream over ALL table data rather than per-table zlib,
//!   * a table directory using variable-length integers and known-tag indices,
//!   * a `glyf`/`loca` transform — outlines are split into parallel streams and
//!     `loca` is discarded, so both have to be rebuilt from the glyphs.
//!
//! Decoding here rather than through a library keeps the font on the same
//! streaming path as every other resource.

/// Byte reader that refuses to run off the end.
struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn new(d: &'a [u8]) -> Self { Self { d, p: 0 } }
    fn left(&self) -> usize { self.d.len().saturating_sub(self.p) }
    fn u8(&mut self) -> Option<u8> {
        let v = *self.d.get(self.p)?; self.p += 1; Some(v)
    }
    fn u16(&mut self) -> Option<u16> {
        let b = self.take(2)?; Some(u16::from_be_bytes([b[0], b[1]]))
    }
    fn i16(&mut self) -> Option<i16> { self.u16().map(|v| v as i16) }
    fn u32(&mut self) -> Option<u32> {
        let b = self.take(4)?; Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.d.get(self.p..self.p + n)?; self.p += n; Some(s)
    }
    /// `UIntBase128` — 1–5 bytes, seven bits each, high bit continues.
    fn base128(&mut self) -> Option<u32> {
        let mut v: u32 = 0;
        for i in 0..5 {
            let b = self.u8()?;
            // Leading zeroes and overflow are both malformed.
            if i == 0 && b == 0x80 { return None }
            if v & 0xfe00_0000 != 0 { return None }
            v = (v << 7) | (b & 0x7f) as u32;
            if b & 0x80 == 0 { return Some(v) }
        }
        None
    }
    /// `255UInt16` — a short integer in one to three bytes.
    fn u255(&mut self) -> Option<u16> {
        const ONE_MORE: u8 = 255;
        const WORD: u8 = 253;
        const LOWEST: u8 = 254;
        let b = self.u8()?;
        match b {
            WORD => self.u16(),
            ONE_MORE => Some(self.u8()? as u16 + LOWEST as u16),
            LOWEST => Some(self.u8()? as u16 + LOWEST as u16 * 2),
            v => Some(v as u16),
        }
    }
}

/// The 63 tags WOFF2 can name by index; 63 means a tag follows literally.
const KNOWN_TAGS: [&[u8; 4]; 63] = [
    b"cmap", b"head", b"hhea", b"hmtx", b"maxp", b"name", b"OS/2", b"post",
    b"cvt ", b"fpgm", b"glyf", b"loca", b"prep", b"CFF ", b"VORG", b"EBDT",
    b"EBLC", b"gasp", b"hdmx", b"kern", b"LTSH", b"PCLT", b"VDMX", b"vhea",
    b"vmtx", b"BASE", b"GDEF", b"GPOS", b"GSUB", b"EBSC", b"JSTF", b"MATH",
    b"CBDT", b"CBLC", b"COLR", b"CPAL", b"SVG ", b"sbix", b"acnt", b"avar",
    b"bdat", b"bloc", b"bsln", b"cvar", b"fdsc", b"feat", b"fmtx", b"fvar",
    b"gvar", b"hsty", b"just", b"lcar", b"mort", b"morx", b"opbd", b"prop",
    b"trak", b"Zapf", b"Silf", b"Glat", b"Gloc", b"Feat", b"Sill",
];

struct TableEntry {
    tag: [u8; 4],
    /// Length of the table once reconstructed.
    orig_len: u32,
    /// Length as stored in the Brotli stream.
    xform_len: u32,
    transformed: bool,
}

/// Decode a WOFF2 file into an sfnt (TrueType/OpenType) the font stack can read.
pub fn decode(data: &[u8]) -> Option<Vec<u8>> {
    let mut r = Reader::new(data);
    if r.take(4)? != b"wOF2" { return None }
    let flavor = r.u32()?;
    let _length = r.u32()?;
    let num_tables = r.u16()? as usize;
    let _reserved = r.u16()?;
    let _total_sfnt_size = r.u32()?;
    let total_compressed = r.u32()? as usize;
    let _major = r.u16()?; let _minor = r.u16()?;
    let _meta_off = r.u32()?; let _meta_len = r.u32()?; let _meta_orig = r.u32()?;
    let _priv_off = r.u32()?; let _priv_len = r.u32()?;
    if num_tables == 0 || num_tables > 4096 { return None }

    // ── Table directory ──────────────────────────────────────────────────────
    let mut dir: Vec<TableEntry> = Vec::with_capacity(num_tables);
    for _ in 0..num_tables {
        let flags = r.u8()?;
        let idx = (flags & 0x3f) as usize;
        let xform_ver = (flags >> 6) & 0x3;
        let tag: [u8; 4] = if idx == 63 {
            let t = r.take(4)?; [t[0], t[1], t[2], t[3]]
        } else {
            **KNOWN_TAGS.get(idx)?
        };
        let orig_len = r.base128()?;
        // `glyf` and `loca` are transformed at version 0 and stored plain at
        // version 3; every other table is the other way round.
        let is_glyf_loca = &tag == b"glyf" || &tag == b"loca";
        let transformed = if is_glyf_loca { xform_ver == 0 } else { xform_ver != 0 };
        let xform_len = if transformed { r.base128()? } else { orig_len };
        dir.push(TableEntry { tag, orig_len, xform_len, transformed });
    }

    // ── One Brotli stream holding every table ────────────────────────────────
    let comp = r.take(total_compressed.min(r.left()))?;
    let mut raw: Vec<u8> = Vec::new();
    {
        use std::io::Read;
        let mut dec = brotli::Decompressor::new(comp, 8192);
        if dec.read_to_end(&mut raw).is_err() { return None }
    }

    // Slice the decompressed stream into the tables, in directory order.
    let mut blobs: Vec<&[u8]> = Vec::with_capacity(dir.len());
    let mut off = 0usize;
    for e in &dir {
        let n = e.xform_len as usize;
        let end = off.checked_add(n)?;
        blobs.push(raw.get(off..end)?);
        off = end;
    }

    // ── Undo the transforms ──────────────────────────────────────────────────
    let mut out_tables: Vec<([u8; 4], Vec<u8>)> = Vec::with_capacity(dir.len());
    let mut rebuilt_loca: Option<Vec<u8>> = None;
    for (i, e) in dir.iter().enumerate() {
        if &e.tag == b"glyf" && e.transformed {
            let index_to_loc = find_index_to_loc(&dir, &blobs);
            let (glyf, loca) = rebuild_glyf(blobs[i], index_to_loc)?;
            rebuilt_loca = Some(loca);
            out_tables.push((e.tag, glyf));
        } else if &e.tag == b"loca" && e.transformed {
            // Rebuilt alongside glyf; filled in below.
            out_tables.push((e.tag, Vec::new()));
        } else {
            let mut v = blobs[i].to_vec();
            v.truncate(e.orig_len as usize);
            out_tables.push((e.tag, v));
        }
    }
    if let Some(loca) = rebuilt_loca {
        for (tag, data) in out_tables.iter_mut() {
            if tag == b"loca" { *data = loca; break }
        }
    }

    Some(build_sfnt(flavor, out_tables))
}

/// `head.indexToLocFormat`, needed to know how wide the rebuilt `loca` is.
fn find_index_to_loc(dir: &[TableEntry], blobs: &[&[u8]]) -> i16 {
    for (i, e) in dir.iter().enumerate() {
        if &e.tag == b"head" {
            if let Some(b) = blobs.get(i) {
                if b.len() >= 52 {
                    return i16::from_be_bytes([b[50], b[51]]);
                }
            }
        }
    }
    0
}

// ─── The glyf transform ───────────────────────────────────────────────────────

/// Rebuild `glyf` and `loca` from the transformed representation.
///
/// The transform splits outlines across parallel streams — contour counts,
/// point counts, flags, coordinate deltas, composites, bounding boxes and
/// instructions — and drops `loca` entirely, so both tables are reconstructed
/// glyph by glyph.
fn rebuild_glyf(data: &[u8], index_to_loc: i16) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut h = Reader::new(data);
    let _reserved = h.u16()?;
    let option_flags = h.u16()?;
    let num_glyphs = h.u16()? as usize;
    let _index_format = h.u16()?;
    let n_contour_size = h.u32()? as usize;
    let n_points_size  = h.u32()? as usize;
    let flag_size      = h.u32()? as usize;
    let glyph_size     = h.u32()? as usize;
    let composite_size = h.u32()? as usize;
    let bbox_size      = h.u32()? as usize;
    let instr_size     = h.u32()? as usize;
    // An optional stream marks simple glyphs whose contours may overlap.
    let overlap_size = if option_flags & 1 != 0 { h.u32()? as usize } else { 0 };

    let base = h.p;
    let mut at = base;
    let mut slice = |n: usize| -> Option<&[u8]> {
        let s = data.get(at..at + n)?; at += n; Some(s)
    };
    let mut n_contour = Reader::new(slice(n_contour_size)?);
    let mut n_points  = Reader::new(slice(n_points_size)?);
    let flags_all     = slice(flag_size)?;
    let mut glyph_str = Reader::new(slice(glyph_size)?);
    let mut composite = Reader::new(slice(composite_size)?);
    let bbox_all      = slice(bbox_size)?;
    let instr_all     = slice(instr_size)?;
    let _overlap      = slice(overlap_size);

    // The bbox stream opens with one bit per glyph saying whether an explicit
    // box follows; composites always set it, simple glyphs usually do not.
    let bitmap_len = (num_glyphs + 7) / 8;
    let bbox_bitmap = bbox_all.get(..bitmap_len)?;
    let mut bbox_vals = Reader::new(bbox_all.get(bitmap_len..)?);

    let mut flags_at = 0usize;
    let mut instr_at = 0usize;
    let mut glyf: Vec<u8> = Vec::with_capacity(data.len() * 2);
    let mut loca: Vec<u32> = Vec::with_capacity(num_glyphs + 1);

    for gid in 0..num_glyphs {
        loca.push(glyf.len() as u32);
        let n = n_contour.i16()?;
        let has_bbox = bbox_bitmap[gid / 8] & (0x80 >> (gid % 8)) != 0;

        if n == 0 {
            // An empty glyph occupies no bytes at all.
            if has_bbox { let _ = bbox_vals.take(8); }
            continue;
        }

        if n < 0 {
            // ── Composite ────────────────────────────────────────────────────
            // Its bounding box is never derived, so it must be present.
            if !has_bbox { return None }
            let b = bbox_vals.take(8)?;
            let start = glyf.len();
            glyf.extend_from_slice(&(-1i16).to_be_bytes());
            glyf.extend_from_slice(b);
            let mut have_instructions = false;
            loop {
                let flags = composite.u16()?;
                let glyph_index = composite.u16()?;
                glyf.extend_from_slice(&flags.to_be_bytes());
                glyf.extend_from_slice(&glyph_index.to_be_bytes());
                // ARG_1_AND_2_ARE_WORDS
                let arg_bytes = if flags & 0x0001 != 0 { 4 } else { 2 };
                glyf.extend_from_slice(composite.take(arg_bytes)?);
                // One of the scale forms may follow.
                let scale_bytes = if flags & 0x0008 != 0 { 2 }        // WE_HAVE_A_SCALE
                    else if flags & 0x0040 != 0 { 4 }                 // X_AND_Y_SCALE
                    else if flags & 0x0080 != 0 { 8 }                 // TWO_BY_TWO
                    else { 0 };
                if scale_bytes > 0 { glyf.extend_from_slice(composite.take(scale_bytes)?); }
                if flags & 0x0100 != 0 { have_instructions = true; }  // WE_HAVE_INSTRUCTIONS
                if flags & 0x0020 == 0 { break }                      // MORE_COMPONENTS
            }
            if have_instructions {
                let len = glyph_str.u255()? as usize;
                glyf.extend_from_slice(&(len as u16).to_be_bytes());
                glyf.extend_from_slice(instr_all.get(instr_at..instr_at + len)?);
                instr_at += len;
            }
            pad4(&mut glyf, start);
            continue;
        }

        // ── Simple glyph ─────────────────────────────────────────────────────
        let n_contours = n as usize;
        let mut end_pts: Vec<u16> = Vec::with_capacity(n_contours);
        let mut total = 0usize;
        for _ in 0..n_contours {
            total += n_points.u255()? as usize;
            if total == 0 || total > 0xffff { return None }
            end_pts.push((total - 1) as u16);
        }

        // Coordinates arrive as (flag, delta) triplets: the flag's low seven
        // bits pick how many bytes follow and how they split between x and y.
        let mut xs: Vec<i16> = Vec::with_capacity(total);
        let mut ys: Vec<i16> = Vec::with_capacity(total);
        let mut on_curve: Vec<bool> = Vec::with_capacity(total);
        let (mut x, mut y) = (0i32, 0i32);
        for _ in 0..total {
            let f = *flags_all.get(flags_at)?; flags_at += 1;
            on_curve.push(f & 0x80 == 0);
            let (dx, dy) = triplet(&mut glyph_str, f & 0x7f)?;
            x += dx; y += dy;
            if x < i16::MIN as i32 || x > i16::MAX as i32 { return None }
            if y < i16::MIN as i32 || y > i16::MAX as i32 { return None }
            xs.push(x as i16); ys.push(y as i16);
        }

        let instr_len = glyph_str.u255()? as usize;
        let instructions = instr_all.get(instr_at..instr_at + instr_len)?;
        instr_at += instr_len;

        let start = glyf.len();
        glyf.extend_from_slice(&(n_contours as i16).to_be_bytes());
        if has_bbox {
            glyf.extend_from_slice(bbox_vals.take(8)?);
        } else {
            // Derived from the points, which is what the encoder omitted it for.
            let x0 = xs.iter().copied().min().unwrap_or(0);
            let y0 = ys.iter().copied().min().unwrap_or(0);
            let x1 = xs.iter().copied().max().unwrap_or(0);
            let y1 = ys.iter().copied().max().unwrap_or(0);
            for v in [x0, y0, x1, y1] { glyf.extend_from_slice(&v.to_be_bytes()); }
        }
        for e in &end_pts { glyf.extend_from_slice(&e.to_be_bytes()); }
        glyf.extend_from_slice(&(instr_len as u16).to_be_bytes());
        glyf.extend_from_slice(instructions);
        write_simple_outline(&mut glyf, &xs, &ys, &on_curve);
        pad4(&mut glyf, start);
    }
    loca.push(glyf.len() as u32);

    // `loca` is short-format when head says so, and each offset is halved.
    let mut loca_bytes = Vec::with_capacity(loca.len() * 4);
    if index_to_loc == 0 {
        for v in &loca {
            if v % 2 != 0 || v / 2 > u16::MAX as u32 { return None }
            loca_bytes.extend_from_slice(&((v / 2) as u16).to_be_bytes());
        }
    } else {
        for v in &loca { loca_bytes.extend_from_slice(&v.to_be_bytes()); }
    }
    Some((glyf, loca_bytes))
}

/// Pad a glyph to a four-byte boundary, as `loca` offsets assume.
fn pad4(buf: &mut Vec<u8>, start: usize) {
    while (buf.len() - start) % 4 != 0 { buf.push(0); }
}

/// One point's (dx, dy) from the triplet encoding (WOFF2 §5.2).
fn triplet(r: &mut Reader<'_>, code: u8) -> Option<(i32, i32)> {
    let c = code as usize;
    if c < 10 {
        // dx is zero; dy is one byte with the sign in the code.
        let b = r.u8()? as i32;
        let dy = ((c & 0x0e) << 7) as i32 + b;
        Some((0, if c & 1 != 0 { dy } else { -dy }))
    } else if c < 20 {
        let b = r.u8()? as i32;
        let dx = (((c - 10) & 0x0e) << 7) as i32 + b;
        Some((if c & 1 != 0 { dx } else { -dx }, 0))
    } else if c < 84 {
        let b = r.u8()? as i32;
        let n = c - 20;
        let dx = 1 + ((n & 0x30) << 2) as i32 + (b >> 4);
        let dy = 1 + (((n & 0x0c) << 4) as i32) + (b & 0x0f);
        Some((sign(dx, n & 0x02 == 0), sign(dy, n & 0x01 == 0)))
    } else if c < 120 {
        let b0 = r.u8()? as i32; let b1 = r.u8()? as i32;
        let n = c - 84;
        let dx = 1 + ((n / 12) << 8) as i32 + b0;
        let dy = 1 + (((n % 12) >> 2) << 8) as i32 + b1;
        Some((sign(dx, n & 0x02 == 0), sign(dy, n & 0x01 == 0)))
    } else if c < 124 {
        let b0 = r.u8()? as i32; let b1 = r.u8()? as i32; let b2 = r.u8()? as i32;
        let n = c - 120;
        let dx = 1 + ((b0 << 4) | (b1 >> 4));
        let dy = 1 + (((b1 & 0x0f) << 8) | b2);
        Some((sign(dx, n & 0x02 == 0), sign(dy, n & 0x01 == 0)))
    } else {
        let b0 = r.u8()? as i32; let b1 = r.u8()? as i32;
        let b2 = r.u8()? as i32; let b3 = r.u8()? as i32;
        let n = c - 124;
        let dx = 1 + ((b0 << 8) | b1);
        let dy = 1 + ((b2 << 8) | b3);
        Some((sign(dx, n & 0x02 == 0), sign(dy, n & 0x01 == 0)))
    }
}

fn sign(v: i32, negative: bool) -> i32 { if negative { -v } else { v } }

/// Write a simple glyph's flags and coordinates in the sfnt encoding.
///
/// The transform stores every delta at full width; the sfnt format packs them,
/// using a short form and a repeat flag. Emitting the long form for everything
/// is valid and keeps the reconstruction honest — no encoder cleverness where a
/// mistake would silently distort outlines.
fn write_simple_outline(buf: &mut Vec<u8>, xs: &[i16], ys: &[i16], on_curve: &[bool]) {
    const ON_CURVE: u8 = 0x01;
    const X_SHORT: u8 = 0x02;
    const Y_SHORT: u8 = 0x04;
    const X_SAME_OR_POSITIVE: u8 = 0x10;
    const Y_SAME_OR_POSITIVE: u8 = 0x20;

    // Deltas between consecutive points, which is what the format stores.
    let n = xs.len();
    let mut dxs: Vec<i32> = Vec::with_capacity(n);
    let mut dys: Vec<i32> = Vec::with_capacity(n);
    let (mut px, mut py) = (0i32, 0i32);
    for i in 0..n {
        dxs.push(xs[i] as i32 - px);
        dys.push(ys[i] as i32 - py);
        px = xs[i] as i32; py = ys[i] as i32;
    }

    for i in 0..n {
        let mut f = if on_curve[i] { ON_CURVE } else { 0 };
        let dx = dxs[i]; let dy = dys[i];
        if dx == 0 { f |= X_SAME_OR_POSITIVE; }
        else if (-255..=255).contains(&dx) {
            f |= X_SHORT;
            if dx > 0 { f |= X_SAME_OR_POSITIVE; }
        }
        if dy == 0 { f |= Y_SAME_OR_POSITIVE; }
        else if (-255..=255).contains(&dy) {
            f |= Y_SHORT;
            if dy > 0 { f |= Y_SAME_OR_POSITIVE; }
        }
        buf.push(f);
    }
    for i in 0..n {
        let dx = dxs[i];
        if dx == 0 { continue }
        if (-255..=255).contains(&dx) { buf.push(dx.unsigned_abs() as u8); }
        else { buf.extend_from_slice(&(dx as i16).to_be_bytes()); }
    }
    for i in 0..n {
        let dy = dys[i];
        if dy == 0 { continue }
        if (-255..=255).contains(&dy) { buf.push(dy.unsigned_abs() as u8); }
        else { buf.extend_from_slice(&(dy as i16).to_be_bytes()); }
    }
}

// ─── sfnt assembly ────────────────────────────────────────────────────────────

/// Assemble the reconstructed tables into an sfnt file.
fn build_sfnt(flavor: u32, mut tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    // The directory is sorted by tag; the data may sit in any order, so it
    // follows the same one.
    tables.sort_by(|a, b| a.0.cmp(&b.0));
    let n = tables.len() as u16;

    // searchRange / entrySelector / rangeShift: the binary-search hints in the
    // header. Wrong values make some parsers reject the font outright.
    let mut entry_selector = 0u16;
    while (1u32 << (entry_selector + 1)) <= n as u32 { entry_selector += 1; }
    let search_range = (1u16 << entry_selector) * 16;
    let range_shift = n * 16 - search_range;

    let header = 12 + 16 * tables.len();
    let mut out = Vec::with_capacity(header + tables.iter().map(|t| t.1.len() + 3).sum::<usize>());
    out.extend_from_slice(&flavor.to_be_bytes());
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    let mut offset = header as u32;
    let mut records: Vec<(u32, u32)> = Vec::with_capacity(tables.len());
    for (_, data) in &tables {
        records.push((offset, data.len() as u32));
        offset += ((data.len() + 3) & !3) as u32;
    }
    for (i, (tag, data)) in tables.iter().enumerate() {
        let (off, len) = records[i];
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum(data).to_be_bytes());
        out.extend_from_slice(&off.to_be_bytes());
        out.extend_from_slice(&len.to_be_bytes());
    }
    for (_, data) in &tables {
        out.extend_from_slice(data);
        while out.len() % 4 != 0 { out.push(0); }
    }
    out
}

/// A table checksum: the sum of its big-endian u32 words, zero-padded.
fn checksum(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    let mut chunks = data.chunks_exact(4);
    for c in &mut chunks {
        sum = sum.wrapping_add(u32::from_be_bytes([c[0], c[1], c[2], c[3]]));
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        let mut last = [0u8; 4];
        last[..rem.len()].copy_from_slice(rem);
        sum = sum.wrapping_add(u32::from_be_bytes(last));
    }
    sum
}
