use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, VecDeque};

use dg_core::{
    BBox, Classification, DataFormat, DataType, Detection, DeviceKind, Error as CoreError,
    FaceDetection, OcrText, Point, Shape, Tensor, TensorDesc, Track, TrackState,
};
use dg_graph::{
    CreatedElement, Element, ElementHandle, ElementIo, Error, NodeSpec, Packet, ParamField,
    ParamType, PortSchema, Result,
};

use crate::math::{iou, resize_letterbox, softmax, top_k, MAX_NMS_CANDIDATES};

const ANY_INPUT: [PortSchema; 1] = [PortSchema {
    name: "in",
    dtype: None,
    required: true,
}];
const TENSOR_INPUT: [PortSchema; 1] = [PortSchema {
    name: "in",
    dtype: Some(DataType::F32),
    required: true,
}];
const TENSOR_OUTPUT: [PortSchema; 1] = [PortSchema {
    name: "out",
    dtype: Some(DataType::F32),
    required: false,
}];
const RESULT_OUTPUT: [PortSchema; 1] = [PortSchema {
    name: "out",
    dtype: None,
    required: false,
}];
const RESNET_PREPROCESS_FIELDS: &[&str] = &["input_width", "input_height", "mean", "std"];
const RESNET_POSTPROCESS_FIELDS: &[&str] = &["top_k", "labels"];
const RETINAFACE_FIELDS: &[&str] = &[
    "image_width",
    "image_height",
    "stride",
    "confidence_threshold",
    "nms_threshold",
    "anchor_sizes",
];
const BYTETRACK_FIELDS: &[&str] = &["max_lost", "match_iou"];
const PPOCR_DET_FIELDS: &[&str] = &["threshold"];
const PPOCR_REC_FIELDS: &[&str] = &["alphabet", "blank_index"];

const RESNET_PREPROCESS_PARAMS: &[ParamField] = &[
    ParamField {
        name: "input_width",
        ty: ParamType::Uint,
        required: false,
    },
    ParamField {
        name: "input_height",
        ty: ParamType::Uint,
        required: false,
    },
    ParamField {
        name: "mean",
        ty: ParamType::Array(&ParamType::Float),
        required: false,
    },
    ParamField {
        name: "std",
        ty: ParamType::Array(&ParamType::Float),
        required: false,
    },
];
const RESNET_POSTPROCESS_PARAMS: &[ParamField] = &[
    ParamField {
        name: "top_k",
        ty: ParamType::Uint,
        required: false,
    },
    ParamField {
        name: "labels",
        ty: ParamType::Array(&ParamType::Str),
        required: false,
    },
];
const RETINAFACE_PARAMS: &[ParamField] = &[
    ParamField {
        name: "image_width",
        ty: ParamType::Uint,
        required: false,
    },
    ParamField {
        name: "image_height",
        ty: ParamType::Uint,
        required: false,
    },
    ParamField {
        name: "stride",
        ty: ParamType::Uint,
        required: false,
    },
    ParamField {
        name: "confidence_threshold",
        ty: ParamType::Float,
        required: false,
    },
    ParamField {
        name: "nms_threshold",
        ty: ParamType::Float,
        required: false,
    },
    ParamField {
        name: "anchor_sizes",
        ty: ParamType::Array(&ParamType::Float),
        required: false,
    },
];
const BYTETRACK_PARAMS: &[ParamField] = &[
    ParamField {
        name: "max_lost",
        ty: ParamType::Uint,
        required: false,
    },
    ParamField {
        name: "match_iou",
        ty: ParamType::Float,
        required: false,
    },
];
const PPOCR_DET_PARAMS: &[ParamField] = &[ParamField {
    name: "threshold",
    ty: ParamType::Float,
    required: false,
}];
const PPOCR_REC_PARAMS: &[ParamField] = &[
    ParamField {
        name: "alphabet",
        ty: ParamType::Str,
        required: false,
    },
    ParamField {
        name: "blank_index",
        ty: ParamType::Uint,
        required: false,
    },
];

const MAX_ANCHOR_SIZES: usize = 64;
const MAX_ANCHORS: usize = 1_000_000;
const MAX_LABELS: usize = 100_000;
const MAX_ALPHABET_LEN: usize = 100_000;
const MAX_MAX_LOST: u32 = 10_000;
const MAX_TRACKS: usize = 10_000;
const MAX_DETECTIONS_PER_FRAME: usize = 100_000;
const MAX_OCR_PIXELS: usize = 10_000_000;
const MAX_OCR_REGIONS: usize = 10_000;
const MAX_OCR_REGION_PIXELS: usize = 1_000_000;
const MAX_OCR_ROWS: usize = 100_000;

inventory::submit! {
    dg_graph::ElementDescriptor {
        kind: "resnet_preprocess",
        input_ports: &ANY_INPUT,
        output_ports: &TENSOR_OUTPUT,
        params: RESNET_PREPROCESS_PARAMS,
        validate: Some(validate_resnet_preprocess),
        create: create_resnet_preprocess,
    }
}
inventory::submit! {
    dg_graph::ElementDescriptor {
        kind: "resnet_postprocess",
        input_ports: &TENSOR_INPUT,
        output_ports: &RESULT_OUTPUT,
        params: RESNET_POSTPROCESS_PARAMS,
        validate: Some(validate_resnet_postprocess),
        create: create_resnet_postprocess,
    }
}
inventory::submit! {
    dg_graph::ElementDescriptor {
        kind: "retinaface",
        input_ports: &TENSOR_INPUT,
        output_ports: &RESULT_OUTPUT,
        params: RETINAFACE_PARAMS,
        validate: Some(validate_retinaface),
        create: create_retinaface,
    }
}
inventory::submit! {
    dg_graph::ElementDescriptor {
        kind: "bytetrack",
        input_ports: &ANY_INPUT,
        output_ports: &RESULT_OUTPUT,
        params: BYTETRACK_PARAMS,
        validate: Some(validate_bytetrack),
        create: create_bytetrack,
    }
}
inventory::submit! {
    dg_graph::ElementDescriptor {
        kind: "ppocr_det",
        input_ports: &TENSOR_INPUT,
        output_ports: &RESULT_OUTPUT,
        params: PPOCR_DET_PARAMS,
        validate: Some(validate_ppocr_det),
        create: create_ppocr_det,
    }
}
inventory::submit! {
    dg_graph::ElementDescriptor {
        kind: "ppocr_rec",
        input_ports: &TENSOR_INPUT,
        output_ports: &RESULT_OUTPUT,
        params: PPOCR_REC_PARAMS,
        validate: Some(validate_ppocr_rec),
        create: create_ppocr_rec,
    }
}

