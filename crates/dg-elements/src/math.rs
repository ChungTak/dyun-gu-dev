use dg_core::{BBox, Detection};
use dg_graph::{Error, Result};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Maximum number of detection candidates that can be passed to NMS before the
/// caller must explicitly pre-filter using a deterministic top-k.
pub(crate) const MAX_NMS_CANDIDATES: usize = 100_000;

/// Maximum number of values that can be ranked by `top_k` and the largest `k`
/// that can be requested.
pub(crate) const MAX_TOP_K_INPUT: usize = 100_000;
pub(crate) const MAX_TOP_K: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Letterbox {
    pub source_width: usize,
    pub source_height: usize,
    pub target_width: usize,
    pub target_height: usize,
    pub scale: f32,
    pub pad_x: f32,
    pub pad_y: f32,
}

impl Letterbox {
    pub fn new(
        source_width: usize,
        source_height: usize,
        target_width: usize,
        target_height: usize,
    ) -> Result<Self> {
        if source_width == 0 || source_height == 0 || target_width == 0 || target_height == 0 {
            return Err(Error::Config(
                "letterbox dimensions must be non-zero".to_string(),
            ));
        }
        let source_width_f = usize_to_f32(source_width)?;
        let source_height_f = usize_to_f32(source_height)?;
        let target_width_f = usize_to_f32(target_width)?;
        let target_height_f = usize_to_f32(target_height)?;
        let scale = (target_width_f / source_width_f).min(target_height_f / source_height_f);
        let resized_width = source_width_f * scale;
        let resized_height = source_height_f * scale;
        Ok(Self {
            source_width,
            source_height,
            target_width,
            target_height,
            scale,
            pad_x: (target_width_f - resized_width) * 0.5,
            pad_y: (target_height_f - resized_height) * 0.5,
        })
    }

    pub fn map_to_source(&self, bbox: BBox) -> BBox {
        let source_width = dimension_as_f32(self.source_width);
        let source_height = dimension_as_f32(self.source_height);
        let x = ((bbox.x - self.pad_x) / self.scale).clamp(0.0, source_width);
        let y = ((bbox.y - self.pad_y) / self.scale).clamp(0.0, source_height);
        let right = ((bbox.x + bbox.w - self.pad_x) / self.scale).clamp(0.0, source_width);
        let bottom = ((bbox.y + bbox.h - self.pad_y) / self.scale).clamp(0.0, source_height);
        BBox::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
    }
}

pub fn resize_letterbox(
    source: &[f32],
    channels: usize,
    source_width: usize,
    source_height: usize,
    target_width: usize,
    target_height: usize,
    padding: f32,
) -> Result<(Vec<f32>, Letterbox)> {
    if channels == 0 {
        return Err(Error::Config(
            "resize channels must be non-zero".to_string(),
        ));
    }
    let expected = source_width
        .checked_mul(source_height)
        .and_then(|size| size.checked_mul(channels))
        .ok_or_else(|| Error::Config("resize source size overflow".to_string()))?;
    if source.len() != expected {
        return Err(Error::Config(
            "resize source length does not match dimensions".to_string(),
        ));
    }
    let letterbox = Letterbox::new(source_width, source_height, target_width, target_height)?;
    let resized_width = round_to_usize(
        usize_to_f32(source_width)? * letterbox.scale,
        "resized width",
    )?
    .max(1)
    .min(target_width);
    let resized_height = round_to_usize(
        usize_to_f32(source_height)? * letterbox.scale,
        "resized height",
    )?
    .max(1)
    .min(target_height);
    let target_size = target_width
        .checked_mul(target_height)
        .and_then(|size| size.checked_mul(channels))
        .ok_or_else(|| Error::Config("resize target size overflow".to_string()))?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(target_size)
        .map_err(|_| Error::Config("resize output allocation failed".to_string()))?;
    output.resize(target_size, padding);
    let pad_x = (target_width - resized_width) / 2;
    let pad_y = (target_height - resized_height) / 2;
    for y in 0..resized_height {
        let source_y = y
            .saturating_mul(source_height)
            .checked_div(resized_height)
            .ok_or_else(|| Error::Config("resize source y overflow".to_string()))?;
        for x in 0..resized_width {
            let source_x = x
                .saturating_mul(source_width)
                .checked_div(resized_width)
                .ok_or_else(|| Error::Config("resize source x overflow".to_string()))?;
            let source_index = (source_y * source_width + source_x) * channels;
            let target_index = ((y + pad_y) * target_width + x + pad_x) * channels;
            output[target_index..target_index + channels]
                .copy_from_slice(&source[source_index..source_index + channels]);
        }
    }
    Ok((output, letterbox))
}

