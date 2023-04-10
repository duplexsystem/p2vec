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
    ) -> (u64, u64) {
        let mut new_start = 0;
        let mut new_end = 0;

        let mut found_suitable_range = false;

        while !found_suitable_range {
            if wanted_sectors > current_sectors {
                if mutable_region_metadata
                    .unclaimed_ranges
                    .load(Ordering::Relaxed)
                    != 0
                {
                    let free_ranges = mutable_region_metadata.free_ranges.read();

                    let mut previous_range: Option<&AtomicU64> = None;
                    let mut previous_start = 2;
                    let mut previous_end = 2;

                    for range in free_ranges.iter() {
                        let acquired_range = range.load(Ordering::Acquire);
                        let start = acquired_range >> 32;
                        let end = acquired_range & 0xFFFFFFFF00000000;

                        if start - end != 0 {
                            let mut available_start = start;
                            let mut available_end = end;

                            if current_end == start {
                                if previous_end == current_start {
                                    available_start = previous_start;
                                } else {
                                    available_start = current_start;
                                }
                            }
                            if current_start == end {
                                available_end = current_end;
                            }

                            if available_end - available_start >= wanted_sectors {
                                new_start = available_start;
                                new_end = available_start + wanted_sectors;

                                let new_range = (new_end << 32) | end;

                                range.store(new_range, Ordering::Release);
                                match previous_range {
                                    None => {}
                                    Some(previous_range) => {
                                        previous_range.store(0, Ordering::Release);
                                        mutable_region_metadata
                                            .unclaimed_ranges
                                            .fetch_add(1, Ordering::Relaxed);
                                    }
                                }
                                if available_start != current_start {
                                    let current_rage = (current_start << 32) | current_end;
                                    mutable_region_metadata
                                        .free_ranges_to_recycle
                                        .write()
                                        .push(AtomicU64::new(current_rage));
                                }
                                found_suitable_range = true;
                                break;
                            }
                        }
                        match previous_range {
                            None => {}
                            Some(previous_range) => {
                                previous_range.store(
                                    (previous_start << 32) | previous_end,
                                    Ordering::Release,
                                );
                            }
                        }
                        previous_range = Some(range);
                        previous_start = start;
                        previous_end = end;
                    }
                }
                if !found_suitable_range {
                    mutable_region_metadata
                        .wanted_space
                        .fetch_add(wanted_sectors as u32, Ordering::Relaxed);
                    let mut free_ranges = mutable_region_metadata.free_ranges.write();
                    let sectors_to_add =
                        mutable_region_metadata.wanted_space.load(Ordering::Relaxed);

                    if sectors_to_add != 0 {
                        let mut free_ranges_to_recycle =
                            mutable_region_metadata.free_ranges_to_recycle.write();
                        free_ranges.append(&mut free_ranges_to_recycle);
                        glidesort::sort_by(&mut free_ranges, |a, b| {
                            let a = a.load(Ordering::Relaxed);
                            let b = b.load(Ordering::Relaxed);

                            let a_start = a >> 32;
                            let b_start = b >> 32;

                            a_start.cmp(&b_start)
                        });
                        let previous_range: Option<&AtomicU64> = None;
                        let previous_start = 2;
                        let previous_end = 2;

                        for range in free_ranges.iter() {
                            let acquired_range = range.load(Ordering::Relaxed);
                            let start = acquired_range >> 32;
                            let end = acquired_range & 0xFFFFFFFF00000000;

                            if start - end != 0 && previous_end == end {
                                range.store((previous_start << 32) | end, Ordering::Relaxed);
                                match previous_range {
                                    None => {}
                                    Some(previous_range) => {
                                        previous_range.store(0, Ordering::Relaxed);
                                    }
                                }
                            }
                        }

                        free_ranges.retain(|range| {
                            let acquired_range = range.load(Ordering::Relaxed);
                            let start = acquired_range >> 32;
                            let end = acquired_range & 0xFFFFFFFF00000000;

                            start - end != 0
                        });

                        mutable_region_metadata
                            .unclaimed_ranges
                            .store(free_ranges.len() as u32, Ordering::Relaxed);
                    }
                }
            } else if wanted_sectors != current_sectors {
                new_start = current_start;
                new_end = current_start + wanted_sectors;
                let new_range = (new_end << 32) | current_end;
                mutable_region_metadata
                    .free_ranges_to_recycle
                    .write()
                    .push(AtomicU64::new(new_range));

                found_suitable_range = true;
            } else {
                new_start = current_start;
                new_end = current_end;
                found_suitable_range = true;
            }
        }
        (new_start, new_end)
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
