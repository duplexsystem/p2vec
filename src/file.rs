use std::fs;
use std::fs::{File, OpenOptions};
use std::io::Error;
use std::path::Path;

pub fn open_file(path: &Path) -> Result<File, Error> {
    if !path.is_file() {
        fs::create_dir_all(path.parent().unwrap())?;
    }

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)?;

    Ok(file)
}
