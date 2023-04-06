use std::fs::File;
use std::io::Error;

use positioned_io::{RandomAccessFile, ReadAt};

use crate::file_util::close_file;
use crate::specialized_file::SpecializedFile;

pub(crate) struct RandomFile {
    file: RandomAccessFile,
}

impl SpecializedFile for RandomFile {
    fn close_file(self: Box<Self>) -> Result<(), Error> {
        let result = self.file.try_into_inner();

        if result.is_err() {
            let error = result.err().unwrap();

            return Err(error.1);
        }

        let file = result.unwrap();

        close_file(file)
    }

    fn read_file(&self, start: usize, buf: &mut [u8]) -> Result<(), Error> {
        self.file.read_at(start as u64, buf)?;

        Ok(())
    }

    fn get_file_size(self) -> Result<(Box<dyn SpecializedFile + Send + Sync>, usize), Error> {
        let file = self.file.try_into_inner().unwrap();
        let size = file.metadata()?.len() as usize;
        return Ok((Box::new(specialize_file(file)?), size));
    }
}

pub(crate) fn specialize_file(file: File) -> Result<RandomFile, Error> {
    let file = RandomAccessFile::try_new(file)?;

    Ok(RandomFile { file })
}
