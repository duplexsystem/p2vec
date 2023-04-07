#[derive(Hash, Eq, PartialEq, Copy, Clone)]
pub(crate) struct RegionKey {
    pub(crate) directory: &'static str,
    pub(crate) x: i32,
    pub(crate) z: i32,
}

#[inline]
pub(crate) fn get_region_key(directory: &'static str, chunk_x: i32, chunk_z: i32) -> RegionKey {
    let x = chunk_x >> 5;
    let z = chunk_z >> 5;

    RegionKey { directory, x, z }
}
