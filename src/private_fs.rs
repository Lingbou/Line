use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub(crate) fn create_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    set_private_mode(path, 0o700)
}

pub(crate) fn set_private_mode(path: &Path, mode: u32) -> io::Result<()> {
    crate::platform::current().set_private_mode(path, mode)
}

pub(crate) fn write_new_private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    set_private_mode(path, 0o600)
}

pub(crate) fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}
