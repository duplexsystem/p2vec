use std::borrow::Cow;

static EMPTY_BUFFER: [u8; 4096] = [0; 4096];

#[inline]
pub(crate) fn get_alignment_vector(number: usize, alignment: usize) -> Cow<'static, [u8]> {
    if alignment > EMPTY_BUFFER.len() {
        return Cow::Owned(vec![0_u8; (alignment - number % alignment) % alignment]);
    }
    Cow::Borrowed(&EMPTY_BUFFER[..(alignment - number % alignment) % alignment])
}

#[inline]
pub(crate) fn u8x3_to_u32(data: &[u8]) -> u32 {
    ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | (data[2] as u32)
}

#[inline]
pub(crate) fn u8x4_to_u32(data: &[u8]) -> u32 {
    ((data[0] as u32) << 24) | ((data[1] as u32) << 16) | ((data[2] as u32) << 8) | (data[3] as u32)
}
