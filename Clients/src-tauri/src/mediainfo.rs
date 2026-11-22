//! Cheap facts about a file before it is sent: content type from the extension, image size, MP4 duration and size,
//! PDF page count, audio duration. Nothing here decodes pixels or video frames.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub fn content_type_for(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" | "jpe" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "avif" => "image/avif",
        "tif" | "tiff" => "image/tiff",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "3gp" => "video/3gpp",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "amr" => "audio/amr",
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "txt" | "log" | "md" => "text/plain",
        "csv" => "text/csv",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        "apk" => "application/vnd.android.package-archive",
        _ => "application/octet-stream",
    }
}

/// `image`, `video`, `audio` or `file`.
pub fn kind_of(content_type: &str) -> &'static str {
    let ct = content_type.to_ascii_lowercase();
    if ct.starts_with("image/") {
        "image"
    } else if ct.starts_with("video/") {
        "video"
    } else if ct.starts_with("audio/") {
        "audio"
    } else {
        "file"
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Info {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u32>,
    pub pages: Option<u32>,
}

pub fn probe(path: &Path, content_type: &str) -> Info {
    match kind_of(content_type) {
        "image" => match imagesize::size(path) {
            Ok(s) if s.width > 0 && s.height > 0 => Info { width: Some(s.width as u32), height: Some(s.height as u32), ..Default::default() },
            _ => Info::default(),
        },
        "video" => mp4_info(path).map(|(w, h, d)| Info { width: Some(w).filter(|w| *w > 0), height: Some(h).filter(|h| *h > 0), duration_ms: Some(d).filter(|d| *d > 0), pages: None }).unwrap_or_default(),
        "audio" => Info { duration_ms: crate::audio::probe_duration_ms(path), ..Default::default() },
        _ if content_type == "application/pdf" => Info { pages: pdf_pages(path), ..Default::default() },
        _ => Info::default(),
    }
}

// ---------------------------------------------------------------- MP4 / MOV

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}
fn be64(b: &[u8]) -> u64 {
    u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

/// Walks the boxes in `data` (a moov or trak body) and calls `f(type, body)`.
fn boxes<'a>(data: &'a [u8], mut f: impl FnMut(&[u8], &'a [u8])) {
    let mut i = 0usize;
    while i + 8 <= data.len() {
        let mut size = be32(&data[i..]) as u64;
        let ty = &data[i + 4..i + 8];
        let mut hdr = 8usize;
        if size == 1 && i + 16 <= data.len() {
            size = be64(&data[i + 8..]);
            hdr = 16;
        }
        if size == 0 {
            size = (data.len() - i) as u64;
        }
        if size < hdr as u64 || i as u64 + size > data.len() as u64 {
            break;
        }
        f(ty, &data[i + hdr..i + size as usize]);
        i += size as usize;
    }
}

/// `(width, height, duration_ms)` of the first video track of an MP4 or MOV file.
pub fn mp4_info(path: &Path) -> Option<(u32, u32, u32)> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut pos = 0u64;
    let mut moov: Option<Vec<u8>> = None;
    while pos + 8 <= len {
        f.seek(SeekFrom::Start(pos)).ok()?;
        let mut h = [0u8; 16];
        let n = f.read(&mut h).ok()?;
        if n < 8 {
            break;
        }
        let mut size = be32(&h) as u64;
        let ty = [h[4], h[5], h[6], h[7]];
        let mut hdr = 8u64;
        if size == 1 && n >= 16 {
            size = be64(&h[8..]);
            hdr = 16;
        }
        if size == 0 {
            size = len - pos;
        }
        if size < hdr {
            break;
        }
        if &ty == b"moov" {
            if size > 64 * 1024 * 1024 {
                return None;
            }
            f.seek(SeekFrom::Start(pos + hdr)).ok()?;
            let mut body = vec![0u8; (size - hdr) as usize];
            f.read_exact(&mut body).ok()?;
            moov = Some(body);
            break;
        }
        pos += size;
    }
    let moov = moov?;
    let mut dur_ms = 0u32;
    let mut dims: Option<(u32, u32)> = None;
    boxes(&moov, |ty, body| {
        if ty == b"mvhd" && body.len() >= 28 {
            let (scale, dur) = if body[0] == 1 && body.len() >= 32 { (be32(&body[20..]), be64(&body[24..])) } else { (be32(&body[12..]), be32(&body[16..]) as u64) };
            if scale > 0 {
                dur_ms = (dur * 1000 / scale as u64) as u32;
            }
        } else if ty == b"trak" && dims.is_none() {
            boxes(body, |t2, b2| {
                if t2 == b"tkhd" {
                    let off = if b2.first() == Some(&1) { 92 } else { 80 };
                    let mat = if b2.first() == Some(&1) { 52 } else { 40 };
                    if b2.len() >= off + 4 {
                        let w = be32(&b2[off - 4..]) >> 16;
                        let h = be32(&b2[off..]) >> 16;
                        if w > 0 && h > 0 {
                            let a = be32(&b2[mat..]);
                            let rotated = a == 0 && be32(&b2[mat + 4..]) != 0;
                            dims = Some(if rotated { (h, w) } else { (w, h) });
                        }
                    }
                }
            });
        }
    });
    let (w, h) = dims.unwrap_or((0, 0));
    Some((w, h, dur_ms))
}

