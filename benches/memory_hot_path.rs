//! Microbenchmarks for the hot paths. Run with `cargo bench` (not `cargo test`).

use ai_memory::{MemoryPolicy, MemoryStore, RecallQuery, RememberRequest, SqliteStore};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

fn fresh_store() -> SqliteStore {
    let store = SqliteStore::open_in_memory().unwrap();
    store.create_project("p", MemoryPolicy::default()).unwrap();
    store
}

fn bench_remember_single(c: &mut Criterion) {
    c.bench_function("remember_single", |b| {
        b.iter_batched(
            fresh_store,
            |store| {
                store
                    .remember("p", RememberRequest::new("a short working note"))
                    .unwrap();
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_remember_batch_vs_loop(c: &mut Criterion) {
    let mut g = c.benchmark_group("remember_50");
    g.bench_function("loop_single", |b| {
        b.iter_batched(
            fresh_store,
            |store| {
                for i in 0..50 {
                    store
                        .remember("p", RememberRequest::new(format!("note number {i} filler")))
                        .unwrap();
                }
            },
            BatchSize::SmallInput,
        );
    });
    g.bench_function("remember_many", |b| {
        b.iter_batched(
            fresh_store,
            |store| {
                let reqs: Vec<_> = (0..50)
                    .map(|i| RememberRequest::new(format!("note number {i} filler")))
                    .collect();
                store.remember_many("p", reqs).unwrap();
            },
            BatchSize::SmallInput,
        );
    });
    g.finish();
}

fn bench_recall_populated(c: &mut Criterion) {
    c.bench_function("recall_200", |b| {
        b.iter_batched(
            || {
                let store = fresh_store();
                let reqs: Vec<_> = (0..200)
                    .map(|i| RememberRequest::new(format!("row {i} lorem ipsum dolor theme")))
                    .collect();
                store.remember_many("p", reqs).unwrap();
                store
            },
            |store| {
                store
                    .recall("p", RecallQuery::new("theme preference").with_limit(8))
                    .unwrap();
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    bench_remember_single,
    bench_remember_batch_vs_loop,
    bench_recall_populated
);
criterion_main!(benches);
