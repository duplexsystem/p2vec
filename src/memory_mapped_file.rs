use std::borrow::Cow;
use std::fs;
use std::fs::OpenOptions;
use std::io::Error;
use std::ops::Range;
use std::path::Path;

use close_file::Closable;
use fs3::FileExt;
use memmap2::MmapMut;
use positioned_io::{RandomAccessFile, ReadAt};

pub struct MemoryMappedFile {
    file: RandomAccessFile,
    data: MmapMut,
    memory_size: usize,
}

impl MemoryMappedFile {
    pub fn open_file(path: &Path) -> Result<MemoryMappedFile, Error> {
        Self::open_file_with_guaranteed_size(0, path)
    }

    pub fn open_file_with_guaranteed_size(
        initial_size: usize,
        path: &Path,
    ) -> Result<MemoryMappedFile, Error> {
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

        let memory_size = file.metadata()?.len() as usize;

        let random_file = RandomAccessFile::try_new(file)?;

        data.advise(memmap2::Advice::Random).unwrap();

        data.advise(memmap2::Advice::WillNeed).unwrap();

        Ok(MemoryMappedFile {
            file: random_file,
            data,
            memory_size,
        })
    }

    pub fn close_file(self) -> Result<(), Error> {
        self.data.flush().unwrap();

        let file = self.file.try_into_inner().unwrap();

        file.unlock().unwrap();

        file.close().unwrap();

        Ok(())
    }

    pub fn read_file(&self, range: Range<usize>) -> Result<Cow<[u8]>, Error> {
        if range.end <= self.memory_size {
            return Ok(Cow::Borrowed(&self.data[range]));
        }

        let mut data = Vec::new();
        data.resize(range.end - range.start, 0u8);

        Self::read_file_disk(&self.file, range.start, &mut data)?;

        Ok(Cow::Owned(data))
    }

    fn read_file_disk(file: &RandomAccessFile, start: usize, buf: &mut [u8]) -> Result<(), Error> {
        file.read_at(start as u64, buf)?;

        Ok(())
    }
}
