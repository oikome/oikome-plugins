// Copyright 2025 The Oikome Authors
// SPDX-License-Identifier: Apache-2.0

//! RAW Preview: the JPEG a camera stored inside its RAW file, taken out as
//! it is. Nothing is developed from the sensor's data, and no RAW decoder
//! is used.
//!
//! RAW files built on TIFF (CR2, NEF, ARW, DNG, ORF, PEF and their kin) keep
//! their previews in image directories, which say where each one is. A
//! Fujifilm RAF file is not built on TIFF: its header says where its one
//! JPEG is, and that JPEG carries its own EXIF (its orientation with it). A
//! Canon CR3 file is an ISO media file (boxes, as in MP4): its first track
//! is the full-size JPEG, a smaller one is in a `PRVW` box, and the
//! orientation is in the TIFF directory of its `CMT1` box. When
//! the directories name none (some keep it in the maker's own notes), the
//! file is searched for JPEG images. The largest one that a browser can
//! show is given back: a lossless JPEG, which is how some RAW files hold the
//! sensor's data itself, is not a picture and is passed over.

// The contract with Oikome (`wit/preview.wit`, a copy of the one in the main
// repository's `crates/oikome-sandbox/wit`): the one file a call may read.
#[cfg(target_arch = "wasm32")]
wit_bindgen::generate!({ world: "preview", path: "wit" });
#[cfg(target_arch = "wasm32")]
use oikome::preview::source;

/// The same file, over bytes held here: for the tests, which run on this
/// computer, not in the sandbox.
#[cfg(not(target_arch = "wasm32"))]
mod source {
    use std::cell::RefCell;

    thread_local! {
        pub static FILE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }

    pub fn size() -> u64 {
        FILE.with(|file| file.borrow().len() as u64)
    }

    /// As the host does: no more than 8 MiB per call, fewer at the end.
    pub fn read(offset: u64, len: u32) -> Result<Vec<u8>, String> {
        FILE.with(|file| {
            let file = file.borrow();
            let start = (offset as usize).min(file.len());
            let end = start.saturating_add((len as usize).min(8 << 20)).min(file.len());
            Ok(file[start..end].to_vec())
        })
    }
}

/// The largest preview given back.
const MAX_PREVIEW: u64 = 48 * 1024 * 1024;
/// How much of the file one read asks for.
const BLOCK: u64 = 256 * 1024;
/// Directories read at most: a file that loops is left.
const MAX_DIRECTORIES: usize = 64;
/// JPEG images looked at when the file is searched.
const MAX_FOUND: usize = 48;

/// The file, read through the host a block at a time.
struct File {
    size: u64,
    at: u64,
    block: Vec<u8>,
}

impl File {
    fn open() -> File {
        File { size: source::size(), at: 0, block: Vec::new() }
    }

    /// `len` bytes from `offset`, or `None` past the end.
    fn bytes(&mut self, offset: u64, len: usize) -> Result<Option<&[u8]>, String> {
        let end = match offset.checked_add(len as u64) {
            Some(end) if end <= self.size => end,
            _ => return Ok(None),
        };
        if offset < self.at || end > self.at + self.block.len() as u64 {
            let want = (len as u64).max(BLOCK).min(self.size - offset);
            self.block = read_all(offset, want)?;
            self.at = offset;
            if (self.block.len() as u64) < len as u64 {
                return Ok(None);
            }
        }
        let start = (offset - self.at) as usize;
        Ok(Some(&self.block[start..start + len]))
    }
}

/// `len` bytes from `offset`, asked for in as many reads as the host needs.
fn read_all(offset: u64, len: u64) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(len as usize);
    while (out.len() as u64) < len {
        let rest = len - out.len() as u64;
        let got = source::read(offset + out.len() as u64, rest.min(u64::from(u32::MAX)) as u32)?;
        if got.is_empty() {
            break;
        }
        out.extend(got);
    }
    Ok(out)
}

