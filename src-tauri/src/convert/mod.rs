use std::fmt;

pub mod docx;
pub mod xlsx;
pub mod pdf;
pub mod pptx;
pub mod html;
pub mod media;
mod inline_fmt;

#[derive(Debug)]
pub struct ConversionError(pub String);

impl fmt::Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<ConversionError> for String {
    fn from(e: ConversionError) -> Self {
        e.0
    }
}

impl From<String> for ConversionError {
    fn from(s: String) -> Self {
        ConversionError(s)
    }
}

impl From<&str> for ConversionError {
    fn from(s: &str) -> Self {
        ConversionError(s.to_string())
    }
}
