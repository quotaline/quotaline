//! Atomically writes private credentials and JSON state beside their destinations.

use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

mod credential_update;
pub mod logs;

pub use credential_update::{CredentialFileError, CredentialFileGuard};

pub fn write_secret_file(path: &Path, data: &[u8]) -> io::Result<()> {
    atomic_write(path, data, false)
}

pub fn write_private_file(path: &Path, data: &[u8]) -> io::Result<usize> {
    atomic_write(path, data, true)?;
    Ok(data.len())
}

/// Opens a file only its owner can read, tightening an existing file that `options` keeps.
#[cfg(unix)]
pub fn open_private(options: &mut OpenOptions, path: &Path) -> io::Result<File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let file = options.mode(0o600).open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

/// Opens a file under the user's profile, whose inherited access list already keeps it to its owner.
#[cfg(windows)]
pub fn open_private(options: &mut OpenOptions, path: &Path) -> io::Result<File> {
    options.open(path)
}

pub fn read_optional_file(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(data) => Ok(Some(data)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn atomic_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let mut data = serde_json::to_vec(value).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    data.push(b'\n');
    atomic_write(path, &data, true)
}

fn atomic_write(path: &Path, data: &[u8], create_parent: bool) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if create_parent {
        fs::create_dir_all(parent)?;
    }
    let destination = destination(path, parent)?;
    let mut file = tempfile::NamedTempFile::new_in(destination.parent().expect("destination has a parent"))?;
    file.write_all(data)?;
    file.as_file().sync_all()?;
    file.persist(destination).map_err(|error| error.error)?;
    Ok(())
}

fn destination(path: &Path, parent: &Path) -> io::Result<PathBuf> {
    if let Ok(canonical) = path.canonicalize() {
        return Ok(canonical);
    }
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "destination has no filename"))?;
    Ok(parent.canonicalize()?.join(name))
}

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;
