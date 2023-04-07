use std::borrow::Cow;
use std::io::{Error, Write};
use std::ops::Range;
use std::path::Path;

use memmap2::MmapMut;

use crate::file_util::open_file_with_guaranteed_size;
use crate::specialized_file::SpecializedFile;
use crate::{random_file, sequential_file};

pub(crate) struct MemoryMappedFile {
    file: Box<dyn SpecializedFile + Send + Sync>,
    data: MmapMut,
    pub(crate) memory_size: usize,
}

impl MemoryMappedFile {
    pub(crate) fn open_file(
        initial_size: usize,
        path: &Path,
        is_random: bool,
    ) -> Result<MemoryMappedFile, Error> {
        let file = open_file_with_guaranteed_size(initial_size, path)?;

        let data = unsafe { MmapMut::map_mut(&file) }?;

        let memory_size = file.metadata()?.len() as usize;

        data.advise(memmap2::Advice::WillNeed)?;

        let file = match is_random {
            true => {
                data.advise(memmap2::Advice::Random)?;
                Box::new(random_file::specialize_file(file)?)
                    as Box<dyn SpecializedFile + Send + Sync>
            }
            false => {
                data.advise(memmap2::Advice::Sequential)?;
                Box::new(sequential_file::specialize_file(file)?)
                    as Box<dyn SpecializedFile + Send + Sync>
            }
        };

        Ok((MemoryMappedFile {
            file,
            data,
            memory_size,
        }))
    }

    pub(crate) fn close_file(self) -> Result<(), Error> {
        self.data.flush()?;

        self.file.close_file()
    }

    pub(crate) fn read_file(&self, range: Range<usize>) -> Result<Cow<[u8]>, Error> {
        if range.end <= self.memory_size {
            return Ok(Cow::Borrowed(&self.data[range]));
        } else if range.start <= self.memory_size {
            let mut vector = Vec::new();

            vector.resize(range.len(), 0u8);

            vector
                .write_all(&self.data[range.start..self.memory_size])
                .unwrap();

            self.file.read_file(
                self.memory_size + 1,
                &mut vector.as_mut_slice()[self.memory_size - range.start..range.len()],
            )?;

            return Ok(Cow::Owned(vector));
        }

        let mut data = Vec::new();
        data.resize(range.end - range.start, 0u8);

        self.file.read_file(range.start, &mut data)?;

        Ok(Cow::Owned(data))
    }
}
