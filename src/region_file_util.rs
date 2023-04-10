use crate::compression::CompressionType;
use crate::memory_mapped_file::MemoryMappedFile;
use crate::memory_util::{u8x3_to_u32, u8x4_to_u32};
use glam::{IVec2, UVec2};
use std::io::Error;

#[inline]
pub(crate) fn get_region_coords(chunk_coords: IVec2) -> IVec2 {
    chunk_coords >> 5
}

#[inline]
pub(crate) fn get_chunk_region_coords(chunk_coords: IVec2) -> IVec2 {
    chunk_coords & 31
}

#[inline]
pub(crate) fn get_chunk_location(chunk_region_coords: IVec2) -> i32 {
    4 * ((chunk_region_coords.x) + (chunk_region_coords.y) * 32)
}

#[inline]
pub(crate) fn get_chunk_offset(offset_data: &[u8; 3]) -> u32 {
    u8x3_to_u32(offset_data)
}

#[inline]
pub(crate) fn get_chunk_length(length_data: &[u8; 4]) -> u32 {
    u8x4_to_u32(length_data)
}

#[inline]
pub(crate) fn get_chunk_timestamp(timestamp_data: &[u8; 4]) -> u32 {
    u8x4_to_u32(timestamp_data)
}

#[inline]
pub(crate) fn get_chunk_compression_type(compression_byte: u8) -> Option<CompressionType> {
    CompressionType::from_u8(compression_byte ^ 128)
}

#[inline]
pub(crate) fn get_oversized_status(compression_byte: u8) -> bool {
    compression_byte & 128 != 0
}
