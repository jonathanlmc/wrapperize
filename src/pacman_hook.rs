use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use indoc::formatdoc;
use tap::Tap;

use crate::WrappedBinaryInfo;

pub const HOOK_DIR: &str = "/etc/pacman.d/hooks";

pub fn create_dir() -> anyhow::Result<()> {
    fs::create_dir_all(HOOK_DIR)
        .with_context(|| format!("failed to create pacman user hook directory at `{HOOK_DIR}`"))
}

#[derive(Copy, Clone)]
pub enum Action {
    InstallOrUpdate,
    Removal,
}

impl Action {
    fn path_verb(self) -> &'static str {
        match self {
            Self::InstallOrUpdate => "install",
            Self::Removal => "remove",
        }
    }
}

pub fn get_hook_path(binary_name: &str, action: Action) -> PathBuf {
    PathBuf::from(HOOK_DIR).tap_mut(|p| {
        p.push(format!(
            "{binary_name}-{program_name}-{action}.hook",
            program_name = env!("CARGO_PKG_NAME"),
            action = action.path_verb(),
        ))
    })
}

fn trim_path_root(path: impl Into<PathBuf>) -> PathBuf {
    let path = path.into();
    let path_str = path.to_string_lossy();

    path_str.strip_prefix('/').map(Into::into).unwrap_or(path)
}

pub fn generate_install_and_update(
    bin_info: &WrappedBinaryInfo,
    hook_script_path: &Path,
) -> String {
    let wrapped_path_trimmed = trim_path_root(&bin_info.wrapped_path);

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
        wrapped_path_trimmed = wrapped_path_trimmed.display(),
        wrapped_bin_name = bin_info.wrapped_exec_name,
        hook_script_path = hook_script_path.display(),
    }
}

// TODO: add ability to remove installed hooks as well
pub fn generate_removal(bin_info: &WrappedBinaryInfo) -> String {
    let wrapped_path_trimmed = trim_path_root(&bin_info.wrapped_path);

    formatdoc! { r#"
        [Trigger]
        Type = File
        Operation = Remove
        Target = {wrapped_path_trimmed}

        [Action]
        Description = Removing traces of wrapper for {wrapped_bin_name} executable...
        When = PostTransaction
        Exec = /usr/bin/rm "{unwrapped_path}"
        "#,
        wrapped_path_trimmed = wrapped_path_trimmed.display(),
        wrapped_bin_name = bin_info.wrapped_exec_name,
        unwrapped_path = bin_info.unwrapped_path.display(),
    }
}
