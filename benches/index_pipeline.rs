use std::{env, path::PathBuf};

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use dirgo::{config::Config, fixture, index};

fn index_pipeline(c: &mut Criterion) {
    let directories = env::var("DIRGO_BENCH_DIRECTORIES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(10_000);
    let fixture_root = env::var_os("DIRGO_BENCH_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let root = std::env::temp_dir().join(format!(
                "dirgo-criterion-{}-{directories}",
                std::process::id()
            ));
            fixture::create(&root, directories, 32).expect("create benchmark fixture");
            root
        });
    let config = Config {
        roots: vec![fixture_root],
        ..Config::default()
    };

    c.bench_function(
        &format!("index/crawl/{directories}_directories"),
        |bencher| {
            bencher.iter(|| index::crawl_local(black_box(&config.roots[0]), black_box(&config)))
        },
    );
}

criterion_group!(benches, index_pipeline);
criterion_main!(benches);
