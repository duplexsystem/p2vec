use close_err::Closable;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::Error;
use std::path::Path;

use fs3::FileExt;
use libc::c_int;

pub(crate) fn file_advise(file: &File, advice: c_int) {
    #[cfg(all(unix, target_os = "linux"))]
    unsafe {
        use std::os::fd::AsRawFd;
        libc::posix_fadvise(file.as_raw_fd(), 0, 0, advice);
    }
}

pub(crate) fn open_file_with_guaranteed_size(
    initial_size: usize,
    path: &Path,
) -> Result<File, Error> {
    if !path.is_file() {
        fs::create_dir_all(path.parent().unwrap())?;
    }

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)?;

    file.try_lock_exclusive()?;

    file_advise(&file, libc::POSIX_FADV_WILLNEED);

    file.allocate(initial_size as u64)?;

    Ok(file)
}

pub(crate) fn close_file(file: File) -> Result<(), Error> {
    file.unlock()?;

    file.close().unwrap();

    Ok(())
}
