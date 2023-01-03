use std::io::Error;
use std::mem::{transmute, MaybeUninit};
use std::path::Path;
use std::sync::atomic::AtomicU32;

use ahash::RandomState;
use dashmap::mapref::one::RefMut;
use dashmap::DashMap;
use libdeflater::CompressionLvl;
use once_cell::sync::Lazy;

use crate::chunk::Chunk;
use crate::compression::CompressionType;
use crate::memory_mapped_file::MemoryMappedFile;
use crate::util::get_alignment_vector;

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

static REGIONS: Lazy<DashMap<(&'static str, i32, i32), Region, RandomState>> =
    Lazy::new(|| DashMap::with_capacity_and_hasher(1, RandomState::default()));

fn create_region(directory: &'static str, region_x: i32, region_z: i32) -> Result<Region, Error> {
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

pub fn open_region(
    directory: &'static str,
    x: i32,
    z: i32,
) -> Result<RefMut<(&str, i32, i32), Region, RandomState>, Error> {
    Ok(REGIONS
        .entry((directory, x, z))
        .or_insert_with(|| create_region(directory, x, z).unwrap()))
}

pub fn close_region(directory: &'static str, region_x: i32, region_z: i32) -> Result<(), Error> {
    let region_option = REGIONS.get_mut(&(directory, region_x, region_z));

    if region_option.is_none() {
        return Ok(());
    }

    let mut region = region_option.unwrap();

    region.inner_region.file.take().unwrap().close_file()?;

    REGIONS.remove(&(directory, region_x, region_z));

    Ok(())
}

fn read_chunk_data(directory: &'static str, chunk_x: i32, chunk_z: i32) -> Result<Vec<u8>, Error> {
    let region_x = chunk_x >> 5;
    let region_z = chunk_z >> 5;

    let region = open_region(directory, region_x, region_z)?;

    let chunk_region_x = (chunk_x & 31) as u8;
    let chunk_region_z = (chunk_z & 31) as u8;

    let chunk = &region.chunks[chunk_region_x as usize][chunk_region_z as usize];

    let data = chunk.read_chunk_data(&region.inner_region)?;

    Ok(data)
}

fn write_chunk_data(
    directory: &'static str,
    chunk_x: i32,
    chunk_z: i32,
    timestamp: u64,
    data: &[u8],
    compression_type: CompressionType,
) -> Result<(), Error> {
    let compressed_data = compression_type.compress(data, CompressionLvl::default())?;

    let alignment_data = get_alignment_vector(compressed_data.len(), 4096);

    let region_x = chunk_x >> 5;
    let region_z = chunk_z >> 5;

    let region = open_region(directory, region_x, region_z)?;

    Ok(())
}
