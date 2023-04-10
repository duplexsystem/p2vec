use std::fs;
use std::fs::{File, OpenOptions};
use std::io::Error;
use std::path::Path;

use close_err::Closable;
use fs3::FileExt;
use libc::c_int;

pub(crate) fn file_advise(file: &File, advice: c_int) -> Result<(), Error> {
    let mut error = 0;
    #[cfg(all(unix, target_os = "linux"))]
    unsafe {
        use std::os::fd::AsRawFd;
        error = libc::posix_fadvise64(file.as_raw_fd(), 0, 0, advice);
    }

    match error {
        0 => Ok(()),
        _ => Err(Error::from_raw_os_error(error)),
    }
}

pub(crate) fn open_file(initial_size: usize, path: &Path) -> Result<File, Error> {
    if !path.is_file() {
        fs::create_dir_all(match path.parent() {
            None => return Err(Error::new(std::io::ErrorKind::Other, "Invalid Directory")),
            Some(path) => path,
        })?;
    }

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)?;

    file.try_lock_exclusive()?;

    file_advise(&file, libc::POSIX_FADV_WILLNEED)?;

    file.allocate(initial_size as u64)?;

    Ok(file)
}

pub(crate) fn close_file(file: File) -> Result<(), Error> {
    file.unlock()?;

    file.close()
}
