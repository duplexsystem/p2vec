use crate::chunk::Chunk;
use crate::memory_mapped_file::MemoryMappedFile;
use parking_lot::RwLock;
use std::io::Error;
use std::mem::{transmute, MaybeUninit};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[derive(Hash, Eq, PartialEq, Copy, Clone)]
pub(crate) struct RegionKey {
    pub(crate) directory: &'static str,
    pub(crate) x: i32,
    pub(crate) z: i32,
}

pub(crate) struct RegionData {
    pub(crate) free_blocks: RwLock<Vec<AtomicU32>>,
    pub(crate) claimed_blocks: AtomicU32,
    pub(crate) end: AtomicU32,
    pub(crate) wanted_end: AtomicU32,
}

pub(crate) struct InnerRegion {
    pub(crate) directory: &'static str,
    pub(crate) file: Option<MemoryMappedFile>,
    pub(crate) data: RegionData,
}

pub(crate) struct Region {
    inner_region: InnerRegion,
    chunks: [[RwLock<Chunk>; 32]; 32],
}

impl Region {
    pub(crate) fn new(key: &RegionKey) -> Result<Region, Error> {
        let (file, file_size) = MemoryMappedFile::open_file(
            8192,
            Path::new(&format!("{0}/r.{1}.{2}.mca", key.directory, key.x, key.z)),
            true,
        )?;
        let end = ((file_size as u32 / 4096) - 2).max(1);

        let mut free_blocks = Vec::with_capacity((end + 1) as usize);
        (2..end).for_each(|i| free_blocks.push(AtomicU32::new(i)));

        let inner_region = InnerRegion {
            directory: key.directory,
            file: Some(file),
            data: RegionData {
                end: AtomicU32::new(end),
                claimed_blocks: AtomicU32::new(0),
                free_blocks: RwLock::new(Vec::with_capacity((end + 1) as usize)),
                wanted_end: AtomicU32::new(end),
            },
        };

        let map = inner_region.data.free_blocks.write();
        let mut claimed_blocks: u32 = 0;

        let chunks = {
            // Create an array of uninitialized values.
            let mut x_array: [MaybeUninit<[RwLock<Chunk>; 32]>; 32] =
                unsafe { MaybeUninit::uninit().assume_init() };

            let chunk_region_x = 0;
            for x in x_array.iter_mut() {
                // Create an array of uninitialized values.
                let mut z_array: [MaybeUninit<RwLock<Chunk>>; 32] =
                    unsafe { MaybeUninit::uninit().assume_init() };

                let chunk_region_z = 0;
                for z in z_array.iter_mut() {
                    let chunk = Chunk::new_from_inner_region(
                        chunk_region_x,
                        chunk_region_z,
                        &inner_region,
                        key.x,
                        key.z,
                    )?;

                    map.iter().for_each(|block| {
                        let block_index = block.load(Ordering::Relaxed);
                        if (block_index >= chunk.region_header_data.range.start)
                            && (block_index <= chunk.region_header_data.range.end)
                        {
                            block.store(0, Ordering::Relaxed);
                            claimed_blocks += 1;
                        }
                    });

                    *z = MaybeUninit::new(RwLock::new(chunk));
                }

                *x = MaybeUninit::new(unsafe { transmute::<_, [RwLock<Chunk>; 32]>(z_array) });
            }

            unsafe { transmute::<_, [[RwLock<Chunk>; 32]; 32]>(x_array) }
        };

        inner_region
            .data
            .claimed_blocks
            .store(claimed_blocks, Ordering::Relaxed);

        drop(map);

        Ok(Region {
            inner_region,
            chunks,
        })
    }

    pub(crate) fn close(&mut self) -> Result<(), Error> {
        self.inner_region.file.take().unwrap().close_file()?;

        Ok(())
    }

    pub(crate) fn read_chunk(&self, chunk_x: i32, chunk_z: i32) -> Result<Option<Vec<u8>>, Error> {
        let chunk_region_x = (chunk_x & 31) as u8;
        let chunk_region_z = (chunk_z & 31) as u8;

        let chunk = &self.chunks[chunk_region_x as usize][chunk_region_z as usize].read();

        let data = chunk.read_chunk_data(&self.inner_region)?;

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
