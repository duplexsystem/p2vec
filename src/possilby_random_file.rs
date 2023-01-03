use std::fs::File;
use std::io;
use std::io::Error;

use positioned_io::{RandomAccessFile, ReadAt};

pub struct PossiblyRandomFile {
    file: Option<File>,
    random_file: Option<RandomAccessFile>,
}

impl PossiblyRandomFile {
    pub fn new_from_random_file(random_file: RandomAccessFile) -> PossiblyRandomFile {
        PossiblyRandomFile {
            file: None,
            random_file: Some(random_file),
        }
    }

    pub fn new_from_file(file: File) -> PossiblyRandomFile {
        PossiblyRandomFile {
            file: Some(file),
            random_file: None,
        }
    }

    pub fn read_at(&self, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        if self.random_file.is_some() {
            self.random_file.as_ref().unwrap().read_at(pos, buf)
        } else {
            self.file.as_ref().unwrap().read_at(pos, buf)
        }
    }

    pub fn try_into_inner(self) -> Result<File, (RandomAccessFile, Error)> {
        if self.random_file.is_some() {
            self.random_file.unwrap().try_into_inner()
        } else {
            Ok(self.file.unwrap())
        }
    }
}