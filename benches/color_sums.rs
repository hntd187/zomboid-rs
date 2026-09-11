use criterion::*;
use std::path::PathBuf;
use std::time::Duration;
use tokio::runtime::{Builder, Runtime};
use zomboid_map::cell::{alt_load_lotpack, load_lotpack};
use zomboid_map::header::load_lotheader;

fn criterion_benchmark(c: &mut Criterion) {
    let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\world_21_48.lotpack");
    let header = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\21_48.lotheader");

    let rt = Runtime::new().unwrap();
    let header = rt.block_on(load_lotheader(&header)).unwrap();

    let mut group = c.benchmark_group("Access");
    group
        .significance_level(0.1)
        .sample_size(500)
        .measurement_time(Duration::from_secs(180));
    group.bench_with_input(
        BenchmarkId::new("get_block", "safe"),
        &(&path, &header),
        |b, i| {
            let h1 = i.1.clone();
            b.to_async(Builder::new_current_thread().build().unwrap())
                .iter(|| load_lotpack(i.0, h1.clone()));
        },
    );
    group.bench_with_input(
        BenchmarkId::new("get_block", "unsafe"),
        &(&path, &header),
        |b, i| {
            let h2 = i.1.clone();
            b.to_async(Builder::new_current_thread().build().unwrap())
                .iter(|| alt_load_lotpack(i.0, h2.clone()))
        },
    );
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
