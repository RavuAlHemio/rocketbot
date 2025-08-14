pub mod model;


use std::collections::HashMap;
use std::fmt;

use printpdf::{
    Cmyk, Color, Greyscale, Line, LinePoint, Mm, Op, ParsedFont, PdfDocument, PdfPage, PdfSaveOptions, Point, Pt, RawImage, Rgb, TextItem, XObjectTransform
};
use rustybuzz::{Face, UnicodeBuffer};

use crate::model::{
    PdfColorDescription, PdfDescription, PdfElementDescription, TextAlignmentDescription,
};


#[derive(Debug)]
pub enum PdfDefinitionError {
    NoPages,
    UndefinedFont(String),
    LoadingFont(String),
    AddingImage(String, String),
    UnsupportedImageType(String),
}
impl fmt::Display for PdfDefinitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PdfDefinitionError::NoPages
                => write!(f, "document has no pages"),
            PdfDefinitionError::UndefinedFont(name)
                => write!(f, "font {:?} is referenced but not defined", name),
            PdfDefinitionError::LoadingFont(name)
                => write!(f, "failed to load font {:?}", name),
            PdfDefinitionError::AddingImage(image_type, e)
                => write!(f, "failed to add image of type {:?}: {}", image_type, e),
            PdfDefinitionError::UnsupportedImageType(image_type)
                => write!(f, "images of type {:?} are not supported", image_type),
        }
    }
}
impl std::error::Error for PdfDefinitionError {
}


pub fn verify_description(description: &PdfDescription) -> Result<(), PdfDefinitionError> {
    for page in &description.pages {
        for element in &page.elements {
            if let PdfElementDescription::Text(text) = &element {
                if !description.fonts.contains_key(&text.font) {
                    return Err(PdfDefinitionError::UndefinedFont(text.font.clone()));
                }
            }
        }
    }

    Ok(())
}

fn color_from_def(color: &PdfColorDescription) -> Color {
    match color {
        PdfColorDescription::Rgb { red, green, blue } => Color::Rgb(Rgb { r: *red, g: *green, b: *blue, icc_profile: None }),
        PdfColorDescription::Cmyk { cyan, magenta, yellow, black } => Color::Cmyk(Cmyk { c: *cyan, m: *magenta, y: *yellow, k: *black, icc_profile: None }),
        PdfColorDescription::Grayscale { white } => Color::Greyscale(Greyscale { percent: *white, icc_profile: None }),
    }
}

