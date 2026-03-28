use std::collections::BTreeMap;

/// In-memory buffer for recent writes. Backed by a sorted `BTreeMap` so that
/// flush to SSTable can iterate in key order and prefix scans are efficient.
#[derive(Clone)]
pub struct MemTable {
    entries: BTreeMap<Vec<u8>, Vec<u8>>,
    /// Approximate byte size of all keys + values stored.
    size: usize,
}

impl MemTable {
    pub fn new() -> Self {
        MemTable {
            entries: BTreeMap::new(),
            size: 0,
        }
    }

    /// Inserts a key-value pair. Returns the previous value if the key already existed.
    pub fn insert<K: Into<Vec<u8>>>(&mut self, key: K, value: Vec<u8>) -> Option<Vec<u8>> {
        let key = key.into();
        let added = key.len() + value.len();
        let old = self.entries.insert(key.to_vec(), value);
        if let Some(ref old_val) = old {
            // subtract old entry size (key was already counted)
            self.size -= old_val.len();
            self.size += added - key.len();
        } else {
            self.size += added;
        }
        old
    }

    /// Looks up a single key.
    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        self.entries.get(key).map(|v| v.as_slice())
    }

    /// Returns an iterator over all entries whose key starts with `prefix`,
    /// in sorted order.
    pub fn scan<'a>(&'a self, prefix: &'a [u8]) -> impl Iterator<Item = (&'a [u8], &'a [u8])> {
        use std::ops::Bound;
        // BTreeMap range with the prefix as inclusive lower bound.
        // Upper bound is the successor of the prefix (increment last byte).
        let start = Bound::Included(prefix.to_vec());
        let end = prefix_successor(prefix)
            .map(Bound::Excluded)
            .unwrap_or(Bound::Unbounded);

        self.entries
            .range((start, end))
            .map(|(k, v)| (k.as_slice(), v.as_slice()))
    }

    /// Iterates over all entries in sorted key order.
    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        self.entries
            .iter()
            .map(|(k, v)| (k.as_slice(), v.as_slice()))
    }

    /// Approximate heap size of stored data (keys + values).
    pub fn estimated_size(&self) -> usize {
        self.size
    }
}

/// Computes the lexicographic successor of `prefix` by incrementing the last
/// non-0xFF byte and truncating. Returns `None` when the prefix is all 0xFF
/// (meaning every key is a match).
fn prefix_successor(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut succ = prefix.to_vec();
    while let Some(&last) = succ.last() {
        if last < 0xFF {
            *succ.last_mut().unwrap() = last + 1;
            return Some(succ);
        }
        succ.pop();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut mt = MemTable::new();
        assert!(mt.get(b"a").is_none());
        mt.insert(b"a", b"v1".to_vec());
        assert_eq!(mt.get(b"a"), Some(b"v1".as_slice()));
    }

    #[test]
    fn insert_overwrites_previous_value() {
        let mut mt = MemTable::new();
        mt.insert(b"a", b"v1".to_vec());
        let old = mt.insert(b"a", b"v2".to_vec());
        assert_eq!(old.as_deref(), Some(b"v1".as_slice()));
        assert_eq!(mt.get(b"a"), Some(b"v2".as_slice()));
    }

    #[test]
    fn scan_prefix() {
        let mut mt = MemTable::new();
        mt.insert(b"abc/1", b"v1".to_vec());
        mt.insert(b"abc/2", b"v2".to_vec());
        mt.insert(b"abd/1", b"v3".to_vec());
        mt.insert(b"xyz", b"v4".to_vec());

        let results: Vec<_> = mt.scan(b"abc/").collect();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], (b"abc/1".as_slice(), b"v1".as_slice()));
        assert_eq!(results[1], (b"abc/2".as_slice(), b"v2".as_slice()));
    }

    #[test]
    fn iter_sorted() {
        let mut mt = MemTable::new();
        mt.insert(b"c", b"3".to_vec());
        mt.insert(b"a", b"1".to_vec());
        mt.insert(b"b", b"2".to_vec());

        let keys: Vec<&[u8]> = mt.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec![b"a".as_slice(), b"b", b"c"]);
    }

    #[test]
    fn estimated_size_tracks_bytes() {
        let mut mt = MemTable::new();
        assert_eq!(mt.estimated_size(), 0);
        mt.insert(b"key", b"val".to_vec()); // 3 + 3 = 6
        assert_eq!(mt.estimated_size(), 6);
        mt.insert(b"key", b"longer".to_vec()); // replace: size = 6 - 3 + 6 = 9
        assert_eq!(mt.estimated_size(), 9);
    }
}
