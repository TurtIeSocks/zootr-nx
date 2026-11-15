//! MSBT message files. `LBL1` hashes each label into a bucket with its
//! index into `TXT2`, which holds the texts. Texts keep their inline tags
//! as stored, so a file written back unchanged is the same bytes.

use std::collections::HashMap;

use anyhow::{Context, Result, bail, ensure};

use crate::reader::Reader;

const TAG: u16 = 0x0e;
const CLOSE: u16 = 0x0f;
const HEADER: usize = 0x20;
const LABELS: [u8; 4] = *b"LBL1";
const TEXTS: [u8; 4] = *b"TXT2";

/// A run of one message: text, or an inline tag as stored, marker included.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Text(String),
    Tag(Vec<u8>),
}

pub struct Msbt {
    header: Vec<u8>,
    wide: bool,
    sections: Vec<([u8; 4], Body)>,
}

enum Body {
    /// Hash buckets of label and text index.
    Labels(Vec<Vec<(String, u32)>>),
    /// Each text as stored, terminator included.
    Texts(Vec<Vec<u8>>),
    Other(Vec<u8>),
}

/// Every label's text, tags dropped.
pub fn messages(data: &[u8]) -> Result<HashMap<String, String>> {
    let msbt = Msbt::parse(data)?;
    let mut out = HashMap::new();
    for (label, i) in msbt.buckets()?.iter().flatten() {
        let text = msbt.parts(msbt.text(label, *i)?)?.into_iter();
        let text = text.filter_map(|p| match p {
            Part::Text(s) => Some(s),
            Part::Tag(_) => None,
        });
        out.insert(label.clone(), text.collect());
    }
    Ok(out)
}

/// The bucket `label` belongs in, of `count`.
fn bucket(label: &str, count: usize) -> usize {
    let hash = label
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(0x492).wrapping_add(b.into()));
    hash as usize % count
}

