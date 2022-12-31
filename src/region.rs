use std::io::Error;
use std::mem::{transmute, MaybeUninit};
use std::path::Path;
use std::sync::atomic::AtomicU32;

use ahash::RandomState;
use dashmap::mapref::one::RefMut;
use dashmap::DashMap;
use libdeflater::CompressionLvl;
use once_cell::sync::Lazy;
use parking_lot::RwLock;

use crate::compression::CompressionType;
use crate::file::{close_file, open_file, MemoryMappedFile};
use crate::util::get_alignment_vector;

pub struct Region<'a> {
    directory: &'static str,
    file: Option<MemoryMappedFile>,
    data: RegionData,
    chunks: Box<[[Chunk<'a>; 32]; 32]>,
}

struct RegionData {
    end: AtomicU32,
    map: DashMap<u32, bool, RandomState>,
}

struct Chunk<'a> {
    header_data: RwLock<HeaderData<'a>>,
    data: RwLock<ChunkData<'a>>,
}

struct HeaderData<'a> {
    location: &'a [u8],
    timestamp: &'a [u8],
}

struct ChunkData<'a> {
    data: &'a [u8],
    oversized_data: Option<MemoryMappedFile>,
}

static REGIONS: Lazy<DashMap<(&'static str, i32, i32), Region, RandomState>> =
    Lazy::new(|| DashMap::with_capacity_and_hasher(1, RandomState::default()));

pub fn open_region(
    directory: &'static str,
    x: i32,
    z: i32,
) -> Result<RefMut<(&str, i32, i32), Region, RandomState>, Error> {
    Ok(REGIONS
        .entry((directory, x, z))
        .or_insert_with(|| create_region(directory, x, z)))
}

fn create_region(directory: &'static str, region_x: i32, region_z: i32) -> Region {
    let file = open_file(Path::new(&format!(
        "{directory}/r.{region_x}.{region_z}.mca"
    )))
    .unwrap();
    let map = DashMap::with_capacity_and_hasher(1, RandomState::default());
    let mut end = 2;
    let chunks = {
        // Create an array of uninitialized values.
        let mut x_array: [MaybeUninit<[Chunk; 32]>; 32] =
            unsafe { MaybeUninit::uninit().assume_init() };

        let x_int = 0;
        for x in x_array.iter_mut() {
            // Create an array of uninitialized values.
            let mut z_array: [MaybeUninit<Chunk>; 32] =
                unsafe { MaybeUninit::uninit().assume_init() };

            let z_int = 0;
            for z in z_array.iter_mut() {
                let offset = ((x_int % 32) + (z_int % 32) * 32) * 4;
                let location = ((file.data[offset] as usize) << 16)
                    | ((file.data[offset + 1] as usize) << 8)
                    | file.data[offset + 3] as usize;
                let sectors = file.data[offset + 4] as usize;
                let chunk_end: usize = location + sectors;
                for i in location..chunk_end {
                    map.insert(i as u32, true).unwrap();
                }
                if chunk_end > end {
                    end = chunk_end;
                }
                let mut chunk_file: Option<MemoryMappedFile> = None;
                if file.data[location] == 0
                    && file.data[location + 1] == 0
                    && file.data[location + 2] == 0
                    && file.data[location + 3] == 1
                    && file.data[location + 4] == 82
                {
                    let chunk_x = x_int << 5 | region_x as usize;
                    let chunk_z = z_int << 5 | region_z as usize;

                    chunk_file = Some(
                        open_file(Path::new(&format!(
                            "{}/c.{}.{}.mcc",
                            directory, chunk_x, chunk_z
                        )))
                        .unwrap(),
                    );
                }
                *z = MaybeUninit::new(Chunk {
                    header_data: RwLock::new(HeaderData {
                        location: &file.data[offset..offset + 4],
                        timestamp: &file.data[offset + 4096..offset + 4100],
                    }),
                    data: RwLock::new(ChunkData {
                        data: &file.data[location..location + sectors],
                        oversized_data: chunk_file,
                    }),
                });
            }

            *x = MaybeUninit::new(unsafe { transmute::<_, [Chunk; 32]>(z_array) });
        }

        Box::new(unsafe { transmute::<_, [[Chunk; 32]; 32]>(x_array) })
    };

    Region {
        directory,
        file,
        data: RegionData {
            end: AtomicU32::new(end as u32),
            map,
        },
        chunks,
    }
}

pub fn close_region(directory: &'static str, region_x: i32, region_z: i32) -> Result<(), Error> {
    let region_option = REGIONS.get_mut(&(directory, region_x, region_z));

    if region_option.is_none() {
        return Ok(());
    }

    let region = region_option.unwrap();

    let mut space_locks = Vec::new();

    for space in region.data.map.iter_mut() {
        space_locks.push(space);
    }

    let mut chunk_locks = Vec::new();

    for x in 0..32 {
        for z in 0..32 {
            let chunk = &region.chunks[x][z];
            let chunk_header_data_lock = chunk.header_data.write();
            let mut chunk_data_lock = chunk.data.write();
            if chunk_data_lock.oversized_data.is_none() {
                let chunk_file = chunk_data_lock.oversized_data.take();
                close_file(chunk_file.unwrap()).unwrap();
            }
            chunk_locks.push((chunk_header_data_lock, chunk_data_lock));
        }
    }

    close_file(region.file.take().unwrap()).unwrap();

    REGIONS.remove(&(directory, region_x, region_z));

    Ok(())
}

fn write_chunk_data(
    directory: &'static str,
    chunk_x: i32,
    chunk_z: i32,
    timestamp: u64,
    data: &[u8],
    compression_type: CompressionType,
) -> Result<(), Error> {
    // TODO Compression heuristic
    let compressed_data = compression_type.compress(data, CompressionLvl::default())?;

    let alignment_data = get_alignment_vector(compressed_data.len(), 4096);

    let region_x = chunk_x >> 5;
    let region_z = chunk_z >> 5;

    let region = open_region(directory, region_x, region_z)?;

    Ok(())
}
