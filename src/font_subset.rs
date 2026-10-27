//! The editor's font subsetter (decision 141): keep only the
//! glyphs a book actually uses. Deliberately conservative —
//! glyph IDs are preserved and unused glyphs are hollowed out
//! (zero-length in `loca`), so cmap, metrics and the OpenType
//! layout tables stay byte-identical and still valid. The win
//! comes from the `glyf` table, which is the large one in any
//! real embedded font.
//!
//! TrueType-`glyf` fonts only, bare or WOFF1-wrapped. CFF and
//! WOFF2 are refused honestly by the caller, never mangled.

use std::collections::{HashMap, HashSet};
use std::io::Read as _;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2)
        .map(|s| u16::from_be_bytes([s[0], s[1]]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn i16_at(b: &[u8], at: usize) -> Option<i16> {
    u16_at(b, at).map(|v| v as i16)
}

fn checksum(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    let (words, rem) = data.as_chunks::<4>();
    for word in words {
        sum = sum.wrapping_add(u32::from_be_bytes(*word));
    }
    if !rem.is_empty() {
        let mut tail = [0u8; 4];
        tail[..rem.len()].copy_from_slice(rem);
        sum = sum.wrapping_add(u32::from_be_bytes(tail));
    }
    sum
}

struct Table {
    tag: [u8; 4],
    data: Vec<u8>,
}

struct Sfnt {
    flavor: u32,
    tables: Vec<Table>,
}

impl Sfnt {
    fn table(&self, tag: &[u8; 4]) -> Option<&[u8]> {
        self.tables
            .iter()
            .find(|t| &t.tag == tag)
            .map(|t| t.data.as_slice())
    }

    fn table_mut(&mut self, tag: &[u8; 4]) -> Option<&mut Vec<u8>> {
        self.tables
            .iter_mut()
            .find(|t| &t.tag == tag)
            .map(|t| &mut t.data)
    }

    fn build(&self) -> Vec<u8> {
        let num = self.tables.len() as u16;
        let mut pow2: u16 = 1;
        let mut selector: u16 = 0;
        while pow2 * 2 <= num {
            pow2 *= 2;
            selector += 1;
        }
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(&self.flavor.to_be_bytes());
        out.extend_from_slice(&num.to_be_bytes());
        out.extend_from_slice(&(pow2 * 16).to_be_bytes());
        out.extend_from_slice(&selector.to_be_bytes());
        out.extend_from_slice(&(num * 16 - pow2 * 16).to_be_bytes());
        let dir_at = out.len();
        out.resize(out.len() + self.tables.len() * 16, 0);
        let mut offsets: Vec<(usize, u32)> = Vec::new();
        for table in &self.tables {
            while !out.len().is_multiple_of(4) {
                out.push(0);
            }
            offsets.push((out.len(), table.data.len() as u32));
            out.extend_from_slice(&table.data);
        }
        for (index, table) in self.tables.iter().enumerate() {
            let (offset, length) = offsets[index];
            let sum = if &table.tag == b"head" {
                // checkSumAdjustment counts as zero in the table
                // checksum.
                let mut data = table.data.clone();
                if data.len() >= 12 {
                    data[8..12].copy_from_slice(&[0, 0, 0, 0]);
                }
                checksum(&data)
            } else {
                checksum(&table.data)
            };
            let rec = dir_at + index * 16;
            out[rec..rec + 4].copy_from_slice(&table.tag);
            out[rec + 4..rec + 8].copy_from_slice(&sum.to_be_bytes());
            out[rec + 8..rec + 12].copy_from_slice(&(offset as u32).to_be_bytes());
            out[rec + 12..rec + 16].copy_from_slice(&length.to_be_bytes());
        }
        // Patch checkSumAdjustment so the whole font sums to the
        // magic constant.
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
        if let Some(head) = self
            .tables
            .iter()
            .position(|t| &t.tag == b"head")
        {
            let (offset, _) = offsets[head];
            if offset + 12 <= out.len() {
                let adjustment = 0xB1B0AFBAu32.wrapping_sub(checksum(&out));
                out[offset + 8..offset + 12]
                    .copy_from_slice(&adjustment.to_be_bytes());
            }
        }
        out
    }
}

fn parse_sfnt(font: &[u8]) -> Result<Sfnt, String> {
    let flavor = u32_at(font, 0).ok_or("not a font file")?;
    if flavor == u32::from_be_bytes(*b"OTTO") {
        return Err("CFF-flavoured OpenType fonts are not supported yet".into());
    }
    if flavor == u32::from_be_bytes(*b"ttcf") {
        return Err("font collections are not supported yet".into());
    }
    if flavor != 0x0001_0000 && flavor != u32::from_be_bytes(*b"true") {
        return Err("not a TrueType font".into());
    }
    let num = u16_at(font, 4).ok_or("truncated font")? as usize;
    let mut tables = Vec::new();
    for i in 0..num {
        let rec = 12 + i * 16;
        let tag_bytes = font
            .get(rec..rec + 4)
            .ok_or("truncated table directory")?;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(tag_bytes);
        let offset = u32_at(font, rec + 8).ok_or("truncated table directory")? as usize;
        let length = u32_at(font, rec + 12).ok_or("truncated table directory")? as usize;
        let data = font
            .get(offset..offset + length)
            .ok_or("truncated font table")?
            .to_vec();
        tables.push(Table { tag, data });
    }
    Ok(Sfnt { flavor, tables })
}

fn parse_woff(font: &[u8]) -> Result<Sfnt, String> {
    let flavor = u32_at(font, 4).ok_or("truncated WOFF")?;
    if flavor == u32::from_be_bytes(*b"OTTO") {
        return Err("CFF-flavoured OpenType fonts are not supported yet".into());
    }
    let num = u16_at(font, 12).ok_or("truncated WOFF")? as usize;
    let mut tables = Vec::new();
    for i in 0..num {
        let rec = 44 + i * 20;
        let tag_bytes = font.get(rec..rec + 4).ok_or("truncated WOFF directory")?;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(tag_bytes);
        let offset = u32_at(font, rec + 4).ok_or("truncated WOFF directory")? as usize;
        let comp = u32_at(font, rec + 8).ok_or("truncated WOFF directory")? as usize;
        let orig = u32_at(font, rec + 12).ok_or("truncated WOFF directory")? as usize;
        let raw = font
            .get(offset..offset + comp)
            .ok_or("truncated WOFF table")?;
        let data = if comp < orig {
            let mut decoder = flate2::read::ZlibDecoder::new(raw);
            let mut out = Vec::with_capacity(orig);
            decoder
                .read_to_end(&mut out)
                .map_err(|e| format!("WOFF table does not decompress: {e}"))?;
            if out.len() != orig {
                return Err("WOFF table size mismatch".into());
            }
            out
        } else {
            raw.to_vec()
        };
        tables.push(Table { tag, data });
    }
    Ok(Sfnt { flavor, tables })
}

fn build_woff(sfnt: &Sfnt, sfnt_bytes: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"wOFF");
    out.extend_from_slice(&sfnt.flavor.to_be_bytes());
    out.extend_from_slice(&[0; 4]); // total length, patched below
    out.extend_from_slice(&(sfnt.tables.len() as u16).to_be_bytes());
    out.extend_from_slice(&[0; 2]); // reserved
    out.extend_from_slice(&(sfnt_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(&[0; 4]); // version (unused)
    out.extend_from_slice(&[0; 12]); // no metadata
    out.extend_from_slice(&[0; 8]); // no private data
    let dir_at = out.len();
    out.resize(out.len() + sfnt.tables.len() * 20, 0);
    for (index, table) in sfnt.tables.iter().enumerate() {
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
        let offset = out.len();
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        let compressed = encoder
            .write_all(&table.data)
            .and_then(|_| encoder.finish())
            .unwrap_or_default();
        let (stored, comp_len) = if !compressed.is_empty()
            && compressed.len() < table.data.len()
        {
            (compressed, 0usize)
        } else {
            (table.data.clone(), table.data.len())
        };
        let comp_len = if comp_len == 0 { stored.len() } else { comp_len };
        out.extend_from_slice(&stored);
        let rec = dir_at + index * 20;
        out[rec..rec + 4].copy_from_slice(&table.tag);
        out[rec + 4..rec + 8].copy_from_slice(&(offset as u32).to_be_bytes());
        out[rec + 8..rec + 12].copy_from_slice(&(comp_len as u32).to_be_bytes());
        out[rec + 12..rec + 16]
            .copy_from_slice(&(table.data.len() as u32).to_be_bytes());
        out[rec + 16..rec + 20]
            .copy_from_slice(&checksum(&table.data).to_be_bytes());
    }
    let total = out.len() as u32;
    out[8..12].copy_from_slice(&total.to_be_bytes());
    out
}

/// Unicode → GID from every format-4/12 cmap subtable.
fn cmap_map(cmap: &[u8]) -> HashMap<u32, u16> {
    let mut map = HashMap::new();
    let Some(count) = u16_at(cmap, 2) else { return map };
    for i in 0..count as usize {
        let rec = 4 + i * 8;
        let Some(offset) = u32_at(cmap, rec + 4) else { continue };
        let Some(sub) = cmap.get(offset as usize..) else { continue };
        match u16_at(sub, 0) {
            Some(4) => cmap_format4(sub, &mut map),
            Some(12) => cmap_format12(sub, &mut map),
            _ => {}
        }
    }
    map
}

fn cmap_format4(sub: &[u8], map: &mut HashMap<u32, u16>) {
    let Some(seg_x2) = u16_at(sub, 6) else { return };
    let seg = (seg_x2 / 2) as usize;
    let end_at = 14;
    let start_at = end_at + seg_x2 as usize + 2;
    let delta_at = start_at + seg_x2 as usize;
    let range_at = delta_at + seg_x2 as usize;
    for s in 0..seg {
        let (Some(end), Some(start), Some(delta), Some(range_off)) = (
            u16_at(sub, end_at + s * 2),
            u16_at(sub, start_at + s * 2),
            i16_at(sub, delta_at + s * 2),
            u16_at(sub, range_at + s * 2),
        ) else {
            continue;
        };
        for code in start..=end {
            let gid = if range_off == 0 {
                (code as i32 + delta as i32).rem_euclid(65536) as u16
            } else {
                let at = range_at + s * 2 + range_off as usize
                    + (code - start) as usize * 2;
                match u16_at(sub, at) {
                    Some(0) | None => continue,
                    Some(g) => (g as i32 + delta as i32).rem_euclid(65536) as u16,
                }
            };
            if gid != 0 {
                map.entry(code as u32).or_insert(gid);
            }
        }
    }
}

fn cmap_format12(sub: &[u8], map: &mut HashMap<u32, u16>) {
    let Some(groups) = u32_at(sub, 12) else { return };
    for g in 0..groups as usize {
        let at = 16 + g * 12;
        let (Some(start), Some(end), Some(first)) = (
            u32_at(sub, at),
            u32_at(sub, at + 4),
            u32_at(sub, at + 8),
        ) else {
            continue;
        };
        for (step, code) in (start..=end).enumerate() {
            let gid = first + step as u32;
            if gid != 0 && gid <= u16::MAX as u32 {
                map.entry(code).or_insert(gid as u16);
            }
        }
    }
}

fn glyph_spans(loca: &[u8], long: bool, num_glyphs: usize) -> Vec<(usize, usize)> {
    let mut spans = Vec::with_capacity(num_glyphs);
    let mut prev: Option<usize> = None;
    for gid in 0..=num_glyphs {
        let at = if long {
            u32_at(loca, gid * 4).map(|v| v as usize)
        } else {
            u16_at(loca, gid * 2).map(|v| v as usize * 2)
        };
        if let (Some(start), Some(end)) = (prev, at) {
            spans.push((start, end));
        }
        prev = at;
    }
    spans
}

/// Component GIDs of a composite glyph (empty for simple glyphs).
fn glyph_components(glyph: &[u8]) -> Vec<u16> {
    let mut out = Vec::new();
    if i16_at(glyph, 0).map(|n| n >= 0).unwrap_or(true) {
        return out;
    }
    let mut at = 10usize;
    while let (Some(flags), Some(component)) =
        (u16_at(glyph, at), u16_at(glyph, at + 2))
    {
        out.push(component);
        at += 4;
        at += if flags & 0x0001 != 0 { 4 } else { 2 };
        if flags & 0x0008 != 0 {
            at += 2;
        } else if flags & 0x0040 != 0 {
            at += 4;
        } else if flags & 0x0080 != 0 {
            at += 8;
        }
        if flags & 0x0020 == 0 {
            break;
        }
    }
    out
}

fn subset_sfnt(font: &[u8], keep_chars: &HashSet<char>) -> Result<Sfnt, String> {
    let mut sfnt = if font.get(..4) == Some(b"wOFF") {
        parse_woff(font)?
    } else if font.get(..4) == Some(b"wOF2") {
        return Err("WOFF2 fonts are not supported yet".into());
    } else {
        parse_sfnt(font)?
    };
    let head = sfnt.table(b"head").ok_or("font has no head table")?;
    let long_loca = i16_at(head, 50).unwrap_or(0) != 0;
    let maxp = sfnt.table(b"maxp").ok_or("font has no maxp table")?;
    let num_glyphs = u16_at(maxp, 4).ok_or("truncated maxp")? as usize;
    let cmap = sfnt
        .table(b"cmap")
        .ok_or("font has no cmap table")?
        .to_vec();
    let loca = sfnt
        .table(b"loca")
        .ok_or("font has no loca table — not a TrueType-glyf font")?
        .to_vec();
    let glyf = sfnt
        .table(b"glyf")
        .ok_or("font has no glyf table — not a TrueType-glyf font")?
        .to_vec();

    let map = cmap_map(&cmap);
    let mut keep: HashSet<u16> = HashSet::new();
    keep.insert(0); // .notdef
    for ch in keep_chars {
        if let Some(gid) = map.get(&(*ch as u32)) {
            keep.insert(*gid);
        }
    }
    let spans = glyph_spans(&loca, long_loca, num_glyphs);
    // Close over composite components.
    let mut work: Vec<u16> = keep.iter().copied().collect();
    while let Some(gid) = work.pop() {
        let Some(&(start, end)) = spans.get(gid as usize) else {
            continue;
        };
        let Some(glyph) = glyf.get(start..end) else {
            continue;
        };
        for component in glyph_components(glyph) {
            if keep.insert(component) {
                work.push(component);
            }
        }
    }

    // Rebuild glyf + loca (always long format) with the same GIDs.
    let mut new_glyf: Vec<u8> = Vec::new();
    let mut new_loca: Vec<u8> = Vec::with_capacity((num_glyphs + 1) * 4);
    for gid in 0..num_glyphs {
        new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
        if keep.contains(&(gid as u16)) {
            if let Some(&(start, end)) = spans.get(gid) {
                if let Some(glyph) = glyf.get(start..end) {
                    new_glyf.extend_from_slice(glyph);
                    while !new_glyf.len().is_multiple_of(4) {
                        new_glyf.push(0);
                    }
                }
            }
        }
    }
    new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());

    if let Some(data) = sfnt.table_mut(b"glyf") {
        *data = new_glyf;
    }
    if let Some(data) = sfnt.table_mut(b"loca") {
        *data = new_loca;
    }
    if let Some(head) = sfnt.table_mut(b"head") {
        if head.len() >= 52 {
            head[50..52].copy_from_slice(&1i16.to_be_bytes()); // long loca
        }
    }
    Ok(sfnt)
}

/// Subset `font` to the glyphs `keep_chars` need (plus `.notdef`
/// and composite components). WOFF1 in → WOFF1 out; bare sfnt
/// in → sfnt out.
pub fn subset_font(font: &[u8], keep_chars: &HashSet<char>) -> Result<Vec<u8>, String> {
    let woff = font.get(..4) == Some(b"wOFF");
    let sfnt = subset_sfnt(font, keep_chars)?;
    let bytes = sfnt.build();
    if woff {
        Ok(build_woff(&sfnt, &bytes))
    } else {
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DM_MONO: &[u8] = include_bytes!("../assets/fonts/DMMono-Regular.ttf");

    fn chars(s: &str) -> HashSet<char> {
        s.chars().collect()
    }

    #[test]
    fn subsetting_shrinks_and_stays_a_font() {
        let out = subset_font(DM_MONO, &chars("AaBb ")).expect("subset");
        assert!(out.len() < DM_MONO.len(), "{} !< {}", out.len(), DM_MONO.len());
        let mut db = fontdb::Database::new();
        db.load_font_data(out.clone());
        assert_eq!(db.faces().count(), 1, "fontdb still parses the subset");
        // The kept letters still have outlines; a dropped one is hollow.
        let reparsed = parse_sfnt(&out).expect("reparse");
        let cmap = reparsed.table(b"cmap").expect("cmap").to_vec();
        let map = cmap_map(&cmap);
        let head = reparsed.table(b"head").expect("head");
        let long = i16_at(head, 50).unwrap_or(0) != 0;
        let maxp = reparsed.table(b"maxp").expect("maxp");
        let n = u16_at(maxp, 4).expect("numGlyphs") as usize;
        let loca = reparsed.table(b"loca").expect("loca").to_vec();
        let spans = glyph_spans(&loca, long, n);
        let a = map[&('A' as u32)] as usize;
        let z = map[&('Z' as u32)] as usize;
        assert!(spans[a].1 > spans[a].0, "A kept its outline");
        assert_eq!(spans[z].1, spans[z].0, "Z was hollowed out");
    }

    #[test]
    fn woff2_and_garbage_are_refused() {
        let mut woff2 = DM_MONO.to_vec();
        woff2[..4].copy_from_slice(b"wOF2");
        assert!(subset_font(&woff2, &chars("A")).is_err());
        assert!(subset_font(b"not a font at all", &chars("A")).is_err());
    }

    #[test]
    fn woff_round_trips_through_the_subsetter() {
        let parsed = parse_sfnt(DM_MONO).expect("parse");
        let wrapped = build_woff(&parsed, DM_MONO);
        let out = subset_font(&wrapped, &chars("Aa")).expect("subset woff");
        assert_eq!(&out[..4], b"wOFF");
        // fontdb reads bare sfnt only — unwrap before asking it.
        let reparsed = parse_woff(&out).expect("reparse woff");
        let mut db = fontdb::Database::new();
        db.load_font_data(reparsed.build());
        assert_eq!(db.faces().count(), 1);
    }
}
