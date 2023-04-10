use glam::IVec2;

#[derive(Hash, Eq, PartialEq, Copy, Clone)]
pub(crate) struct RegionKey {
    pub(crate) coords: IVec2,
    pub(crate) directory: &'static str,
}
