//! Board-side ResNet50v2 classification test via `dg-rknn`.
//!
//! Cross-compile (RK3568 / aarch64):
//! ```bash
//! export RKNN_SDK_ROOT=/path/to/sdk   # include/rknn_api.h + lib/librknnrt.so
//! export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
//! cargo build -p dg-rknn --features backend --example resnet50_board_test \
//!   --target aarch64-unknown-linux-gnu --release
//! ```
//!
//! Run on board:
//! ```bash
//! LD_LIBRARY_PATH=./lib ./resnet50_board_test \
//!   resnet50v2.rknn dog_224x224.jpg labels.txt result.txt
//! ```

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use dg_core::{CpuDevice, DataFormat, DataType, DeviceKind, Shape, Tensor, TensorDesc};
use dg_rknn::RknnBackend;
use dg_runtime::{
    BackendKind, BackendOptions, InferBackend, ModelSource, RknnOptions, RuntimeOption,
};

fn main() -> ExitCode {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let model_path = args.next().ok_or_else(|| {
        "usage: resnet50_board_test <model.rknn> <image.jpg> [labels.txt] [result.txt]".to_string()
    })?;
    let image_path = args
        .next()
        .ok_or_else(|| "missing image path".to_string())?;
    let labels_path = args.next();
    let result_path = args.next();

    let model = fs::read(&model_path).map_err(|e| format!("read model: {e}"))?;
    let option = RuntimeOption::new(
        BackendKind::Rknn,
        ModelSource::Bytes(Arc::new(model)),
        BackendOptions::Rknn(RknnOptions {
            core_mask: None,
            enable_zero_copy: false,
            dynamic_shape: false,
        }),
    );

    let mut backend = RknnBackend::new();
    backend
        .init(&option)
        .map_err(|e| format!("rknn init: {e}"))?;

    let input_info = backend
        .input_info(0)
        .map_err(|e| format!("input_info: {e}"))?
        .clone();
    let (h, w, c) = dims_nhwc(&input_info.shape)?;
    if c != 3 {
        return Err(format!("expected 3-channel input, got {c}"));
    }
    println!(
        "input: name={:?} shape={:?} dtype={:?} layout={:?}",
        input_info.name,
        input_info.shape.dims(),
        input_info.dtype,
        input_info.layout
    );
    let output_info = backend
        .output_info(0)
        .map_err(|e| format!("output_info: {e}"))?
        .clone();
    println!(
        "output: name={:?} shape={:?} dtype={:?} quant={:?}",
        output_info.name,
        output_info.shape.dims(),
        output_info.dtype,
        output_info.quant
    );

    let rgb = load_rgb888(&image_path, h, w)?;
    let device = CpuDevice::new();
    let desc = TensorDesc::new(
        Shape::new([1, h, w, 3]),
        DataType::U8,
        DataFormat::NHWC,
        DeviceKind::Cpu,
    );
    let input = Tensor::allocate(&device, desc).map_err(|e| format!("alloc input: {e}"))?;
    input
        .buffer()
        .write_from_slice(&rgb)
        .map_err(|e| format!("write input: {e}"))?;

    let outputs = backend
        .run(std::slice::from_ref(&input))
        .map_err(|e| format!("run: {e}"))?;
    let out = outputs.first().ok_or_else(|| "no outputs".to_string())?;
    let bytes = out
        .buffer()
        .read_bytes()
        .map_err(|e| format!("read output: {e}"))?;
    if bytes.len() % 4 != 0 {
        return Err(format!("output byte len {} not multiple of 4", bytes.len()));
    }
    let mut logits = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        logits.push(f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    let probs = softmax(&logits);
    let mut order: Vec<usize> = (0..probs.len()).collect();
    order.sort_by(|&a, &b| {
        probs[b]
            .partial_cmp(&probs[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let labels = labels_path
        .as_deref()
        .map(load_labels)
        .transpose()?
        .unwrap_or_default();

    println!("-----TOP 5-----");
    for &idx in order.iter().take(5) {
        let name = labels.get(idx).map(String::as_str).unwrap_or("?");
        println!("[{idx}] score:{:.6} class:\"{name}\"", probs[idx]);
    }

    if let Some(path) = result_path {
        if let Some((exp_cls, exp_score)) = parse_result_top1(&path)? {
            let got_cls = order[0];
            let got_score = probs[got_cls];
            let abs_err = (got_score - exp_score).abs();
            let rel_err = if exp_score > 0.0 {
                abs_err / exp_score
            } else {
                abs_err
            };
            println!(
                "\nAlign vs result.txt top1: expected[{exp_cls}]={exp_score:.6} \
                 got[{got_cls}]={got_score:.6} abs_err={abs_err:.6} rel_err={rel_err:.4} \
                 class_match={}",
                if got_cls == exp_cls { "YES" } else { "NO" }
            );
            if got_cls != exp_cls || rel_err > 0.05 {
                return Err("ALIGNMENT FAILED".to_string());
            }
            println!("ALIGNMENT OK");
        }
    }

    Ok(())
}

fn dims_nhwc(shape: &Shape) -> Result<(usize, usize, usize), String> {
    let d = shape.dims();
    match d {
        [1, h, w, c] => Ok((*h, *w, *c)),
        [h, w, c] => Ok((*h, *w, *c)),
        other => Err(format!("unexpected input shape {other:?}, expect NHWC")),
    }
}

fn load_rgb888(path: &str, h: usize, w: usize) -> Result<Vec<u8>, String> {
    let data = fs::read(path).map_err(|e| format!("read image: {e}"))?;
    if Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("bin"))
    {
        let need = h * w * 3;
        if data.len() != need {
            return Err(format!("raw rgb size {} != {need}", data.len()));
        }
        return Ok(data);
    }

    let img = image::load_from_memory(&data).map_err(|e| format!("decode image: {e}"))?;
    let rgb = img.to_rgb8();
    if rgb.width() as usize != w || rgb.height() as usize != h {
        return Err(format!(
            "image {}x{}, model expects {w}x{h}",
            rgb.width(),
            rgb.height()
        ));
    }
    Ok(rgb.into_raw())
}

fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut exps: Vec<f32> = logits.iter().map(|v| (v - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    if sum > 0.0 {
        for v in &mut exps {
            *v /= sum;
        }
    }
    exps
}

fn load_labels(path: &str) -> Result<Vec<String>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read labels: {e}"))?;
    Ok(text
        .lines()
        .map(|line| {
            line.split_once(':')
                .map(|(_, name)| name.trim().to_string())
                .unwrap_or_else(|| line.trim().to_string())
        })
        .collect())
}

fn parse_result_top1(path: &str) -> Result<Option<(usize, f32)>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read result: {e}"))?;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix('[') {
            if let Some((idx_s, after)) = rest.split_once(']') {
                if let Ok(idx) = idx_s.trim().parse::<usize>() {
                    if let Some(score_part) = after.split("score:").nth(1) {
                        let score_s = score_part.split_whitespace().next().unwrap_or("");
                        if let Ok(score) = score_s.parse::<f32>() {
                            return Ok(Some((idx, score)));
                        }
                    }
                }
            }
        }
    }
    Ok(None)
}
