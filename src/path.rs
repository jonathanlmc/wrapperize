use std::path::PathBuf;

use crate::str;

pub struct Escaped {
    pub original: PathBuf,
    pub escaped: String,
}

impl Escaped {
    pub const QUOTE_ESCAPE_CHAR: char = '"';

    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();

        let escaped =
            str::escape_quote::<{ Self::QUOTE_ESCAPE_CHAR }>(path.to_string_lossy().as_ref());

        Self {
            original: path,
            escaped,
        }
    }
}
