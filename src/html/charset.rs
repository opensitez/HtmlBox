//! Charset sniffing, and the byte-oriented parse entry points.

#![allow(unused_imports)]
use super::*;
use crate::types::*;
use crate::css::*;

// ─── Charset detection ─────────────────────────────────────────────────────

/// Detect charset from `<meta>` charset declarations and BOM in raw bytes.
fn detect_charset(data: &[u8]) -> &'static str {
    let scan_len = data.len().min(1024);
    let head = &data[..scan_len];

    // Search for charset= in the first 1024 bytes
    let lower: Vec<u8> = head.iter().map(|&b| b.to_ascii_lowercase()).collect();
    if let Some(pos) = find_subsequence(&lower, b"charset") {
        let mut p = pos + 7;
        // Skip whitespace and '='
        while p < scan_len && (lower[p] == b' ' || lower[p] == b'=') { p += 1; }
        // Skip quote
        let quote = if p < scan_len && (head[p] == b'"' || head[p] == b'\'') {
            let q = head[p];
            p += 1;
            Some(q)
        } else {
            None
        };
        let start = p;
        while p < scan_len {
            if let Some(q) = quote {
                if head[p] == q { break; }
            } else if head[p] == b'"' || head[p] == b'\'' || head[p] == b';'
                   || head[p] == b'>' || head[p] == b' ' { break; }
            p += 1;
        }
        if p > start {
            let charset_raw = std::str::from_utf8(&head[start..p]).unwrap_or("");
            let stripped: String = charset_raw.chars()
                .filter(|&c| c != '-' && c != '_')
                .flat_map(|c| c.to_lowercase())
                .collect();
            return match stripped.as_str() {
                "utf8" => "UTF-8",
                "iso88591" | "latin1" => "windows-1252",  // web compat
                "iso88592" => "ISO-8859-2",
                "iso88595" => "ISO-8859-5",
                "iso88596" => "ISO-8859-6",
                "iso88597" => "ISO-8859-7",
                "iso88598" => "ISO-8859-8",
                "iso88599" => "windows-1254",  // web compat
                "iso885915" => "ISO-8859-15",
                "windows1250" => "windows-1250",
                "windows1251" => "windows-1251",
                "windows1252" => "windows-1252",
                "windows1253" => "windows-1253",
                "windows1254" => "windows-1254",
                "windows1255" => "windows-1255",
                "windows1256" => "windows-1256",
                "shiftjis" | "shift_jis" => "Shift_JIS",
                "eucjp" => "EUC-JP",
                "euckr" => "EUC-KR",
                "gb2312" | "gbk" => "GBK",
                "gb18030" => "gb18030",
                "big5" => "Big5",
                "koi8r" => "KOI8-R",
                "usascii" | "ascii" => "UTF-8",
                _ => "UTF-8",
            };
        }
    }

    // Check for UTF-8 BOM
    if data.len() >= 3 && data[0] == 0xEF && data[1] == 0xBB && data[2] == 0xBF {
        return "UTF-8";
    }

    "UTF-8"
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Parse raw bytes with charset auto-detection into a Document.
pub fn parse_html_bytes(data: &[u8]) -> Document {
    parse_html_bytes_with_base(data, "")
}

/// Like `parse_html_bytes` but with a base URL for resolving relative resources.
pub fn parse_html_bytes_with_base(data: &[u8], base_url: &str) -> Document {
    let charset = detect_charset(data);
    let html = if charset == "UTF-8" {
        // Skip BOM if present
        let start = if data.len() >= 3 && data[0] == 0xEF && data[1] == 0xBB && data[2] == 0xBF {
            3
        } else {
            0
        };
        String::from_utf8_lossy(&data[start..]).into_owned()
    } else {
        let encoding = encoding_rs::Encoding::for_label(charset.as_bytes())
            .unwrap_or(encoding_rs::UTF_8);
        let (cow, _, _) = encoding.decode(data);
        cow.into_owned()
    };
    parse_html_with_base(&html, base_url)
}
