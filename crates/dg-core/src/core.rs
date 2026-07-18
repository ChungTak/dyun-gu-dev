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
            Self::Mask(mask) => 1u32
                .checked_shl(u32::from(core_id))
                .is_some_and(|bit| mask & bit != 0),
        }
    }

    /// Returns whether this selection explicitly names a core or mask.
    pub fn is_explicit(self) -> bool {
        !matches!(self, Self::Auto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_does_not_panic_for_out_of_range_core_id() {
        let selection = CoreSelection::Mask(0x0000_0001);
        assert!(!selection.contains(32));
        assert!(!selection.contains(255));
        assert!(selection.contains(0));
    }

    #[test]
    fn single_match_is_exact() {
        assert!(CoreSelection::Single(7).contains(7));
        assert!(!CoreSelection::Single(7).contains(3));
    }
}
