use std::{
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use indoc::{concatdoc, formatdoc};
use strum::IntoEnumIterator;
use tap::Tap;

use crate::{error::IoError, path, wrapper};

/// Points to the user `pacman` hook directory.
pub const HOOK_DIR: &str = "/etc/pacman.d/hooks";

/// Create the user `pacman` hook directory if it doesn't exist.
/// Returns an error if the directory couldn't be created (likely due to permissions).
pub fn create_dir() -> anyhow::Result<()> {
    fs::create_dir_all(HOOK_DIR)
        .with_context(|| format!("failed to create pacman user hook directory at `{HOOK_DIR}`"))
}

/// A trigger for a hook's target.
#[derive(Debug, strum::EnumIter)]
pub enum TriggerAction {
    /// The hook target was installed or updated.
    InstallOrUpdate,
    /// The hook target was uninstalled / removed.
    Removal {
        wrapper_install_script_path: PathBuf,
    },
}

impl TriggerAction {
    /// Returns the verb form of the action for use in paths.
    fn path_verb(&self) -> &'static str {
        match self {
            Self::InstallOrUpdate => "install",
            Self::Removal { .. } => "remove",
        }
    }

    fn operations_str(&self) -> &'static str {
        match self {
            Self::InstallOrUpdate => concatdoc! { "
                Operation = Install
                Operation = Upgrade" },
            Self::Removal { .. } => "Operation = Remove",
        }
    }
}

pub struct Hook {
    pub trigger_action: TriggerAction,
    pub path: PathBuf,
}

impl Hook {
    pub fn new(target_filename: &str, trigger_action: TriggerAction) -> Self {
        let path = get_path(target_filename, &trigger_action);

        Self {
            trigger_action,
            path,
        }
    }

    pub fn generate_and_write_to_disk(self, paths: &wrapper::ExecPaths) -> anyhow::Result<()> {
        // `trigger_action` is moved below, so we need to get this now for error messages
        let trigger_path_verb = self.trigger_action.path_verb();

        let content = match self.trigger_action {
            TriggerAction::InstallOrUpdate => generate_install_and_update(paths, &self.path),
            TriggerAction::Removal {
                wrapper_install_script_path,
            } => generate_removal(paths, wrapper_install_script_path)
                .context("failed to generate content for pacman removal hook")?,
        };

        fs::write(&self.path, content).with_context(|| {
            IoError::new(
                &self.path,
                format!("failed to write pacman {trigger_path_verb} hook",),
            )
        })
    }
}

