use std::path::Path;

use printpdf::{
    BuiltinFont, Mm, Op, ParsedFont, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point,
    Pt, Rgb, TextItem,
};

pub fn find_system_font() -> Option<std::path::PathBuf> {
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
    ];
    CANDIDATES
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.exists())
}

pub fn export_pdf(text: &str, out: &Path) -> Result<(), String> {
    let mut doc = PdfDocument::new("Editor");
    let fallback = BuiltinFont::Helvetica;
    let mut ops: Vec<Op> = vec![
        Op::SaveGraphicsState,
        Op::StartTextSection,
        Op::SetTextCursor {
            pos: Point::new(Mm(20.0), Mm(277.0)),
        },
        Op::SetFillColor {
            col: printpdf::Color::Rgb(Rgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                icc_profile: None,
            }),
        },
    ];

    match find_system_font() {
        Some(path) => {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            let mut warnings = Vec::new();
            match ParsedFont::from_bytes(&bytes, 0, &mut warnings) {
                Some(parsed) => {
                    let font_id = doc.add_font(&parsed);
                    append_text_ops(&mut ops, PdfFontHandle::External(font_id), text);
                }
                None => {
                    eprintln!("printpdf: не удалось распознать шрифт {}", path.display());
                    append_builtin_ops(&mut ops, &fallback, text);
                }
            }
        }
        None => append_builtin_ops(&mut ops, &fallback, text),
    }

    ops.push(Op::EndTextSection);
    ops.push(Op::RestoreGraphicsState);

    let page = PdfPage::new(Mm(210.0), Mm(297.0), ops);
    let bytes = doc
        .with_pages(vec![page])
        .save(&PdfSaveOptions::default(), &mut Vec::new());

    std::fs::write(out, bytes).map_err(|e| format!("не удалось записать PDF: {e}"))?;
    Ok(())
}

fn append_text_ops(ops: &mut Vec<Op>, font: PdfFontHandle, text: &str) {
    let font_size = Pt(11.0);
    let line_height = Pt(13.0);
    ops.push(Op::SetFont {
        font: font.clone(),
        size: font_size,
    });
    ops.push(Op::SetLineHeight { lh: line_height });
    for line in text.lines() {
        if !line.is_empty() {
            ops.push(Op::ShowText {
                items: vec![TextItem::Text(line.to_string())],
            });
        }
        ops.push(Op::AddLineBreak);
    }
}

fn append_builtin_ops(ops: &mut Vec<Op>, font: &BuiltinFont, text: &str) {
    let font_size = Pt(11.0);
    let font = PdfFontHandle::Builtin(*font);
    ops.push(Op::SetFont {
        font: font.clone(),
        size: font_size,
    });
    for line in text.lines() {
        if !line.is_empty() {
            ops.push(Op::ShowText {
                items: vec![TextItem::Text(line.to_string())],
            });
        }
        ops.push(Op::AddLineBreak);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_generates_bytes() {
        let out = std::env::temp_dir().join("editor-test-export.pdf");
        export_pdf("Привет!\nВторая строка.\n", &out).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 1000);
    }
}
