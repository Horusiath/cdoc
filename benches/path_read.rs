use cdoc::PID;
use cdoc::path::lseq::{FractionalIndex, Segment};
use cdoc::path::read::PathReader;
use cdoc::path::write::PathWriter;
use criterion::{Criterion, criterion_group, criterion_main};

fn make_index(segments: &[(u32, u32)]) -> Vec<u8> {
    let mut buf = Vec::new();
    for &(pid_val, seq) in segments {
        let seg = Segment::new(PID::new(pid_val).unwrap(), seq);
        seg.write(&mut buf).unwrap();
    }
    buf
}

/// 1 field + LWW terminator
fn build_path_1_level() -> Vec<u8> {
    let mut w = PathWriter::new(Vec::new(), 0);
    w.push_field("content").unwrap();
    w.lww().unwrap()
}

/// 3 fields + 1 FractionalIndex + LWW terminator (4 path segments)
fn build_path_4_levels() -> Vec<u8> {
    let mut w = PathWriter::new(Vec::new(), 0);
    w.push_field("documents").unwrap();
    w.push_field("users").unwrap();
    let idx_bytes = make_index(&[(1, 5), (2, 3)]);
    let idx = FractionalIndex::new_unchecked(&idx_bytes);
    w.push_index(idx).unwrap();
    w.push_field("name").unwrap();
    w.lww().unwrap()
}

/// 17 fields + 3 FractionalIndex segments + LWW terminator (20 path segments)
fn build_path_20_levels() -> Vec<u8> {
    let idx1_bytes = make_index(&[(1, 10), (3, 7)]);
    let idx2_bytes = make_index(&[(2, 4), (5, 1), (1, 9)]);
    let idx3_bytes = make_index(&[(4, 2)]);

    let mut w = PathWriter::new(Vec::new(), 0);
    w.push_field("root").unwrap();
    w.push_field("organizations").unwrap();
    w.push_field("departments").unwrap();
    w.push_field("teams").unwrap();
    let idx1 = FractionalIndex::new_unchecked(&idx1_bytes);
    w.push_index(idx1).unwrap();
    w.push_field("members").unwrap();
    w.push_field("profile").unwrap();
    w.push_field("settings").unwrap();
    w.push_field("preferences").unwrap();
    w.push_field("notifications").unwrap();
    let idx2 = FractionalIndex::new_unchecked(&idx2_bytes);
    w.push_index(idx2).unwrap();
    w.push_field("channels").unwrap();
    w.push_field("config").unwrap();
    w.push_field("rules").unwrap();
    w.push_field("actions").unwrap();
    w.push_field("metadata").unwrap();
    let idx3 = FractionalIndex::new_unchecked(&idx3_bytes);
    w.push_index(idx3).unwrap();
    w.push_field("tags").unwrap();
    w.push_field("labels").unwrap();
    w.push_field("description").unwrap();
    w.push_field("value").unwrap();
    w.lww().unwrap()
}

fn read_path_1_level(c: &mut Criterion) {
    let buf = build_path_1_level();
    c.bench_function("path_read/1_level", |b| {
        b.iter(|| {
            let reader = PathReader::new(&buf);
            for seg in reader {
                let _ = seg.unwrap();
            }
        });
    });
}

fn read_path_4_levels(c: &mut Criterion) {
    let buf = build_path_4_levels();
    c.bench_function("path_read/4_levels", |b| {
        b.iter(|| {
            let reader = PathReader::new(&buf);
            for seg in reader {
                let _ = seg.unwrap();
            }
        });
    });
}

fn read_path_20_levels(c: &mut Criterion) {
    let buf = build_path_20_levels();
    c.bench_function("path_read/20_levels", |b| {
        b.iter(|| {
            let reader = PathReader::new(&buf);
            for seg in reader {
                let _ = seg.unwrap();
            }
        });
    });
}

criterion_group!(benches, read_path_1_level, read_path_4_levels, read_path_20_levels);
criterion_main!(benches);
