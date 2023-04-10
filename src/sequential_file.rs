use std::fs::File;
use std::io::Error;

use positioned_io::ReadAt;

use crate::file_util::{close_file, file_advise};
use crate::specialized_file::SpecializedFile;

pub(crate) struct SequentialFile {
    file: File,
}

impl SpecializedFile for SequentialFile {
    fn close_file(self: Box<Self>) -> Result<(), Error> {
        close_file(self.file)
    }

    fn read_file(&self, start: usize, buf: &mut [u8]) -> Result<(), Error> {
        self.file.read_at(start as u64, buf)?;

        Ok(())
    }

    fn get_file_size(self) -> Result<(Box<dyn SpecializedFile + Send + Sync>, usize), Error> {
        let size = self.file.metadata()?.len() as usize;
        Ok((Box::new(self), size))
    }
}

pub(crate) fn specialize_file(file: File) -> Result<SequentialFile, Error> {
    file_advise(&file, libc::POSIX_FADV_SEQUENTIAL);

    Ok(SequentialFile { file })
}
