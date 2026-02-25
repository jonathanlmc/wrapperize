use std::path::Path;

use color_eyre::{Section, SectionExt, eyre};

pub trait ReportExt {
    fn with_path_section(self, path: &Path) -> Self;
}

impl<T> ReportExt for eyre::Result<T> {
    fn with_path_section(self, path: &Path) -> Self {
        self.with_section(|| path.display().to_string().header("Path:"))
    }
}