struct ResnetPreprocess {
    width: usize,
    height: usize,
    mean: [f32; 3],
    std: [f32; 3],
}

#[derive(Debug)]
struct ResnetPostprocess {
    top_k: usize,
    labels: Vec<String>,
}

struct Retinaface {
    width: usize,
    height: usize,
    score_threshold: f32,
    nms_threshold: f32,
    anchors: Vec<BBox>,
}

struct ByteTrack {
    next_id: u64,
    max_lost: u32,
    match_threshold: f32,
    tracks: Vec<TrackStateInner>,
}

struct TrackStateInner {
    track_id: u64,
    detection: Detection,
    lost: u32,
}

struct PpocrDet {
    threshold: f32,
}

struct PpocrRec {
    alphabet: Vec<char>,
    blank: usize,
}

impl Element for ResnetPreprocess {
    fn run(self: Box<Self>, io: ElementIo) -> Result<()> {
        let output_bytes = self
            .width
            .checked_mul(self.height)
            .and_then(|pixels| pixels.checked_mul(3))
            .and_then(|pixels| pixels.checked_mul(std::mem::size_of::<f32>()))
            .ok_or_else(|| Error::Config("resnet preprocess output bytes overflow".to_string()))?;
        io.policy().check_tensor_bytes(output_bytes)?;
        loop {
            let packet = next_packet(&io)?;
            if packet.is_eos() {
                io.broadcast_eos()?;
                return Ok(());
            }
            let tensor = resnet_preprocess_tensor(
                packet.tensor_ref().ok_or_else(|| {
                    Error::Runtime("resnet preprocess expects a tensor".to_string())
                })?,
                self.width,
                self.height,
                self.mean,
                self.std,
                io.policy(),
            )?;
            io.send("out", Packet::tensor(tensor).with_meta(packet.meta))?;
        }
    }
}

