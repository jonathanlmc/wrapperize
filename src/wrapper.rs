use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
};

use anyhow::Context;
use tap::Tap;

use crate::{EscapedPath, error::IoError, file, pacman_hook, script};

pub use script::WrapperParams;

pub struct GeneratedPaths {
    pub unwrapped_path: EscapedPath,
    pub wrapped_path: EscapedPath,
    pub wrapped_filename: String,
}

impl GeneratedPaths {
    pub fn try_from_path(path: &Path) -> anyhow::Result<Self> {
        let wrapped_path =
            EscapedPath::new(&path.to_string_lossy()).context("path is not valid")?;

        let filename = path
            .file_name()
            .context("invalid path provided")?
            .to_string_lossy()
            .into_owned();

        let unwrapped_path = EscapedPath::new(
            &path
                .with_file_name(format!(".{filename}-unwrapped"))
                .to_string_lossy(),
        )
        .context("generated unwrapped path is not valid")?;

        Ok(Self {
            unwrapped_path,
            wrapped_path,
            wrapped_filename: filename,
        })
    }
}

pub enum InstallScript {
    Saved(PathBuf),
    MemoryOnly(String),
}

impl InstallScript {
    pub fn create(
        paths: &GeneratedPaths,
        wrapper_script: &str,
        save_to_disk: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        let wrapper_install_script = script::generate_wrapper_install(paths, wrapper_script)
            .context("failed to generate wrapper install script")?;

        let Some(path) = save_to_disk else {
            return Ok(Self::MemoryOnly(wrapper_install_script));
        };

        file::write_with_execute_bit(&path, wrapper_install_script.as_bytes()).with_context(
            || {
                IoError::new(
                    &path,
                    "failed to write wrapper install script for pacman hook",
                )
            },
        )?;

        Ok(Self::Saved(path))
    }

    pub fn execute(self) -> anyhow::Result<process::ExitStatus> {
        match self {
            Self::MemoryOnly(script) => {
                let mut cmd = Command::new("/usr/bin/env")
                    .arg("bash")
                    .stdin(Stdio::piped())
                    .spawn()
                    .context("failed to spawn bash to execute wrapper installer")?;

                cmd.stdin
                    .take()
                    .context("no stdin configured for bash")?
                    .write_all(script.as_bytes())
                    .context("failed to pipe wrapper install script to bash")?;

                cmd.wait().map_err(Into::into)
            }
            Self::Saved(path) => Command::new(&path)
                .status()
                .with_context(|| IoError::new(path, "failed to execute wrapper install script")),
        }
    }
}

pub fn create(
    paths: &GeneratedPaths,
    wrapper_params: &WrapperParams,
    use_pacman_hooks: bool,
) -> anyhow::Result<InstallScript> {
    let wrapper_already_exists = paths.unwrapped_path.path.try_exists().with_context(|| {
        IoError::new(
            &paths.unwrapped_path.path,
            "failed to check if wrapped path already exists",
        )
    })?;

    if wrapper_already_exists {
        return Err(IoError::new(
            &paths.wrapped_path.path,
            format!(
                "wrapper already exists for this file at `{}`",
                paths.unwrapped_path.path.display()
            ),
        )
        .into());
    }

    let wrapper_script = script::generate_binary_wrapper(&paths.unwrapped_path, wrapper_params)
        .context("failed to generate binary wrapper")?;

    // the wrapper install script uses the same path / filename as the pacman install hook but with a different
    // extension, so we can initialize the pacman install hook now to simply clone and alter its pre-computed path
    // for the wrapper install script
    let pacman_install_hook = pacman_hook::Hook::new(
        &paths.wrapped_filename,
        pacman_hook::TriggerAction::InstallOrUpdate,
    );

    // the wrapper install script only needs a path if it's going to be saved to disk,
    // and we only need to write it to disk if we're generating pacman hooks
    let wrapper_install_script_path = use_pacman_hooks.then(|| {
        pacman_install_hook.path.clone().tap_mut(|p| {
            p.set_extension("sh");
        })
    });

    let wrapper_install_script =
        InstallScript::create(paths, &wrapper_script, wrapper_install_script_path)?;

    if !use_pacman_hooks {
        return Ok(wrapper_install_script);
    }

    // since we are using pacman hooks, generate their contents and write them all to disk now

    pacman_hook::create_dir()?;

    pacman_install_hook.generate_and_write_to_disk(paths)?;

    pacman_hook::Hook::new(&paths.wrapped_filename, pacman_hook::TriggerAction::Removal)
        .generate_and_write_to_disk(paths)?;

    Ok(wrapper_install_script)
}
