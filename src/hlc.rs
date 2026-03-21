use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// A hybrid logical timestamp. It's based on UNIX milliseconds timestamp, but the last 16bits are
/// assigned from monotonically increasing sequencer.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(u64);

static COUNTER: AtomicU64 = AtomicU64::new(0);

impl Timestamp {
    // only clear up the lowest 16 bits
    const MASK: u64 = !0xff;

    #[inline]
    pub const fn new(unix_millis: u64) -> Self {
        Timestamp(unix_millis)
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
                return Timestamp(max);
            }
        }
    }

    pub fn sync(timestamp: Self) -> Self {
        let latest = COUNTER.fetch_max(timestamp.0, Ordering::SeqCst);
        Timestamp(latest)
    }
}

impl From<SystemTime> for Timestamp {
    fn from(value: SystemTime) -> Self {
        let ts = value.duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let masked = (ts.as_millis() as u64) & Self::MASK;
        Timestamp(masked)
    }
}

impl From<Timestamp> for SystemTime {
    fn from(value: Timestamp) -> Self {
        SystemTime::UNIX_EPOCH + Duration::from_millis(value.0)
    }
}

impl From<chrono::DateTime<chrono::Utc>> for Timestamp {
    fn from(value: chrono::DateTime<chrono::Utc>) -> Self {
        let ts = (value.timestamp_millis() as u64) & Self::MASK;
        Timestamp(ts)
    }
}

impl From<Timestamp> for chrono::DateTime<chrono::Utc> {
    fn from(value: Timestamp) -> Self {
        chrono::DateTime::from_timestamp_millis(value.0 as i64).unwrap()
    }
}

impl Serialize for Timestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.0)
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
                let t = Timestamp(value);
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
}
