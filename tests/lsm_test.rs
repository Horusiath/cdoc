use cdoc::{Db, DbOptions};

/// Opens a fresh database in a temporary directory.
fn open_temp_db() -> (Db, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let opts = DbOptions::new(dir.path());
    let db = Db::open(opts).expect("failed to open db");
    (db, dir)
}

/// Opens a database with a small MemTable to force flushes.
fn open_temp_db_small_memtable() -> (Db, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let opts = DbOptions::new(dir.path()).with_memtable_size(256);
    let db = Db::open(opts).expect("failed to open db");
    (db, dir)
}

#[test]
fn basic_insert_and_get() {
    let (db, _dir) = open_temp_db();

    {
        let mut tx = db.begin();
        tx.insert(b"hello", b"world").unwrap();
        tx.commit().unwrap();
    }

    let ro = db.begin_readonly();
    assert_eq!(ro.get(b"hello").as_deref(), Some(b"world".as_slice()));
    assert!(ro.get(b"missing").is_none());
}

#[test]
fn prefix_scan() {
    let (db, _dir) = open_temp_db();

    {
        let mut tx = db.begin();
        tx.insert(b"users/alice", b"a").unwrap();
        tx.insert(b"users/bob", b"b").unwrap();
        tx.insert(b"posts/1", b"p").unwrap();
        tx.commit().unwrap();
    }

    let ro = db.begin_readonly();
    let results = ro.scan(b"users/");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, b"users/alice");
    assert_eq!(results[1].0, b"users/bob");
}

#[test]
fn transaction_isolation_readonly_snapshot() {
    let (db, _dir) = open_temp_db();

    // Write initial data.
    {
        let mut tx = db.begin();
        tx.insert(b"key", b"v1").unwrap();
        tx.commit().unwrap();
    }

    // Take a snapshot.
    let ro = db.begin_readonly();

    // Overwrite the key in a new transaction.
    {
        let mut tx = db.begin();
        tx.insert(b"key", b"v2").unwrap();
        tx.commit().unwrap();
    }

    // The read-only snapshot should still see the old value.
    assert_eq!(ro.get(b"key").as_deref(), Some(b"v1".as_slice()));

    // A fresh read sees the new value.
    let ro2 = db.begin_readonly();
    assert_eq!(ro2.get(b"key").as_deref(), Some(b"v2".as_slice()));
}

#[test]
fn uncommitted_writes_invisible_to_readers() {
    let (db, _dir) = open_temp_db();

    let mut tx = db.begin();
    tx.insert(b"key", b"val").unwrap();

    // Writer can see its own writes.
    assert_eq!(tx.get(b"key").as_deref(), Some(b"val".as_slice()));

    // Reader should not see uncommitted writes.
    let ro = db.begin_readonly();
    assert!(ro.get(b"key").is_none());

    tx.abort().unwrap();
}

#[test]
fn transaction_abort_discards_changes() {
    let (db, _dir) = open_temp_db();

    {
        let mut tx = db.begin();
        tx.insert(b"key", b"val").unwrap();
        tx.abort().unwrap();
    }

    let ro = db.begin_readonly();
    assert!(ro.get(b"key").is_none());
}

#[test]
fn memtable_flush_to_sstable() {
    let (db, _dir) = open_temp_db_small_memtable();

    // Insert enough data to trigger a flush (memtable_size = 256 bytes).
    {
        let mut tx = db.begin();
        for i in 0u32..50 {
            let key = format!("key_{:04}", i);
            let val = format!("value_{:04}", i);
            tx.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }
        tx.commit().unwrap();
    }

    // Data should be readable from the SSTable.
    let ro = db.begin_readonly();
    assert_eq!(
        ro.get(b"key_0025").as_deref(),
        Some(b"value_0025".as_slice())
    );
    assert!(ro.get(b"nonexistent").is_none());
}

#[test]
fn wal_replay_after_reopen() {
    let dir = tempfile::tempdir().unwrap();

    // Open, write, commit, close.
    {
        let opts = DbOptions::new(dir.path());
        let db = Db::open(opts).unwrap();
        let mut tx = db.begin();
        tx.insert(b"persist", b"this").unwrap();
        tx.commit().unwrap();
    }

    // Re-open the database — WAL should be replayed.
    {
        let opts = DbOptions::new(dir.path());
        let db = Db::open(opts).unwrap();
        let ro = db.begin_readonly();
        assert_eq!(ro.get(b"persist").as_deref(), Some(b"this".as_slice()));
    }
}

#[test]
fn multiple_sequential_transactions() {
    let (db, _dir) = open_temp_db();

    for i in 0u32..10 {
        let mut tx = db.begin();
        let key = format!("key_{}", i);
        let val = format!("val_{}", i);
        tx.insert(key.as_bytes(), val.as_bytes()).unwrap();
        tx.commit().unwrap();
    }

    let ro = db.begin_readonly();
    for i in 0u32..10 {
        let key = format!("key_{}", i);
        let val = format!("val_{}", i);
        assert_eq!(
            ro.get(key.as_bytes()).as_deref(),
            Some(val.as_bytes()),
            "missing key: {}",
            key
        );
    }
}

#[test]
fn overwrite_key_across_transactions() {
    let (db, _dir) = open_temp_db();

    {
        let mut tx = db.begin();
        tx.insert(b"key", b"first").unwrap();
        tx.commit().unwrap();
    }
    {
        let mut tx = db.begin();
        tx.insert(b"key", b"second").unwrap();
        tx.commit().unwrap();
    }

    let ro = db.begin_readonly();
    assert_eq!(ro.get(b"key").as_deref(), Some(b"second".as_slice()));
}

#[test]
fn scan_merges_memtable_and_sstable() {
    let (db, _dir) = open_temp_db_small_memtable();

    // First batch — will be flushed to SSTable.
    {
        let mut tx = db.begin();
        for i in 0u32..50 {
            let key = format!("item/{:04}", i);
            let val = format!("v{}", i);
            tx.insert(key.as_bytes(), val.as_bytes()).unwrap();
        }
        tx.commit().unwrap();
    }

    // Second batch — stays in MemTable.
    {
        let mut tx = db.begin();
        tx.insert(b"item/0099", b"v99").unwrap();
        tx.commit().unwrap();
    }

    let ro = db.begin_readonly();
    let results = ro.scan(b"item/");
    // Should include both SSTable entries and MemTable entries.
    assert!(results.len() >= 50);
    // The MemTable entry should be present.
    assert!(results.iter().any(|(k, _)| k == b"item/0099"));
}