/// A JPEG image inside the file.
#[derive(Clone, Copy)]
struct Found {
    offset: u64,
    len: u64,
    width: u32,
    height: u32,
}

/// What a JPEG's headers say: its size in pixels, whether a browser shows
/// it (baseline, extended or progressive), and where its headers end.
struct Frame {
    width: u32,
    height: u32,
    shown: bool,
}

/// The frame of the JPEG at `offset`, if one starts there.
fn frame_at(file: &mut File, offset: u64) -> Result<Option<Frame>, String> {
    match file.bytes(offset, 3)? {
        Some([0xFF, 0xD8, 0xFF]) => {}
        _ => return Ok(None),
    }
    let mut at = offset + 2;
    // Segments before the frame: a few at most.
    for _ in 0..64 {
        let Some(head) = file.bytes(at, 4)? else { return Ok(None) };
        if head[0] != 0xFF {
            return Ok(None);
        }
        let marker = head[1];
        let len = u64::from(u16::from_be_bytes([head[2], head[3]]));
        match marker {
            // Fill bytes before a marker.
            0xFF => {
                at += 1;
                continue;
            }
            0xC0..=0xCF if marker != 0xC4 && marker != 0xC8 && marker != 0xCC => {
                let Some(body) = file.bytes(at + 4, 5)? else { return Ok(None) };
                let height = u32::from(u16::from_be_bytes([body[1], body[2]]));
                let width = u32::from(u16::from_be_bytes([body[3], body[4]]));
                return Ok(Some(Frame { width, height, shown: matches!(marker, 0xC0..=0xC2) }));
            }
            // The image's data, or its end, before any frame: not a picture.
            0xDA | 0xD9 => return Ok(None),
            _ if len < 2 => return Ok(None),
            _ => at += 2 + len,
        }
    }
    Ok(None)
}

/// Where the JPEG at `offset` ends, reading no further than `limit`.
fn end_of(file: &mut File, offset: u64, limit: u64) -> Result<Option<u64>, String> {
    let mut at = offset + 2;
    while at < limit {
        let Some(head) = file.bytes(at, 2)? else { return Ok(None) };
        let (first, marker) = (head[0], head[1]);
        if first != 0xFF {
            return Ok(None);
        }
        match marker {
            0xFF => at += 1,
            0xD9 => return Ok(Some(at + 2)),
            0x01 | 0xD0..=0xD7 => at += 2,
            _ => {
                let Some(len) = file.bytes(at + 2, 2)? else { return Ok(None) };
                let len = u64::from(u16::from_be_bytes([len[0], len[1]]));
                if len < 2 {
                    return Ok(None);
                }
                at += 2 + len;
                if marker == 0xDA {
                    // Compressed data: up to the next marker that is not
                    // a stuffed byte or a restart.
                    at = match next_marker(file, at, limit)? {
                        Some(next) => next,
                        None => return Ok(None),
                    };
                }
            }
        }
    }
    Ok(None)
}

/// The first marker from `from` that ends a scan's data.
fn next_marker(file: &mut File, from: u64, limit: u64) -> Result<Option<u64>, String> {
    let mut at = from;
    while at < limit {
        let len = BLOCK.min(file.size - at).min(limit - at) as usize;
        if len < 2 {
            return Ok(None);
        }
        let Some(block) = file.bytes(at, len)? else { return Ok(None) };
        let mut i = 0;
        while i + 1 < block.len() {
            if block[i] == 0xFF && !matches!(block[i + 1], 0x00 | 0xD0..=0xD7 | 0xFF) {
                return Ok(Some(at + i as u64));
            }
            i += 1;
        }
        // The last byte may start a marker: it is read again.
        at += (block.len() - 1) as u64;
    }
    Ok(None)
}

/// A TIFF file's byte order.
#[derive(Clone, Copy)]
struct Order {
    little: bool,
}

