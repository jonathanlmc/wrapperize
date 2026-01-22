use std::path::PathBuf;

pub struct Escaped {
    pub original: PathBuf,
    pub escaped: String,
}

impl Escaped {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();

        let escaped = path
            .to_string_lossy()
            .as_ref()
            .as_bytes()
            .escape_ascii()
            .to_string();

        Self {
            original: path,
            escaped,
        }
    }
}
