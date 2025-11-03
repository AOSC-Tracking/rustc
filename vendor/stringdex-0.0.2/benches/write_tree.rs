use std::cell::RefCell;
use std::collections::HashMap;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use stringdex::internals::tree::encode_search_tree_ukkonen;
use stringdex::internals::write_tree;

pub fn write_tree_words_mini(c: &mut Criterion) {
    let dataset = std::fs::read("words.mini")
        .or_else(|_| std::fs::read("benches/words.mini"))
        .unwrap();
    let dataset: Vec<&[u8]> = dataset.split(|c| *c == b'\n').collect();
    let tree = encode_search_tree_ukkonen(dataset.iter().map(|x| &x[..]));
    c.bench_function("write_tree_mini", |b| {
        b.iter(|| {
            let hm = RefCell::new(HashMap::new());
            write_tree(
                black_box(&tree),
                &mut |filename, bytes| {
                    hm.borrow_mut()
                        .insert(filename.to_owned(), bytes.to_owned());
                    black_box({
                        (filename, bytes);
                        Ok(())
                    })
                },
                &mut |filename| Ok(hm.borrow().get(filename).cloned()),
            )
        })
    });
}

pub fn write_tree_words(c: &mut Criterion) {
    let dataset = std::fs::read("words")
        .or_else(|_| std::fs::read("benches/words"))
        .unwrap();
    let dataset: Vec<&[u8]> = dataset.split(|c| *c == b'\n').collect();
    let tree = encode_search_tree_ukkonen(dataset.iter().map(|x| &x[..]));
    c.bench_function("write_tree_mini", |b| {
        b.iter(|| {
            let hm = RefCell::new(HashMap::new());
            write_tree(
                black_box(&tree),
                &mut |filename, bytes| {
                    hm.borrow_mut()
                        .insert(filename.to_owned(), bytes.to_owned());
                    black_box({
                        (filename, bytes);
                        Ok(())
                    })
                },
                &mut |filename| Ok(hm.borrow().get(filename).cloned()),
            )
        })
    });
}

criterion_group!(benches, write_tree_words_mini, write_tree_words);
criterion_main!(benches);
