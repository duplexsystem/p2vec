use std::borrow::Cow;
use std::fs::File;
use std::io::{Error, Write};
use std::ops::Range;
use std::path::Path;

use memmap2::MmapMut;
use positioned_io::ReadAt;

use crate::file_util::{close_file, file_advise, open_file};

pub(crate) struct MemoryMappedFile {
    file: File,
    data: MmapMut,
    memory_size: usize,
}

impl MemoryMappedFile {
    pub(crate) fn open_file(
        initial_size: usize,
        path: &Path,
        is_random: bool,
    ) -> Result<MemoryMappedFile, Error> {
        let file = open_file(initial_size, path)?;

        let data = unsafe { MmapMut::map_mut(&file) }?;

        let memory_size = file.metadata()?.len() as usize;

        data.advise(memmap2::Advice::WillNeed)?;

        match is_random {
            true => {
                file_advise(&file, libc::POSIX_FADV_RANDOM)?;
                data.advise(memmap2::Advice::Random)?;
            }
            false => {
                file_advise(&file, libc::POSIX_FADV_SEQUENTIAL)?;
                data.advise(memmap2::Advice::Sequential)?;
            }
        };

        Ok(MemoryMappedFile {
            file,
            data,
            memory_size,
        })
    }

    pub(crate) fn close_file(self) -> Result<(), Error> {
        self.data.flush()?;

        close_file(self.file)
    }

    pub(crate) fn read_file(&self, range: Range<usize>) -> Result<Cow<[u8]>, Error> {
        if range.end <= self.memory_size {
            return Ok(Cow::Borrowed(&self.data[range]));
        } else if range.start <= self.memory_size {
            let mut vector = Vec::new();

            vector.resize(range.len(), 0u8);

            vector.write_all(&self.data[range.start..self.memory_size])?;

            self.file.read_at(
                (self.memory_size + 1) as u64,
                &mut vector.as_mut_slice()[self.memory_size - range.start..range.len()],
            )?;

            return Ok(Cow::Owned(vector));
        }

        let mut data = Vec::new();
        data.resize(range.end - range.start, 0u8);

        self.file.read_at(range.start as u64, &mut data)?;

        Ok(Cow::Owned(data))
    }

    pub(crate) fn get_file_size(&self) -> Result<u64, Error> {
        Ok(self.file.metadata()?.len())
    }
}