pub fn sigmoid(value: f32) -> f32 {
    if value >= 0.0 {
        1.0 / (1.0 + (-value).exp())
    } else {
        let exp = value.exp();
        exp / (1.0 + exp)
    }
}

pub fn softmax(values: &[f32]) -> Result<Vec<f32>> {
    if !values.iter().all(|v| v.is_finite()) {
        return Err(Error::Config(
            "softmax input contains non-finite values".to_string(),
        ));
    }
    if values.is_empty() {
        return Ok(Vec::new());
    }
    let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exponents = values.iter().map(|value| (*value - max).exp());
    let sum: f32 = exponents.clone().sum();
    let mut output = Vec::new();
    output
        .try_reserve_exact(values.len())
        .map_err(|_| Error::Runtime("softmax output allocation failed".to_string()))?;
    if sum == 0.0 || !sum.is_finite() {
        output.resize(values.len(), 0.0);
        return Ok(output);
    }
    for value in exponents {
        output.push(value / sum);
    }
    Ok(output)
}

pub fn top_k(values: &[f32], k: usize) -> Result<Vec<(usize, f32)>> {
    if values.len() > MAX_TOP_K_INPUT {
        return Err(Error::ResourceLimit {
            resource: "top_k input values".to_string(),
            requested: values.len(),
            limit: MAX_TOP_K_INPUT,
        });
    }
    if k > MAX_TOP_K {
        return Err(Error::ResourceLimit {
            resource: "top_k k".to_string(),
            requested: k,
            limit: MAX_TOP_K,
        });
    }
    let mut indexed = values.iter().copied().enumerate().collect::<Vec<_>>();
    indexed.sort_by(|left, right| right.1.total_cmp(&left.1));
    indexed.truncate(k);
    Ok(indexed)
}

