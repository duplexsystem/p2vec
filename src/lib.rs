use std::io::Error;

use ahash::RandomState;
use dashmap::mapref::one::RefMut;
use dashmap::DashMap;
use libdeflater::CompressionLvl;
use once_cell::sync::Lazy;

use crate::compression::CompressionType;
use crate::region::Region;
use crate::util::get_alignment_vector;

mod chunk;
mod compression;
mod memory_mapped_file;
mod possilby_random_file;
mod region;
mod util;

static REGIONS: Lazy<DashMap<(&'static str, i32, i32), Region, RandomState>> =
    Lazy::new(|| DashMap::with_capacity_and_hasher(1, RandomState::default()));

pub fn open_region(
    directory: &'static str,
    x: i32,
    z: i32,
) -> Result<RefMut<(&str, i32, i32), Region, RandomState>, Error> {
    Ok(REGIONS
        .entry((directory, x, z))
        .or_insert_with(|| Region::new(directory, x, z).unwrap()))
}

pub fn close_region(directory: &'static str, region_x: i32, region_z: i32) -> Result<(), Error> {
    let region_option = REGIONS.get_mut(&(directory, region_x, region_z));

    if region_option.is_none() {
        return Ok(());
    }

    let mut region = region_option.unwrap();

    region.close()?;

    REGIONS.remove(&(directory, region_x, region_z));

    Ok(())
}

pub fn read_chunk(directory: &'static str, chunk_x: i32, chunk_z: i32) -> Result<Vec<u8>, Error> {
    let region_x = chunk_x >> 5;
    let region_z = chunk_z >> 5;

    let region = open_region(directory, region_x, region_z)?;

    region.read_chunk(chunk_x, chunk_z)
}

pub fn write_chunk(
    directory: &'static str,
    chunk_x: i32,
    chunk_z: i32,
    timestamp: u64,
    data: &[u8],
    compression_type: CompressionType,
    compression_level: CompressionLvl,
) -> Result<(), Error> {
    let compressed_data = compression_type.compress(data, compression_level)?;

    let alignment_data = get_alignment_vector(compressed_data.len(), 4096);

    let region_x = chunk_x >> 5;
    let region_z = chunk_z >> 5;

    let region = open_region(directory, region_x, region_z)?;

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
