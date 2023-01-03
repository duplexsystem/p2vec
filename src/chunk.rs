use std::io::Error;
use std::ops::Range;
use std::path::Path;

use ahash::RandomState;
use dashmap::mapref::one::Ref;
use parking_lot::RwLock;

use crate::compression::CompressionType;
use crate::memory_mapped_file::MemoryMappedFile;
use crate::region::InnerRegion;

pub struct RegionHeaderData {
    pub(crate) offset: u16,
    pub(crate) location: u32,
    pub(crate) size: u8,
    pub(crate) end: u32,
    pub(crate) range: Range<u32>,
}

struct ChunkHeaderData {
    location: usize,
    length: u32,
    compression_type: CompressionType,
    is_oversized: bool,
    data_range: Range<usize>,
}

struct ChunkData {
    oversized_data: Option<MemoryMappedFile>,
}

pub struct Chunk {
    pub(crate) region_header_data: RwLock<RegionHeaderData>,
    chunk_header_data: RwLock<ChunkHeaderData>,
    data: RwLock<ChunkData>,
}

impl Chunk {
    pub(crate) fn new_from_proto_region(
        chunk_region_x: u8,
        chunk_region_z: u8,
        region: &InnerRegion,
        region_x: i32,
        region_z: i32,
    ) -> Result<Self, Error> {
        let file = region.file.as_ref().unwrap();
        let region_header_data =
            Self::read_region_header_data(chunk_region_x, chunk_region_z, file)?;

        let mut chunk_file: Option<MemoryMappedFile> = None;

        let chunk_header_data = Self::read_chunk_header_data(region_header_data.location, file)?;

        if chunk_header_data.is_oversized {
            let chunk_x = region_x << 5 | chunk_region_x as i32;
            let chunk_z = region_z << 5 | chunk_region_z as i32;

            chunk_file = Some(
                MemoryMappedFile::open_file_with_guaranteed_size(
                    4096,
                    Path::new(&format!(
                        "{}/c.{}.{}.mcc",
                        region.directory, chunk_x, chunk_z
                    )),
                )
                    .unwrap(),
            );
        }

        Ok(Chunk {
            region_header_data: RwLock::new(region_header_data),
            chunk_header_data: RwLock::new(chunk_header_data),
            data: RwLock::new(ChunkData {
                oversized_data: chunk_file,
            }),
        })
    }

    fn read_region_header_data(
        chunk_region_x: u8,
        chunk_region_z: u8,
        file: &MemoryMappedFile,
    ) -> Result<RegionHeaderData, Error> {
        let offset = ((chunk_region_x % 32) as u16 + (chunk_region_z % 32) as u16 * 32) * 4;

        let data = file.read_file(offset as usize..offset as usize + 3)?;

        let location = ((data[offset as usize] as u32) << 16)
            | ((data[offset as usize + 1] as u32) << 8)
            | data[offset as usize + 2] as u32;

        let size = data[offset as usize + 3];

        let end = location + size as u32;

        let range = location..end;

        Ok(RegionHeaderData {
            offset,
            location,
            size,
            end,
            range,
        })
    }

    fn read_chunk_header_data(
        offset: u32,
        file: &MemoryMappedFile,
    ) -> Result<ChunkHeaderData, Error> {
        let location = offset as usize * 4096;

        let data = file.read_file(offset as usize..offset as usize + 4)?;

        let length = ((data[location] as u32) << 24)
            | ((data[location + 1] as u32) << 16)
            | ((data[location + 2] as u32) << 8)
            | (data[location + 3] as u32);

        let compression_type_byte = data[location + 4];

        let compression_type = CompressionType::from_u8(data[location + 4]).unwrap();

        let is_oversized = compression_type_byte == 82;

        let data_start = location + 5;
        let data_end = location + 4 + length as usize;

        let data_range = data_start..data_end;

        Ok(ChunkHeaderData {
            location,
            length,
            compression_type,
            is_oversized,
            data_range,
        })
    }

    pub fn read_chunk_data<
        F: FnOnce(
            &InnerRegion,
            u8,
            Range<u32>,
        ) -> Vec<Option<Ref<u32, (bool, RwLock<()>), RandomState>>>,
    >(
        &self,
        inner_region: &InnerRegion,
        lock_sectors: F,
    ) -> Result<Vec<u8>, Error> {
        let region_header_data = self.region_header_data.read();

        let locked_sectors = lock_sectors(
            inner_region,
            region_header_data.size,
            region_header_data.range.clone(),
        );

        let chunk_header_data = self.chunk_header_data.read();

        if inner_region.file.is_none() {
            return Err(Error::new(
                std::io::ErrorKind::Other,
                "Region file is not open",
            ));
        }

        let compressed_data = inner_region
            .file
            .as_ref()
            .unwrap()
            .read_file(chunk_header_data.data_range.clone())?;

        let data = chunk_header_data
            .compression_type
            .decompress(compressed_data)?;

        drop(locked_sectors);

        Ok(data)
    }
}