impl Order {
    fn u16(self, b: &[u8]) -> u16 {
        if self.little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) }
    }

    fn u32(self, b: &[u8]) -> u32 {
        let b = [b[0], b[1], b[2], b[3]];
        if self.little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) }
    }
}

/// One entry of an image directory: its numbers, up to sixteen.
fn values(file: &mut File, order: Order, entry: &[u8]) -> Result<Vec<u32>, String> {
    let kind = order.u16(&entry[2..4]);
    let count = order.u32(&entry[4..8]) as usize;
    let width = match kind {
        1 | 7 => 1,
        3 => 2,
        4 | 13 => 4,
        _ => return Ok(Vec::new()),
    };
    let count = count.min(16);
    let held;
    let data: &[u8] = if count * width <= 4 {
        &entry[8..12]
    } else {
        let at = u64::from(order.u32(&entry[8..12]));
        match file.bytes(at, count * width)? {
            Some(bytes) => {
                held = bytes.to_vec();
                &held
            }
            None => return Ok(Vec::new()),
        }
    };
    Ok((0..count)
        .map(|i| match width {
            1 => u32::from(data[i]),
            2 => u32::from(order.u16(&data[i * 2..])),
            _ => order.u32(&data[i * 4..]),
        })
        .collect())
}

/// The JPEG images a TIFF file's directories name, and the orientation its
/// first directory gives.
fn from_directories(file: &mut File) -> Result<(Vec<(u64, u64)>, Option<u8>), String> {
    let Some(head) = file.bytes(0, 8)? else { return Ok((Vec::new(), None)) };
    let order = match &head[..2] {
        b"II" => Order { little: true },
        b"MM" => Order { little: false },
        _ => return Ok((Vec::new(), None)),
    };
    let first = u64::from(order.u32(&head[4..8]));
    let mut todo = vec![first];
    let mut seen: Vec<u64> = Vec::new();
    let mut named = Vec::new();
    let mut orientation = None;
    while let Some(at) = todo.pop() {
        if at == 0 || seen.contains(&at) || seen.len() >= MAX_DIRECTORIES {
            continue;
        }
        seen.push(at);
        let Some(count) = file.bytes(at, 2)? else { continue };
        let count = usize::from(order.u16(count)).min(512);
        let Some(entries) = file.bytes(at + 2, count * 12 + 4)?.map(<[u8]>::to_vec) else {
            continue;
        };
        let (mut jpeg_at, mut jpeg_len) = (None, None);
        let (mut strip_at, mut strip_len, mut compression) = (None, None, None);
        for entry in entries[..count * 12].chunks_exact(12) {
            let tag = order.u16(&entry[..2]);
            match tag {
                0x0103 => compression = values(file, order, entry)?.first().copied(),
                0x0111 | 0x0117 => {
                    let all = values(file, order, entry)?;
                    // A preview is one strip; the sensor's data is many.
                    let one = (order.u32(&entry[4..8]) == 1).then(|| all.first().copied()).flatten();
                    if tag == 0x0111 { strip_at = one } else { strip_len = one }
                }
                0x0112 if seen.len() == 1 => {
                    orientation = values(file, order, entry)?.first().map(|v| *v as u8);
                }
                0x0201 => jpeg_at = values(file, order, entry)?.first().copied(),
                0x0202 => jpeg_len = values(file, order, entry)?.first().copied(),
                // Panasonic's JpgFromRaw: the image is the entry's own data.
                0x002E if order.u32(&entry[4..8]) > 4 => {
                    named.push((u64::from(order.u32(&entry[8..12])), u64::from(order.u32(&entry[4..8]))));
                }
                // Directories under this one, and the EXIF directory.
                0x014A | 0x8769 => {
                    todo.extend(values(file, order, entry)?.into_iter().map(u64::from));
                }
                _ => {}
            }
        }
        if let (Some(at), Some(len)) = (jpeg_at, jpeg_len) {
            named.push((u64::from(at), u64::from(len)));
        }
        if let (Some(at), Some(len), Some(6 | 7)) = (strip_at, strip_len, compression) {
            named.push((u64::from(at), u64::from(len)));
        }
        todo.push(u64::from(order.u32(&entries[count * 12..])));
    }
    Ok((named, orientation))
}