impl Element for ResnetPostprocess {
    fn run(self: Box<Self>, io: ElementIo) -> Result<()> {
        loop {
            let packet = next_packet(&io)?;
            if packet.is_eos() {
                io.broadcast_eos()?;
                return Ok(());
            }
            let values = f32_values(packet.tensor_ref().ok_or_else(|| {
                Error::Runtime("resnet postprocess expects a tensor".to_string())
            })?)?;
            let probabilities = softmax(&values)?;
            let results = top_k(&probabilities, self.top_k)?
                .into_iter()
                .map(|(index, score)| {
                    Ok(Classification {
                        class_id: u32::try_from(index)
                            .map_err(|_| Error::Runtime("class id is out of range".to_string()))?,
                        score,
                        label: self.labels.get(index).cloned(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            io.send(
                "out",
                Packet::classifications(results).with_meta(packet.meta),
            )?;
        }
    }
}

impl Element for Retinaface {
    fn run(self: Box<Self>, io: ElementIo) -> Result<()> {
        loop {
            let packet = next_packet(&io)?;
            if packet.is_eos() {
                io.broadcast_eos()?;
                return Ok(());
            }
            let values = f32_values(
                packet
                    .tensor_ref()
                    .ok_or_else(|| Error::Runtime("retinaface expects a tensor".to_string()))?,
            )?;
            let faces = decode_retinaface(
                &values,
                &self.anchors,
                self.width,
                self.height,
                self.score_threshold,
                self.nms_threshold,
            )?;
            io.send("out", Packet::faces(faces).with_meta(packet.meta))?;
        }
    }
}

impl Element for ByteTrack {
    fn run(mut self: Box<Self>, io: ElementIo) -> Result<()> {
        loop {
            let packet = next_packet(&io)?;
            if packet.is_eos() {
                io.broadcast_eos()?;
                return Ok(());
            }
            let detections = packet.detections_ref().ok_or_else(|| {
                Error::Runtime("bytetrack expects detections payload".to_string())
            })?;
            let results = self.update(detections)?;
            io.send("out", Packet::tracks(results).with_meta(packet.meta))?;
        }
    }
}

impl Element for PpocrDet {
    fn run(self: Box<Self>, io: ElementIo) -> Result<()> {
        loop {
            let packet = next_packet(&io)?;
            if packet.is_eos() {
                io.broadcast_eos()?;
                return Ok(());
            }
            let tensor = packet
                .tensor_ref()
                .ok_or_else(|| Error::Runtime("ppocr det expects a tensor".to_string()))?;
            let results = detect_text_regions(tensor, self.threshold)?;
            io.send("out", Packet::ocr(results).with_meta(packet.meta))?;
        }
    }
}

impl Element for PpocrRec {
    fn run(self: Box<Self>, io: ElementIo) -> Result<()> {
        loop {
            let packet = next_packet(&io)?;
            if packet.is_eos() {
                io.broadcast_eos()?;
                return Ok(());
            }
            let logits = f32_values(
                packet
                    .tensor_ref()
                    .ok_or_else(|| Error::Runtime("ppocr rec expects a tensor".to_string()))?,
            )?;
            let class_count = self
                .alphabet
                .len()
                .checked_add(1)
                .ok_or_else(|| Error::Runtime("ocr alphabet size overflow".to_string()))?;
            if class_count == 0 || logits.len() % class_count != 0 {
                return Err(Error::Runtime(
                    "ocr logits do not match alphabet size".to_string(),
                ));
            }
            let rows = logits.len() / class_count;
            if rows > MAX_OCR_ROWS {
                return Err(Error::ResourceLimit {
                    resource: "ppocr_rec rows".to_string(),
                    requested: rows,
                    limit: MAX_OCR_ROWS,
                });
            }
            let rows: Vec<&[f32]> = logits.chunks_exact(class_count).collect();
            let text = ctc_greedy_decode(&rows, &self.alphabet, self.blank)?;
            io.send(
                "out",
                Packet::ocr(vec![OcrText {
                    text,
                    score: 1.0,
                    bbox: None,
                }])
                .with_meta(packet.meta),
            )?;
        }
    }
}

impl ByteTrack {
    fn update(&mut self, detections: &[Detection]) -> Result<Vec<Track>> {
        if detections.len() > MAX_DETECTIONS_PER_FRAME {
            return Err(Error::ResourceLimit {
                resource: "bytetrack detections per frame".to_string(),
                requested: detections.len(),
                limit: MAX_DETECTIONS_PER_FRAME,
            });
        }
        for track in &mut self.tracks {
            track.lost = track.lost.saturating_add(1);
        }
        let mut matched = vec![false; self.tracks.len()];
        let mut output = Vec::new();
        for detection in detections {
            let best = self
                .tracks
                .iter()
                .enumerate()
                .filter(|(index, track)| {
                    !matched[*index]
                        && track.detection.class_id == detection.class_id
                        && iou(track.detection.bbox, detection.bbox) >= self.match_threshold
                })
                .max_by(|left, right| {
                    iou(left.1.detection.bbox, detection.bbox)
                        .total_cmp(&iou(right.1.detection.bbox, detection.bbox))
                })
                .map(|(index, _)| index);
            let (track_id, state) = if let Some(index) = best {
                matched[index] = true;
                let track = &mut self.tracks[index];
                track.detection = detection.clone();
                track.lost = 0;
                (track.track_id, TrackState::Tracked)
            } else {
                let track_id = self.next_id;
                self.next_id = self.next_id.saturating_add(1);
                self.tracks.push(TrackStateInner {
                    track_id,
                    detection: detection.clone(),
                    lost: 0,
                });
                matched.push(true);
                (track_id, TrackState::New)
            };
            output.push(Track {
                track_id,
                detection: detection.clone(),
                state,
            });
        }
        self.tracks.retain(|track| track.lost <= self.max_lost);
        if self.tracks.len() > MAX_TRACKS {
            self.tracks
                .sort_by_key(|track| (track.lost, track.track_id));
            self.tracks.truncate(MAX_TRACKS);
        }
        Ok(output)
    }
}

pub fn generate_anchors(
    width: usize,
    height: usize,
    stride: usize,
    sizes: &[f32],
) -> Result<Vec<BBox>> {
    if stride == 0 {
        return Err(Error::Config("anchor stride must be non-zero".to_string()));
    }
    if sizes.is_empty() {
        return Err(Error::Config("anchor sizes must not be empty".to_string()));
    }
    if sizes.len() > MAX_ANCHOR_SIZES {
        return Err(Error::ResourceLimit {
            resource: "anchor_sizes count".to_string(),
            requested: sizes.len(),
            limit: MAX_ANCHOR_SIZES,
        });
    }
    if sizes.iter().any(|size| *size <= 0.0) {
        return Err(Error::Config(
            "anchor sizes must be greater than zero".to_string(),
        ));
    }
    let cells_x = width.div_ceil(stride);
    let cells_y = height.div_ceil(stride);
    let cells = cells_x
        .checked_mul(cells_y)
        .ok_or_else(|| Error::Config("anchor cell count overflow".to_string()))?;
    let anchor_count = cells
        .checked_mul(sizes.len())
        .ok_or_else(|| Error::Config("anchor count overflow".to_string()))?;
    if anchor_count > MAX_ANCHORS {
        return Err(Error::ResourceLimit {
            resource: "anchor count".to_string(),
            requested: anchor_count,
            limit: MAX_ANCHORS,
        });
    }
    let mut anchors = Vec::new();
    anchors.try_reserve_exact(anchor_count).map_err(|_| {
        Error::Runtime(format!(
            "retinaface anchor allocation failed for {anchor_count} anchors"
        ))
    })?;
    for y in (0..height).step_by(stride) {
        for x in (0..width).step_by(stride) {
            let center_x = dimension_or_zero(x) / dimension_or_one(width);
            let center_y = dimension_or_zero(y) / dimension_or_one(height);
            for size in sizes {
                anchors.push(BBox::new(center_x, center_y, *size, *size));
            }
        }
    }
    Ok(anchors)
}

fn decode_retinaface(
    values: &[f32],
    anchors: &[BBox],
    width: usize,
    height: usize,
    score_threshold: f32,
    nms_threshold: f32,
) -> Result<Vec<FaceDetection>> {
    const ATTRIBUTES: usize = 15;
    if values.len().checked_rem(ATTRIBUTES) != Some(0) {
        return Err(Error::Runtime(
            "retinaface output must have 15 values per anchor".to_string(),
        ));
    }
    let width_f = usize_f32(width)?;
    let height_f = usize_f32(height)?;
    let candidate_capacity = MAX_NMS_CANDIDATES.min(values.len() / ATTRIBUTES);
    let mut heap: BinaryHeap<Reverse<ByFaceScore>> = BinaryHeap::with_capacity(candidate_capacity);
    for (index, row) in values.chunks_exact(ATTRIBUTES).enumerate() {
        if !row.iter().all(|value| value.is_finite()) {
            continue;
        }
        let anchor = match anchors.get(index).copied() {
            Some(anchor) => anchor,
            None => {
                return Err(Error::Runtime(
                    "retinaface values length does not match anchors".to_string(),
                ))
            }
        };
        let score = crate::math::sigmoid(row[4]);
        if score < score_threshold {
            continue;
        }
        if heap.len() == MAX_NMS_CANDIDATES {
            let lowest = heap.peek().ok_or_else(|| {
                Error::Runtime("retinaface candidate heap unexpectedly empty".to_string())
            })?;
            if score.total_cmp(&lowest.0.score()) != Ordering::Greater {
                continue;
            }
            heap.pop();
        }
        let center_x = (anchor.x + row[0] * 0.1 * anchor.w).clamp(0.0, 1.0) * width_f;
        let center_y = (anchor.y + row[1] * 0.1 * anchor.h).clamp(0.0, 1.0) * height_f;
        let box_width = (anchor.w * row[2].exp()).clamp(0.0, 1.0) * width_f;
        let box_height = (anchor.h * row[3].exp()).clamp(0.0, 1.0) * height_f;
        let bbox = BBox::new(
            (center_x - box_width * 0.5).max(0.0),
            (center_y - box_height * 0.5).max(0.0),
            box_width,
            box_height,
        );
        let mut landmarks = Vec::with_capacity(5);
        for point in row[5..].chunks_exact(2) {
            landmarks.push(Point {
                x: (anchor.x + point[0] * 0.1 * anchor.w).clamp(0.0, 1.0) * width_f,
                y: (anchor.y + point[1] * 0.1 * anchor.h).clamp(0.0, 1.0) * height_f,
            });
        }
        heap.push(Reverse(ByFaceScore(FaceDetection {
            bbox,
            score,
            landmarks,
        })));
    }

    let sorted = heap.into_sorted_vec();
    let mut candidates = Vec::new();
    candidates
        .try_reserve_exact(sorted.len())
        .map_err(|_| Error::Runtime("retinaface candidate allocation failed".to_string()))?;
    for Reverse(ByFaceScore(detection)) in sorted {
        candidates.push(detection);
    }
    // `into_sorted_vec` on a max-heap of `Reverse<ByFaceScore>` returns the
    // highest-score detections first, which is what the greedy NMS below expects.
    let mut selected = Vec::new();
    selected
        .try_reserve_exact(candidates.len())
        .map_err(|_| Error::Runtime("retinaface selected allocation failed".to_string()))?;
    for candidate in candidates {
        if selected
            .iter()
            .all(|existing: &FaceDetection| iou(existing.bbox, candidate.bbox) <= nms_threshold)
        {
            selected.push(candidate);
        }
    }
    Ok(selected)
}

/// A score-only view of a face detection so the top-k heap does not need to
/// keep every candidate in memory.
struct ByFaceScore(FaceDetection);

impl ByFaceScore {
    fn score(&self) -> f32 {
        self.0.score
    }
}

impl PartialEq for ByFaceScore {
    fn eq(&self, other: &Self) -> bool {
        self.0.score.to_bits() == other.0.score.to_bits()
    }
}

impl Eq for ByFaceScore {}

impl PartialOrd for ByFaceScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ByFaceScore {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.score.total_cmp(&other.0.score)
    }
}

pub fn ctc_greedy_decode(
    rows: &[impl AsRef<[f32]>],
    alphabet: &[char],
    blank: usize,
) -> Result<String> {
    if rows.len() > MAX_OCR_ROWS {
        return Err(Error::ResourceLimit {
            resource: "ctc rows".to_string(),
            requested: rows.len(),
            limit: MAX_OCR_ROWS,
        });
    }
    let mut output = String::new();
    let mut previous = None;
    for row in rows {
        let row = row.as_ref();
        if row.is_empty() {
            continue;
        }
        let index = row
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(index, _)| index)
            .ok_or_else(|| Error::Runtime("empty CTC row".to_string()))?;
        if index != blank && Some(index) != previous {
            let alphabet_index = if index < blank { index } else { index - 1 };
            let character = alphabet
                .get(alphabet_index)
                .ok_or_else(|| Error::Runtime("CTC class exceeds alphabet".to_string()))?;
            output.push(*character);
        }
        previous = Some(index);
    }
    Ok(output)
}

fn detect_text_regions(tensor: &Tensor, threshold: f32) -> Result<Vec<OcrText>> {
    let dims = tensor.desc().shape().dims();
    let (height, width) = match dims {
        [1, 1, height, width] | [1, height, width] => (*height, *width),
        _ => {
            return Err(Error::Config(
                "ppocr det expects [1,1,H,W] or [1,H,W]".to_string(),
            ))
        }
    };
    let values = f32_values(tensor)?;
    if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
        return Err(Error::Config(
            "ppocr det threshold must be a probability".to_string(),
        ));
    }
    let expected = height
        .checked_mul(width)
        .ok_or_else(|| Error::Runtime("ocr map dimensions overflow".to_string()))?;
    if expected > MAX_OCR_PIXELS {
        return Err(Error::ResourceLimit {
            resource: "ppocr_det input pixels".to_string(),
            requested: expected,
            limit: MAX_OCR_PIXELS,
        });
    }
    if values.len() != expected {
        return Err(Error::Runtime("ocr map size mismatch".to_string()));
    }
    let mut visited = Vec::new();
    visited.try_reserve_exact(expected).map_err(|_| {
        Error::Runtime(format!(
            "ppocr_det visited allocation failed for {expected} pixels"
        ))
    })?;
    visited.resize(expected, false);
    let mut output = Vec::new();
    for start in 0..expected {
        if visited[start] || values[start] < threshold {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        visited[start] = true;
        let mut points = Vec::new();
        while let Some(index) = queue.pop_front() {
            points.push(index);
            if points.len() > MAX_OCR_REGION_PIXELS {
                return Err(Error::ResourceLimit {
                    resource: "ppocr_det connected component pixels".to_string(),
                    requested: points.len(),
                    limit: MAX_OCR_REGION_PIXELS,
                });
            }
            let y = index / width;
            let x = index % width;
            for (next_x, next_y) in [
                (x.saturating_sub(1), y),
                (x.saturating_add(1), y),
                (x, y.saturating_sub(1)),
                (x, y.saturating_add(1)),
            ] {
                if next_x >= width || next_y >= height {
                    continue;
                }
                let next = next_y * width + next_x;
                if !visited[next] && values[next] >= threshold {
                    visited[next] = true;
                    queue.push_back(next);
                }
            }
        }
        let first = points[0];
        let mut min_x = first % width;
        let mut max_x = min_x;
        let mut min_y = first / width;
        let mut max_y = min_y;
        for index in points.iter().copied().skip(1) {
            min_x = min_x.min(index % width);
            max_x = max_x.max(index % width);
            min_y = min_y.min(index / width);
            max_y = max_y.max(index / width);
        }
        let width_f = usize_f32(width)?;
        let height_f = usize_f32(height)?;
        let score =
            points.iter().map(|index| values[*index]).sum::<f32>() / usize_f32(points.len())?;
        output.push(OcrText {
            text: String::new(),
            score,
            bbox: Some(BBox::new(
                usize_f32(min_x)? / width_f,
                usize_f32(min_y)? / height_f,
                usize_f32(max_x.saturating_sub(min_x).saturating_add(1))? / width_f,
                usize_f32(max_y.saturating_sub(min_y).saturating_add(1))? / height_f,
            )),
        });
        if output.len() >= MAX_OCR_REGIONS {
            break;
        }
    }
    Ok(output)
}

fn resnet_preprocess_tensor(
    input: &Tensor,
    width: usize,
    height: usize,
    mean: [f32; 3],
    std: [f32; 3],
    policy: &dg_core::ResourcePolicy,
) -> Result<Tensor> {
    let dims = input.desc().shape().dims();
    let (channels, source_height, source_width) = match (input.desc().format(), dims) {
        (DataFormat::NCHW, [1, channels, height, width])
        | (DataFormat::NCHW, [channels, height, width]) => (*channels, *height, *width),
        _ => {
            return Err(Error::Config(
                "resnet preprocess expects NCHW rank 3/4 input".to_string(),
            ))
        }
    };
    if channels != 3 {
        return Err(Error::Config("resnet expects three channels".to_string()));
    }
    let values = tensor_values(input)?;
    let mut hwc = Vec::new();
    hwc.try_reserve_exact(values.len())
        .map_err(|_| Error::Runtime("resnet preprocess hwc allocation failed".to_string()))?;
    hwc.resize(values.len(), 0.0);
    for channel in 0..channels {
        for y in 0..source_height {
            for x in 0..source_width {
                let source = (channel * source_height + y) * source_width + x;
                let target = (y * source_width + x) * channels + channel;
                hwc[target] = values[source];
            }
        }
    }
    let (resized, _) = resize_letterbox(
        &hwc,
        channels,
        source_width,
        source_height,
        width,
        height,
        0.0,
    )?;
    let device = dg_core::CpuDevice::new();
    let output = Tensor::allocate_with_policy(
        &device,
        TensorDesc::new(
            Shape::new([1, 3, height, width]),
            DataType::F32,
            DataFormat::NCHW,
            DeviceKind::Cpu,
        ),
        policy,
    )?;
    let mut bytes = Vec::new();
    for channel in 0..3 {
        for y in 0..height {
            for x in 0..width {
                let value = resized[(y * width + x) * 3 + channel] / 255.0;
                let normalized = (value - mean[channel]) / std[channel];
                bytes.extend_from_slice(&normalized.to_ne_bytes());
            }
        }
    }
    output.buffer().write_from_slice(&bytes)?;
    Ok(output)
}

fn create_resnet_preprocess(node: &NodeSpec) -> Result<CreatedElement> {
    Ok(CreatedElement {
        element: Box::new(parse_resnet_preprocess(node)?),
        handle: ElementHandle::None,
    })
}

fn create_resnet_postprocess(node: &NodeSpec) -> Result<CreatedElement> {
    Ok(CreatedElement {
        element: Box::new(parse_resnet_postprocess(node)?),
        handle: ElementHandle::None,
    })
}

fn create_retinaface(node: &NodeSpec) -> Result<CreatedElement> {
    let config = parse_retinaface(node)?;
    let anchors = generate_anchors(config.width, config.height, config.stride, &config.sizes)?;
    Ok(CreatedElement {
        element: Box::new(Retinaface {
            width: config.width,
            height: config.height,
            score_threshold: config.score_threshold,
            nms_threshold: config.nms_threshold,
            anchors,
        }),
        handle: ElementHandle::None,
    })
}

fn create_bytetrack(node: &NodeSpec) -> Result<CreatedElement> {
    let (max_lost, match_threshold) = parse_bytetrack(node)?;
    Ok(CreatedElement {
        element: Box::new(ByteTrack {
            next_id: 1,
            max_lost,
            match_threshold,
            tracks: Vec::new(),
        }),
        handle: ElementHandle::None,
    })
}

fn create_ppocr_det(node: &NodeSpec) -> Result<CreatedElement> {
    Ok(CreatedElement {
        element: Box::new(PpocrDet {
            threshold: parse_ppocr_det(node)?,
        }),
        handle: ElementHandle::None,
    })
}

fn create_ppocr_rec(node: &NodeSpec) -> Result<CreatedElement> {
    let (alphabet, blank) = parse_ppocr_rec(node)?;
    Ok(CreatedElement {
        element: Box::new(PpocrRec { alphabet, blank }),
        handle: ElementHandle::None,
    })
}

fn validate_resnet_preprocess(node: &NodeSpec) -> Result<()> {
    parse_resnet_preprocess(node).map(|_| ())
}

fn validate_resnet_postprocess(node: &NodeSpec) -> Result<()> {
    parse_resnet_postprocess(node).map(|_| ())
}

fn validate_retinaface(node: &NodeSpec) -> Result<()> {
    parse_retinaface(node).map(|_| ())
}

fn validate_bytetrack(node: &NodeSpec) -> Result<()> {
    parse_bytetrack(node).map(|_| ())
}

fn validate_ppocr_det(node: &NodeSpec) -> Result<()> {
    parse_ppocr_det(node).map(|_| ())
}

fn validate_ppocr_rec(node: &NodeSpec) -> Result<()> {
    parse_ppocr_rec(node).map(|_| ())
}

fn parse_resnet_preprocess(node: &NodeSpec) -> Result<ResnetPreprocess> {
    let params = params_object(node)?;
    reject_unknown_fields(params, RESNET_PREPROCESS_FIELDS)?;
    let width = read_nonzero_usize(params, "input_width", 224)?;
    let height = read_nonzero_usize(params, "input_height", width)?;
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or_else(|| Error::Config("resnet input dimensions overflow".to_string()))?;
    let mean = read_f32_array(params, "mean", [0.485, 0.456, 0.406])?;
    let std = read_f32_array(params, "std", [0.229, 0.224, 0.225])?;
    if std.iter().any(|value| *value <= 0.0) {
        return Err(Error::Config(
            "field std values must be greater than zero".to_string(),
        ));
    }
    Ok(ResnetPreprocess {
        width,
        height,
        mean,
        std,
    })
}

fn parse_resnet_postprocess(node: &NodeSpec) -> Result<ResnetPostprocess> {
    let params = params_object(node)?;
    reject_unknown_fields(params, RESNET_POSTPROCESS_FIELDS)?;
    let top_k = read_nonzero_usize(params, "top_k", 5)?;
    if top_k > crate::math::MAX_TOP_K {
        return Err(Error::ResourceLimit {
            resource: "resnet_postprocess top_k".to_string(),
            requested: top_k,
            limit: crate::math::MAX_TOP_K,
        });
    }
    let labels = read_string_vec(params, "labels")?;
    if labels.len() > MAX_LABELS {
        return Err(Error::ResourceLimit {
            resource: "resnet_postprocess labels".to_string(),
            requested: labels.len(),
            limit: MAX_LABELS,
        });
    }
    Ok(ResnetPostprocess { top_k, labels })
}

#[derive(Debug)]
struct RetinafaceConfig {
    width: usize,
    height: usize,
    stride: usize,
    score_threshold: f32,
    nms_threshold: f32,
    sizes: Vec<f32>,
}

fn parse_retinaface(node: &NodeSpec) -> Result<RetinafaceConfig> {
    let params = params_object(node)?;
    reject_unknown_fields(params, RETINAFACE_FIELDS)?;
    let width = read_nonzero_usize(params, "image_width", 640)?;
    let height = read_nonzero_usize(params, "image_height", width)?;
    let stride = read_nonzero_usize(params, "stride", 16)?;
    let score_threshold = read_probability(params, "confidence_threshold", 0.5)?;
    let nms_threshold = read_probability(params, "nms_threshold", 0.4)?;
    let sizes = read_f32_vec(params, "anchor_sizes")?;
    let sizes = if sizes.is_empty() {
        vec![0.1, 0.2]
    } else {
        sizes
    };
    if sizes.len() > MAX_ANCHOR_SIZES {
        return Err(Error::ResourceLimit {
            resource: "retinaface anchor_sizes".to_string(),
            requested: sizes.len(),
            limit: MAX_ANCHOR_SIZES,
        });
    }
    if sizes.iter().any(|size| *size <= 0.0) {
        return Err(Error::Config(
            "field anchor_sizes values must be greater than zero".to_string(),
        ));
    }
    let cells_x = width.div_ceil(stride);
    let cells_y = height.div_ceil(stride);
    let cells = cells_x
        .checked_mul(cells_y)
        .ok_or_else(|| Error::Config("retinaface anchor cell count overflow".to_string()))?;
    let anchor_count = cells
        .checked_mul(sizes.len())
        .ok_or_else(|| Error::Config("retinaface anchor count overflow".to_string()))?;
    if anchor_count > MAX_ANCHORS {
        return Err(Error::ResourceLimit {
            resource: "retinaface anchor count".to_string(),
            requested: anchor_count,
            limit: MAX_ANCHORS,
        });
    }
    Ok(RetinafaceConfig {
        width,
        height,
        stride,
        score_threshold,
        nms_threshold,
        sizes,
    })
}

fn parse_bytetrack(node: &NodeSpec) -> Result<(u32, f32)> {
    let params = params_object(node)?;
    reject_unknown_fields(params, BYTETRACK_FIELDS)?;
    let max_lost = read_usize(params, "max_lost", 2)?
        .try_into()
        .map_err(|_| Error::Config("max_lost is out of range".to_string()))?;
    if max_lost > MAX_MAX_LOST {
        return Err(Error::ResourceLimit {
            resource: "bytetrack max_lost".to_string(),
            requested: max_lost as usize,
            limit: MAX_MAX_LOST as usize,
        });
    }
    let match_threshold = read_probability(params, "match_iou", 0.3)?;
    Ok((max_lost, match_threshold))
}

fn parse_ppocr_det(node: &NodeSpec) -> Result<f32> {
    let params = params_object(node)?;
    reject_unknown_fields(params, PPOCR_DET_FIELDS)?;
    read_probability(params, "threshold", 0.3)
}

fn parse_ppocr_rec(node: &NodeSpec) -> Result<(Vec<char>, usize)> {
    let params = params_object(node)?;
    reject_unknown_fields(params, PPOCR_REC_FIELDS)?;
    let alphabet_str = match params.get("alphabet") {
        None => "0123456789",
        Some(value) => value
            .as_str()
            .ok_or_else(|| Error::Config("field alphabet must be a string".to_string()))?,
    };
    let char_count = alphabet_str.chars().count();
    if char_count == 0 {
        return Err(Error::Config(
            "field alphabet must not be empty".to_string(),
        ));
    }
    if char_count > MAX_ALPHABET_LEN {
        return Err(Error::ResourceLimit {
            resource: "ppocr_rec alphabet length".to_string(),
            requested: char_count,
            limit: MAX_ALPHABET_LEN,
        });
    }
    let mut alphabet = Vec::new();
    alphabet.try_reserve_exact(char_count).map_err(|_| {
        Error::Runtime(format!(
            "ppocr_rec alphabet allocation failed for {char_count} chars"
        ))
    })?;
    alphabet.extend(alphabet_str.chars());
    let blank = read_usize(params, "blank_index", alphabet.len())?;
    if blank > alphabet.len() {
        return Err(Error::Config(format!(
            "field blank_index must not exceed the alphabet length ({})",
            alphabet.len()
        )));
    }
    Ok((alphabet, blank))
}

fn next_packet(io: &ElementIo) -> Result<Packet> {
    loop {
        if let Some(packet) = io.recv("in")? {
            return Ok(packet);
        }
        if io.should_stop() {
            return Err(Error::NotRunning);
        }
    }
}

fn tensor_values(tensor: &Tensor) -> Result<Vec<f32>> {
    if !tensor.buffer().is_host_readable() {
        return Err(Error::Core(CoreError::Unsupported(
            "tensor buffer is not host-readable; staging required".to_string(),
        )));
    }
    match tensor.desc().dtype() {
        DataType::U8 => {
            let bytes = tensor.buffer().read_bytes()?;
            let mut values = Vec::new();
            values.try_reserve_exact(bytes.len()).map_err(|_| {
                Error::Runtime("extras tensor_values u8 allocation failed".to_string())
            })?;
            for byte in bytes {
                values.push(f32::from(byte));
            }
            Ok(values)
        }
        DataType::F32 => f32_values(tensor),
        dtype => Err(Error::Config(format!(
            "algorithm elements support only u8/f32 tensors, got {dtype:?}"
        ))),
    }
}

fn f32_values(tensor: &Tensor) -> Result<Vec<f32>> {
    if !tensor.buffer().is_host_readable() {
        return Err(Error::Core(CoreError::Unsupported(
            "tensor buffer is not host-readable; staging required".to_string(),
        )));
    }
    let bytes = tensor.buffer().read_bytes()?;
    let elem_bytes = std::mem::size_of::<f32>();
    if bytes.len() % elem_bytes != 0 {
        return Err(Error::Runtime("f32 tensor has partial element".to_string()));
    }
    let count = bytes.len() / elem_bytes;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| Error::Runtime("extras f32_values allocation failed".to_string()))?;
    for chunk in bytes.chunks_exact(elem_bytes) {
        let bytes: [u8; 4] = chunk
            .try_into()
            .map_err(|_| Error::Runtime("invalid f32 tensor element".to_string()))?;
        values.push(f32::from_ne_bytes(bytes));
    }
    if !values.iter().all(|value| value.is_finite()) {
        return Err(Error::Config(
            "tensor contains non-finite floating point values".to_string(),
        ));
    }
    Ok(values)
}

fn params_object(node: &NodeSpec) -> Result<&serde_json::Map<String, serde_json::Value>> {
    node.params
        .as_object()
        .ok_or_else(|| Error::Config(format!("node {} params must be an object", node.name)))
}

fn reject_unknown_fields(
    params: &serde_json::Map<String, serde_json::Value>,
    allowed: &[&str],
) -> Result<()> {
    for key in params.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(Error::Config(format!(
                "unknown field `{key}`; expected one of {}",
                allowed.join(", ")
            )));
        }
    }
    Ok(())
}

