use criterion::{Criterion, black_box, criterion_group, criterion_main};
use stringdex::internals::tree::{encode_search_tree_naive, encode_search_tree_ukkonen};

pub fn tree_builder_naive_vs_multiple(c: &mut Criterion) {
    let dataset = std::fs::read("words")
        .or_else(|_| std::fs::read("benches/words"))
        .unwrap();
    let dataset: Vec<&[u8]> = dataset.split(|c| *c == b'\n').collect();
    c.bench_function("encode_search_tree_ukkonen", |b| {
        b.iter(|| encode_search_tree_ukkonen(black_box(dataset.iter().map(|x| &x[..]))))
    });
    c.bench_function("encode_search_tree_naive", |b| {
        b.iter(|| encode_search_tree_naive(black_box(dataset.iter().map(|x| &x[..]))))
    });
}

pub fn tree_builder_naive_vs_multiple_mini(c: &mut Criterion) {
    let dataset = std::fs::read("words.mini")
        .or_else(|_| std::fs::read("benches/words.mini"))
        .unwrap();
    let dataset: Vec<&[u8]> = dataset.split(|c| *c == b'\n').collect();
    c.bench_function("encode_search_tree_ukkonen", |b| {
        b.iter(|| encode_search_tree_ukkonen(black_box(dataset.iter().map(|x| &x[..]))))
    });
    c.bench_function("encode_search_tree_naive", |b| {
        b.iter(|| encode_search_tree_naive(black_box(dataset.iter().map(|x| &x[..]))))
    });
}

criterion_group!(
    benches,
    tree_builder_naive_vs_multiple,
    tree_builder_naive_vs_multiple_mini
);
criterion_main!(benches);