/// The JPEG image a Fujifilm RAF file's header names: after the 16-byte
/// signature, the format version (4), the camera's id (8) and name (32) and
/// the directory's version (4) with 20 bytes kept free, the JPEG's offset
/// and length, big-endian, at bytes 84 and 88.
fn from_fuji(file: &mut File) -> Result<Option<(u64, u64)>, String> {
    let Some(head) = file.bytes(0, 92)? else { return Ok(None) };
    if !head.starts_with(b"FUJIFILMCCD-RAW") {
        return Ok(None);
    }
    let offset = u64::from(u32::from_be_bytes([head[84], head[85], head[86], head[87]]));
    let len = u64::from(u32::from_be_bytes([head[88], head[89], head[90], head[91]]));
    Ok(Some((offset, len)))
}

/// One box of an ISO media file: its type and where its content is.
struct Part {
    kind: [u8; 4],
    start: u64,
    end: u64,
}

/// The boxes from `at` to `end`, as far as they are well formed.
fn parts(file: &mut File, mut at: u64, end: u64) -> Result<Vec<Part>, String> {
    let mut out = Vec::new();
    while at.checked_add(8).is_some_and(|head| head <= end) && out.len() < MAX_DIRECTORIES {
        let Some(head) = file.bytes(at, 8)? else { break };
        let short = u64::from(u32::from_be_bytes([head[0], head[1], head[2], head[3]]));
        let kind = [head[4], head[5], head[6], head[7]];
        let (start, size) = match short {
            // To the end of what holds it.
            0 => (at + 8, end - at),
            // A length of 64 bits follows.
            1 => {
                let Some(long) = file.bytes(at + 8, 8)? else { break };
                let mut size = [0u8; 8];
                size.copy_from_slice(long);
                (at + 16, u64::from_be_bytes(size))
            }
            size => (at + 8, size),
        };
        let Some(stop) = at.checked_add(size).filter(|stop| *stop <= end && *stop >= start) else { break };
        out.push(Part { kind, start, end: stop });
        at = stop;
    }
    Ok(out)
}

/// The first box of `kind` from `at` to `end`.
fn part(file: &mut File, at: u64, end: u64, kind: &[u8; 4]) -> Result<Option<Part>, String> {
    Ok(parts(file, at, end)?.into_iter().find(|part| &part.kind == kind))
}

fn be32(file: &mut File, at: u64) -> Result<Option<u64>, String> {
    Ok(file.bytes(at, 4)?.map(|b| u64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))))
}

/// Where a track's first sample is and how long, by its sample table: the
/// first size in `stsz` (or the one size all share) and the first offset in
/// `co64` or `stco`.
fn first_sample(file: &mut File, track: &Part) -> Result<Option<(u64, u64)>, String> {
    let mut within = (track.start, track.end);
    for kind in [b"mdia", b"minf", b"stbl"] {
        let Some(inner) = part(file, within.0, within.1, kind)? else { return Ok(None) };
        within = (inner.start, inner.end);
    }
    let Some(sizes) = part(file, within.0, within.1, b"stsz")? else { return Ok(None) };
    let len = match be32(file, sizes.start + 4)? {
        Some(0) => be32(file, sizes.start + 12)?,
        shared => shared,
    };
    let offset = match part(file, within.0, within.1, b"co64")? {
        Some(offsets) => file.bytes(offsets.start + 8, 8)?.map(|b| {
            let mut offset = [0u8; 8];
            offset.copy_from_slice(b);
            u64::from_be_bytes(offset)
        }),
        None => match part(file, within.0, within.1, b"stco")? {
            Some(offsets) => be32(file, offsets.start + 8)?,
            None => None,
        },
    };
    Ok(offset.zip(len))
}

