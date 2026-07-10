use criterion::{black_box, criterion_group, criterion_main, Criterion};
use dg_core::{BBox, Detection};
use dg_elements::{nms, resize_letterbox, softmax, top_k};

fn index_f32(value: usize) -> f32 {
    u16::try_from(value).map_or(f32::from(u16::MAX), f32::from)
}

fn benchmark_resize(c: &mut Criterion) {
    let source = vec![0.5_f32; 640 * 480 * 3];
    c.bench_function("letterbox_resize", |bench| {
        bench.iter(|| {
            let result = resize_letterbox(black_box(&source), 3, 640, 480, 640, 640, 0.0);
            let _ = black_box(result);
        });
    });
}

fn benchmark_softmax_and_top_k(c: &mut Criterion) {
    let values = (0..1000)
        .map(|value| index_f32(value) * 0.001)
        .collect::<Vec<_>>();
    c.bench_function("softmax_1000", |bench| {
        bench.iter(|| black_box(softmax(black_box(&values))));
    });
    c.bench_function("top_k_1000", |bench| {
        bench.iter(|| black_box(top_k(black_box(&values), 10)));
    });
}

fn benchmark_nms(c: &mut Criterion) {
    let detections = (0..100)
        .map(|index| {
            Detection::new(
                BBox::new(
                    index_f32(index % 10) * 4.0,
                    index_f32(index / 10) * 4.0,
                    12.0,
                    12.0,
                ),
                1.0 - index_f32(index) * 0.005,
                u32::try_from(index % 3).map_or(0, |value| value),
            )
        })
        .collect::<Vec<_>>();
    c.bench_function("nms_100", |bench| {
        bench.iter(|| black_box(nms(black_box(&detections), 0.5)));
    });
}

criterion_group!(
    primitives,
    benchmark_resize,
    benchmark_softmax_and_top_k,
    benchmark_nms
);
criterion_main!(primitives);
