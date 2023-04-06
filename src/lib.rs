use std::io::Error;

use ahash::RandomState;
use dashmap::mapref::one::Ref;
use dashmap::DashMap;
use libdeflater::CompressionLvl;
use once_cell::sync::Lazy;

use crate::compression::CompressionType;
use crate::memory_util::get_alignment_vector;
use crate::region::{Region, RegionKey};

mod chunk;
mod compression;
mod file_util;
mod memory_mapped_file;
mod memory_util;
mod random_file;
mod region;
mod sequential_file;
mod specialized_file;

static REGIONS: Lazy<DashMap<RegionKey, Region, RandomState>> =
    Lazy::new(|| DashMap::with_capacity_and_hasher(1, RandomState::default()));

#[inline(never)]
pub(crate) fn open_region_inner(key: RegionKey) -> Ref<'static, RegionKey, Region, RandomState> {
    REGIONS
        .entry(key)
        .or_insert_with(|| Region::new(&key).unwrap())
        .downgrade()
}

pub(crate) fn open_region(
    key: RegionKey,
) -> Result<Ref<'static, RegionKey, Region, RandomState>, Error> {
    Ok(REGIONS.get(&key).unwrap_or_else(|| open_region_inner(key)))
}

#[inline]
pub(crate) fn get_region_key(directory: &'static str, chunk_x: i32, chunk_z: i32) -> RegionKey {
    let x = chunk_x >> 5;
    let z = chunk_z >> 5;

    RegionKey { directory, x, z }
}

pub(crate) fn get_region(
    directory: &'static str,
    chunk_x: i32,
    chunk_z: i32,
) -> Result<Ref<'static, RegionKey, Region, RandomState>, Error> {
    open_region(get_region_key(directory, chunk_x, chunk_z))
}

pub fn close_region(directory: &'static str, chunk_x: i32, chunk_z: i32) -> Result<(), Error> {
    let key = get_region_key(directory, chunk_x, chunk_z);

    let region_option = REGIONS.get_mut(&key);

    if region_option.is_none() {
        return Ok(());
    }

    let mut region = region_option.unwrap();

    region.close()?;

    REGIONS.remove(&key);

    Ok(())
}

pub fn read_chunk(
    directory: &'static str,
    chunk_x: i32,
    chunk_z: i32,
) -> Result<Option<Vec<u8>>, Error> {
    let region = get_region(directory, chunk_x, chunk_z)?;

    region.read_chunk(chunk_x, chunk_z)
}

pub fn write_chunk(
    directory: &'static str,
    chunk_x: i32,
    chunk_z: i32,
    timestamp: u64,
    data: &[u8],
    compression_type: u8,
    compression_level: i32,
) -> Result<(), Error> {
    let compressed_data = CompressionType::from_u8(compression_type)
        .unwrap()
        .compress(data, CompressionLvl::new(compression_level).unwrap())?;

    let alignment_data = get_alignment_vector(compressed_data.len(), 4096);

    let region = get_region(directory, chunk_x, chunk_z)?;

    region.write_chunk(
        directory,
        chunk_x,
        chunk_z,
        timestamp,
        &compressed_data,
        &alignment_data,
    )?;

    Ok(())
}
