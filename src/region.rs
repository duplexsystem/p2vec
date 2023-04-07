use std::io::Error;
use std::mem::{transmute, MaybeUninit};
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64};

use parking_lot::RwLock;

use crate::chunk::Chunk;
use crate::memory_mapped_file::MemoryMappedFile;
use crate::region_key::RegionKey;

pub(crate) struct MutableRegionMetadata {
    pub(crate) free_ranges: RwLock<Vec<AtomicU64>>,
    pub(crate) unclaimed_blocks: AtomicU32,
    pub(crate) end: AtomicU32,
    pub(crate) wanted_end: AtomicU32,
}

pub(crate) struct StaticRegionMetadata {
    pub(crate) directory: &'static str,
    pub(crate) file: Option<MemoryMappedFile>,
}

pub(crate) struct Region {
    static_metadata: StaticRegionMetadata,
    mutable_metadata: MutableRegionMetadata,
    chunks: [[RwLock<Chunk>; 32]; 32],
}

impl Region {
    pub(crate) fn new(key: &RegionKey) -> Result<Region, Error> {
        let file = MemoryMappedFile::open_file(
            8192,
            Path::new(&format!("{0}/r.{1}.{2}.mca", key.directory, key.x, key.z)),
            true,
        )?;
        let end = ((file.memory_size as u32 / 4096) - 2).max(1);

        let static_region_metadata = StaticRegionMetadata {
            directory: key.directory,
            file: Some(file),
        };

        let mut taken_ranges: [MaybeUninit<Range<u32>>; 1024] =
            unsafe { MaybeUninit::uninit().assume_init() };

        let chunks = {
            // Create an array of uninitialized values.
            let mut x_array: [MaybeUninit<[RwLock<Chunk>; 32]>; 32] =
                unsafe { MaybeUninit::uninit().assume_init() };

            for x in x_array.iter_mut().enumerate() {
                // Create an array of uninitialized values.
                let mut z_array: [MaybeUninit<RwLock<Chunk>>; 32] =
                    unsafe { MaybeUninit::uninit().assume_init() };

                for z in z_array.iter_mut().enumerate() {
                    let chunk = Chunk::new(
                        x.0 as u32,
                        z.0 as u32,
                        &static_region_metadata,
                        key.x,
                        key.z,
                    )?;

                    taken_ranges[(x.0 * 32) + z.0] =
                        MaybeUninit::new(chunk.region_header_data.range.clone());

                    *z.1 = MaybeUninit::new(RwLock::new(chunk));
                }

                *x.1 = MaybeUninit::new(unsafe { transmute::<_, [RwLock<Chunk>; 32]>(z_array) });
            }

            unsafe { transmute::<_, [[RwLock<Chunk>; 32]; 32]>(x_array) }
        };

        let mut taken_ranges = unsafe { transmute::<_, [Range<u32>; 1024]>(taken_ranges) };

        glidesort::sort_by(&mut taken_ranges, |a, b| a.end.cmp(&b.start));

        let mut free_ranges: Vec<AtomicU64> = Vec::with_capacity(1024);

        let mut previous_end: u32 = 1;

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
                unclaimed_blocks: AtomicU32::new(unclaimed_blocks),
                free_ranges: RwLock::new(free_ranges),
                wanted_end: AtomicU32::new(end),
            },
            chunks,
        })
    }

    pub(crate) fn close(&mut self) -> Result<(), Error> {
        self.static_metadata.file.take().unwrap().close_file()?;

        Ok(())
    }

    pub(crate) fn read_chunk(&self, chunk_x: i32, chunk_z: i32) -> Result<Option<Vec<u8>>, Error> {
        let chunk_region_x = (chunk_x & 31) as u8;
        let chunk_region_z = (chunk_z & 31) as u8;

        let chunk = &self.chunks[chunk_region_x as usize][chunk_region_z as usize].read();

        let data = chunk.read_chunk_data(&self.static_metadata)?;

        Ok(data)
    }

    pub(crate) fn write_chunk(
        &self,
        directory: &'static str,
        chunk_x: i32,
        chunk_z: i32,
        timestamp: u64,
        data: &[u8],
        alignment_data: &[u8],
    ) -> Result<(), Error> {
        Ok(())
    }
}
