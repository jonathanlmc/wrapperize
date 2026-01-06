use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use indoc::formatdoc;
use tap::Tap;

use crate::{EscapedPath, WrappedBinaryInfo, error::IoError};

/// Points to the user `pacman` hook directory.
pub const HOOK_DIR: &str = "/etc/pacman.d/hooks";

/// Create the user `pacman` hook directory if it doesn't exist.
/// Returns an error if the directory couldn't be created (likely due to permissions).
pub fn create_dir() -> anyhow::Result<()> {
    fs::create_dir_all(HOOK_DIR)
        .with_context(|| format!("failed to create pacman user hook directory at `{HOOK_DIR}`"))
}

/// A trigger for a hook's target.
#[derive(Copy, Clone)]
pub enum TriggerAction {
    /// The hook target was installed or updated.
    InstallOrUpdate,
    /// The hook target was uninstalled / removed.
    Removal,
}

impl TriggerAction {
    /// Returns the verb form of the action for use in paths.
    fn path_verb(self) -> &'static str {
        match self {
            Self::InstallOrUpdate => "install",
            Self::Removal => "remove",
        }
    }
}

pub struct Hook {
    pub trigger_action: TriggerAction,
    pub path: PathBuf,
}

impl Hook {
    pub fn new(binary_name: &str, trigger_action: TriggerAction) -> Self {
        Self {
            trigger_action,
            path: path_for_binary(binary_name, trigger_action),
        }
    }

    pub fn generate_and_write_to_disk(&self, bin_info: &WrappedBinaryInfo) -> anyhow::Result<()> {
        let content = match self.trigger_action {
            TriggerAction::InstallOrUpdate => generate_install_and_update(bin_info, &self.path),
            TriggerAction::Removal => generate_removal(bin_info),
        };

        fs::write(&self.path, content).with_context(|| {
            IoError::new(
                &self.path,
                format!(
                    "failed to write pacman {} hook",
                    self.trigger_action.path_verb()
                ),
            )
        })
    }
}

/// Generate the full path for a `pacman` hook script.
fn path_for_binary(binary_name: &str, trigger_action: TriggerAction) -> PathBuf {
    PathBuf::from(HOOK_DIR).tap_mut(|p| {
        p.push(format!(
            "{binary_name}-{program_name}-{trigger_action}.hook",
            program_name = env!("CARGO_PKG_NAME"),
            trigger_action = trigger_action.path_verb(),
        ))
    })
}

/// Trim the leading slash from a path if one is present.
fn trim_path_root(path: impl Into<PathBuf>) -> PathBuf {
    let path = path.into();
    let path_str = path.to_string_lossy();

    path_str.strip_prefix('/').map(Into::into).unwrap_or(path)
}

/// Generate a `pacman` hook to execute the script at the path given by
/// `hook_script_path` when the provided wrapped binary is installed or updated.
///
/// Returns the generated hook string.
pub fn generate_install_and_update(
    bin_info: &WrappedBinaryInfo,
    hook_script_path: &Path,
) -> String {
    let wrapped_path_trimmed = trim_path_root(&bin_info.wrapped_path.path);

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
/// Generate a `pacman` hook to remove all wrapper traces when the specified wrapped binary is uninstalled.
/// Returns the generated hook string.
pub fn generate_removal(bin_info: &WrappedBinaryInfo) -> String {
    let wrapped_path_trimmed = trim_path_root(&bin_info.wrapped_path);

    formatdoc! { r#"
        [Trigger]
        Type = File
        {operations}
        Target = {trimmed_target_path}

        [Action]
        Description = {description}
        When = PostTransaction
        Exec = {exec_str}
        "#,
        operations = trigger.operations_str(),
        trimmed_target_path = trimmed_target_path.display(),
    }
}

#[cfg(test)]
mod tests {
    use crate::EscapedPath;

    use super::*;

    mod trim_path_root {
        use super::*;

        #[test]
        fn test_absolute() {
            let input = PathBuf::from("/home/user/file");
            let expected = PathBuf::from("home/user/file");
            assert_eq!(trim_path_root(input), expected);
        }

        #[test]
        fn test_relative() {
            let input = PathBuf::from("relative/path");
            let expected = PathBuf::from("relative/path");
            assert_eq!(trim_path_root(input), expected);
        }
    }

    mod get_hook_path_tests {
        use super::*;

        fn test_get_hook_path_helper(
            binary_name: &str,
            trigger_action: TriggerAction,
            expected_suffix: &str,
        ) {
            let expected_program_name = env!("CARGO_PKG_NAME");
            let expected_path =
                format!("{HOOK_DIR}/{binary_name}-{expected_program_name}-{expected_suffix}.hook");

            let result = path_for_binary(binary_name, trigger_action);
            assert_eq!(result.to_string_lossy(), expected_path);
        }

        #[test]
        fn test_install_or_update() {
            test_get_hook_path_helper("test_binary", TriggerAction::InstallOrUpdate, "install");
        }

        #[test]
        fn test_removal() {
            test_get_hook_path_helper("test_binary", TriggerAction::Removal, "remove");
        }
    }

    #[test]
    fn test_generate_install_and_update() {
        let bin_info = WrappedBinaryInfo {
            wrapped_path: EscapedPath::new("/usr/bin/test_executable").unwrap(),
            wrapped_exec_name: "test_executable".to_string(),
            unwrapped_path: EscapedPath::new("/usr/bin/original_executable").unwrap(),
        };

        let hook_script_path = PathBuf::from("/etc/test_script.sh");

        let result = generate_install_and_update(&bin_info, &hook_script_path);

        let expected = formatdoc! { r#"
              [Trigger]
              Type = File
              Operation = Install
              Operation = Upgrade
              Target = usr/bin/test_executable

              [Action]
              Description = Wrapping test_executable executable...
              When = PostTransaction
              Exec = /etc/test_script.sh
              "#
        };

        assert_eq!(result, expected);
    }

    #[test]
    fn test_generate_removal() {
        let bin_info = WrappedBinaryInfo {
            wrapped_path: EscapedPath::new("/usr/bin/wrapped_exec").unwrap(),
            wrapped_exec_name: "wrapped_exec".to_string(),
            unwrapped_path: EscapedPath::new("/usr/bin/original_exec").unwrap(),
        };

        let result = generate_removal(&bin_info);

        let expected = formatdoc! { r#"
              [Trigger]
              Type = File
              Operation = Remove
              Target = usr/bin/wrapped_exec

              [Action]
              Description = Removing traces of wrapper for wrapped_exec executable...
              When = PostTransaction
              Exec = /usr/bin/rm /usr/bin/original_exec
              "#
        };

        assert_eq!(result, expected);
    }
}
