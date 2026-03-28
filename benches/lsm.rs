use cdoc::hlc::Timestamp;
use cdoc::sst::memtable::MemTable;
use cdoc::sst::read::SSTableReader;
use cdoc::sst::write::SSTableWriter;
use cdoc::wal::WriteAheadLog;
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::fs::File;

const NUM_ENTRIES: u64 = 1000;
const SCAN_GROUPS: u32 = 10;
const ENTRIES_PER_GROUP: u32 = 100;

fn make_key(i: u64) -> [u8; 64] {
    let mut key = [0u8; 64];
    key[..8].copy_from_slice(&i.to_be_bytes());
    key
}

fn make_value(i: u64) -> [u8; 64] {
    let mut val = [0u8; 64];
    val[..8].copy_from_slice(&i.to_le_bytes());
    val
}

/// Keys for scan benchmarks: group (4B BE) + entry (4B BE) + padding.
fn make_scan_key(group: u32, entry: u32) -> [u8; 64] {
    let mut key = [0u8; 64];
    key[..4].copy_from_slice(&group.to_be_bytes());
    key[4..8].copy_from_slice(&entry.to_be_bytes());
    key
}

/// Deterministic pseudo-random shuffle using LCG.
fn shuffled_indices(count: u64) -> Vec<u64> {
    let mut indices: Vec<u64> = (0..count).collect();
    let mut rng = 42u64;
    for i in (1..indices.len()).rev() {
        rng = rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = (rng >> 33) as usize % (i + 1);
        indices.swap(i, j);
    }
    indices
}

fn filled_memtable(n: u64) -> MemTable {
    let mut mt = MemTable::new();
    for i in 0..n {
        mt.insert(make_key(i).to_vec(), make_value(i).to_vec());
    }
    mt
}

fn create_sstable(dir: &tempfile::TempDir, n: u64) -> SSTableReader {
    let path = dir.path().join("bench.sst");
    let ts = Timestamp::now();
    let file = File::create(&path).unwrap();
    let mut w = SSTableWriter::new(file, ts, ts);
    for i in 0..n {
        w.add(&make_key(i), &make_value(i)).unwrap();
    }
    w.finish().unwrap();
    SSTableReader::open(&path).unwrap()
}

fn create_scan_sstable(dir: &tempfile::TempDir) -> SSTableReader {
    let path = dir.path().join("scan.sst");
    let ts = Timestamp::now();
    let file = File::create(&path).unwrap();
    let mut w = SSTableWriter::new(file, ts, ts);
    for group in 0..SCAN_GROUPS {
        for entry in 0..ENTRIES_PER_GROUP {
            w.add(&make_scan_key(group, entry), &make_value(entry as u64))
                .unwrap();
        }
    }
    w.finish().unwrap();
    SSTableReader::open(&path).unwrap()
}

// ── 1. Memtable writes ──

fn memtable_write(c: &mut Criterion) {
    c.bench_function("memtable/write", |b| {
        b.iter_batched(
            MemTable::new,
            |mut mt| {
                for i in 0..NUM_ENTRIES {
                    mt.insert(make_key(i).to_vec(), make_value(i).to_vec());
                }
            },
            BatchSize::SmallInput,
        );
    });
}

// ── 2. Memtable reads (single, random access) ──

fn memtable_read_random(c: &mut Criterion) {
    let mt = filled_memtable(NUM_ENTRIES);
    let indices = shuffled_indices(NUM_ENTRIES);

    c.bench_function("memtable/read_random", |b| {
        let mut pos = 0usize;
        b.iter(|| {
            let key = make_key(indices[pos]);
            pos = (pos + 1) % indices.len();
            mt.get(&key)
        });
    });
}

// ── 3. Memtable reads (repeated hits, random access) ──

fn memtable_read_repeated(c: &mut Criterion) {
    let mt = filled_memtable(NUM_ENTRIES);
    let hot_keys: Vec<[u8; 64]> = (0..10).map(|i| make_key(i * 100)).collect();

    c.bench_function("memtable/read_repeated", |b| {
        let mut pos = 0usize;
        b.iter(|| {
            let result = mt.get(&hot_keys[pos]);
            pos = (pos + 1) % hot_keys.len();
            result
        });
    });
}

// ── 4. Memtable reads (sequential range scan) ──

fn memtable_scan(c: &mut Criterion) {
    let mut mt = MemTable::new();
    for group in 0..SCAN_GROUPS {
        for entry in 0..ENTRIES_PER_GROUP {
            mt.insert(
                make_scan_key(group, entry).to_vec(),
                make_value(entry as u64).to_vec(),
            );
        }
    }

    c.bench_function("memtable/scan", |b| {
        let prefix = 5u32.to_be_bytes();
        b.iter(|| mt.scan(&prefix).count());
    });
}

