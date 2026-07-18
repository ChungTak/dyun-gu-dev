use dg_core::{DataFormat, DataType, DeviceKind, Shape, Strides, TensorDesc};
use proptest::prelude::*;

proptest! {
    #[test]
    fn contiguous_strides_round_trip_and_storage(
        dims in prop::collection::vec(1usize..16, 1..6)
    ) {
        let shape = Shape::new(dims);
        let strides = Strides::contiguous_for(&shape)?;
        prop_assert!(strides.is_contiguous_for(&shape)?);

        let desc = TensorDesc::new(shape.clone(), DataType::U8, DataFormat::NCHW, DeviceKind::Cpu);
        let default_bytes = desc.storage_bytes()?;
        let explicit = desc.with_strides(strides);
        let explicit_bytes = explicit.storage_bytes()?;
        prop_assert_eq!(default_bytes, explicit_bytes);
    }

    #[test]
    fn scaled_strides_increase_physical_elements(
        dims in prop::collection::vec(1usize..16, 1..6),
        scale in 1usize..8
    ) {
        let shape = Shape::new(dims);
        let contiguous = Strides::contiguous_for(&shape)?;
        let scaled = contiguous
            .values()
            .iter()
            .map(|value| value * scale)
            .collect::<Vec<_>>();
        let strides = Strides::new(scaled);

        let physical = strides.physical_element_count(&shape)?;
        let logical = shape.element_count()?;
        prop_assert!(physical >= logical);
        if scale == 1 {
            prop_assert_eq!(physical, logical);
        } else if physical > logical {
            prop_assert!(!strides.is_contiguous_for(&shape)?);
        }
    }
}
