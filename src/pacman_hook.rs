use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use indoc::formatdoc;
use tap::Tap;

use crate::WrappedBinaryInfo;

/// Points to the user `pacman` hook directory.
pub const HOOK_DIR: &str = "/etc/pacman.d/hooks";

/// Create the user `pacman` hook directory if it doesn't exist.
/// Returns an error if the directory couldn't be created (likely due to permissions).
pub fn create_dir() -> anyhow::Result<()> {
    fs::create_dir_all(HOOK_DIR)
        .with_context(|| format!("failed to create pacman user hook directory at `{HOOK_DIR}`"))
}

/// A specific action / operation for a hook's target needed to trigger the hook.
#[derive(Copy, Clone)]
pub enum Action {
    /// The hook target was installed or updated.
    InstallOrUpdate,
    /// The hook target was uninstalled / removed.
    Removal,
}

impl Action {
    /// Returns the verb form of the action for use in paths.
    fn path_verb(self) -> &'static str {
        match self {
            Self::InstallOrUpdate => "install",
            Self::Removal => "remove",
        }
    }
}

/// Generate the full path for a `pacman` hook script.
pub fn get_hook_path(binary_name: &str, action: Action) -> PathBuf {
    PathBuf::from(HOOK_DIR).tap_mut(|p| {
        p.push(format!(
            "{binary_name}-{program_name}-{action}.hook",
            program_name = env!("CARGO_PKG_NAME"),
            action = action.path_verb(),
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
/// Generate a `pacman` hook to remove all wrapper traces when the specified wrapped binary is uninstalled.
/// Returns the generated hook string.
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

#[cfg(test)]
mod tests {
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

        fn test_get_hook_path_helper(binary_name: &str, action: Action, expected_suffix: &str) {
            let expected_program_name = env!("CARGO_PKG_NAME");
            let expected_path =
                format!("{HOOK_DIR}/{binary_name}-{expected_program_name}-{expected_suffix}.hook");

            let result = get_hook_path(binary_name, action);
            assert_eq!(result.to_string_lossy(), expected_path);
        }

        #[test]
        fn test_install_or_update() {
            test_get_hook_path_helper("test_binary", Action::InstallOrUpdate, "install");
        }

        #[test]
        fn test_removal() {
            test_get_hook_path_helper("test_binary", Action::Removal, "remove");
        }
    }

    #[test]
    fn test_generate_install_and_update() {
        let bin_info = WrappedBinaryInfo {
            wrapped_path: PathBuf::from("/usr/bin/test_executable"),
            wrapped_exec_name: "test_executable".to_string(),
            unwrapped_path: PathBuf::from("/usr/bin/original_executable"),
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
            wrapped_path: PathBuf::from("/usr/bin/wrapped_exec"),
            wrapped_exec_name: "wrapped_exec".to_string(),
            unwrapped_path: PathBuf::from("/usr/bin/original_exec"),
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
              Exec = /usr/bin/rm "/usr/bin/original_exec"
              "#
        };

        assert_eq!(result, expected);
    }
}
