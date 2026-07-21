use crate::{Error, Result};

/// Logical tensor extents.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Shape {
    dims: Vec<usize>,
}

impl Shape {
    pub fn new(dims: impl Into<Vec<usize>>) -> Self {
        Self { dims: dims.into() }
    }

    pub fn scalar() -> Self {
        Self { dims: Vec::new() }
    }

    pub fn dims(&self) -> &[usize] {
        &self.dims
    }

    pub fn rank(&self) -> usize {
        self.dims.len()
    }

    pub fn element_count(&self) -> Result<usize> {
        self.dims.iter().try_fold(1usize, |acc, &dim| {
            acc.checked_mul(dim)
                .ok_or_else(|| Error::Shape("shape element count overflow".to_string()))
        })
    }

    pub fn contiguous_strides(&self) -> Result<Strides> {
        Strides::contiguous_for(self)
    }
}

/// Stride vector expressed in logical elements.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Strides {
    values: Vec<usize>,
}

impl Strides {
    pub fn new(values: impl Into<Vec<usize>>) -> Self {
        Self {
            values: values.into(),
        }
    }

    pub fn values(&self) -> &[usize] {
        &self.values
    }

    pub fn rank(&self) -> usize {
        self.values.len()
    }

    pub fn contiguous_for(shape: &Shape) -> Result<Self> {
        let mut values = Vec::new();
        values
            .try_reserve_exact(shape.rank())
            .map_err(|_| Error::Shape("failed to allocate contiguous strides".to_string()))?;
        values.resize(shape.rank(), 0);
        let mut stride = 1usize;
        for (index, dim) in shape.dims().iter().enumerate().rev() {
            values[index] = stride;
            stride = stride
                .checked_mul(*dim)
                .ok_or_else(|| Error::Shape("contiguous stride overflow".to_string()))?;
        }
        Ok(Self { values })
    }

    pub fn is_contiguous_for(&self, shape: &Shape) -> Result<bool> {
        Ok(self.values == Self::contiguous_for(shape)?.values)
    }

    /// Computes the number of logical elements required to store a tensor
    /// with these strides, i.e. `1 + sum((dim - 1) * stride)` across all
    /// dimensions. All arithmetic is checked.
    pub fn physical_element_count(&self, shape: &Shape) -> Result<usize> {
        if self.values.len() != shape.dims.len() {
            return Err(Error::Shape(
                "stride rank does not match shape rank".to_string(),
            ));
        }
        if shape.dims.contains(&0) {
            return Ok(0);
        }
        if shape.dims.is_empty() {
            return Ok(1);
        }

        let mut max_index = 0usize;
        for (&dim, &stride) in shape.dims.iter().zip(self.values.iter()) {
            if stride == 0 {
                return Err(Error::Shape("stride must be non-zero".to_string()));
            }
            let offset = dim
                .checked_sub(1)
                .and_then(|d| d.checked_mul(stride))
                .ok_or_else(|| Error::Shape("stride physical offset overflow".to_string()))?;
            max_index = max_index
                .checked_add(offset)
                .ok_or_else(|| Error::Shape("physical element count overflow".to_string()))?;
        }
        max_index
            .checked_add(1)
            .ok_or_else(|| Error::Shape("physical element count overflow".to_string()))
    }
}
