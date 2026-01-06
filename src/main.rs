mod error;
mod file;
mod pacman_hook;
mod script;

use anyhow::Context;
use argh::FromArgs;
use error::IoError;
use std::{
    io::Write,
    os::unix::process::ExitStatusExt,
    path::PathBuf,
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

    let wrapper_install_script_status =
        create_wrapper_for_binary(&bin_info, &wrapper_params, !args.skip_pacman_hooks)?
            .execute()?;

    if wrapper_install_script_status.success() {
        println!(
            "wrapper successfully created for `{}`",
            bin_info.wrapped_path.display()
        );
    } else if let Some(code) = wrapper_install_script_status.code() {
        eprintln!("wrapper install script failed with code `{code}`");
    } else if let Some(signal) = wrapper_install_script_status.signal() {
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

    // the wrapper install script uses the same path / filename as the pacman install hook but with a different
    // extension, so we can initialize the pacman install hook now to simply clone and alter its pre-computed path
    // for the wrapper install script
    let pacman_install_hook = pacman_hook::Hook::new(
        &bin_info.wrapped_exec_name,
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
        WrapperInstallScript::create(bin_info, &wrapper_script, wrapper_install_script_path)?;

    if !use_pacman_hooks {
        return Ok(wrapper_install_script);
    }

    // since we are using pacman hooks, generate their contents and write them all to disk now

    pacman_hook::create_dir()?;

    pacman_install_hook.generate_and_write_to_disk(bin_info)?;

    pacman_hook::Hook::new(
        &bin_info.wrapped_exec_name,
        pacman_hook::TriggerAction::Removal,
    )
    .generate_and_write_to_disk(bin_info)?;

    Ok(wrapper_install_script)
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
        save_to_disk: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        let wrapper_install_script = script::generate_wrapper_install(bin_info, wrapper_script)
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

        Ok(WrapperInstallScript::Saved(path))
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
