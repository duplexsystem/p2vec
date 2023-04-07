use std::io::Error;
use std::ops::Range;
use std::path::Path;

use crate::compression::CompressionType;
use crate::memory_mapped_file::MemoryMappedFile;
use crate::region::StaticRegionMetadata;

pub(crate) struct RegionHeaderData {
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
    oversized: bool,
    data_range: Range<usize>,
}

struct ChunkData {
    oversized_data: Option<MemoryMappedFile>,
}

pub(crate) struct Chunk {
    pub(crate) region_header_data: RegionHeaderData,
    chunk_header_data: ChunkHeaderData,
    data: ChunkData,
}

impl Chunk {
    pub(crate) fn new(
        chunk_region_x: u32,
        chunk_region_z: u32,
        static_region_metadata: &StaticRegionMetadata,
        region_x: i32,
        region_z: i32,
    ) -> Result<Self, Error> {
        let file = static_region_metadata.file.as_ref().unwrap();
        let region_header_data =
            Self::read_region_header_data(chunk_region_x, chunk_region_z, file)?;

        let mut chunk_file: Option<MemoryMappedFile> = None;

        let chunk_header_data = Self::read_chunk_header_data(region_header_data.location, file)?;

        if chunk_header_data.oversized {
            let chunk_x = region_x << 5 | chunk_region_x as i32;
            let chunk_z = region_z << 5 | chunk_region_z as i32;

            chunk_file = Some(MemoryMappedFile::open_file(
                4096,
                Path::new(&format!(
                    "{}/c.{}.{}.mcc",
                    static_region_metadata.directory, chunk_x, chunk_z
                )),
                false,
            )?);
        }

        Ok(Chunk {
            region_header_data,
            chunk_header_data,
            data: ChunkData {
                oversized_data: chunk_file,
            },
        })
    }

    fn read_region_header_data(
        chunk_region_x: u32,
        chunk_region_z: u32,
        file: &MemoryMappedFile,
    ) -> Result<RegionHeaderData, Error> {
        let offset = ((chunk_region_x % 32) as u16 + (chunk_region_z % 32) as u16 * 32) * 4;

        let data = file.read_file(offset as usize..offset as usize + 3)?;

        let location = ((data[offset as usize] as u32) << 16)
            | ((data[offset as usize + 1] as u32) << 8)
            | (data[offset as usize + 2] as u32);

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

        let oversized = compression_type_byte == 82;

        let data_start = location + 5;
        let data_end = location + 4 + length as usize;

        let data_range = data_start..data_end;

        Ok(ChunkHeaderData {
            location,
            length,
            compression_type,
            oversized,
            data_range,
        })
    }

    pub(crate) fn read_chunk_data(
        &self,
        static_region_metadata: &StaticRegionMetadata,
    ) -> Result<Option<Vec<u8>>, Error> {
        if self.region_header_data.location <= 1 || self.region_header_data.size == 0 {
            return Ok(None);
        }

        if static_region_metadata.file.is_none() {
            return Err(Error::new(
                std::io::ErrorKind::Other,
                "Region file is not open",
            ));
        }

        let compressed_data = static_region_metadata
            .file
            .as_ref()
            .unwrap()
            .read_file(self.chunk_header_data.data_range.clone())?;

        let data = self
            .chunk_header_data
            .compression_type
            .decompress(compressed_data)?;

        Ok(Some(data))
    }
}