pub fn iou(left: BBox, right: BBox) -> f32 {
    let left_right = left.x + left.w;
    let right_right = right.x + right.w;
    let left_bottom = left.y + left.h;
    let right_bottom = right.y + right.h;
    let intersection_width = (left_right.min(right_right) - left.x.max(right.x)).max(0.0);
    let intersection_height = (left_bottom.min(right_bottom) - left.y.max(right.y)).max(0.0);
    let intersection = intersection_width * intersection_height;
    let union = left.area() + right.area() - intersection;
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

/// NMS with a hard candidate ceiling. Inputs larger than the ceiling must be
/// pre-filtered with `nms_with_top_k` so the product contract is explicit.
pub fn nms(detections: &[Detection], threshold: f32) -> Result<Vec<Detection>> {
    if !(0.0..=1.0).contains(&threshold) {
        return Err(Error::Config(
            "nms iou threshold must be in [0.0, 1.0]".to_string(),
        ));
    }
    if detections.len() > MAX_NMS_CANDIDATES {
        return Err(Error::ResourceLimit {
            resource: "nms candidates".to_string(),
            requested: detections.len(),
            limit: MAX_NMS_CANDIDATES,
        });
    }
    let mut cloned = Vec::new();
    cloned
        .try_reserve_exact(detections.len())
        .map_err(|_| Error::Runtime("nms candidate allocation failed".to_string()))?;
    cloned.extend(detections.iter().cloned());
    nms_inner(cloned, threshold)
}

/// NMS that deterministically keeps only the top `max_candidates` by score
/// before applying the IoU threshold. This is the explicit top-k truncation
/// path; the returned vector is still bounded by `MAX_NMS_CANDIDATES`.
pub fn nms_with_top_k(
    detections: &[Detection],
    threshold: f32,
    max_candidates: usize,
) -> Result<Vec<Detection>> {
    if !(0.0..=1.0).contains(&threshold) {
        return Err(Error::Config(
            "nms iou threshold must be in [0.0, 1.0]".to_string(),
        ));
    }
    let max_candidates = max_candidates.min(MAX_NMS_CANDIDATES);
    if max_candidates == 0 || detections.is_empty() {
        return Ok(Vec::new());
    }

    // Keep a min-heap of the best `max_candidates` detections by score.
    // This avoids cloning the entire (potentially huge) input slice before
    // truncation, which could OOM abort on adversarial model output.
    let mut heap = BinaryHeap::with_capacity(max_candidates.min(detections.len()));
    for detection in detections {
        if heap.len() < max_candidates {
            heap.push(Reverse(ByScore(detection)));
        } else {
            let lowest = heap.peek().ok_or_else(|| {
                Error::Runtime("nms candidate heap unexpectedly empty".to_string())
            })?;
            if detection.score.total_cmp(&lowest.0.score()) == std::cmp::Ordering::Greater {
                heap.pop();
                heap.push(Reverse(ByScore(detection)));
            }
        }
    }

    let sorted = heap.into_sorted_vec();
    let mut top = Vec::new();
    top.try_reserve_exact(sorted.len())
        .map_err(|_| Error::Runtime("nms top-k output allocation failed".to_string()))?;
    for Reverse(ByScore(detection)) in sorted {
        top.push(detection.clone());
    }
    // `into_sorted_vec` on a max-heap of `Reverse<ByScore>` returns the
    // highest-score detections first, matching `nms_inner`'s expectation.
    nms_inner(top, threshold)
}

/// A score-only view of a detection so the top-k heap does not need to
/// clone candidates until they are kept in the final set.
struct ByScore<'a>(&'a Detection);

impl<'a> ByScore<'a> {
    fn score(&self) -> f32 {
        self.0.score
    }
}

impl<'a> PartialEq for ByScore<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.0.score.to_bits() == other.0.score.to_bits()
    }
}

impl<'a> Eq for ByScore<'a> {}

impl<'a> PartialOrd for ByScore<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for ByScore<'a> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.score.total_cmp(&other.0.score)
    }
}

fn nms_inner(mut detections: Vec<Detection>, threshold: f32) -> Result<Vec<Detection>> {
    detections.sort_by(|left, right| right.score.total_cmp(&left.score));
    let mut selected = Vec::new();
    selected
        .try_reserve_exact(detections.len())
        .map_err(|_| Error::Runtime("nms selected allocation failed".to_string()))?;
    for candidate in detections {
        let suppressed = selected.iter().any(|existing: &Detection| {
            existing.class_id == candidate.class_id
                && iou(existing.bbox, candidate.bbox) > threshold
        });
        if !suppressed {
            selected.push(candidate);
        }
    }
    Ok(selected)
}

fn usize_to_f32(value: usize) -> Result<f32> {
    if value > 16_777_216 {
        return Err(Error::Config(
            "dimension cannot be represented exactly as f32".to_string(),
        ));
    }
    let value =
        u32::try_from(value).map_err(|_| Error::Config("dimension is out of range".to_string()))?;
    value
        .to_string()
        .parse::<f32>()
        .map_err(|_| Error::Config("dimension cannot be represented as f32".to_string()))
}

