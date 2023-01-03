use std::io::Error;
use std::mem::{transmute, MaybeUninit};
use std::path::Path;
use std::sync::atomic::AtomicU32;

use ahash::RandomState;
use dashmap::DashMap;

use crate::chunk::Chunk;
use crate::memory_mapped_file::MemoryMappedFile;

pub struct RegionData {
    end: AtomicU32,
    pub(crate) map: DashMap<u32, bool, RandomState>,
}

pub struct InnerRegion {
    pub(crate) directory: &'static str,
    pub(crate) file: Option<MemoryMappedFile>,
    pub(crate) data: RegionData,
}

pub struct Region {
    inner_region: InnerRegion,
    chunks: [[Chunk; 32]; 32],
}

impl Region {
    pub(crate) fn new(
        directory: &'static str,
        region_x: i32,
        region_z: i32,
    ) -> Result<Region, Error> {
        let file = MemoryMappedFile::open_file_with_guaranteed_size(
            8192,
            Path::new(&format!("{directory}/r.{region_x}.{region_z}.mca")),
        )?;
        let map = DashMap::with_capacity_and_hasher(1, RandomState::default());
        let mut end = 2;

        let inner_region = InnerRegion {
            directory,
            file: Some(file),
            data: RegionData {
                end: AtomicU32::new(end),
                map,
            },
        };

        let chunks = {
            // Create an array of uninitialized values.
            let mut x_array: [MaybeUninit<[Chunk; 32]>; 32] =
                unsafe { MaybeUninit::uninit().assume_init() };

            let chunk_region_x = 0;
            for x in x_array.iter_mut() {
                // Create an array of uninitialized values.
                let mut z_array: [MaybeUninit<Chunk>; 32] =
                    unsafe { MaybeUninit::uninit().assume_init() };

                let chunk_region_z = 0;
                for z in z_array.iter_mut() {
                    let chunk = Chunk::new_from_proto_region(
                        chunk_region_x,
                        chunk_region_z,
                        &inner_region,
                        region_x,
                        region_z,
                    )?;

                    let region_header_data = chunk.region_header_data.read();

                    for i in region_header_data.range.clone() {
                        inner_region.data.map.insert(i as u32, true).unwrap();
                    }

                    if region_header_data.end > end {
                        end = region_header_data.end;
                    }

                    drop(region_header_data);

                    *z = MaybeUninit::new(chunk);
                }

                *x = MaybeUninit::new(unsafe { transmute::<_, [Chunk; 32]>(z_array) });
            }

            unsafe { transmute::<_, [[Chunk; 32]; 32]>(x_array) }
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

    pub fn read_chunk(&self, chunk_x: i32, chunk_z: i32) -> Result<Vec<u8>, Error> {
        let chunk_region_x = (chunk_x & 31) as u8;
        let chunk_region_z = (chunk_z & 31) as u8;

        let chunk = &self.chunks[chunk_region_x as usize][chunk_region_z as usize];

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