fn read_usize(
    params: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    default: usize,
) -> Result<usize> {
    params.get(key).map_or(Ok(default), |value| {
        let value = value
            .as_u64()
            .ok_or_else(|| Error::Config(format!("field {key} must be an integer")))?;
        usize::try_from(value).map_err(|_| Error::Config(format!("field {key} out of range")))
    })
}

fn read_f32(
    params: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    default: f32,
) -> Result<f32> {
    params.get(key).map_or(Ok(default), |value| {
        let value = value
            .as_f64()
            .ok_or_else(|| Error::Config(format!("field {key} must be a number")))?;
        let narrowed = value
            .to_string()
            .parse::<f32>()
            .map_err(|_| Error::Config(format!("field {key} out of range")))?;
        if narrowed.is_finite() {
            Ok(narrowed)
        } else {
            Err(Error::Config(format!("field {key} must be finite")))
        }
    })
}

fn read_f32_array(
    params: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    default: [f32; 3],
) -> Result<[f32; 3]> {
    let values = match params.get(key) {
        None => return Ok(default),
        Some(value) => value
            .as_array()
            .ok_or_else(|| Error::Config(format!("field {key} must be an array")))?,
    };
    if values.len() != 3 {
        return Err(Error::Config(format!(
            "field {key} must contain three values"
        )));
    }
    let mut output = default;
    for (index, value) in values.iter().enumerate() {
        let parsed = value
            .as_f64()
            .ok_or_else(|| Error::Config(format!("field {key} must contain numbers")))?
            .to_string()
            .parse::<f32>()
            .map_err(|_| Error::Config(format!("field {key} out of range")))?;
        if !parsed.is_finite() {
            return Err(Error::Config(format!("field {key} values must be finite")));
        }
        output[index] = parsed;
    }
    Ok(output)
}