impl Msbt {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let r = Reader(data);
        ensure!(r.bytes(0, 8)? == b"MsgStdBn", "not an MSBT file");
        ensure!(
            r.bytes(8, 2)? == [0xff, 0xfe],
            "big-endian MSBT isn't supported"
        );
        let wide = match r.u8(0x0c)? {
            0 => false,
            1 => true,
            e => bail!("unsupported MSBT encoding {e}"),
        };
        let mut sections = Vec::new();
        let mut at = HEADER;
        for _ in 0..r.u16(0x0e)? {
            let magic: [u8; 4] = r.bytes(at, 4)?.try_into()?;
            let size = r.u32(at + 4)? as usize;
            let body = Reader(r.bytes(at + 16, size)?);
            let body = match magic {
                LABELS => Body::Labels(read_labels(&body)?),
                TEXTS => Body::Texts(read_texts(&body)?),
                _ => Body::Other(body.0.to_vec()),
            };
            sections.push((magic, body));
            at = (at + 16 + size).next_multiple_of(16);
        }
        Ok(Self {
            header: r.bytes(0, HEADER)?.to_vec(),
            wide,
            sections,
        })
    }

    pub fn to_binary(&self) -> Vec<u8> {
        let mut out = self.header.clone();
        for (magic, body) in &self.sections {
            let body = match body {
                Body::Labels(buckets) => write_labels(buckets),
                Body::Texts(texts) => write_texts(texts),
                Body::Other(raw) => raw.clone(),
            };
            out.extend(magic);
            out.extend((body.len() as u32).to_le_bytes());
            out.extend([0; 8]);
            out.extend(body);
            out.resize(out.len().next_multiple_of(16), 0xab);
        }
        let size = (out.len() as u32).to_le_bytes();
        out[0x12..0x16].copy_from_slice(&size);
        out
    }

    /// A label's message, or `None` if there's no such label.
    pub fn get(&self, label: &str) -> Result<Option<Vec<Part>>> {
        match self.index(label)? {
            Some(i) => self.parts(self.text(label, i)?).map(Some),
            None => Ok(None),
        }
    }

    /// Give `label` this message, adding the label if it's new. Adding
    /// refuses files with other sections, like `ATR1`, which would need a
    /// row for it too.
    pub fn set(&mut self, label: &str, parts: &[Part]) -> Result<()> {
        let text = self.encode(parts);
        if let Some(i) = self.index(label)? {
            self.texts_mut()?[i as usize] = text;
            return Ok(());
        }
        if let Some((magic, _)) = self
            .sections
            .iter()
            .find(|(_, b)| matches!(b, Body::Other(_)))
        {
            bail!(
                "adding a message to an MSBT with {} isn't supported",
                String::from_utf8_lossy(magic)
            );
        }
        let texts = self.texts_mut()?;
        let i = texts.len() as u32;
        texts.push(text);
        let buckets = self.buckets_mut()?;
        let n = bucket(label, buckets.len());
        buckets[n].push((label.to_owned(), i));
        Ok(())
    }

    /// Labels outside the bucket their hash picks.
    pub fn misplaced(&self) -> Vec<&str> {
        let Ok(buckets) = self.buckets() else {
            return vec![];
        };
        let at = (0..).zip(buckets);
        at.flat_map(|(n, b)| b.iter().map(move |(label, _)| (n, label.as_str())))
            .filter(|&(n, label)| bucket(label, buckets.len()) != n)
            .map(|(_, label)| label)
            .collect()
    }

    fn section_mut(&mut self, magic: [u8; 4]) -> Option<&mut Body> {
        let found = self.sections.iter_mut().find(|(m, _)| *m == magic);
        found.map(|(_, b)| b)
    }

    fn buckets(&self) -> Result<&Vec<Vec<(String, u32)>>> {
        match self.sections.iter().find(|(m, _)| *m == LABELS) {
            Some((_, Body::Labels(b))) => Ok(b),
            _ => bail!("MSBT has no LBL1"),
        }
    }

    fn buckets_mut(&mut self) -> Result<&mut Vec<Vec<(String, u32)>>> {
        match self.section_mut(LABELS) {
            Some(Body::Labels(b)) => Ok(b),
            _ => bail!("MSBT has no LBL1"),
        }
    }

    fn texts(&self) -> Result<&Vec<Vec<u8>>> {
        match self.sections.iter().find(|(m, _)| *m == TEXTS) {
            Some((_, Body::Texts(t))) => Ok(t),
            _ => bail!("MSBT has no TXT2"),
        }
    }

    fn texts_mut(&mut self) -> Result<&mut Vec<Vec<u8>>> {
        match self.section_mut(TEXTS) {
            Some(Body::Texts(t)) => Ok(t),
            _ => bail!("MSBT has no TXT2"),
        }
    }

    fn index(&self, label: &str) -> Result<Option<u32>> {
        let found = self.buckets()?.iter().flatten().find(|(l, _)| l == label);
        Ok(found.map(|(_, i)| *i))
    }

    /// Text `i`, as stored, for `label`.
    fn text(&self, label: &str, i: u32) -> Result<&[u8]> {
        let text = self.texts()?.get(i as usize);
        text.map(Vec::as_slice)
            .with_context(|| format!("label {label} points past the last text"))
    }

    /// A stored text split into parts, up to its terminator. A tag is its
    /// marker unit, then group, type and params size as u16s, then the
    /// params. A closing tag has no params.
    fn parts(&self, text: &[u8]) -> Result<Vec<Part>> {
        let r = Reader(text);
        let width = if self.wide { 2 } else { 1 };
        let (mut out, mut units, mut at) = (Vec::new(), Vec::new(), 0);
        loop {
            let unit = if self.wide {
                r.u16(at)?
            } else {
                r.u8(at)?.into()
            };
            let len = match unit {
                0 => break,
                TAG => width + 6 + r.u16(at + width + 4)? as usize,
                CLOSE => width + 4,
                u => {
                    units.push(u);
                    at += width;
                    continue;
                }
            };
            self.flush(&mut units, &mut out)?;
            out.push(Part::Tag(r.bytes(at, len)?.to_vec()));
            at += len;
        }
        self.flush(&mut units, &mut out)?;
        Ok(out)
    }

    /// Move `units` into `out` as text, if there are any.
    fn flush(&self, units: &mut Vec<u16>, out: &mut Vec<Part>) -> Result<()> {
        if units.is_empty() {
            return Ok(());
        }
        let text = match self.wide {
            true => String::from_utf16(units)?,
            false => String::from_utf8(units.iter().map(|&u| u as u8).collect())?,
        };
        units.clear();
        out.push(Part::Text(text));
        Ok(())
    }

    fn encode(&self, parts: &[Part]) -> Vec<u8> {
        let mut out = Vec::new();
        for p in parts {
            match p {
                Part::Text(s) if self.wide => {
                    out.extend(s.encode_utf16().flat_map(u16::to_le_bytes))
                }
                Part::Text(s) => out.extend(s.bytes()),
                Part::Tag(raw) => out.extend(raw),
            }
        }
        out.extend(vec![0; if self.wide { 2 } else { 1 }]);
        out
    }
}