// ---------------------------------------------------------------- PDF

/// Counts `/Type /Page` objects. Works for ordinary PDFs; files that keep pages inside compressed object streams
/// report `None` and the card simply shows no page count.
pub fn pdf_pages(path: &Path) -> Option<u32> {
    let len = std::fs::metadata(path).ok()?.len();
    if len > 40 * 1024 * 1024 {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    pdf_pages_in(&data)
}

pub fn pdf_pages_in(data: &[u8]) -> Option<u32> {
    if !data.starts_with(b"%PDF") {
        return None;
    }
    let mut n = 0u32;
    let mut i = 0usize;
    while let Some(p) = find(data, b"/Type", i) {
        let mut j = p + 5;
        while j < data.len() && data[j].is_ascii_whitespace() {
            j += 1;
        }
        if data[j..].starts_with(b"/Page") {
            let next = data.get(j + 5).copied().unwrap_or(b' ');
            if !next.is_ascii_alphanumeric() {
                n += 1;
            }
        }
        i = p + 5;
    }
    if n > 0 {
        return Some(n);
    }
    // Page tree root: /Count N
    let mut best = 0u32;
    let mut i = 0usize;
    while let Some(p) = find(data, b"/Count", i) {
        let rest = &data[p + 6..data.len().min(p + 20)];
        let digits: String = rest.iter().skip_while(|b| b.is_ascii_whitespace()).take_while(|b| b.is_ascii_digit()).map(|b| *b as char).collect();
        if let Ok(v) = digits.parse::<u32>() {
            best = best.max(v);
        }
        i = p + 6;
    }
    (best > 0).then_some(best)
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= hay.len() {
        return None;
    }
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(n: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(n)
    }

    #[test]
    fn types_and_kinds() {
        assert_eq!(content_type_for("Report.PDF"), "application/pdf");
        assert_eq!(content_type_for("a.xlsx"), "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet");
        assert_eq!(content_type_for("weird.xyz"), "application/octet-stream");
        assert_eq!(kind_of("image/png"), "image");
        assert_eq!(kind_of("video/mp4"), "video");
        assert_eq!(kind_of("audio/ogg"), "audio");
        assert_eq!(kind_of("application/zip"), "file");
    }

    #[test]
    fn mp4_dimensions_and_duration() {
        let (w, h, d) = mp4_info(&fixture("tiny.mp4")).unwrap();
        assert_eq!((w, h), (160, 120));
        assert!((900..=1100).contains(&d), "{d}");
        let info = probe(&fixture("tiny.mp4"), "video/mp4");
        assert_eq!(info.width, Some(160));
    }

    #[test]
    fn pdf_page_count() {
        let pdf = b"%PDF-1.4\n1 0 obj<</Type /Pages /Count 3>>endobj\n2 0 obj<</Type /Page /Parent 1 0 R>>endobj\n3 0 obj<</Type/Page>>endobj\n4 0 obj<< /Type /Page >>endobj";
        assert_eq!(pdf_pages_in(pdf), Some(3));
        assert_eq!(pdf_pages_in(b"%PDF-1.5 /Count 12 junk"), Some(12));
        assert_eq!(pdf_pages_in(b"not a pdf"), None);
    }

    #[test]
    fn audio_duration_probe() {
        for n in ["tone.ogg", "tone.m4a", "tone.mp3", "tone.wav"] {
            let d = crate::audio::probe_duration_ms(&fixture(n)).unwrap_or_else(|| panic!("{n}"));
            assert!((1300..=1800).contains(&d), "{n} {d}");
        }
    }
}
