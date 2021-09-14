use qrcode::QrCode;

use crate::BarcodeError;
use crate::bitmap::BarcodeBitmap;


pub fn qr_string_to_bitmap(string: &str) -> Result<BarcodeBitmap, BarcodeError> {
    let qr = QrCode::new(string)
        .map_err(|e| BarcodeError::QrEncoding(e))?;
    let qr_bits: Vec<bool> = qr.to_colors()
        .iter()
        .map(|c| c.select(true, false))
        .collect();

    // ensure the QR code bits are complete
    assert_eq!(0, qr_bits.len() % qr.width());

    Ok(BarcodeBitmap::new(
        qr.width(),
        qr_bits.len() / qr.width(),
        qr_bits,
    ).expect("invalid QR bitmap"))
}
