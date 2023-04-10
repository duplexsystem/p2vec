use std::io::Error;
use std::ops::Range;
use std::path::Path;

use glam::IVec2;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};

use crate::memory_mapped_file::MemoryMappedFile;
use crate::region::StaticRegionMetadata;
use crate::region_file_util::{
    get_chunk_compression_type, get_chunk_length, get_chunk_location, get_chunk_offset,
    get_oversized_status,
};

pub(crate) struct Chunk {
    data: RwLock<Option<MemoryMappedFile>>,
}

impl Chunk {
    pub(crate) fn new(
        chunk_region_coords: IVec2,
        region_coords: IVec2,
        static_region_metadata: &StaticRegionMetadata,
    ) -> Result<(Self, Range<usize>), Error> {
        let file = static_region_metadata.file.as_ref().unwrap();

        if static_region_metadata.file.is_none() {
            return Err(Error::new(
                std::io::ErrorKind::Other,
                "Region file is not open",
            ));
        }

        let location = get_chunk_location(chunk_region_coords) as usize;

        let chunk_region_table_data = file.read_file(location..location + 4)?;

        let offset_data: &[u8; 3] = chunk_region_table_data[0..3].try_into().unwrap();

        let offset = get_chunk_offset(offset_data) as usize;

        let file_offset = offset * 4096;

        let chunk_header_oversized_byte = file.read_file(file_offset + 4..file_offset + 5)?[0];

        let data: Option<MemoryMappedFile> = match get_oversized_status(chunk_header_oversized_byte)
        {
            true => {
                let chunk_coords: IVec2 = region_coords << 5 | chunk_region_coords;

                Some(MemoryMappedFile::open_file(
                    4096,
                    Path::new(&format!(
                        "{}/c.{}.{}.mcc",
                        static_region_metadata.directory, chunk_coords.x, chunk_coords.y
                    )),
                    false,
                )?)
            }
            false => None,
        };

        Ok((
            Chunk {
                data: RwLock::new(data),
            },
            offset..offset + (chunk_region_table_data[4] as usize),
        ))
    }

    pub(crate) fn read_chunk_data(
        &self,
        chunk_coords: IVec2,
        chunk_region_coords: IVec2,
        static_region_metadata: &StaticRegionMetadata,
    ) -> Result<Option<Vec<u8>>, Error> {
        let file = &static_region_metadata.file;

        if static_region_metadata.file.is_none() {
            return Err(Error::new(
                std::io::ErrorKind::Other,
                "Region file is not open",
            ));
        }

        let mut file = static_region_metadata.file.as_ref().unwrap();

        let location = get_chunk_location(chunk_region_coords) as usize;

        let chunk_region_table_data: &[u8; 3] = &(file.read_file(location..location + 4)?)[0..3]
            .try_into()
            .unwrap();

        let offset = get_chunk_offset(chunk_region_table_data) as usize;

        let chunk_header_data = file.read_file(offset..offset + 5)?;

        let compression_byte = chunk_header_data[4];

        let compression_type = get_chunk_compression_type(compression_byte).unwrap();

        let oversized = get_oversized_status(compression_byte);

        let file_lock;

        let mut file_write_lock;

        let compressed_data = match oversized {
            true => {
                file_lock = self.data.upgradable_read();
                if file_lock.is_none() {
                    file_write_lock = RwLockUpgradableReadGuard::upgrade(file_lock);
                    file = file_write_lock.insert(
                        Chunk::open_oversized_file(static_region_metadata.directory, chunk_coords)
                            .unwrap(),
                    );
                }
                file.read_file(0..file.file_size)?
            }
            false => {
                let length_data: &[u8; 4] = &chunk_header_data[0..4].try_into().unwrap();

                let length = get_chunk_length(length_data) as usize;

                static_region_metadata
                    .file
                    .as_ref()
                    .unwrap()
                    .read_file(offset + 5..offset + 5 + length)?
            }
        };

        let data = compression_type.decompress(compressed_data)?;

        Ok(Some(data))
    }

    pub(crate) fn open_oversized_file(
        directory: &'static str,
        chunk_coords: IVec2,
    ) -> Result<MemoryMappedFile, Error> {
        MemoryMappedFile::open_file(
            4096,
            Path::new(&format!(
                "{}/c.{}.{}.mcc",
                directory, chunk_coords.x, chunk_coords.y
            )),
            false,
        )
    }
}
