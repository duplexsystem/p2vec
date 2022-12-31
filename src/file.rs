use std::fs;
use std::fs::{File, OpenOptions};
use std::io::Error;
use std::path::Path;

use close_file::Closable;
use fs3::FileExt;
use memmap2::MmapMut;

pub struct MemoryMappedFile {
    file: File,
    pub(crate) data: MmapMut,
}

pub fn open_file(path: &Path) -> Result<MemoryMappedFile, Error> {
    if !path.is_file() {
        fs::create_dir_all(path.parent().unwrap())?;
    }

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)?;

    file.try_lock_exclusive().unwrap();

    let data = unsafe { MmapMut::map_mut(&file) }?;

    data.advise(memmap2::Advice::Random).unwrap();

    data.advise(memmap2::Advice::WillNeed).unwrap();

    Ok(MemoryMappedFile { file, data })
}

pub fn close_file(file: MemoryMappedFile) -> Result<(), Error> {
    file.data.flush().unwrap();

    file.file.unlock().unwrap();

    file.file.close().unwrap();

    Ok(())
}