// ── 5. Memtable flushes to SSTable ──

fn memtable_flush(c: &mut Criterion) {
    let mut group = c.benchmark_group("memtable/flush");
    for &count in &[1u64, 100, 1000] {
        group.throughput(Throughput::Elements(count));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter_batched(
                || {
                    let mt = filled_memtable(n);
                    let dir = tempfile::tempdir().unwrap();
                    (mt, dir)
                },
                |(mt, dir)| {
                    let path = dir.path().join("bench.sst");
                    let ts = Timestamp::now();
                    let file = File::create(&path).unwrap();
                    let mut w = SSTableWriter::new(file, ts, ts);
                    for (key, value) in mt.iter() {
                        w.add(key, value).unwrap();
                    }
                    let file = w.finish().unwrap();
                    file.sync_all().unwrap();
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

// ── 6. WAL writes ──

fn wal_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal/write");
    for &count in &[1u64, 100, 1000] {
        group.throughput(Throughput::Elements(count));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter_batched(
                || {
                    let dir = tempfile::tempdir().unwrap();
                    let path = dir.path().join("bench.wal");
                    let wal = WriteAheadLog::create(&path).unwrap();
                    (wal, dir)
                },
                |(mut wal, _dir)| {
                    let ts = Timestamp::now();
                    for i in 0..n {
                        wal.write_record(&make_key(i), &make_value(i), None)
                            .unwrap();
                    }
                    wal.write_commit(ts).unwrap();
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

// ── 6b. WAL replays ──

fn wal_replay(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal/replay");
    for &count in &[1u64, 100, 1000] {
        group.throughput(Throughput::Elements(count));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter_batched(
                || {
                    let dir = tempfile::tempdir().unwrap();
                    let path = dir.path().join("bench.wal");
                    let ts = Timestamp::now();
                    let mut wal = WriteAheadLog::create(&path).unwrap();
                    for i in 0..n {
                        wal.write_record(&make_key(i), &make_value(i), None)
                            .unwrap();
                    }
                    wal.write_commit(ts).unwrap();
                    drop(wal);
                    (dir, path)
                },
                |(_dir, path)| {
                    WriteAheadLog::replay(&path).unwrap();
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

// ── 7. SSTable reads (single, random access) ──

fn sstable_read_random(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let reader = create_sstable(&dir, NUM_ENTRIES);
    let indices = shuffled_indices(NUM_ENTRIES);

    c.bench_function("sstable/read_random", |b| {
        let mut pos = 0usize;
        b.iter(|| {
            let key = make_key(indices[pos]);
            pos = (pos + 1) % indices.len();
            reader.get(&key)
        });
    });
}

// ── 8. SSTable reads (repeated key hits, random access) ──

fn sstable_read_repeated(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let reader = create_sstable(&dir, NUM_ENTRIES);
    let hot_keys: Vec<[u8; 64]> = (0..10).map(|i| make_key(i * 100)).collect();

    c.bench_function("sstable/read_repeated", |b| {
        let mut pos = 0usize;
        b.iter(|| {
            let result = reader.get(&hot_keys[pos]);
            pos = (pos + 1) % hot_keys.len();
            result
        });
    });
}

// ── 9. SSTable reads (sequential range scan) ──

fn sstable_scan(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let reader = create_scan_sstable(&dir);

    c.bench_function("sstable/scan", |b| {
        let prefix = 5u32.to_be_bytes();
        b.iter(|| reader.scan_prefix(&prefix));
    });
}

// ── 10. SSTable reads: missing key ──

fn sstable_read_missing(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let reader = create_sstable(&dir, NUM_ENTRIES);
    let missing_keys: Vec<[u8; 64]> = (10_000..10_100).map(make_key).collect();

    c.bench_function("sstable/read_missing", |b| {
        let mut pos = 0usize;
        b.iter(|| {
            let result = reader.get(&missing_keys[pos]);
            pos = (pos + 1) % missing_keys.len();
            result
        });
    });
}

criterion_group!(
    benches,
    memtable_write,
    memtable_read_random,
    memtable_read_repeated,
    memtable_scan,
    memtable_flush,
    wal_write,
    wal_replay,
    sstable_read_random,
    sstable_read_repeated,
    sstable_scan,
    sstable_read_missing,
);
criterion_main!(benches);
