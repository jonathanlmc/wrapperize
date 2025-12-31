mod error;
mod file;
mod pacman_hook;
mod script;

use anyhow::Context;
use argh::FromArgs;
use error::IoError;
use std::{
    fs,
    io::Write,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
};
use tap::Tap;

#[derive(FromArgs)]
/// Wrap an executable to always execute with additional arguments or environment variables.
struct Args<'a> {
    #[argh(positional)]
    binary_path: PathBuf,

    /// an additional argument to launch the binary with; can be used multiple times
    #[argh(option, short = 'a', long = "arg")]
    args: Vec<String>,

    /// an environment variable in the format of `ENV=value` to launch the binary with; can be used multiple times
    #[argh(option, short = 'e', long = "env")]
    envs: Vec<script::EnvVar<'a>>,

    /// do not generate hooks for pacman; intended to be used for paths not managed by pacman (such as `/home`)
    #[argh(switch, long = "nohooks")]
    skip_pacman_hooks: bool,
}

impl Args<'_> {
    fn verify(&self) -> anyhow::Result<()> {
        if self.args.is_empty() && self.envs.is_empty() {
            anyhow::bail!("no arguments or environment variables provided to wrap");
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

        if !self.binary_path.is_absolute() {
            return Err(IoError::new(&self.binary_path, "path must be absolute").into());
        }

        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let args: Args = argh::from_env();
    args.verify()?;

    let bin_info = WrappedBinaryInfo::try_from_path(args.binary_path.clone())?;

    let wrapper_params = script::WrapperParams {
        args: &args.args,
        env_vars: &args.envs,
    };

    let script_status =
        create_wrapper_for_binary(&bin_info, &wrapper_params, !args.skip_pacman_hooks)?
            .execute()?;

    if script_status.success() {
        println!(
            "wrapper successfully created for `{}`",
            bin_info.wrapped_path.display()
        );
    } else if let Some(code) = script_status.code() {
        eprintln!("wrapper install script failed with code `{code}`");
    } else if let Some(signal) = script_status.signal() {
        eprintln!("wrapper install script failed with signal `{signal}`");
    } else {
        eprintln!("wrapper install script failed");
    }

    Ok(())
}

fn create_wrapper_for_binary(
    bin_info: &WrappedBinaryInfo,
    wrapper_params: &script::WrapperParams,
    use_pacman_hooks: bool,
) -> anyhow::Result<WrapperInstallScript> {
    let wrapper_already_exists = bin_info.unwrapped_path.try_exists().with_context(|| {
        IoError::new(
            &bin_info.unwrapped_path,
            "failed to check if wrapped path already exists",
        )
    })?;

    if wrapper_already_exists {
        return Err(IoError::new(
            &bin_info.wrapped_path,
            format!(
                "wrapper already exists for this file at `{}`",
                bin_info.unwrapped_path.display()
            ),
        )
        .into());
    }

    let wrapper_script = script::generate_binary_wrapper(&bin_info.unwrapped_path, wrapper_params)
        .context("failed to generate binary wrapper")?;

    WrapperInstallScript::create(bin_info, &wrapper_script, use_pacman_hooks)
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
            .context("invalid path provided")?
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

enum WrapperInstallScript {
    Saved(PathBuf),
    MemoryOnly(String),
}

impl WrapperInstallScript {
    fn create(
        bin_info: &WrappedBinaryInfo,
        wrapper_script: &str,
        using_pacman_hooks: bool,
    ) -> anyhow::Result<Self> {
        let wrapper_install_script = script::generate_wrapper_install(bin_info, wrapper_script)
            .context("failed to generate wrapper install script")?;

        if !using_pacman_hooks {
            return Ok(Self::MemoryOnly(wrapper_install_script));
        }

        let wrapper_install_script_path =
            Self::write_pacman_hooks_for_script(bin_info, &wrapper_install_script)?;

        Ok(WrapperInstallScript::Saved(wrapper_install_script_path))
    }

    fn write_pacman_hooks_for_script(
        bin_info: &WrappedBinaryInfo,
        wrapper_install_script: &str,
    ) -> anyhow::Result<PathBuf> {
        pacman_hook::create_dir()?;

        let wrapper_install_script_path = pacman_hook::get_hook_path(
            &bin_info.wrapped_exec_name,
            pacman_hook::Action::InstallOrUpdate,
        )
        .tap_mut(|p| {
            p.set_extension("sh");
        });

        file::write_with_execute_bit(
            &wrapper_install_script_path,
            wrapper_install_script.as_bytes(),
        )
        .with_context(|| {
            IoError::new(
                &wrapper_install_script_path,
                "failed to write wrapper install script for pacman hook",
            )
        })?;

        write_pacman_hooks(bin_info, &wrapper_install_script_path)?;

        Ok(wrapper_install_script_path)
    }

    fn execute(self) -> anyhow::Result<process::ExitStatus> {
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

fn write_pacman_hooks(
    bin_info: &WrappedBinaryInfo,
    wrapper_install_script_path: &Path,
) -> anyhow::Result<()> {
    let install_hook_content =
        pacman_hook::generate_install_and_update(bin_info, wrapper_install_script_path);

    let install_hook_path = wrapper_install_script_path.with_extension("hook");

    fs::write(&install_hook_path, install_hook_content)
        .with_context(|| IoError::new(&install_hook_path, "failed to write pacman install hook"))?;

    let remove_hook_path =
        pacman_hook::get_hook_path(&bin_info.wrapped_exec_name, pacman_hook::Action::Removal);

    let remove_hook_content = pacman_hook::generate_removal(bin_info);

    fs::write(&remove_hook_path, remove_hook_content)
        .with_context(|| IoError::new(&remove_hook_path, "failed to write pacman remove hook"))?;

    Ok(())
}
