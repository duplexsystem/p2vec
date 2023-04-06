use crate::chunk::Chunk;
use crate::memory_mapped_file::MemoryMappedFile;
use parking_lot::RwLock;
use std::io::Error;
use std::mem::{transmute, MaybeUninit};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub struct RegionData {
    end: AtomicU32,
    pub(crate) map: RwLock<Vec<AtomicBool>>,
    pub(crate) wanted_file_size: AtomicU32,
}

pub struct InnerRegion {
    pub(crate) directory: &'static str,
    pub(crate) file: Option<MemoryMappedFile>,
    pub(crate) data: RegionData,
}

pub struct Region {
    inner_region: InnerRegion,
    chunks: [[RwLock<Chunk>; 32]; 32],
}

impl Region {
    pub(crate) fn new(
        directory: &'static str,
        region_x: i32,
        region_z: i32,
    ) -> Result<Region, Error> {
        let (file, file_size) = MemoryMappedFile::open_file(
            8192,
            Path::new(&format!("{directory}/r.{region_x}.{region_z}.mca")),
            true,
        )?;
        let map = RwLock::new(Vec::with_capacity(file_size / 4096));
        let mut end = 2;

        let mut inner_region = InnerRegion {
            directory,
            file: Some(file),
            data: RegionData {
                end: AtomicU32::new(end),
                map,
                wanted_file_size: AtomicU32::new(file_size as u32),
            },
        };

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
                        region_x,
                        region_z,
                    )?;

                    let map = inner_region.data.map.read();
                    for i in chunk.region_header_data.range.clone() {
                        map.get(i as usize).unwrap().store(true, Ordering::Relaxed);
                    }

                    if chunk.region_header_data.end > end {
                        end = chunk.region_header_data.end;
                    }

                    *z = MaybeUninit::new(RwLock::new(chunk));
                }

                *x = MaybeUninit::new(unsafe { transmute::<_, [RwLock<Chunk>; 32]>(z_array) });
            }

            unsafe { transmute::<_, [[RwLock<Chunk>; 32]; 32]>(x_array) }
        };

        Ok(Region {
            inner_region,
            chunks,
        })
    }

    pub fn close(&mut self) -> Result<(), Error> {
        self.inner_region.file.take().unwrap().close_file()?;

        Ok(())
    }

    pub fn read_chunk(&self, chunk_x: i32, chunk_z: i32) -> Result<Option<Vec<u8>>, Error> {
        let chunk_region_x = (chunk_x & 31) as u8;
        let chunk_region_z = (chunk_z & 31) as u8;

        let chunk = &self.chunks[chunk_region_x as usize][chunk_region_z as usize].read();

        let data = chunk.read_chunk_data(&self.inner_region)?;

        Ok(data)
    }

    pub fn write_chunk(
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
