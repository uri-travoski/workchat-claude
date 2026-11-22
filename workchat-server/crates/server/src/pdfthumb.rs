//! F-03: a PNG preview of the first page of a PDF, drawn by `hayro` (a pure-Rust renderer, no system tools, so it works
//! the same in the Docker image, on a laptop and on CI). Rendering untrusted PDFs is bounded: size cap, panic guard,
//! at most two renders at a time, and a result (or the fact that rendering failed) is cached with the attachment.

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{render, PixmapSettings, RenderCache, RenderSettings};

/// PDFs larger than this are not previewed (a file card is shown instead).
pub const MAX_PDF_BYTES: u64 = 30 * 1024 * 1024;
const WIDTH_PX: f32 = 520.0;

/// The first page as a PNG about 520 px wide, or None when the file is not a PDF this renderer can draw.
pub fn first_page_png(bytes: Vec<u8>) -> Option<Vec<u8>> {
    if bytes.len() as u64 > MAX_PDF_BYTES || !bytes.windows(5).take(1024).any(|w| w == b"%PDF-") {
        return None;
    }
    // A malformed file must never take the server down: any panic in the renderer becomes "no preview".
    std::panic::catch_unwind(|| {
        let pdf = Pdf::new(bytes).ok()?;
        let page = pdf.pages().first()?;
        let (w, h) = page.render_dimensions();
        if !(w > 1.0 && h > 1.0) || w > 20_000.0 || h > 20_000.0 {
            return None;
        }
        let scale = (WIDTH_PX / w).clamp(0.05, 4.0);
        let settings = PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
        let pixmap = render(page, &RenderCache::new(), &InterpreterSettings::default(), &RenderSettings::default(), &settings);
        pixmap.into_png().ok()
    })
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-page PDF with a filled rectangle and a line of text, written by hand.
    pub fn tiny_pdf() -> Vec<u8> {
        let content = "0.8 0.1 0.1 rg 40 600 300 100 re f BT /F1 24 Tf 50 500 Td (Hello preview) Tj ET";
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{}\nendstream", content.len(), content),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offs = vec![];
        for (i, o) in objs.iter().enumerate() {
            offs.push(out.len());
            out.extend(format!("{} 0 obj\n{}\nendobj\n", i + 1, o).as_bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
        for o in offs {
            out.extend(format!("{o:010} 00000 n \n").as_bytes());
        }
        out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", objs.len() + 1, xref).as_bytes());
        out
    }

    #[test]
    fn renders_a_page_and_refuses_garbage() {
        let png = first_page_png(tiny_pdf()).expect("renders");
        assert_eq!(&png[1..4], b"PNG");
        // 400x800 page scaled to about 520 px wide.
        let w = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        let h = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
        assert!((515..=525).contains(&w) && (1030..=1050).contains(&h), "{w}x{h}");
        assert!(first_page_png(b"%PDF-1.4 not really".to_vec()).is_none());
        assert!(first_page_png(b"plain text".to_vec()).is_none());
        assert!(first_page_png(vec![]).is_none());
    }
}
