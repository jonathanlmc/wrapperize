mod error;
mod file;
mod pacman_hook;
mod script;

use anyhow::Context;
use argh::FromArgs;
use error::IoError;
use std::{fs, os::unix::process::ExitStatusExt, path::PathBuf, process::Command};
use tap::Tap;

#[derive(FromArgs)]
/// Wrap an executable to always execute with additional arguments or environment variables.
struct Args {
    #[argh(positional)]
    binary_path: PathBuf,

    /// an additional argument to launch the binary with; can be used multiple times
    #[argh(option, short = 'a', long = "arg")]
    args: Vec<String>,
}

impl Args {
    fn verify(&self) -> anyhow::Result<()> {
        if self.args.is_empty() {
            anyhow::bail!("no arguments provided to wrap");
        }

        let binary_exists = self.binary_path.try_exists().with_context(|| {
            IoError::new(
                &self.binary_path,
                "failed to check if specified path exists",
            )
        })?;

        if !binary_exists {
            return Err(IoError::new(&self.binary_path, "path does not exist").into());
        }

        if !self.binary_path.is_file() {
            return Err(IoError::new(&self.binary_path, "path does not point to a file").into());
        }

        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let args: Args = argh::from_env();
    args.verify()?;

    let bin_info = WrappedBinaryInfo::try_from_path(args.binary_path.clone())?;

    let wrapper_already_exists = bin_info.wrapped_path.try_exists().with_context(|| {
        IoError::new(
            &bin_info.wrapped_path,
            "failed to check if wrapped path already exists",
        )
    })?;

    if wrapper_already_exists {
        return Err(IoError::new(
            &args.binary_path,
            format!(
                "wrapper already exists for this file at `{}`",
                bin_info.unwrapped_path.display()
            ),
        )
        .into());
    }

    pacman_hook::create_dir()?;

    let wrapper_install_script = script::generate_wrapper_install(&bin_info, &args.args);

    let wrapper_install_script_path = PathBuf::from(pacman_hook::HOOK_DIR).tap_mut(|p| {
        p.push(format!(
            "{wrapped_bin_name}-{program_name}-install.sh",
            wrapped_bin_name = bin_info.wrapped_exec_name,
            program_name = env!("CARGO_PKG_NAME")
        ))
    });

    file::write_with_execute_bit(
        &wrapper_install_script_path,
        wrapper_install_script.as_bytes(),
    )
    .with_context(|| {
        IoError::new(
            &wrapper_install_script_path,
            "failed to create install script for pacman hook",
        )
    })?;

    let pacman_install_hook_path = wrapper_install_script_path.with_extension("hook");

    let pacman_hook_content =
        pacman_hook::generate_install_and_update(&bin_info, &wrapper_install_script_path);

    fs::write(&pacman_install_hook_path, pacman_hook_content).with_context(|| {
        IoError::new(
            &pacman_install_hook_path,
            "failed to write pacman install hook",
        )
    })?;

    let status = Command::new(&wrapper_install_script_path)
        .status()
        .with_context(|| {
            IoError::new(
                wrapper_install_script_path,
                "failed to execute wrapper install script",
            )
        })?;

    if status.success() {
        println!(
            "wrapper successfully created for `{}`",
            bin_info.wrapped_path.display()
        );
    } else if let Some(code) = status.code() {
        eprintln!("wrapper install script failed with code `{code}`");
    } else if let Some(signal) = status.signal() {
        eprintln!("wrapper install script failed with signal `{signal}`");
    } else {
        eprintln!("wrapper install script failed");
    }

    Ok(())
}

struct WrappedBinaryInfo {
    unwrapped_path: PathBuf,
    wrapped_path: PathBuf,
    wrapped_exec_name: String,
}

impl WrappedBinaryInfo {
    fn try_from_path(path: PathBuf) -> anyhow::Result<Self> {
        let exec_name = path
            .file_name()
            .context("path does not point to a file")?
            .to_string_lossy()
            .into_owned();

        let unwrapped_path = path.with_file_name(format!(".{exec_name}-unwrapped"));

        Ok(Self {
            unwrapped_path,
            wrapped_path: path,
            wrapped_exec_name: exec_name,
        })
    }
}
