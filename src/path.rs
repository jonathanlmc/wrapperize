use std::path::PathBuf;

pub struct Escaped {
    pub original: PathBuf,
    pub escaped: String,
}

impl Escaped {
    pub const ESCAPE_CHAR: char = '"';

    // should be the escaped version of `ESCAPE_CHAR`
    const ESCAPE_CHAR_REPLACEMENT: &str = "\\\"";

    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();

        let escaped = path
            .to_string_lossy()
            .replace(Self::ESCAPE_CHAR, Self::ESCAPE_CHAR_REPLACEMENT);

        Self {
            original: path,
            escaped,
        }
    }
}
