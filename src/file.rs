use std::{fs::File, io::Write, os::unix::fs::PermissionsExt, path::Path};

use color_eyre::eyre::{self, Context};
use tap::Tap;

pub fn write_with_execute_bit(path: &Path, content: &[u8]) -> eyre::Result<()> {
    let mut file = File::create(path).wrap_err("failed to create file")?;
    file.write_all(content)
        .wrap_err("failed to write to file")?;

    let file_perms = file
        .metadata()
        .wrap_err("failed to get metadata for created file")?
        .permissions()
        .tap_mut(|p| {
            // add the execute bit to the current file permissions
            p.set_mode(p.mode() | 0o700)
        });

    file.set_permissions(file_perms)
        .wrap_err("failed to set execute bit for file")?;

    Ok(())
}
