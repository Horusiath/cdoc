use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// A hybrid logical timestamp. It's based on UNIX milliseconds timestamp, but the last 16bits are
/// assigned from monotonically increasing sequencer.
#[repr(transparent)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    FromBytes,
    IntoBytes,
    Immutable,
    KnownLayout,
)]
pub struct Timestamp(crate::U64);

static COUNTER: AtomicU64 = AtomicU64::new(0);

impl Timestamp {
    // only clear up the lowest 16 bits
    const MASK: u64 = !0xff;

    #[inline]
    pub const fn new(unix_millis: u64) -> Self {
        Timestamp(crate::U64::new(unix_millis))
    }

    pub fn now() -> Self {
        loop {
            let latest = COUNTER.load(Ordering::SeqCst);
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap();
            let masked = (now.as_millis() as u64) & Self::MASK;
            let max = masked.max(latest) + 1;

            if COUNTER
                .compare_exchange(latest, max, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Timestamp(max.into());
            }
        }
    }

    pub fn sync(timestamp: Self) -> Self {
        let latest = COUNTER.fetch_max(timestamp.0.get(), Ordering::SeqCst);
        Timestamp(latest.into())
    }
}

impl From<SystemTime> for Timestamp {
    fn from(value: SystemTime) -> Self {
        let ts = value.duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let masked = (ts.as_millis() as u64) & Self::MASK;
        Timestamp(masked.into())
    }
}

impl From<Timestamp> for SystemTime {
    fn from(value: Timestamp) -> Self {
        SystemTime::UNIX_EPOCH + Duration::from_millis(value.0.get())
    }
}

impl From<chrono::DateTime<chrono::Utc>> for Timestamp {
    fn from(value: chrono::DateTime<chrono::Utc>) -> Self {
        let ts = (value.timestamp_millis() as u64) & Self::MASK;
        Timestamp(ts.into())
    }
}

impl From<Timestamp> for chrono::DateTime<chrono::Utc> {
    fn from(value: Timestamp) -> Self {
        chrono::DateTime::from_timestamp_millis(value.0.get() as i64).unwrap()
    }
}

impl Serialize for Timestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.0.get())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TimestampVisitor;
        impl<'de> Visitor<'de> for TimestampVisitor {
            type Value = Timestamp;

            fn expecting(&self, f: &mut Formatter) -> std::fmt::Result {
                f.write_str("HLC timestamp")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Timestamp, E> {
                let t = Timestamp(value.into());
                // synchronize the timestamp with our current knowledge
                Timestamp::sync(t);
                Ok(t)
            }
        }

        deserializer.deserialize_u64(TimestampVisitor)
    }
}

impl Display for Timestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let chrono: chrono::DateTime<chrono::Utc> = (*self).into();
        chrono.format("%+").fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use crate::hlc::Timestamp;
    use std::time::{Duration, SystemTime};

    #[test]
    fn timestamp_precision() {
        let now = SystemTime::now();
        let t1 = Timestamp::from(now);
        let now_masked = SystemTime::from(t1);

        let elapsed = now.duration_since(now_masked).unwrap();
        let threshold = Duration::from_millis(250);
        assert!(
            elapsed < threshold,
            "timestamp time window should remain under 250ms, but was {:?}",
            elapsed
        );
    }

    #[test]
    fn now_returns_strictly_increasing_timestamps_across_threads() {
        let thread_count = 8;
        let per_thread = 1000;

        let handles: Vec<_> = (0..thread_count)
            .map(|_| {
                std::thread::spawn(move || {
                    (0..per_thread)
                        .map(|_| Timestamp::now())
                        .collect::<Vec<_>>()
                })
            })
            .collect();

        let mut all: Vec<Timestamp> = Vec::with_capacity(thread_count * per_thread);
        for handle in handles {
            let timestamps = handle.join().expect("thread panicked");
            // each thread's sequence must be strictly monotonic on its own
            for window in timestamps.windows(2) {
                assert!(
                    window[0] < window[1],
                    "per-thread timestamps must be strictly increasing, got {:?} >= {:?}",
                    window[0],
                    window[1]
                );
            }
            all.extend(timestamps);
        }

        // globally, every timestamp must be unique
        all.sort();
        for window in all.windows(2) {
            assert!(
                window[0] < window[1],
                "global timestamps must be unique, got duplicate {:?}",
                window[0]
            );
        }
    }

    #[test]
    fn deserialized_future_timestamp_advances_counter() {
        let before = Timestamp::now();

        // extract the raw u64 via serde, then push it far into the future
        let mut buf = Vec::new();
        let future = Timestamp(before.0 + 100_000);
        ciborium::into_writer(&future, &mut buf).unwrap();

        // deserialize triggers Timestamp::sync internally
        let remote: Timestamp = ciborium::de::from_reader(&buf[..]).unwrap();

        let after = Timestamp::now();
        assert!(
            after > remote,
            "timestamp generated after sync must exceed the remote value, \
             got after={:?}, remote={:?}",
            after,
            remote
        );
    }
}
