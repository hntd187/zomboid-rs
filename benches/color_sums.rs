use criterion::*;
use std::path::PathBuf;
use tokio::runtime::{Builder, Runtime};
use zomboid_map::cell::LotPackReader;
use zomboid_map::header::LotHeaderReader;

fn criterion_benchmark(c: &mut Criterion) {
    let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\world_21_48.lotpack");
    let header_path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\21_48.lotheader");
    let mut reader = LotHeaderReader::new();
    let pack_reader = LotPackReader::new();
    let rt = Runtime::new().unwrap();
    let header = rt.block_on(reader.load_lotheader(&header_path)).unwrap();

    let mut group = c.benchmark_group("Access");
    // group.significance_level(0.1).sample_size(500).measurement_time(Duration::from_secs(180));
    group.bench_with_input(BenchmarkId::new("get_block", "inline"), &(&pack_reader, &path, &header), |b, i| {
        let h2 = i.2.clone();

        b.to_async(Builder::new_current_thread().build().unwrap()).iter(|| async {
            let mut r = i.0.clone();
            r.load_lotpack(i.1, h2.clone()).await
        })
    });
    group.finish();

    let mut group2 = c.benchmark_group("Lot Header");
    // group2.significance_level(0.1).sample_size(500).measurement_time(Duration::from_secs(180));
    group2.bench_with_input(BenchmarkId::new("lotheader", "new"), &(&reader, &header_path), |b, i| {
        b.to_async(Runtime::new().unwrap()).iter(|| async {
            let mut r = i.0.clone();
            r.load_lotheader(&header_path).await
        })
    });
    group2.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