pub fn render_description(description: &PdfDescription) -> Result<Vec<u8>, PdfDefinitionError> {
    if description.pages.len() == 0 {
        return Err(PdfDefinitionError::NoPages);
    }

    let mut doc = PdfDocument::new(description.title.as_str());

    let mut pdf_fonts = HashMap::new();
    let mut buzz_fonts = HashMap::new();
    for (font_name, font_data) in &description.fonts {
        let font = ParsedFont::from_bytes(
            &font_data.0,
            0,
            &mut vec![],
        ).unwrap();
        let font_ref = doc.add_font(&font);
        pdf_fonts.insert(font_name.clone(), font_ref);

        let face = Face::from_slice(&font_data.0, 0)
            .ok_or_else(|| PdfDefinitionError::LoadingFont(font_name.clone()))?;
        buzz_fonts.insert(font_name.clone(), face);
    }

    for page in &description.pages {
        let mut page_ops = Vec::new();

        page_ops.push(Op::SaveGraphicsState);
        for elem in &page.elements {
            match elem {
                PdfElementDescription::Path(path) => {
                    if path.stroke.is_none() && path.fill.is_none() {
                        // nothing to paint
                        continue;
                    }

                    page_ops.push(Op::SaveGraphicsState);

                    if let Some(stroke) = &path.stroke {
                        page_ops.push(Op::SetOutlineColor { col: color_from_def(stroke) });
                    }
                    if let Some(stroke_width) = path.stroke_width {
                        page_ops.push(Op::SetOutlineThickness { pt: Pt(stroke_width) });
                    }
                    if let Some(fill) = &path.fill {
                        page_ops.push(Op::SetFillColor { col: color_from_def(fill) });
                    }

                    let mut points = Vec::with_capacity(path.points_mm.len());
                    for point in &path.points_mm {
                        points.push(LinePoint {
                            p: Point::new(Mm(point.x), Mm(point.y)),
                            bezier: false,
                        });
                    }

                    page_ops.push(Op::DrawLine {
                        line: Line {
                            points: points,
                            is_closed: path.close,
                        }
                    });

                    page_ops.push(Op::RestoreGraphicsState);
                },
                PdfElementDescription::Image(img) => {
                    let image: RawImage = match img.mime_type.as_str() {
                        "image/jpeg" => RawImage::decode_from_bytes(
                            &img.data.0,
                            &mut vec![],
                            )
                                .map_err(|e| PdfDefinitionError::AddingImage("JPEG".to_owned(), e))?,
                        "image/png" => RawImage::decode_from_bytes(
                            &img.data.0,
                            &mut vec![],
                            )
                                .map_err(|e| PdfDefinitionError::AddingImage("PNG".to_owned(), e))?,
                        other => return Err(PdfDefinitionError::UnsupportedImageType(other.to_owned())),
                    };
                    let image_id = doc.add_image(&image);
                    let transform = XObjectTransform {
                        translate_x: Some(Mm(img.x).into_pt()),
                        translate_y: Some(Mm(img.y).into_pt()),
                        scale_x: Some(img.scale_x),
                        scale_y: Some(img.scale_y),
                        ..Default::default()
                    };
                    page_ops.push(Op::UseXobject {
                        id: image_id,
                        transform,
                    });
                },
                PdfElementDescription::Text(txt) => {
                    if txt.text.len() == 0 {
                        continue;
                    }

                    let font_ref = match pdf_fonts.get(&txt.font) {
                        Some(f) => f.clone(),
                        None => return Err(PdfDefinitionError::UndefinedFont(txt.font.clone())),
                    };

                    let offset_mm = match txt.alignment {
                        TextAlignmentDescription::Left => 0.0,
                        TextAlignmentDescription::Center|TextAlignmentDescription::Right => {
                            // have rustybuzz calculate text length
                            let buzz_font = buzz_fonts.get(&txt.font)
                                .expect("font exists in pdf_fonts but not in buzz_fonts");
                            let units_per_em = buzz_font.units_per_em();
                            let mut buf = UnicodeBuffer::new();
                            buf.push_str(&txt.text);
                            let glyphs = rustybuzz::shape(buzz_font, &[], buf);
                            let total_advance_units: i32 = glyphs.glyph_positions()
                                .iter()
                                .map(|gp| gp.x_advance)
                                .sum();
                            let total_advance_em = (total_advance_units as f32) / (units_per_em as f32);
                            let total_advance_pt = Pt(txt.size_pt * total_advance_em);
                            let total_advance_mm = Mm::from(total_advance_pt);

                            if let TextAlignmentDescription::Center = txt.alignment {
                                -total_advance_mm.0 / 2.0
                            } else {
                                -total_advance_mm.0
                            }
                        },
                    };

                    page_ops.push(Op::SaveGraphicsState);
                    page_ops.push(Op::StartTextSection);
                    page_ops.push(Op::SetTextCursor {
                        pos: Point::new(Mm(txt.x + offset_mm), Mm(txt.y)),
                    });
                    page_ops.push(Op::SetFontSize {
                        size: Pt(txt.size_pt),
                        font: font_ref.clone(),
                    });
                    page_ops.push(Op::SetFillColor {
                        col: Color::Greyscale(Greyscale {
                            percent: 0.0,
                            icc_profile: None,
                        }),
                    });

                    page_ops.push(Op::WriteText {
                        items: vec![
                            TextItem::Text(txt.text.clone()),
                        ],
                        font: font_ref.clone(),
                    });

                    page_ops.push(Op::EndTextSection);
                    page_ops.push(Op::RestoreGraphicsState);
                },
            }
        }
        page_ops.push(Op::RestoreGraphicsState);

        let pdf_page = PdfPage::new(
            Mm(description.pages[0].width_mm),
            Mm(description.pages[0].height_mm),
            Vec::new(),
        );
        doc.pages.push(pdf_page);
    }

    // render
    let opts = PdfSaveOptions {
        optimize: false,
        subset_fonts: false,
        secure: false,
        image_optimization: None,
    };
    let bytes = doc.save(&opts, &mut vec![]);
    Ok(bytes)
}
