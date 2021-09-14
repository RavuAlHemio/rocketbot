pub mod datamatrix;
pub mod bitmap;
pub mod qr;


use std::fmt;

use crate::bitmap::BitmapError;


#[derive(Debug)]
pub enum BarcodeError {
    DataMatrixEncoding(::datamatrix::data::DataEncodingError),
    QrEncoding(qrcode::types::QrError),
    Bitmap(BitmapError),
}
impl fmt::Display for BarcodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DataMatrixEncoding(e)
                => write!(f, "Data Matrix encoding error: {:?}", e),
            Self::QrEncoding(e)
                => write!(f, "QR encoding error: {:?}", e),
            Self::Bitmap(e)
                => write!(f, "{}", e),
        }
    }
}
impl std::error::Error for BarcodeError {
}
