use std::io::{Cursor, Write};

use chrono::{DateTime, NaiveDate, Utc};
use ciborium::cbor;
use flate2::write::ZlibEncoder;
use rocketbot_barcode::qr::qr_string_to_bitmap;
use rocketbot_makepdf;
use rocketbot_makepdf::model::{
    PdfColorDescription, PdfDescription, PdfElementDescription, PdfPathDescription, PdfPoint,
};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VaxInfo {
    pub issued: DateTime<Utc>,
    pub expires: DateTime<Utc>,
    pub issuer: String,
    pub country_code: String,
    pub country_name_de: String,
    pub country_name_en: String,
    pub dose_number: usize,
    pub total_doses: usize,
    pub date_of_birth: NaiveDate,
    pub cert_id: String,
    pub surname: String,
    pub surname_normalized: String,
    pub given_name: String,
    pub given_name_normalized: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct PdfSettings {
    pub qr_top_left_x: f32,
    pub qr_top_left_y: f32,
    pub qr_pixel_width: f32,
    pub qr_pixel_height: f32,
    pub base_description: PdfDescription,
}

/// Normalizes a name according to ICAO 9303 Part 3 Section 6A
pub fn normalize_name(name: &str) -> String {
    // uppercase and replaces spaces with "<"
    let upcased = name.to_uppercase().replace(" ", "<");

    // some recommended transliterations are multi-letter
    let mut multis = String::new();
    for c in upcased.chars() {
        match c {
            'Ä' => multis.push_str("AE"),
            'Å' => multis.push_str("AA"),
            'Æ' => multis.push_str("AE"),
            'Ð' => multis.push_str("D"),
            'Ö' => multis.push_str("OE"),
            'Ø' => multis.push_str("OE"),
            'Ü' => multis.push_str("UE"),
            'ß' => multis.push_str("SS"),
            'Þ' => multis.push_str("TH"),
            'Ĳ' => multis.push_str("IJ"),
            'Œ' => multis.push_str("OE"),
            'ẞ' => multis.push_str("SS"),
            other => multis.push(other),
        }
    }

    // most other transliterations are "strip diacritics"
    let stripped: String = multis.nfd()
        .filter(|c| !is_combining_mark(*c))
        .collect();

    // TODO: Cyrillic and Arabic transliterations?

    stripped
}

pub(crate) fn encode_vax(vax_info: &VaxInfo) -> String {
    // CWT is a sequence of:
    // protected (bytes of CBOR data), unprotected (map),
    // payload (bytes of CBOR data), signature (bytes)

    // we can't fake a signature, so just use completely invalid data

    // protected: 4 = key ID (8 bytes), 1 = algorithm (-7 = ECDSA-256)
    let protected_cbor = cbor!(
        {
            4 => b"FUCKING!",
            1 => -7,
        }
    )
        .expect("failed to construct CBOR value");
    let mut protected = Vec::new();
    ciborium::into_writer(&protected_cbor, &mut protected)
        .expect("failed to write CBOR value");

    // unprotected is empty

    // payload is the whole structure itself
    // let's start with the inner part, the health certificate
    let payload_cbor = cbor!(
        {
            // Issued and Expires are Unix timestamps
            4 => vax_info.issued.timestamp(),
            6 => vax_info.expires.timestamp(),
            1 => &vax_info.country_code,
            -260 => {
                1 => {
                    "v" => [
                        {
                            "dn" => vax_info.dose_number,
                            "ma" => "ORG-100030215", // marketing authorization holder = Biontech
                            "vp" => "J07BX03", // vaccine prophylaxis = covid-19 vaccine
                            "dt" => vax_info.issued.format("%Y-%m-%d").to_string(),
                            "co" => &vax_info.country_code,
                            "ci" => &vax_info.cert_id,
                            "mp" => "EU/1/20/1528", // medicinal product = Comirnaty
                            "is" => &vax_info.issuer,
                            "sd" => vax_info.total_doses,
                            "tg" => "840539006", // target disease or agent = COVID-19
                        },
                    ],
                    "nam" => {
                        "fnt" => &vax_info.surname_normalized,
                        "fn" => &vax_info.surname,
                        "gnt" => &vax_info.given_name_normalized,
                        "gn" => &vax_info.given_name,
                    },
                    "ver" => "1.0.0",
                    "dob" => vax_info.date_of_birth.format("%Y-%m-%d").to_string(),
                },
            },
        }
    )
        .expect("failed to construct CBOR value");
    let mut payload = Vec::new();
    ciborium::into_writer(&payload_cbor, &mut payload)
        .expect("failed to write CBOR value");

    // signature is, in the case of ECDSA-256, 64 bytes of signature
    let signature = b"Too stupid to scan a QR code? Your restaurant is a health risk!!";

    // encode the whole thing as CBOR again
    let full_cbor = ciborium::Value::Tag(
        0x12,
        Box::new(cbor!(
            [
                &protected,
                {}, // unprotected is empty
                &payload,
                &signature[..],
            ]
        ).unwrap()),
    );
    let mut full = Vec::new();
    ciborium::into_writer(&full_cbor, &mut full)
        .expect("failed to write CBOR value");

    // compress with zlib
    let mut zlib_data = Vec::new();
    {
        let zlib_cur = Cursor::new(&mut zlib_data);
        let mut zlib_enc = ZlibEncoder::new(zlib_cur, flate2::Compression::best());
        zlib_enc.write_all(&full)
            .expect("zlib encoding failed");
    }

    // base45-encode
    let mut b45 = base45::encode(zlib_data);

    // prefix with "HC1:"
    b45.insert_str(0, "HC1:");

    b45
}

pub(crate) fn make_vax_pdf(vax_info: &VaxInfo, pdf_settings: &PdfSettings) -> Vec<u8> {
    // generate the QR code data
    let qr_str = encode_vax(&vax_info);
    let qr_bitmap = qr_string_to_bitmap(&qr_str)
        .expect("failed to render QR code");

    // clone the settings
    let mut my_pdf = pdf_settings.base_description.clone();
    let first_page = &mut my_pdf.pages[0];

    // find and fill placeholders
    for elem in &mut first_page.elements {
        if let PdfElementDescription::Text(txt) = elem {
            let replacement = match txt.text.as_str() {
                "{{VAXID}}" => vax_info.cert_id.clone(),
                "{{NAME}}" => format!("{}, {}", vax_info.surname, vax_info.given_name),
                "{{DOB}}" => vax_info.date_of_birth.format("%Y-%m-%d").to_string(),
                "{{VAXNO}}" => format!("{}/{}", vax_info.dose_number, vax_info.total_doses),
                "{{VAXDATE}}" => vax_info.issued.format("%Y-%m-%d").to_string(),
                "{{DECOUNTRY}}" => vax_info.country_name_de.clone(),
                "{{ENCOUNTRY}}" => vax_info.country_name_en.clone(),
                other => other.to_owned(),
            };
            txt.text = replacement;
        }
    }

    // paint the QR code
    for y in 0..qr_bitmap.height() {
        // PDF's origin is the bottom left; compensate
        let pdf_y = pdf_settings.qr_top_left_y - (y as f32) * pdf_settings.qr_pixel_height;
        for x in 0..qr_bitmap.width() {
            let pdf_x = pdf_settings.qr_top_left_x + (x as f32) * pdf_settings.qr_pixel_width;

            if qr_bitmap.bits()[y * qr_bitmap.width() + x] {
                let path = PdfPathDescription {
                    stroke: None,
                    stroke_width: None,
                    fill: Some(PdfColorDescription::Grayscale { white: 0.0 }),
                    close: true,
                    points_mm: vec![
                        PdfPoint { x: pdf_x, y: pdf_y },
                        PdfPoint { x: pdf_x + pdf_settings.qr_pixel_width, y: pdf_y },
                        PdfPoint { x: pdf_x + pdf_settings.qr_pixel_width, y: pdf_y - pdf_settings.qr_pixel_height },
                        PdfPoint { x: pdf_x, y: pdf_y - pdf_settings.qr_pixel_height },
                    ],
                };
                first_page.elements.push(PdfElementDescription::Path(path));
            }
        }
    }

    // render the PDF
    let pdf_bytes = rocketbot_makepdf::render_description(&my_pdf)
        .expect("rendering static PDF data failed");
    pdf_bytes
}
