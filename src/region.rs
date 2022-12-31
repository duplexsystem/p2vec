use std::fs::File;
use std::io::Error;
use std::mem::{transmute, MaybeUninit};
use std::path::Path;

use crate::compression::CompressionType;
use ahash::RandomState;
use bitvec::vec::BitVec;
use dashmap::mapref::one::RefMut;
use dashmap::DashMap;
use libdeflater::CompressionLvl;
use memmap2::MmapMut;
use once_cell::sync::Lazy;
use parking_lot::RwLock;

use crate::file::open_file;
use crate::util::get_alignment_vector;

pub struct Region<'a> {
    directory: &'static str,
    file: File,
    data: RwLock<RegionData>,
    chunks: Box<[[RwLock<Chunk<'a>>; 32]; 32]>,
}

struct RegionData {
    file: MmapMut,
    map: BitVec,
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
    oversized_data: Option<File>,
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
    let memmap = unsafe { MmapMut::map_mut(&file).unwrap() };
    let mut map = BitVec::new();
    let chunks = {
        // Create an array of uninitialized values.
        let mut x_array: [MaybeUninit<[RwLock<Chunk>; 32]>; 32] =
            unsafe { MaybeUninit::uninit().assume_init() };

        let x_int = 0;
        for x in x_array.iter_mut() {
            // Create an array of uninitialized values.
            let mut z_array: [MaybeUninit<RwLock<Chunk>>; 32] =
                unsafe { MaybeUninit::uninit().assume_init() };

            let z_int = 0;
            for z in z_array.iter_mut() {
                let offset = ((x_int % 32) + (z_int % 32) * 32) * 4;
                let location = ((memmap[offset] as usize) << 16)
                    | ((memmap[offset + 1] as usize) << 8)
                    | memmap[offset + 3] as usize;
                let sectors = memmap[offset + 4] as usize * 4096;
                for i in location..sectors {
                    map.set(i, true);
                }
                let mut file: Option<File> = None;
                if memmap[location] == 0
                    && memmap[location + 1] == 0
                    && memmap[location + 2] == 0
                    && memmap[location + 3] == 1
                    && memmap[location + 4] == 82
                {
                    let chunk_x = x_int << 5 | region_x as usize;
                    let chunk_z = z_int << 5 | region_z as usize;

                    let mut chunk_file = open_file(Path::new(&format!(
                        "{}/c.{}.{}.mcc",
                        directory, chunk_x, chunk_z
                    )))
                    .unwrap();
                    file = Some(chunk_file);
                }
                *z = MaybeUninit::new(RwLock::new(Chunk {
                    header_data: RwLock::new(HeaderData {
                        location: &memmap[offset..offset + 4],
                        timestamp: &memmap[offset + 4096..offset + 4100],
                    }),
                    data: RwLock::new(ChunkData {
                        data: &memmap[location..location + sectors],
                        oversized_data: file,
                    }),
                }));
            }

            *x = MaybeUninit::new(unsafe { transmute::<_, [RwLock<Chunk>; 32]>(z_array) });
        }

        Box::new(unsafe { transmute::<_, [[RwLock<Chunk>; 32]; 32]>(x_array) })
    };

    Region {
        directory,
        file,
        data: RwLock::new(RegionData { file: memmap, map }),
        chunks,
    }
}

pub fn close_region(directory: &'static str, region_x: i32, region_z: i32) -> Result<(), Error> {
    let region_option = REGIONS.get_mut(&(directory, region_x, region_z));

    if region_option.is_none() {
        return Ok(());
    }

    let region = region_option.unwrap();

    let region_data_lock = region.data.write();

    let mut chunk_locks = Vec::new();

    for x in 0..32 {
        for z in 0..32 {
            let chunk_lock = region.chunks[x][z]
            let chunk_header_data_lock = chunk_lock.header_data\
            let chunk_data_lock = chunk_lock.header_data.write();
            chunk_locks.push((chunk_lock, chunk_header_data_lock, chunk_data_lock));
        }
    }

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
