use std::io::Error;
use std::mem::{MaybeUninit, transmute};
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use glam::IVec2;
use parking_lot::RwLock;

use crate::chunk::{Chunk, ChunkGuard};
use crate::memory_mapped_file::MemoryMappedFile;
use crate::region_file_util::get_chunk_region_coords;
use crate::region_key::RegionKey;

pub(crate) struct MutableRegionMetadata {
    pub(crate) free_ranges: RwLock<Vec<AtomicU64>>,
    pub(crate) free_ranges_to_recycle: RwLock<Vec<AtomicU64>>,
    pub(crate) unclaimed_ranges: AtomicU32,
    pub(crate) end: AtomicU32,
    pub(crate) wanted_space: AtomicU32,
}

pub(crate) struct StaticRegionMetadata {
    pub(crate) directory: &'static str,
    pub(crate) file: Option<MemoryMappedFile>,
}

pub(crate) struct Region {
    static_metadata: StaticRegionMetadata,
    mutable_metadata: MutableRegionMetadata,
    chunks: [[ChunkGuard; 32]; 32],
}

impl Region {
    pub(crate) fn new(key: &RegionKey) -> Result<Region, Error> {
        let file = MemoryMappedFile::open_file(
            8192,
            Path::new(&format!(
                "{0}/r.{1}.{2}.mca",
                key.directory, key.coords.x, key.coords.y
            )),
            true,
        )?;
        let end = ((file.get_file_size()? as u32 / 4096) - 2).max(1);

        let static_region_metadata = StaticRegionMetadata {
            directory: key.directory,
            file: Some(file),
        };

        let mut taken_ranges: [MaybeUninit<Range<usize>>; 1024] =
            unsafe { MaybeUninit::uninit().assume_init() };

        let chunks = {
            // Create an array of uninitialized values.
            let mut x_array: [MaybeUninit<[ChunkGuard; 32]>; 32] =
                unsafe { MaybeUninit::uninit().assume_init() };

            for x in x_array.iter_mut().enumerate() {
                // Create an array of uninitialized values.
                let mut y_array: [MaybeUninit<ChunkGuard>; 32] =
                    unsafe { MaybeUninit::uninit().assume_init() };

                for y in y_array.iter_mut().enumerate() {
                    let (chunk, chunk_range) = Chunk::new(
                        IVec2::new(x.0 as i32, y.0 as i32),
                        key.coords,
                        &static_region_metadata,
                    )?;

                    taken_ranges[(x.0 * 32) + y.0] = MaybeUninit::new(chunk_range);

                    *y.1 = MaybeUninit::new(ChunkGuard {
                        chunk: RwLock::new(chunk),
                        timestamp: AtomicU32::new(0),
                    });
                }

                *x.1 = MaybeUninit::new(unsafe { transmute::<_, [ChunkGuard; 32]>(y_array) });
            }

            unsafe { transmute::<_, [[ChunkGuard; 32]; 32]>(x_array) }
        };

        let mut taken_ranges = unsafe { transmute::<_, [Range<usize>; 1024]>(taken_ranges) };

        glidesort::sort_by(&mut taken_ranges, |a, b| a.start.cmp(&b.start));

        let mut free_ranges: Vec<AtomicU64> = Vec::with_capacity(1024);

        let mut previous_end: usize = 1;

        let mut unclaimed_blocks = 0;

        for range in taken_ranges.iter() {
            let end = range.end;
            if previous_end != range.start {
                free_ranges.push(AtomicU64::new(((previous_end as u64) << 32) | (end as u64)));
            }
            previous_end = range.end;
        }

        free_ranges
            .spare_capacity_mut()
            .iter_mut()
            .for_each(|item| {
                item.write(AtomicU64::new(0));

                unclaimed_blocks += 1;
            });

        unsafe {
            free_ranges.set_len(free_ranges.capacity());
        }

        Ok(Region {
            static_metadata: static_region_metadata,
            mutable_metadata: MutableRegionMetadata {
                end: AtomicU32::new(end),
                unclaimed_ranges: AtomicU32::new(unclaimed_blocks),
                free_ranges: RwLock::new(free_ranges),
                free_ranges_to_recycle: RwLock::new(Vec::new()),
                wanted_space: AtomicU32::new(end),
            },
            chunks,
        })
    }

    pub(crate) fn close(&mut self) -> Result<(), Error> {
        match self.static_metadata.file.take() {
            None => {
                return Err(Error::new(
                    std::io::ErrorKind::Other,
                    "Region File can't be closed because it is not open",
                ));
            }
            Some(file) => file,
        }
            .close_file()?;

        Ok(())
    }

    pub(crate) fn read_chunk(&self, chunk_coords: IVec2) -> Result<Option<Vec<u8>>, Error> {
        let chunk_region_coords = get_chunk_region_coords(chunk_coords);

        let chunk = &self.chunks[chunk_region_coords.x as usize][chunk_region_coords.y as usize]
            .chunk
            .read();

        let data =
            chunk.read_chunk_data(chunk_coords, chunk_region_coords, &self.static_metadata)?;

        Ok(data)
    }

    pub(crate) fn write_chunk(
        &self,
        chunk_coords: IVec2,
        timestamp: u32,
        data: &[u8],
        alignment_data: &[u8],
    ) -> Result<(), Error> {
        let chunk_region_coords = get_chunk_region_coords(chunk_coords);

        let chunk_guard =
            &self.chunks[chunk_region_coords.x as usize][chunk_region_coords.y as usize];

        if chunk_guard
            .timestamp
            .fetch_max(timestamp, Ordering::Relaxed)
            >= timestamp
        {
            return Ok(());
        }

        chunk_guard.chunk.write().write_chunk_data(
            chunk_coords,
            chunk_region_coords,
            &self.static_metadata,
            &self.mutable_metadata,
            timestamp,
            data,
            alignment_data,
        )?;

        Ok(())
    }
}