fn read_f32_vec(
    params: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Vec<f32>> {
    let values = match params.get(key) {
        None => return Ok(Vec::new()),
        Some(value) => value
            .as_array()
            .ok_or_else(|| Error::Config(format!("field {key} must be an array")))?,
    };
    values
        .iter()
        .map(|value| {
            let parsed = value
                .as_f64()
                .ok_or_else(|| Error::Config(format!("field {key} must contain numbers")))?
                .to_string()
                .parse::<f32>()
                .map_err(|_| Error::Config(format!("field {key} out of range")))?;
            if !parsed.is_finite() {
                return Err(Error::Config(format!("field {key} values must be finite")));
            }
            Ok(parsed)
        })
        .collect()
}

fn read_string_vec(
    params: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Vec<String>> {
    let values = match params.get(key) {
        None => return Ok(Vec::new()),
        Some(value) => value
            .as_array()
            .ok_or_else(|| Error::Config(format!("field {key} must be an array")))?,
    };
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| Error::Config(format!("field {key} must contain strings")))
        })
        .collect()
}

fn read_nonzero_usize(
    params: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    default: usize,
) -> Result<usize> {
    let value = read_usize(params, key, default)?;
    if value == 0 {
        return Err(Error::Config(format!("field {key} must be non-zero")));
    }
    Ok(value)
}

