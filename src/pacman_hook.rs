use std::{fs, path::Path};

use anyhow::Context;
use indoc::formatdoc;

use crate::WrappedBinaryInfo;

pub const HOOK_DIR: &str = "/etc/pacman.d/hooks";

pub fn create_dir() -> anyhow::Result<()> {
    fs::create_dir_all(HOOK_DIR)
        .with_context(|| format!("failed to create pacman user hook directory at `{HOOK_DIR}`"))
}

pub fn generate_install_and_update(
    bin_info: &WrappedBinaryInfo,
    hook_script_path: &Path,
) -> String {
    let wrapped_path_trimmed = bin_info.wrapped_path.to_string_lossy();
    let wrapped_path_trimmed = wrapped_path_trimmed
        .strip_prefix('/')
        .unwrap_or(wrapped_path_trimmed.as_ref());

    formatdoc! { r#"
        [Trigger]
        Type = File
        Operation = Install
        Operation = Upgrade
        Target = {wrapped_path_trimmed}

        [Action]
        Description = Wrapping {wrapped_bin_name} executable...
        When = PostTransaction
        Exec = {hook_script_path}
        "#,
        wrapped_bin_name = bin_info.wrapped_exec_name,
        hook_script_path = hook_script_path.display(),
    }
}

// TODO: create remove hook
