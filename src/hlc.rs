use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

/// A hybrid logical timestamp. It's based on UNIX milliseconds timestamp, but the last 16bits are
/// assigned from monotonically increasing sequencer.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp(u64);

static COUNTER: AtomicU64 = AtomicU64::new(0);

impl Timestamp {
    // only clear up the lowest 16 bits
    const MASK: u64 = !0xff;

    pub fn now() -> Self {
        loop {
            let latest = COUNTER.load(Ordering::SeqCst);
            let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
            let masked = (now.as_millis() as u64) & Self::MASK;
            let max = masked.max(latest) + 1;

            if COUNTER.compare_exchange(latest, max, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                return Timestamp(max);
            }
        }
    }

    pub fn sync(timestamp: Self) -> Self {
        let latest = COUNTER.fetch_max(timestamp.0, Ordering::SeqCst);
        Timestamp(latest)
    }
}

impl Display for Timestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        todo!("write human-readable ISO 8601 formatted timestamp string")
    }
}