/// The orientation the first directory of the TIFF at `base` gives.
fn orientation_at(file: &mut File, base: u64) -> Result<Option<u8>, String> {
    let Some(head) = file.bytes(base, 8)? else { return Ok(None) };
    let order = match &head[..2] {
        b"II" => Order { little: true },
        b"MM" => Order { little: false },
        _ => return Ok(None),
    };
    let directory = base + u64::from(order.u32(&head[4..8]));
    let Some(count) = file.bytes(directory, 2)?.map(|b| order.u16(b)) else { return Ok(None) };
    for i in 0..u64::from(count.min(64)) {
        let Some(entry) = file.bytes(directory + 2 + 12 * i, 12)? else { break };
        if order.u16(&entry[..2]) == 0x0112 {
            return Ok(u8::try_from(order.u16(&entry[8..10])).ok());
        }
    }
    Ok(None)
}

/// The JPEG images a Canon CR3 file names, and its orientation: each
/// track's first sample (the first track is the full-size JPEG; the tracks
/// of the sensor's data are not JPEG and are passed over later), and the
/// preview in the `PRVW` box (after its 16 bytes of header, its length in
/// the last four of them).
fn from_canon(file: &mut File) -> Result<Option<(Vec<(u64, u64)>, Option<u8>)>, String> {
    if file.bytes(4, 8)?.is_none_or(|brand| brand != b"ftypcrx ") {
        return Ok(None);
    }
    let size = file.size;
    let mut named = Vec::new();
    let mut orientation = None;
    for top in parts(file, 0, size)? {
        match &top.kind {
            b"moov" => {
                for inner in parts(file, top.start, top.end)? {
                    match &inner.kind {
                        b"trak" => named.extend(first_sample(file, &inner)?),
                        // Canon's own box: 16 bytes of its id, then its boxes.
                        b"uuid" => {
                            if let Some(notes) = part(file, inner.start + 16, inner.end, b"CMT1")? {
                                orientation = orientation_at(file, notes.start)?;
                            }
                        }
                        _ => {}
                    }
                }
            }
            // The preview's box: its id, then (in the files seen) 8 bytes more.
            b"uuid" => {
                for skip in [24, 16] {
                    if let Some(preview) = part(file, top.start + skip, top.end, b"PRVW")? {
                        if let Some(len) = be32(file, preview.start + 12)? {
                            named.push((preview.start + 16, len));
                        }
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(Some((named, orientation)))
}

/// The JPEG images found by reading the file through.
fn by_searching(file: &mut File) -> Result<Vec<(u64, u64)>, String> {
    let mut found = Vec::new();
    let mut at = 0u64;
    while at + 3 <= file.size && found.len() < MAX_FOUND {
        let len = BLOCK.min(file.size - at) as usize;
        let Some(block) = file.bytes(at, len)? else { break };
        let hit = block.windows(3).position(|w| w == [0xFF, 0xD8, 0xFF]);
        let Some(hit) = hit else {
            if len < 3 {
                break;
            }
            at += (len - 2) as u64;
            continue;
        };
        let start = at + hit as u64;
        let limit = file.size.min(start + MAX_PREVIEW);
        match end_of(file, start, limit)? {
            // The images inside it (its own thumbnail) are passed with it.
            Some(end) => {
                found.push((start, end - start));
                at = end;
            }
            None => at = start + 3,
        }
    }
    Ok(found)
}

/// The picture stored in the file, as it is, and the orientation to show
/// it in when the file says one apart from the picture.
fn stored() -> Result<(Vec<u8>, Option<u8>), String> {
    let mut file = File::open();
    let (mut named, orientation) = match from_fuji(&mut file)? {
        Some(named) => (vec![named], None),
        None => match from_canon(&mut file)? {
            Some(found) => found,
            None => from_directories(&mut file)?,
        },
    };
    // A header that names nothing usable: the file is read through.
    let size = file.size;
    let usable =
        |&(offset, len): &(u64, u64)| len >= 4 && offset.checked_add(len).is_some_and(|end| end <= size);
    named.retain(usable);
    if named.is_empty() {
        named = by_searching(&mut file)?;
    }
    let mut best: Option<Found> = None;
    for (offset, len) in named {
        if len < 4 || len > MAX_PREVIEW || offset.checked_add(len).is_none_or(|end| end > file.size) {
            continue;
        }
        let Some(frame) = frame_at(&mut file, offset)? else { continue };
        if !frame.shown {
            continue;
        }
        let found = Found { offset, len, width: frame.width, height: frame.height };
        let area = |f: &Found| (u64::from(f.width) * u64::from(f.height), f.len);
        if best.as_ref().is_none_or(|b| area(&found) > area(b)) {
            best = Some(found);
        }
    }
    let best = best.ok_or("this file holds no preview image")?;
    let bytes = read_all(best.offset, best.len)?;
    if bytes.len() as u64 != best.len {
        return Err("the preview image is cut short".into());
    }
    Ok((bytes, orientation.filter(|o| (1..=8).contains(o))))
}

#[cfg(target_arch = "wasm32")]
struct Plugin;

#[cfg(target_arch = "wasm32")]
impl Guest for Plugin {
    fn render(_format: String) -> Result<Image, String> {
        let (bytes, orientation) = stored()?;
        Ok(Image {
            content_type: "image/jpeg".into(),
            bytes,
            origin: oikome::preview::types::Origin::Embedded,
            orientation,
        })
    }
}

#[cfg(target_arch = "wasm32")]
export!(Plugin);

#[cfg(test)]
mod tests {
    use super::{source, stored};

    /// A JPEG's structure: headers a reader walks, `data` bytes of scan, its
    /// end. `frame` is its kind (0xC0 baseline, 0xC3 lossless); `inside` goes
    /// into an application segment, as a preview's own thumbnail does.
    fn jpeg(frame: u8, width: u16, height: u16, data: usize, inside: &[u8]) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8];
        if !inside.is_empty() {
            out.extend([0xFF, 0xE1]);
            out.extend(((inside.len() + 2) as u16).to_be_bytes());
            out.extend(inside);
        }
        out.extend([0xFF, frame, 0x00, 0x0B, 8]);
        out.extend(height.to_be_bytes());
        out.extend(width.to_be_bytes());
        out.extend([1, 1, 0x11, 0]);
        out.extend([0xFF, 0xDA, 0x00, 0x08, 1, 1, 0, 0, 63, 0]);
        // Scan data with a stuffed byte and a restart marker in it.
        out.extend([0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0]);
        out.extend(std::iter::repeat_n(0x55, data));
        out.extend([0xFF, 0xD9]);
        out
    }

    /// One directory entry holding a single number.
    fn entry(tag: u16, long: bool, value: u32) -> Vec<u8> {
        let mut out = tag.to_le_bytes().to_vec();
        out.extend(if long { 4u16 } else { 3u16 }.to_le_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend(value.to_le_bytes());
        out
    }

    fn directory(entries: &[Vec<u8>], next: u32) -> Vec<u8> {
        let mut out = (entries.len() as u16).to_le_bytes().to_vec();
        for e in entries {
            out.extend(e);
        }
        out.extend(next.to_le_bytes());
        out
    }

    /// A RAW file as cameras write them on TIFF: a small preview in the first
    /// directory, the sensor's data as a lossless JPEG in a directory under it,
    /// and the full-size preview in another, turned on its side.
    fn tiff_raw() -> (Vec<u8>, Vec<u8>) {
        let small = jpeg(0xC0, 160, 120, 64, &[]);
        let sensor = jpeg(0xC3, 6000, 4000, 4096, &[]);
        let full = jpeg(0xC2, 3000, 2000, 512, &jpeg(0xC0, 160, 120, 16, &[]));
        // Header (8), first directory (4 entries: 2 + 48 + 4 = 54), two more
        // (3 entries each: 42), then the images.
        let (first, second, third) = (8u32, 62u32, 104u32);
        let small_at = 146u32;
        let sensor_at = small_at + small.len() as u32;
        let full_at = sensor_at + sensor.len() as u32;
        let mut file = b"II\x2A\x00".to_vec();
        file.extend(first.to_le_bytes());
        file.extend(directory(
            &[
                entry(0x0112, false, 6),
                entry(0x014A, true, second),
                entry(0x0201, true, small_at),
                entry(0x0202, true, small.len() as u32),
            ],
            third,
        ));
        file.extend(directory(
            &[
                entry(0x0103, false, 7),
                entry(0x0111, true, sensor_at),
                entry(0x0117, true, sensor.len() as u32),
            ],
            0,
        ));
        file.extend(directory(
            &[
                entry(0x0103, false, 6),
                entry(0x0111, true, full_at),
                entry(0x0117, true, full.len() as u32),
            ],
            // A directory that points back at the first: a loop to be left.
            first,
        ));
        assert_eq!(file.len() as u32, small_at);
        file.extend(&small);
        file.extend(&sensor);
        file.extend(&full);
        (file, full)
    }

    /// A box of an ISO media file.
    fn part(kind: &[u8; 4], content: &[u8]) -> Vec<u8> {
        let mut out = ((content.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend(kind);
        out.extend(content);
        out
    }

    /// A track whose one sample is `len` bytes at `offset`.
    fn track(offset: u64, len: u32) -> Vec<u8> {
        let sizes = [&[0u8; 8][..], &1u32.to_be_bytes(), &len.to_be_bytes()].concat();
        let offsets = [&[0u8; 4][..], &1u32.to_be_bytes(), &offset.to_be_bytes()].concat();
        let table = [
            part(b"stsd", &[0; 8]),
            part(b"stsz", &sizes),
            part(b"co64", &offsets),
        ]
        .concat();
        let media = [
            part(b"hdlr", &[0; 24]),
            part(b"minf", &part(b"stbl", &table)),
        ]
        .concat();
        part(
            b"trak",
            &[part(b"tkhd", &[0; 84]), part(b"mdia", &media)].concat(),
        )
    }

    /// A Canon CR3 file's structure: its notes (orientation 8), a track for the
    /// full-size picture (when `with_full`) and one for the sensor's data, the
    /// smaller preview in its own box, then the data. Gives the file, the
    /// full-size picture and the preview.
    fn canon_raw(with_full: bool) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let full = jpeg(0xC0, 6000, 4000, 300_000, &[]);
        let preview = jpeg(0xC0, 1620, 1080, 40_000, &[]);
        let sensor = vec![0x5A; 80_000];
        let before = |data: u64| {
            let mut notes = b"II\x2A\x00\x08\x00\x00\x00".to_vec();
            notes.extend(directory(
                &[entry(0x0100, false, 6000), entry(0x0112, false, 8)],
                0,
            ));
            let canon = [
                &[0x85u8; 16][..],
                &part(b"CNCV", &[0; 30]),
                &part(b"CMT1", &notes),
            ]
            .concat();
            let mut movie = part(b"uuid", &canon);
            if with_full {
                movie.extend(track(data, full.len() as u32));
            }
            movie.extend(track(data + full.len() as u64, sensor.len() as u32));
            let mut shown = vec![0u8; 4];
            shown.extend([0, 1]);
            shown.extend(1620u16.to_be_bytes());
            shown.extend(1080u16.to_be_bytes());
            shown.extend([0, 1]);
            shown.extend((preview.len() as u32).to_be_bytes());
            shown.extend(&preview);
            let previews = [
                &[0xEAu8; 16][..],
                &[0, 0, 0, 0, 0, 0, 0, 1],
                &part(b"PRVW", &shown),
            ]
            .concat();
            [
                part(b"ftyp", b"crx \x00\x00\x00\x01crx isom"),
                part(b"moov", &movie),
                part(b"uuid", &previews),
            ]
            .concat()
        };
        // Where the data starts: after everything before it and its box's head.
        let data = before(0).len() as u64 + 8;
        let mut file = before(data);
        file.extend(part(b"mdat", &[&full[..], &sensor[..]].concat()));
        (file, full, preview)
    }

    /// What the reader gives for `file`.
    fn render(file: Vec<u8>) -> Result<(Vec<u8>, Option<u8>), String> {
        source::FILE.with(|held| *held.borrow_mut() = file);
        stored()
    }

    #[test]
    fn the_picture_a_camera_stored_is_taken_out() {

        // The largest picture a browser shows, not the sensor's data.
        let (file, full) = tiff_raw();
        let made = render(file).unwrap();
        assert_eq!(made.0, full);
        assert_eq!(made.1, Some(6));

        // Kept where no directory says (a maker's notes): found by reading the
        // file through, the preview's own thumbnail not taken for it.
        let inner = jpeg(0xC0, 160, 120, 16, &[]);
        let preview = jpeg(0xC0, 1600, 1200, 300_000, &inner);
        let mut noted = b"IIRO\x08\x00\x00\x00".to_vec();
        noted.extend(directory(&[entry(0x0112, false, 1)], 0));
        noted.extend(vec![0xAB; 70_000]);
        noted.extend(&preview);
        noted.extend(vec![0xCD; 1000]);
        let made = render(noted).unwrap();
        assert_eq!(made.0, preview);
        assert_eq!(made.1, Some(1));

        // A Fujifilm RAF file: its header says where its one picture is; the
        // picture's own EXIF says which way up, so the reader names none.
        let picture = jpeg(0xC0, 6240, 4160, 200_000, &jpeg(0xC0, 160, 120, 16, &[]));
        let fuji = |offset: u32, len: u32| {
            let mut file = b"FUJIFILMCCD-RAW 0201FF159501X-T5".to_vec();
            file.resize(84, 0);
            file.extend(offset.to_be_bytes());
            file.extend(len.to_be_bytes());
            file.resize(148, 0);
            file.extend(&picture);
            // The sensor's data after it.
            file.extend(vec![0x3C; 50_000]);
            file
        };
        let made = render(fuji(148, picture.len() as u32)).unwrap();
        assert_eq!(made.0, picture);
        assert_eq!(made.1, None);
        // A header that points outside the file: the picture is still found.
        let made = render(fuji(0xFFFF_FF00, 4096)).unwrap();
        assert_eq!(made.0, picture);

        // A Canon CR3 file: boxes, as in MP4. The full-size picture is its
        // first track; the sensor's data, another track, is not a picture.
        let (file, full, _) = canon_raw(true);
        let made = render(file).unwrap();
        assert_eq!(made.0, full);
        assert_eq!(made.1, Some(8));
        // With no picture in its tracks, the smaller preview in its own box.
        let (file, _, preview) = canon_raw(false);
        let made = render(file).unwrap();
        assert_eq!(made.0, preview);
        assert_eq!(made.1, Some(8));

        // Nothing to show: said, not failed.
        for file in [
            Vec::new(),
            b"II\x2A\x00\x08\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec(),
            jpeg(0xC3, 6000, 4000, 4096, &[]),
            // A Canon file with nothing in it, and one whose boxes are cut short.
            part(b"ftyp", b"crx \x00\x00\x00\x01crx isom"),
            [
                &part(b"ftyp", b"crx \x00\x00\x00\x01crx isom")[..],
                &[0, 0, 0xFF, 0xFF],
                b"moov",
                &[0; 40],
            ]
            .concat(),
            // A Fujifilm header and nothing after it.
            b"FUJIFILMCCD-RAW 0201".to_vec(),
            // Directories that point outside the file.
            [&b"II\x2A\x00"[..], &0xFFFF_FFF0u32.to_le_bytes()].concat(),
        ] {
            let none = render(file);
            assert!(none.is_err(), "{none:?}");
        }
    }
}