/// Generate the full path for a `pacman` hook script.
fn get_path(target_filename: &str, trigger_action: &TriggerAction) -> PathBuf {
    PathBuf::from(HOOK_DIR).tap_mut(|p| {
        p.push(format!(
            "{target_filename}-{program_name}-{trigger_action}.hook",
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
/// `hook_script_path` when the provided wrapped executable is installed or updated.
///
/// Returns the generated hook string.
pub fn generate_install_and_update(paths: &wrapper::ExecPaths, hook_script_path: &Path) -> String {
    generate(
        &paths.wrapped,
        TriggerAction::InstallOrUpdate,
        &format!("Wrapping {}...", paths.wrapped_filename),
        &hook_script_path.to_string_lossy(),
    )
}

/// Generate a `pacman` hook to remove all wrapper traces when the specified wrapped executable is uninstalled.
/// Returns the generated hook string.
pub fn generate_removal(
    paths: &wrapper::ExecPaths,
    wrapper_install_script_path: PathBuf,
) -> anyhow::Result<String> {
    let mut remove_cmd = String::from("/usr/bin/rm");

    // add all hook target paths for the wrapped executable to the remove command
    for action in TriggerAction::iter() {
        let path = get_path(&paths.wrapped_filename, &action)
            .to_string_lossy()
            // escape double quotes, since we'll be wrapping the path in our own
            .replace('"', "\\\"");

        write!(&mut remove_cmd, r#" "{}""#, path)
            .with_context(|| format!("failed to append path for `{action:?}` pacman hook"))?;
    }

    // also add the wrapper install script for removal
    write!(
        &mut remove_cmd,
        r#" "{}""#,
        wrapper_install_script_path
            .to_string_lossy()
            // escape double quotes since we're wrapping it with our own
            .replace('"', "\\\"")
    )
    .context("failed to append wrapper install script path")?;

    // include the unwrapped executable path since it isn't managed by pacman
    write!(&mut remove_cmd, r#" "{}""#, paths.unwrapped.escaped)
        .context("failed to append unwrapped executable path")?;

    let hook = generate(
        &paths.wrapped,
        TriggerAction::Removal {
            wrapper_install_script_path,
        },
        &format!(
            "Removing traces of wrapper for {}...",
            paths.wrapped_filename
        ),
        &remove_cmd,
    );

    Ok(hook)
}

fn generate(
    target_path: &path::Escaped,
    trigger: TriggerAction,
    description: &str,
    exec_str: &str,
) -> String {
    let trimmed_target_path = trim_path_root(&target_path.original);

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
            target_filename: &str,
            trigger_action: &TriggerAction,
            expected_suffix: &str,
        ) {
            let expected_program_name = env!("CARGO_PKG_NAME");
            let expected_path = format!(
                "{HOOK_DIR}/{target_filename}-{expected_program_name}-{expected_suffix}.hook"
            );

            let result = get_path(target_filename, trigger_action);
            assert_eq!(result.to_string_lossy(), expected_path);
        }

        fn gen_remove_trigger() -> TriggerAction {
            TriggerAction::Removal {
                wrapper_install_script_path: PathBuf::from("/wrapper/install/path"),
            }
        }

        #[test]
        fn test_install_or_update() {
            test_get_hook_path_helper("test_binary", &TriggerAction::InstallOrUpdate, "install");
        }

        #[test]
        fn test_removal() {
            test_get_hook_path_helper("test_binary", &gen_remove_trigger(), "remove");
        }
    }

    #[test]
    fn test_generate_install_and_update() {
        let paths = wrapper::ExecPaths {
            unwrapped: path::Escaped::new("/usr/bin/original_executable"),
            wrapped: path::Escaped::new("/usr/bin/test_executable"),
            wrapped_filename: "test_executable".to_string(),
        };

        let hook_script_path = PathBuf::from("/etc/test_script.sh");

        let result = generate_install_and_update(&paths, &hook_script_path);

        let expected = formatdoc! { r#"
              [Trigger]
              Type = File
              Operation = Install
              Operation = Upgrade
              Target = usr/bin/test_executable

              [Action]
              Description = Wrapping test_executable...
              When = PostTransaction
              Exec = /etc/test_script.sh
              "#
        };

        assert_eq!(result, expected);
    }

    #[test]
    fn test_generate_removal() {
        let bin_info = wrapper::ExecPaths {
            unwrapped: path::Escaped::new("/usr/bin/original_exec"),
            wrapped: path::Escaped::new("/usr/bin/wrapped_exec"),
            wrapped_filename: "wrapped_exec".to_string(),
        };

        let result = generate_removal(&bin_info, PathBuf::from("install/script"))
            .expect("expected generation to succeed");

        let expected = formatdoc! { r#"
              [Trigger]
              Type = File
              Operation = Remove
              Target = usr/bin/wrapped_exec

              [Action]
              Description = Removing traces of wrapper for wrapped_exec...
              When = PostTransaction
              Exec = /usr/bin/rm "/etc/pacman.d/hooks/wrapped_exec-wrapperize-install.hook" "/etc/pacman.d/hooks/wrapped_exec-wrapperize-remove.hook" "install/script" "/usr/bin/original_exec"
              "#
        };

        assert_eq!(result, expected);
    }
}
