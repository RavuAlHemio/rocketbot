pub mod model;


use std::collections::HashMap;
use std::fmt;
use std::io::Cursor;

use printpdf::{
    Cmyk, Color, Greyscale, Image, Mm, OP_PATH_CONST_4BEZIER, OP_PATH_CONST_CLOSE_SUBPATH,
    OP_PATH_CONST_LINE_TO, OP_PATH_CONST_MOVE_TO, OP_PATH_PAINT_END, OP_PATH_PAINT_FILL_NZ,
    OP_PATH_PAINT_FILL_STROKE_CLOSE_NZ, OP_PATH_PAINT_FILL_STROKE_NZ, OP_PATH_PAINT_STROKE,
    OP_PATH_PAINT_STROKE_CLOSE, PdfDocument, PdfDocumentReference, Pt, Rgb,
};
use printpdf::image::jpeg::JpegDecoder;
use printpdf::image::png::PngDecoder;
use printpdf::lopdf::Object;
use printpdf::lopdf::content::Operation;

use crate::model::{
    PdfColorDescription, PdfDescription, PdfElementDescription, PdfPathCommandDescription,
};


#[derive(Debug)]
pub enum PdfDefinitionError {
    NoPages,
    UndefinedFont(String),
    AddingFont(String, printpdf::Error),
    AddingImage(String, printpdf::image::ImageError),
    UnsupportedImageType(String),
    SavingFailed(printpdf::Error),
}
impl fmt::Display for PdfDefinitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PdfDefinitionError::NoPages
                => write!(f, "document has no pages"),
            PdfDefinitionError::UndefinedFont(name)
                => write!(f, "font {:?} is referenced but not defined", name),
            PdfDefinitionError::AddingFont(name, e)
                => write!(f, "failed to add font {:?}: {}", name, e),
            PdfDefinitionError::AddingImage(image_type, e)
                => write!(f, "failed to add image of type {:?}: {}", image_type, e),
            PdfDefinitionError::UnsupportedImageType(image_type)
                => write!(f, "images of type {:?} are not supported", image_type),
            PdfDefinitionError::SavingFailed(e)
                => write!(f, "failed to save PDF file: {}", e),
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
        PdfColorDescription::Cmyk { cyan, magenta, yellow, black, } => Color::Cmyk(Cmyk { c: *cyan, m: *magenta, y: *yellow, k: *black, icc_profile: None }),
        PdfColorDescription::Grayscale { white } => Color::Greyscale(Greyscale { percent: *white, icc_profile: None }),
    }
}

#[inline]
fn object_from_mm(mm: f64) -> Object {
    let pt: Pt = Mm(mm).into();
    pt.into()
}

pub fn render_description(description: &PdfDescription) -> Result<PdfDocumentReference, PdfDefinitionError> {
    if description.pages.len() == 0 {
        return Err(PdfDefinitionError::NoPages);
    }

    let (doc, page1, layer1) = PdfDocument::new(
        description.title.as_str(),
        Mm(description.pages[0].width_mm), Mm(description.pages[0].height_mm),
        "Layer",
    );

    let mut fonts = HashMap::new();
    for (font_name, font_data) in &description.fonts {
        let font_ref = doc.add_external_font(Cursor::new(&font_data.0))
            .map_err(|e| PdfDefinitionError::AddingFont(font_name.clone(), e))?;
        fonts.insert(font_name.clone(), font_ref);
    }

    for (i, page) in description.pages.iter().enumerate() {
        let (this_page_index, this_layer_index) = if i == 0 {
            (page1, layer1)
        } else {
            doc.add_page(Mm(page.width_mm), Mm(page.height_mm), "Layer")
        };

        for elem in &page.elements {
            let this_layer = doc.get_page(this_page_index).get_layer(this_layer_index);

            match elem {
                PdfElementDescription::Path(path) => {
                    if let Some(stroke) = &path.stroke {
                        this_layer.set_outline_color(color_from_def(stroke));
                    }
                    if let Some(stroke_width) = path.stroke_width {
                        this_layer.set_outline_thickness(stroke_width);
                    }
                    if let Some(fill) = &path.fill {
                        this_layer.set_fill_color(color_from_def(fill));
                    }

                    for cmd in &path.commands_mm {
                        let op = match cmd {
                            PdfPathCommandDescription::LineTo { x, y }
                                => Operation::new(OP_PATH_CONST_LINE_TO, vec![object_from_mm(*x), object_from_mm(*y)]),
                            PdfPathCommandDescription::MoveTo { x, y }
                                => Operation::new(OP_PATH_CONST_MOVE_TO, vec![object_from_mm(*x), object_from_mm(*y)]),
                            PdfPathCommandDescription::CubicBezierTo { c1x, c1y, c2x, c2y, x, y }
                                => Operation::new(OP_PATH_CONST_4BEZIER, vec![object_from_mm(*c1x), object_from_mm(*c1y), object_from_mm(*c2x), object_from_mm(*c2y), object_from_mm(*x), object_from_mm(*y)]),
                        };
                        this_layer.add_operation(op);
                    }

                    let final_char = match (path.stroke.is_some(), path.fill.is_some(), path.close) {
                        (false, false, false) => OP_PATH_PAINT_END,
                        (false, false, true) => OP_PATH_CONST_CLOSE_SUBPATH,
                        (false, true, _) => OP_PATH_PAINT_FILL_NZ,
                        (true, false, false) => OP_PATH_PAINT_STROKE,
                        (true, false, true) => OP_PATH_PAINT_STROKE_CLOSE,
                        (true, true, false) => OP_PATH_PAINT_FILL_STROKE_NZ,
                        (true, true, true) => OP_PATH_PAINT_FILL_STROKE_CLOSE_NZ,
                    };
                    let final_op = Operation::new(final_char, vec![]);
                    this_layer.add_operation(final_op);
                },
                PdfElementDescription::Image(img) => {
                    let cursor = Cursor::new(&img.data.0);

                    let image: Image = match img.mime_type.as_str() {
                        "image/jpeg" => Image::try_from(
                            JpegDecoder::new(cursor)
                                .map_err(|e| PdfDefinitionError::AddingImage("JPEG".to_owned(), e))?
                            ).expect("failed to convert JPEG to image"),
                        "image/png" => Image::try_from(
                            PngDecoder::new(cursor)
                                .map_err(|e| PdfDefinitionError::AddingImage("PNG".to_owned(), e))?
                            ).expect("failed to convert PNG to image"),
                        other => return Err(PdfDefinitionError::UnsupportedImageType(other.to_owned())),
                    };
                    image.add_to_layer(
                        this_layer,
                        Some(Mm(img.x)), Some(Mm(img.y)),
                        None,
                        Some(img.scale_x), Some(img.scale_y),
                        None,
                    );
                },
                PdfElementDescription::Text(txt) => {
                    this_layer.set_fill_color(Color::Greyscale(Greyscale { percent: 0.0, icc_profile: None }));

                    let font_ref = match fonts.get(&txt.font) {
                        Some(f) => f,
                        None => return Err(PdfDefinitionError::UndefinedFont(txt.font.clone())),
                    };

                    this_layer.use_text(&txt.text, txt.size_pt, Mm(txt.x), Mm(txt.y), font_ref);
                },
            }
        }
    }

    Ok(doc)
}
