use concurrent_queue::{ConcurrentQueue, PushError};
use std::io::Error;
use std::mem::{transmute, MaybeUninit};
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
    pub(crate) free_ranges: Box<[ConcurrentQueue<Range<u32>>; 256]>,
    pub(crate) wanted_space: AtomicU32,
    pub(crate) modify_lock: RwLock<()>,
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

        let mut free_ranges = Vec::with_capacity(256);

        free_ranges.fill_with(ConcurrentQueue::<Range<u32>>::unbounded);

        glidesort::sort_by(&mut taken_ranges, |a, b| a.start.cmp(&b.start));

        Ok(Region {
            static_metadata: static_region_metadata,
            mutable_metadata: MutableRegionMetadata {
                free_ranges: Box::try_from(free_ranges.into_boxed_slice()).unwrap(),
                wanted_space: AtomicU32::new(end),
                modify_lock: RwLock::new(()),
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