fn round_to_usize(value: f32, field: &str) -> Result<usize> {
    if !value.is_finite() || value < 0.0 {
        return Err(Error::Config(format!(
            "{field} must be finite and non-negative"
        )));
    }
    let rounded = value.round();
    rounded
        .to_string()
        .parse::<usize>()
        .map_err(|_| Error::Config(format!("{field} is out of range")))
}

fn dimension_as_f32(value: usize) -> f32 {
    u32::try_from(value)
        .ok()
        .and_then(|value| value.to_string().parse::<f32>().ok())
        .unwrap_or(f32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn nms_suppresses_overlapping_same_class() {
        let detections = vec![
            Detection::new(BBox::new(0.0, 0.0, 10.0, 10.0), 0.9, 1),
            Detection::new(BBox::new(1.0, 1.0, 10.0, 10.0), 0.8, 1),
            Detection::new(BBox::new(1.0, 1.0, 10.0, 10.0), 0.7, 2),
        ];
        assert_eq!(nms(&detections, 0.5).unwrap().len(), 2);
    }

    #[test]
    fn nms_rejects_invalid_thresholds() {
        let detections = [Detection::new(BBox::new(0.0, 0.0, 1.0, 1.0), 1.0, 0)];
        assert!(nms(&detections, -0.1).is_err());
        assert!(nms(&detections, 1.1).is_err());
        assert!(nms(&detections, f32::NAN).is_err());
        assert!(nms_with_top_k(&detections, -0.1, 10).is_err());
        assert!(nms_with_top_k(&detections, 1.1, 10).is_err());
        assert!(nms_with_top_k(&detections, f32::NAN, 10).is_err());
    }

    #[test]
    fn nms_rejects_excess_candidates() {
        let detections = (0..MAX_NMS_CANDIDATES + 1)
            .map(|index| Detection::new(BBox::new(index as f32, 0.0, 1.0, 1.0), 1.0, index as u32))
            .collect::<Vec<_>>();
        let err = nms(&detections, 0.5).expect_err("should exceed candidate limit");
        assert!(err.to_string().contains("nms candidates"));
    }

    #[test]
    fn nms_with_top_k_truncates_and_runs() {
        let detections = (0..2000)
            .map(|index| {
                Detection::new(
                    BBox::new(index as f32, 0.0, 1.0, 1.0),
                    1.0 - index as f32 * 1e-9,
                    0,
                )
            })
            .collect::<Vec<_>>();
        let selected = nms_with_top_k(&detections, 0.5, 100).expect("nms top-k");
        assert!(selected.len() <= 100);
    }

    #[test]
    fn letterbox_maps_coordinates_back_to_source() {
        let letterbox = Letterbox::new(200, 100, 100, 100).expect("valid dimensions");
        let mapped = letterbox.map_to_source(BBox::new(25.0, 25.0, 50.0, 50.0));
        assert_eq!(mapped, BBox::new(50.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn top_k_rejects_oversized_input() {
        let values = vec![0.0; MAX_TOP_K_INPUT + 1];
        let err = top_k(&values, 1).expect_err("should exceed input limit");
        assert!(err.to_string().contains("top_k input values"));
    }

    #[test]
    fn top_k_rejects_oversized_k() {
        let values = vec![0.0; 2];
        let err = top_k(&values, MAX_TOP_K + 1).expect_err("should exceed k limit");
        assert!(err.to_string().contains("top_k k"));
    }

    proptest! {
        #[test]
        fn softmax_sums_to_one(values in proptest::collection::vec(-10.0_f32..10.0, 1..8)) {
            let output = softmax(&values).expect("softmax");
            let sum: f32 = output.iter().sum();
            prop_assert!((sum - 1.0).abs() < 0.0001);
        }

        #[test]
        fn top_k_never_returns_more_than_k(
            values in proptest::collection::vec(-10.0_f32..10.0, 0..16),
            k in 0_usize..16,
        ) {
            let result = top_k(&values, k).expect("top_k within bounds");
            prop_assert!(result.len() <= k.min(values.len()));
        }
    }
}
