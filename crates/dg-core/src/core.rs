/// Backend-agnostic selection of one or more device cores.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CoreSelection {
    #[default]
    Auto,
    Single(u8),
    Mask(u32),
    All,
}

impl CoreSelection {
    /// Returns whether this selection includes the specified core.
    pub fn contains(self, core_id: u8) -> bool {
        match self {
            Self::Auto | Self::All => true,
            Self::Single(selected) => selected == core_id,
            Self::Mask(mask) => mask & (1u32 << u32::from(core_id)) != 0,
        }
    }

    /// Returns whether this selection explicitly names a core or mask.
    pub fn is_explicit(self) -> bool {
        !matches!(self, Self::Auto)
    }
}
