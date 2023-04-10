use std::io::Error;

use ahash::RandomState;
use dashmap::mapref::one::Ref;
use dashmap::DashMap;
use glam::IVec2;
use libdeflater::CompressionLvl;
use once_cell::sync::Lazy;

use crate::compression::CompressionType;
use crate::memory_util::get_alignment_vector;
use crate::region::Region;
use crate::region_file_util::get_chunk_region_coords;
use crate::region_key::RegionKey;

mod chunk;
mod compression;
mod file_util;
mod memory_mapped_file;
mod memory_util;
mod region;
mod region_file_util;
mod region_key;

static REGIONS: Lazy<DashMap<RegionKey, Region, RandomState>> =
    Lazy::new(|| DashMap::with_capacity_and_hasher(1, RandomState::default()));

pub(crate) fn open_region(
    key: RegionKey,
) -> Result<Ref<'static, RegionKey, Region, RandomState>, Error> {
    Ok(REGIONS
        .entry(key)
        .or_try_insert_with(|| Region::new(&key))?
        .downgrade())
}

pub(crate) fn get_region(
    key: RegionKey,
) -> Result<Ref<'static, RegionKey, Region, RandomState>, Error> {
    match REGIONS.get(&key) {
        Some(region) => Ok(region),
        None => open_region(key),
    }
}

pub fn close_region(directory: &'static str, coords: IVec2) -> Result<(), Error> {
    let key = RegionKey { directory, coords };

    let region_option = match REGIONS.get_mut(&key) {
        None => return Ok(()),
        Some(region_option) => region_option,
    };

    let mut region = region_option;

    region.close()?;

    REGIONS.remove(&key);

    Ok(())
}

pub fn read_chunk(directory: &'static str, coords: IVec2) -> Result<Option<Vec<u8>>, Error> {
    let key = RegionKey {
        directory,
        coords: get_chunk_region_coords(coords),
    };
    let region = get_region(key)?;

    region.read_chunk(coords)
}

pub fn write_chunk(
    directory: &'static str,
    coords: IVec2,
    timestamp: u64,
    data: &[u8],
    compression_type: u8,
    compression_level: i32,
) -> Result<(), Error> {
    let compressed_data = match CompressionType::from_u8(compression_type) {
        None => {
            return Err(Error::new(
                std::io::ErrorKind::Other,
                "Invalid compression type",
            ));
        }
        Some(compression_type) => compression_type,
    }
    .compress(
        data,
        match CompressionLvl::new(compression_level) {
            Ok(level) => level,
            Err(_) => {
                return Err(Error::new(
                    std::io::ErrorKind::Other,
                    "Invalid compression level",
                ));
            }
        },
    )?;

    let alignment_data = get_alignment_vector(compressed_data.len(), 4096);
    let key = RegionKey {
        directory,
        coords: get_chunk_region_coords(coords),
    };

    let region = get_region(key)?;

    region.write_chunk(
        directory,
        coords,
        timestamp,
        &compressed_data,
        &alignment_data,
    )?;

    Ok(())
}
