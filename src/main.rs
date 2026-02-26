mod env;
mod error;
mod file;
mod pacman_hook;
mod path;
mod wrapper;

use argh::FromArgs;
use color_eyre::eyre::{self, Context, eyre};
use std::{os::unix::process::ExitStatusExt, path::PathBuf};

use crate::error::ReportExt;

#[derive(FromArgs)]
/// Wrap an executable to always execute with additional arguments and/or environment variables.
struct Args<'a> {
    /// path of the executable to wrap
    #[argh(positional)]
    executable_path: PathBuf,

    /// an additional argument to launch the executable with; can be used multiple times
    #[argh(option, short = 'a', long = "arg")]
    args: Vec<String>,

    /// an environment variable in the format of `ENV=value` to launch the executable with; can be used multiple times
    #[argh(option, short = 'e', long = "env")]
    envs: Vec<env::Variable<'a>>,

    /// do not generate hooks for pacman; intended to be used for paths not managed by pacman (such as `/home`)
    #[argh(switch, long = "nohooks")]
    skip_pacman_hooks: bool,

    /// place the wrapper arguments after the passthrough arguments, so they are seen last by the wrapped executable
    #[argh(switch, long = "passthrough-args-first")]
    add_passthrough_args_first: bool,

    /// prevent the provided path from being canonicalized (made absolute); this
    /// can only be used in combination with `--nohooks` to minimize the chance
    /// of a path-related attack
    #[argh(switch, long = "keep-relative")]
    keep_relative_path: bool,
}

impl Args<'_> {
    fn process(&mut self) -> eyre::Result<()> {
        if self.args.is_empty() && self.envs.is_empty() {
            eyre::bail!("no arguments or environment variables provided to wrap");
        }

        match self.keep_relative_path {
            true => {
                if !self.skip_pacman_hooks {
                    eyre::bail!("relative paths can only be used with the `--nohooks` flag")
                }

                // nothing else to do; the given path can be relative or absolute
            }
            false => {
                self.executable_path = std::fs::canonicalize(&self.executable_path)
                    .wrap_err("failed to canonicalize path")
                    .with_path_section(&self.executable_path)?;
            }
        }

        let executable_exists = self
            .executable_path
            .try_exists()
            .wrap_err("failed to check if specified path exists")
            .with_path_section(&self.executable_path)?;

        if !executable_exists {
            return Err(eyre!("path does not exist")).with_path_section(&self.executable_path);
        }

        if !self.executable_path.is_file() {
            return Err(eyre!("path does not point to a file"))
                .with_path_section(&self.executable_path);
        }

        Ok(())
    }
}

fn main() -> eyre::Result<()> {
    color_eyre::config::HookBuilder::default()
        .display_env_section(false)
        .install()?;

    let args = {
        let mut args: Args = argh::from_env();
        args.process()?;
        args
    };

    let wrapper_paths = wrapper::ExecPaths::try_from_path(&args.executable_path)?;

    let wrapper_params = wrapper::Params {
        args: &args.args,
        add_passthrough_args_first: args.add_passthrough_args_first,
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