fn read_probability(
    params: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    default: f32,
) -> Result<f32> {
    let value = read_f32(params, key, default)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(Error::Config(format!(
            "field {key} must be between 0 and 1"
        )));
    }
    Ok(value)
}

fn usize_f32(value: usize) -> Result<f32> {
    value
        .to_string()
        .parse::<f32>()
        .map_err(|_| Error::Runtime("dimension cannot be represented as f32".to_string()))
}

fn dimension_or_zero(value: usize) -> f32 {
    value.to_string().parse::<f32>().map_or(0.0, |value| value)
}

fn dimension_or_one(value: usize) -> f32 {
    match value.to_string().parse::<f32>() {
        Ok(value) if value != 0.0 => value,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctc_decodes_repeated_symbols_and_blank() {
        let rows = vec![
            vec![0.9, 0.1, 0.0],
            vec![0.8, 0.1, 0.0],
            vec![0.1, 0.9, 0.0],
            vec![0.1, 0.9, 0.0],
        ];
        assert_eq!(
            ctc_greedy_decode(&rows, &['a', 'b'], 2).expect("decode"),
            "ab"
        );
        let blank_first_rows = vec![
            vec![0.0, 0.9, 0.1],
            vec![0.9, 0.1, 0.0],
            vec![0.0, 0.1, 0.9],
        ];
        assert_eq!(
            ctc_greedy_decode(&blank_first_rows, &['a', 'b'], 0).expect("decode"),
            "ab"
        );
    }

    #[test]
    fn anchor_generation_and_retina_decode_are_bounded() {
        let anchors = generate_anchors(32, 32, 16, &[0.25]).expect("anchors");
        assert_eq!(anchors.len(), 4);
        let values = vec![0.0; 15];
        let faces = decode_retinaface(
            &values,
            &[BBox::new(0.5, 0.5, 0.25, 0.25)],
            32,
            32,
            0.4,
            0.5,
        )
        .expect("decode");
        assert_eq!(faces.len(), 1);
        assert!((faces[0].bbox.x - 12.0).abs() < f32::EPSILON);
        assert!((faces[0].bbox.y - 12.0).abs() < f32::EPSILON);
        assert!((faces[0].bbox.w - 8.0).abs() < f32::EPSILON);
        assert!((faces[0].bbox.h - 8.0).abs() < f32::EPSILON);
        assert!((0.0..=32.0).contains(&faces[0].bbox.x));
        assert!((0.0..=32.0).contains(&faces[0].bbox.y));
        assert!(faces[0]
            .landmarks
            .iter()
            .all(|point| { (0.0..=32.0).contains(&point.x) && (0.0..=32.0).contains(&point.y) }));
    }

    #[test]
    fn anchor_generation_rejects_excess() {
        let sizes = (0..MAX_ANCHOR_SIZES + 1).map(|_| 0.1).collect::<Vec<_>>();
        let err = generate_anchors(32, 32, 16, &sizes).expect_err("should exceed anchor sizes");
        assert!(err.to_string().contains("anchor_sizes"));
    }

    #[test]
    fn bytetrack_keeps_ids_and_reclaims_expired_tracks() {
        let detection = Detection::new(BBox::new(0.0, 0.0, 10.0, 10.0), 0.9, 0);
        let mut tracker = ByteTrack {
            next_id: 1,
            max_lost: 1,
            match_threshold: 0.3,
            tracks: Vec::new(),
        };
        let first = tracker
            .update(std::slice::from_ref(&detection))
            .expect("update");
        let second = tracker
            .update(std::slice::from_ref(&detection))
            .expect("update");
        assert_eq!(first[0].track_id, second[0].track_id);
        assert_eq!(second[0].state, TrackState::Tracked);
        assert!(tracker.update(&[]).expect("empty").is_empty());
        assert!(tracker.update(&[]).expect("empty").is_empty());
        let replacement = tracker
            .update(std::slice::from_ref(&detection))
            .expect("update");
        assert_ne!(replacement[0].track_id, first[0].track_id);
    }

    #[test]
    fn bytetrack_rejects_excess_detections() {
        let detections = (0..MAX_DETECTIONS_PER_FRAME + 1)
            .map(|index| Detection::new(BBox::new(index as f32, 0.0, 1.0, 1.0), 0.5, 0))
            .collect::<Vec<_>>();
        let mut tracker = ByteTrack {
            next_id: 1,
            max_lost: 100,
            match_threshold: 0.0,
            tracks: Vec::new(),
        };
        let err = tracker
            .update(&detections)
            .expect_err("should exceed detection limit");
        assert!(err.to_string().contains("bytetrack detections"));
    }

    #[test]
    fn bytetrack_caps_active_tracks() {
        let mut tracker = ByteTrack {
            next_id: 1,
            max_lost: 100,
            match_threshold: 1.1, // no matches possible
            tracks: Vec::new(),
        };
        for index in 0..MAX_TRACKS + 5 {
            let _ = tracker
                .update(&[Detection::new(
                    BBox::new(index as f32, 0.0, 1.0, 1.0),
                    0.5,
                    0,
                )])
                .expect("update");
        }
        assert!(tracker.tracks.len() <= MAX_TRACKS);
    }

    #[test]
    fn ppocr_rec_rejects_oversized_alphabet() {
        let alphabet = (0..MAX_ALPHABET_LEN + 1).map(|_| 'a').collect::<String>();
        let node = NodeSpec {
            name: "ppocr_rec".to_string(),
            params: serde_json::json!({"alphabet": alphabet}),
            ..Default::default()
        };
        let err = parse_ppocr_rec(&node).expect_err("should exceed alphabet limit");
        assert!(err.to_string().contains("alphabet"));
    }

    #[test]
    fn retinaface_rejects_oversized_anchors() {
        let node = NodeSpec {
            name: "retinaface".to_string(),
            params: serde_json::json!({
                "image_width": 64000,
                "image_height": 64000,
                "stride": 1,
                "anchor_sizes": [0.1],
            }),
            ..Default::default()
        };
        let err = parse_retinaface(&node).expect_err("should exceed anchor limit");
        assert!(err.to_string().contains("anchor count"));
    }

    #[test]
    fn resnet_postprocess_rejects_oversized_top_k() {
        let node = NodeSpec {
            name: "resnet_postprocess".to_string(),
            params: serde_json::json!({"top_k": crate::math::MAX_TOP_K + 1}),
            ..Default::default()
        };
        let err = parse_resnet_postprocess(&node).expect_err("should exceed top_k limit");
        assert!(err.to_string().contains("top_k"));
    }
}
