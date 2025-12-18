# wrapperize

This is a small Rust program for Linux intended to make it easy to create global wrappers for specific programs so they are always launched with additional arguments and/or environment variables. It is currently only meant to be used on Arch Linux, as it generates `pacman` hooks to recreate the wrapper when a wrapped program is reinstalled or otherwise updated.

At the moment it's still a bit of a WIP and needs the following features to be considered ready for use:
* Environment variable support.
* Removal hook for `pacman`, so no trace of a wrapper is left behind if its associated program is uninstalled.
* Code cleanup & tests.

Once those items are complete a more helpful README describing its use will be written. For the time being, the program can be ran with `--help` to get an idea for its use.
