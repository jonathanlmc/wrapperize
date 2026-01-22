mod env;
mod error;
mod file;
mod pacman_hook;
mod path;
mod script;
mod wrapper;

use anyhow::Context;
use argh::FromArgs;
use error::IoError;
use std::{os::unix::process::ExitStatusExt, path::PathBuf};

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
    envs: Vec<env::Variable<'a>>,

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

    let wrapper_paths = wrapper::ExecPaths::try_from_path(&args.binary_path)?;

    let wrapper_params = wrapper::WrapperParams {
        args: &args.args,
        env_vars: &args.envs,
    };

    let wrapper_install_script_status =
        wrapper::create(&wrapper_paths, &wrapper_params, !args.skip_pacman_hooks)?.execute()?;

    if wrapper_install_script_status.success() {
        println!(
            "wrapper successfully created for `{}`",
            wrapper_paths.wrapped.original.display()
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
