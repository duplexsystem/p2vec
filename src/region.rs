use std::io::Error;
use std::mem::{transmute, MaybeUninit};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use ahash::RandomState;
use dashmap::mapref::one::RefMut;
use dashmap::DashMap;
use libdeflater::CompressionLvl;
use once_cell::sync::Lazy;
use parking_lot::RwLock;

use crate::compression::CompressionType;
use crate::file::{close_file, open_file, MemoryMappedFile};
use crate::util::get_alignment_vector;

pub struct RegionHeaderData {
    offset: u16,
    location: u32,
    size: u8,
    end: u32,
}

pub struct Region {
    directory: &'static str,
    file: Option<MemoryMappedFile>,
    data: RegionData,
    chunks: [[Chunk; 32]; 32],
}

struct RegionData {
    end: AtomicU32,
    map: DashMap<u32, bool, RandomState>,
}

struct Chunk {
    header_data: RwLock<()>,
    data: RwLock<ChunkData>,
}

struct ChunkData {
    oversized_data: Option<MemoryMappedFile>,
}

static REGIONS: Lazy<DashMap<(&'static str, i32, i32), Region, RandomState>> =
    Lazy::new(|| DashMap::with_capacity_and_hasher(1, RandomState::default()));

fn read_region_header_data(
    chunk_region_x: u8,
    chunk_region_z: u8,
    data: &[u8],
) -> RegionHeaderData {
    let offset = ((chunk_region_x % 32) as u16 + (chunk_region_z % 32) as u16 * 32) * 4;

    let location = ((data[offset as usize] as u32) << 16)
        | ((data[offset as usize + 1] as u32) << 8)
        | data[offset as usize + 3] as u32;

    let size = data[offset as usize + 4];

    let end = location + size as u32;

    return RegionHeaderData {
        offset,
        location,
        size,
        end,
    };
}

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

        let chunk_region_x = 0;
        for x in x_array.iter_mut() {
            // Create an array of uninitialized values.
            let mut z_array: [MaybeUninit<Chunk>; 32] =
                unsafe { MaybeUninit::uninit().assume_init() };

            let chunk_region_z = 0;
            for z in z_array.iter_mut() {
                let region_header_data =
                    read_region_header_data(chunk_region_x, chunk_region_z, &file.data);

                for i in region_header_data.location..region_header_data.end {
                    map.insert(i as u32, true).unwrap();
                }
                if region_header_data.end > end {
                    end = region_header_data.end;
                }
                let mut chunk_file: Option<MemoryMappedFile> = None;
                if file.data[region_header_data.location as usize] == 0
                    && file.data[region_header_data.location as usize + 1] == 0
                    && file.data[region_header_data.location as usize + 2] == 0
                    && file.data[region_header_data.location as usize + 3] == 1
                    && file.data[region_header_data.location as usize + 4] == 82
                {
                    let chunk_x = region_x << 5 | chunk_region_x as i32;
                    let chunk_z = region_z << 5 | chunk_region_z as i32;

                    chunk_file = Some(
                        open_file(Path::new(&format!(
                            "{}/c.{}.{}.mcc",
                            directory, chunk_x, chunk_z
                        )))
                        .unwrap(),
                    );
                }
                *z = MaybeUninit::new(Chunk {
                    header_data: RwLock::new(()),
                    data: RwLock::new(ChunkData {
                        oversized_data: chunk_file,
                    }),
                });
            }

            *x = MaybeUninit::new(unsafe { transmute::<_, [Chunk; 32]>(z_array) });
        }

        unsafe { transmute::<_, [[Chunk; 32]; 32]>(x_array) }
    };

    Region {
        directory,
        file: Some(file),
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

    let mut region = region_option.unwrap();

    close_file(region.file.take().unwrap()).unwrap();

    REGIONS.remove(&(directory, region_x, region_z));

    Ok(())
}

fn read_chunk_data(directory: &'static str, chunk_x: i32, chunk_z: i32) -> Result<&[u8], Error> {
    let region_x = chunk_x >> 5;
    let region_z = chunk_z >> 5;

    let region = open_region(directory, region_x, region_z)?;

    let chunk_region_x = (chunk_x & 31) as u8;
    let chunk_region_z = (chunk_z & 31) as u8;

    let chunk = &region.chunks[chunk_region_x as usize][chunk_region_z as usize];

    if region.file.is_none() {
        return Err(Error::new(
            std::io::ErrorKind::Other,
            "Region file is not open",
        ));
    }

    let file = region.file.as_ref().unwrap();

    let region_header_data = read_region_header_data(chunk_region_x, chunk_region_z, &file.data);

    let mut locked_position = Vec::new();
    locked_position.resize_with(region_header_data.size as usize, || None);
    for position in region_header_data.location..region_header_data.end {
        locked_position.push(Some(region.data.map.get(&(position as u32)).unwrap()));
    }

    if region_header_data.end > region.data.end.load(Ordering::Relaxed) {};

    Ok(&[])
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
