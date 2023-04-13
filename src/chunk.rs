use std::io::Error;
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use glam::IVec2;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};

use crate::memory_mapped_file::MemoryMappedFile;
use crate::region::{MutableRegionMetadata, StaticRegionMetadata};
use crate::region_file_util::{
    get_chunk_compression_type, get_chunk_length, get_chunk_location, get_chunk_offset,
    get_chunk_timestamp, get_oversized_status,
};

pub(crate) struct ChunkGuard {
    pub(crate) chunk: RwLock<Chunk>,
    pub(crate) timestamp: AtomicU32,
}

pub(crate) struct Chunk {
    data: RwLock<Option<MemoryMappedFile>>,
}

impl Chunk {
    pub(crate) fn new(
        chunk_region_coords: IVec2,
        region_coords: IVec2,
        static_region_metadata: &StaticRegionMetadata,
    ) -> Result<(Self, Range<usize>), Error> {
        let file = match static_region_metadata.file.as_ref() {
            None => {
                return Err(Error::new(
                    std::io::ErrorKind::Other,
                    "Region file is not open",
                ));
            }
            Some(file) => file,
        };

        if static_region_metadata.file.is_none() {}

        let location = get_chunk_location(chunk_region_coords) as usize;

        let chunk_region_table_data = file.read_file(location..location + 4)?;

        let offset_data = &chunk_region_table_data[0..3];

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
        let mut file = match &static_region_metadata.file {
            Some(file) => file,
            None => {
                return Err(Error::new(
                    std::io::ErrorKind::Other,
                    "Region file is not open",
                ));
            }
        };

        let location = get_chunk_location(chunk_region_coords) as usize;

        let chunk_region_table_data = &(file.read_file(location..location + 4)?)[0..3];

        let offset = get_chunk_offset(chunk_region_table_data) as usize;

        let chunk_header_data = file.read_file(offset..offset + 5)?;

        let compression_byte = chunk_header_data[4];

        let compression_type = match get_chunk_compression_type(compression_byte) {
            None => {
                return Err(Error::new(
                    std::io::ErrorKind::Other,
                    "Invalid Compression Type",
                ));
            }
            Some(result) => result,
        };

        let oversized = get_oversized_status(compression_byte);

        let file_lock;

        let mut file_write_lock;

        let compressed_data = match oversized {
            true => {
                file_lock = self.data.upgradable_read();
                if file_lock.is_none() {
                    file_write_lock = RwLockUpgradableReadGuard::upgrade(file_lock);
                    file = file_write_lock.insert(Chunk::open_oversized_file(
                        static_region_metadata.directory,
                        chunk_coords,
                    )?);
                }
                file.read_file(0..file.get_file_size()? as usize)?
            }
            false => {
                let length_data = &chunk_header_data[0..4];

                let length = get_chunk_length(length_data) as usize;

                file.read_file(offset + 5..offset + 5 + length)?
            }
        };

        let data = compression_type.decompress(compressed_data)?;

        Ok(Some(data))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_chunk_data(
        &self,
        chunk_coords: IVec2,
        chunk_region_coords: IVec2,
        static_region_metadata: &StaticRegionMetadata,
        mutable_region_metadata: &MutableRegionMetadata,
        timestamp: u32,
        data: &[u8],
        alignment_data: &[u8],
    ) -> Result<(), Error> {
        let location = get_chunk_location(chunk_region_coords) as usize;

        let file = match &static_region_metadata.file {
            Some(file) => file,
            None => {
                return Err(Error::new(
                    std::io::ErrorKind::Other,
                    "Region file is not open",
                ));
            }
        };

        let timestamp_location = location * 2;

        if get_chunk_timestamp(&data[timestamp_location..timestamp_location + 2]) > timestamp as u32
        {
            return Ok(());
        }

        let chunk_region_table_data = &(file.read_file(location..location + 4)?);

        let offset = get_chunk_offset(&chunk_region_table_data[0..4]) as usize;

        let sectors = chunk_region_table_data[3] as usize;

        let mut wanted_sectors = data.len() >> 12;

        let oversized = wanted_sectors > u8::MAX as usize;

        if oversized {
            wanted_sectors = 1;
        }

        let location = Chunk::find_space_to_write(
            offset as u64,
            offset as u64 + sectors as u64,
            sectors as u64,
            wanted_sectors as u64,
            mutable_region_metadata,
        );

        Ok(())
    }

    pub(crate) fn find_space_to_write(
        current_start: u64,
        current_end: u64,
        current_sectors: u64,
        wanted_sectors: u64,
        mutable_region_metadata: &MutableRegionMetadata,
    ) {
        let mut new_start = 0;
        let mut new_end = 0;
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
