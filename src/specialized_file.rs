use std::io::Error;

pub trait SpecializedFile {
    fn close_file(self: Box<Self>) -> Result<(), Error>;
    fn read_file(&self, start: usize, buf: &mut [u8]) -> Result<(), Error>;
}