fn read_labels(r: &Reader) -> Result<Vec<Vec<(String, u32)>>> {
    let mut out = Vec::new();
    for g in 0..r.u32(0)? as usize {
        let mut at = r.u32(8 + g * 8)? as usize;
        let mut labels = Vec::new();
        for _ in 0..r.u32(4 + g * 8)? {
            let len = r.u8(at)? as usize;
            let name = std::str::from_utf8(r.bytes(at + 1, len)?)?.to_owned();
            labels.push((name, r.u32(at + 1 + len)?));
            at += 1 + len + 4;
        }
        out.push(labels);
    }
    Ok(out)
}

fn write_labels(buckets: &[Vec<(String, u32)>]) -> Vec<u8> {
    let mut table = (buckets.len() as u32).to_le_bytes().to_vec();
    let mut names = Vec::new();
    let start = 4 + 8 * buckets.len();
    for labels in buckets {
        table.extend((labels.len() as u32).to_le_bytes());
        table.extend(((start + names.len()) as u32).to_le_bytes());
        for (name, i) in labels {
            names.push(name.len() as u8);
            names.extend(name.bytes());
            names.extend(i.to_le_bytes());
        }
    }
    table.extend(names);
    table
}

/// Each text runs to the next one's start, the last to the section's end.
fn read_texts(r: &Reader) -> Result<Vec<Vec<u8>>> {
    let n = r.u32(0)? as usize;
    let starts = (0..n).map(|i| Ok(r.u32(4 + i * 4)? as usize));
    let mut starts: Vec<usize> = starts.collect::<Result<_>>()?;
    starts.push(r.0.len());
    let texts = starts
        .windows(2)
        .map(|w| Ok(r.bytes(w[0], w[1].saturating_sub(w[0]))?.to_vec()));
    texts.collect()
}

