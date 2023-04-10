#[inline]
pub(crate) fn get_alignment_vector(number: usize, alignment: usize) -> Vec<u8> {
    vec![0_u8; (alignment - number % alignment) % alignment]
}

#[inline]
pub(crate) fn u8x3_to_u32(data: &[u8; 3]) -> u32 {
    ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | (data[2] as u32)
}

#[inline]
pub(crate) fn u8x4_to_u32(data: &[u8; 4]) -> u32 {
    ((data[0] as u32) << 24) | ((data[1] as u32) << 16) | ((data[2] as u32) << 8) | (data[3] as u32)
}
