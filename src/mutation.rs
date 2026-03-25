use std::cmp::Ordering;

/// Mutation is a descriptor of changes to be applied to the document tree structure. For regular
/// scenarios, use `mutation!` macro for more convenient syntax.
#[derive(Debug, Clone, PartialEq)]
pub enum Mutation {
    /// Assign operation to a given path segment.
    Apply(Segment, Op),
    /// Compose other mutations in a nested structure.
    Compose(Vec<Mutation>),
}

impl Mutation {
    pub fn assign<S, O>(segment: S, operation: O) -> Self
    where
        S: Into<Segment>,
        O: Into<Op>,
    {
        Self::Apply(segment.into(), operation.into())
    }

    pub fn compose<I>(iter: I) -> Mutation
    where
        I: IntoIterator<Item = Mutation>,
    {
        Mutation::Compose(iter.into_iter().collect())
    }
}

/// Individual operation defined on a segment within the scope of its parent [Mutation].
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Assign a new value to it owner [Mutation] segment.
    Assign(ciborium::Value),
    /// Delete a segment.
    Delete,
}

impl<V: Into<ciborium::Value>> From<V> for Op {
    fn from(value: V) -> Self {
        Op::Assign(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Segment {
    Field(String),
    FractionalIndex(Vec<u8>),
}

impl Ord for Segment {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

impl PartialOrd for Segment {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl AsRef<[u8]> for Segment {
    fn as_ref(&self) -> &[u8] {
        match self {
            Segment::Field(field) => field.as_bytes(),
            Segment::FractionalIndex(index) => index.as_slice(),
        }
    }
}

impl From<String> for Segment {
    #[inline]
    fn from(s: String) -> Self {
        Segment::Field(s)
    }
}

impl<'a> From<&'a str> for Segment {
    fn from(value: &'a str) -> Self {
        Segment::Field(value.to_string())
    }
}

impl From<Vec<u8>> for Segment {
    #[inline]
    fn from(value: Vec<u8>) -> Self {
        Segment::FractionalIndex(value)
    }
}

impl<'a> From<&'a [u8]> for Segment {
    fn from(value: &'a [u8]) -> Self {
        Segment::FractionalIndex(value.to_vec())
    }
}

/// Macro used to generate [Mutation] for possibly nested series of document changes. It follows a
/// similar pattern to regular JSON object notation. However, it doesn't support arrays
/// (we use [FractionalIndex]es to work with indexed sequences instead).
///
/// - Regular `"key": value` means an assignment operation. This can be either a **CBOR**-compatible
///   value (any value implementing `Into<ciborium::Value>` will work) or a special directive
///   prefixed with `@` (i.e. `@delete`).
/// - Using brackets `{}` means stepping down in generated document tree path and applying many
///   operations under it.
///
/// # Example
///
/// ```rust
/// use cdoc::*;
///
/// let db = Db::open(DbOptions::new("./path/to/db"))?;
/// let mut tx = db.begin()?;
///
/// tx.apply(mutation!({
///   "users": {
///     "fd99bc9e-3258-492a-8d3c-335d713309eb": {
///       "name": "Alice",
///       "age": @delete,
///       "friends": {
///         FractionalIndex::between(tx.pid(), None, None): {
///           "name": "Bob"
///         }
///       }
///     }
///   }
/// }))?;
/// tx.commit()?;
/// ```
macro_rules! mutation {
    () => {};
}