fn write_texts(texts: &[Vec<u8>]) -> Vec<u8> {
    let mut out = (texts.len() as u32).to_le_bytes().to_vec();
    let mut at = 4 + 4 * texts.len();
    for t in texts {
        out.extend((at as u32).to_le_bytes());
        at += t.len();
    }
    out.extend(texts.concat());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    enum Piece {
        Text(&'static str),
        /// Group, type, params.
        Tag(u16, u16, &'static [u8]),
        Close(u16, u16),
    }
    use Piece::*;

    fn encode(wide: bool, pieces: &[Piece]) -> Vec<u8> {
        let marker = |u: u16| match wide {
            true => u.to_le_bytes().to_vec(),
            false => vec![u as u8],
        };
        let mut out = Vec::new();
        for p in pieces {
            match p {
                Text(s) if wide => out.extend(s.encode_utf16().flat_map(u16::to_le_bytes)),
                Text(s) => out.extend(s.bytes()),
                Tag(g, t, params) => {
                    out.extend(marker(TAG));
                    out.extend([*g, *t, params.len() as u16].map(u16::to_le_bytes).concat());
                    out.extend(*params);
                }
                Close(g, t) => {
                    out.extend(marker(CLOSE));
                    out.extend([*g, *t].map(u16::to_le_bytes).concat());
                }
            }
        }
        out.extend(marker(0));
        out
    }

    /// A one-group MSBT holding `texts` under labels `a`, `b`, ..., with an
    /// empty `ATR1` if `attributes`.
    fn build(wide: bool, attributes: bool, texts: &[&[Piece]]) -> Vec<u8> {
        let section = |magic: &[u8], mut body: Vec<u8>| {
            let mut s = magic.to_vec();
            s.extend((body.len() as u32).to_le_bytes());
            s.extend([0; 8]);
            body.resize(body.len().next_multiple_of(16), 0xab);
            s.extend(body);
            s
        };
        let n = texts.len() as u32;
        let mut lbl = [1, n, 12].map(u32::to_le_bytes).concat();
        for i in 0..n {
            lbl.extend([1, b'a' + i as u8]);
            lbl.extend(i.to_le_bytes());
        }
        let strings: Vec<Vec<u8>> = texts.iter().map(|t| encode(wide, t)).collect();
        let mut txt = n.to_le_bytes().to_vec();
        let mut at = 4 + 4 * n;
        for s in &strings {
            txt.extend(at.to_le_bytes());
            at += s.len() as u32;
        }
        txt.extend(strings.concat());
        let mut out = b"MsgStdBn\xff\xfe\0\0".to_vec();
        out.extend([wide as u8, 3, 2 + attributes as u8, 0]);
        out.resize(0x20, 0);
        out.extend(section(b"LBL1", lbl));
        if attributes {
            out.extend(section(b"ATR1", vec![0; 4]));
        }
        out.extend(section(b"TXT2", txt));
        let size = (out.len() as u32).to_le_bytes();
        out[0x12..0x16].copy_from_slice(&size);
        out
    }

    #[test]
    fn labels_map_to_their_text_without_tags() {
        let tagged = [
            Text("x"),
            Tag(1, 2, &[0, 0x0e]),
            Text("\u{e9}"),
            Close(1, 2),
            Text("z"),
        ];
        for wide in [false, true] {
            let got = messages(&build(wide, true, &[&[Text("first")], &tagged])).unwrap();
            let want = [("a", "first"), ("b", "x\u{e9}z")];
            assert_eq!(got, want.map(|(k, v)| (k.into(), v.into())).into());
        }
    }

    #[test]
    fn unchanged_files_write_back_byte_for_byte() {
        let tagged = [Text("x"), Tag(1, 2, &[7, 0]), Close(1, 2)];
        for wide in [false, true] {
            let data = build(wide, true, &[&[Text("first")], &tagged]);
            assert_eq!(Msbt::parse(&data).unwrap().to_binary(), data);
        }
    }

    #[test]
    fn a_new_label_reads_back_with_its_tags_intact() {
        for wide in [false, true] {
            let mut msbt = Msbt::parse(&build(wide, false, &[&[Text("first")]])).unwrap();
            let marker: &[u8] = if wide { &[0x0e, 0] } else { &[0x0e] };
            let parts = [
                Part::Text("It's ".into()),
                Part::Tag([marker, &[0, 1, 0, 2, 2, 0, 9, 9]].concat()),
                Part::Text(" rupees".into()),
            ];
            msbt.set("new_one", &parts).unwrap();
            msbt.set("a", &[Part::Text("changed".into())]).unwrap();
            let data = msbt.to_binary();
            let back = Msbt::parse(&data).unwrap();
            assert_eq!(back.get("new_one").unwrap().unwrap(), parts);
            assert_eq!(back.to_binary(), data);
            let got = messages(&data).unwrap();
            assert_eq!(got["new_one"], "It's  rupees");
            assert_eq!(got["a"], "changed");
            assert!(back.misplaced().is_empty());
        }
    }

    #[test]
    fn labels_hash_into_buckets_the_msbt_way() {
        assert_eq!(bucket("Enemy_Scrub_Business_Heart_01", 101), 13);
        let data = build(true, false, &[&[Text("x")], &[Text("y")]]);
        let mut msbt = Msbt::parse(&data).unwrap();
        assert!(msbt.misplaced().is_empty());
        if let Some(Body::Labels(buckets)) = msbt.section_mut(LABELS) {
            buckets.push(vec![]);
        }
        assert_eq!(msbt.misplaced(), ["a"]);
    }

    #[test]
    fn adding_a_message_refuses_sections_it_would_leave_behind() {
        let mut msbt = Msbt::parse(&build(true, true, &[&[Text("x")]])).unwrap();
        assert!(msbt.set("new", &[]).is_err());
        assert!(msbt.set("a", &[]).is_ok());
    }

    #[test]
    fn other_files_are_refused() {
        assert!(messages(b"YB\x07\0").is_err());
    }
}
