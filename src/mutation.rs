use crate::FractionalIndex;
use crate::path::PathError;
use crate::path::write::PathWriter;
use std::cmp::Ordering;
use std::io::Write;

/// Mutation is a descriptor of changes to be applied to the document tree structure. For regular
/// scenarios, use `mutation!` macro for more convenient syntax.
#[derive(Debug, Clone, PartialEq)]
pub enum Mutation {
    /// Assign operation to a given path segment.
    Apply(Segment, Op),
    /// Step into a segment and apply a series of mutations within it.
    Nested(Segment, Vec<Mutation>),
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

    /// Creates a nested mutation that steps into a segment and applies inner mutations.
    pub fn nested<S, I>(segment: S, iter: I) -> Self
    where
        S: Into<Segment>,
        I: IntoIterator<Item = Mutation>,
    {
        Self::Nested(segment.into(), iter.into_iter().collect())
    }

    pub fn compose<I>(iter: I) -> Mutation
    where
        I: IntoIterator<Item = Mutation>,
    {
        Mutation::Compose(iter.into_iter().collect())
    }

    pub fn for_each<F>(&self, mut f: F) -> crate::Result<()>
    where
        F: FnMut(Vec<u8>, Vec<u8>),
    {
        let mut w = PathWriter::new(Vec::new(), 0);
        self.for_each_internal(&mut f, &mut w)
    }

    fn for_each_internal<F>(&self, f: &mut F, w: &mut PathWriter<Vec<u8>>) -> crate::Result<()>
    where
        F: FnMut(Vec<u8>, Vec<u8>),
    {
        match self {
            Mutation::Apply(segment, Op::Delete) => {
                segment.write(w)?;
                f(w.clone().lww()?, Vec::new());
            }
            Mutation::Apply(segment, Op::Assign(value)) => {
                segment.write(w)?;
                let mut buf = Vec::new();
                crate::cbor::into_writer(value, &mut buf)?;
                f(w.clone().lww()?, buf);
            }
            Mutation::Nested(segment, mutations) => {
                segment.write(w)?;
                let trunc = w.inner().len();
                for mutation in mutations {
                    mutation.for_each_internal(f, w)?;
                    w.inner_mut().truncate(trunc);
                }
            }
            Mutation::Compose(mutations) => {
                let trunc = w.inner().len();
                for mutation in mutations {
                    mutation.for_each_internal(f, w)?;
                    w.inner_mut().truncate(trunc);
                }
            }
        }
        Ok(())
    }
}

/// Individual operation defined on a segment within the scope of its parent [Mutation].
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Assign a new value to it owner [Mutation] segment.
    Assign(crate::cbor::Value),
    /// Delete a segment.
    Delete,
}

impl<V: Into<crate::cbor::Value>> From<V> for Op {
    fn from(value: V) -> Self {
        Op::Assign(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Segment {
    Field(String),
    FractionalIndex(Vec<u8>),
}

impl Segment {
    fn write<W: Write>(&self, w: &mut PathWriter<W>) -> crate::Result<()> {
        match self {
            Segment::Field(field) => w.push_field(field),
            Segment::FractionalIndex(index) => {
                let findex = FractionalIndex::new(index)
                    .ok_or_else(|| crate::Error::Path(PathError::InvalidIndex))?;
                w.push_index(findex)
            }
        }
    }
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
///   value (any value implementing `Into<crate::cbor::Value>` will work) or a special directive
///   prefixed with `@` (i.e. `@delete`).
/// - Using brackets `{}` means stepping down in generated document tree path and applying many
///   operations under it.
///
/// # Example
///
/// ```rust,ignore
/// use cdoc::*;
/// use crate::cbor::cbor;
///
/// let db = Db::open(DbOptions::new("./path/to/db"))?;
/// let mut tx = db.begin()?;
///
/// tx.execute(mutation!({
///   "users": {
///     "fd99bc9e-3258-492a-8d3c-335d713309eb": {
///       "name": "Alice",
///       "age": @delete,
///       "friends": {
///         FractionalIndex::between(tx.pid(), None, None): cbor!({
///           "name" => "Bob"
///         })
///       }
///     }
///   }
/// }))?;
/// tx.commit()?;
/// ```
#[macro_export]
macro_rules! mutation {
    // Entry point: wraps entries in a Compose.
    ({ $($body:tt)* }) => {
        $crate::Mutation::Compose(
            $crate::mutation!(@__entries [] $($body)*)
        )
    };

    // --- Internal rules for parsing entries ---

    // Base case: all entries consumed.
    (@__entries [$($acc:expr),*]) => {
        ::std::vec![$($acc),*]
    };

    // Nested block followed by more entries.
    (@__entries [$($acc:expr),*] $key:tt : { $($inner:tt)* } , $($rest:tt)*) => {
        $crate::mutation!(@__entries [
            $($acc,)*
            $crate::Mutation::Nested(
                $crate::Segment::from($key),
                $crate::mutation!(@__entries [] $($inner)*)
            )
        ] $($rest)*)
    };

    // Nested block as last entry.
    (@__entries [$($acc:expr),*] $key:tt : { $($inner:tt)* }) => {
        $crate::mutation!(@__entries [
            $($acc,)*
            $crate::Mutation::Nested(
                $crate::Segment::from($key),
                $crate::mutation!(@__entries [] $($inner)*)
            )
        ])
    };

    // @delete followed by more entries.
    (@__entries [$($acc:expr),*] $key:tt : @delete , $($rest:tt)*) => {
        $crate::mutation!(@__entries [
            $($acc,)*
            $crate::Mutation::Apply(
                $crate::Segment::from($key),
                $crate::Op::Delete
            )
        ] $($rest)*)
    };

    // @delete as last entry.
    (@__entries [$($acc:expr),*] $key:tt : @delete) => {
        $crate::mutation!(@__entries [
            $($acc,)*
            $crate::Mutation::Apply(
                $crate::Segment::from($key),
                $crate::Op::Delete
            )
        ])
    };

    // Value expression followed by more entries.
    (@__entries [$($acc:expr),*] $key:tt : $val:expr , $($rest:tt)*) => {
        $crate::mutation!(@__entries [
            $($acc,)*
            $crate::Mutation::Apply(
                $crate::Segment::from($key),
                $crate::Op::from($val)
            )
        ] $($rest)*)
    };

    // Value expression as last entry.
    (@__entries [$($acc:expr),*] $key:tt : $val:expr) => {
        $crate::mutation!(@__entries [
            $($acc,)*
            $crate::Mutation::Apply(
                $crate::Segment::from($key),
                $crate::Op::from($val)
            )
        ])
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cbor::cbor;
    use crate::path::write::PathWriter;
    use crate::{FractionalIndex, PID};

    /// Builds a finalized LWW path from the given mutation segments.
    fn build_path(segments: &[&Segment]) -> Vec<u8> {
        let mut w = PathWriter::new(Vec::new(), 0);
        for seg in segments {
            seg.write(&mut w).unwrap();
        }
        w.lww().unwrap()
    }

    /// Serializes a CBOR value into bytes.
    fn encode_cbor(value: &crate::cbor::Value) -> Vec<u8> {
        let mut buf = Vec::new();
        crate::cbor::into_writer(value, &mut buf).unwrap();
        buf
    }

    #[test]
    fn assign_int() {
        let m = mutation!({
            "count": 42
        });
        assert_eq!(
            m,
            Mutation::Compose(vec![Mutation::Apply(
                Segment::Field("count".to_string()),
                Op::Assign(crate::cbor::Value::from(42)),
            )])
        );
    }

    #[test]
    fn assign_float() {
        let m = mutation!({
            "ratio": 2.5
        });
        assert_eq!(
            m,
            Mutation::Compose(vec![Mutation::Apply(
                Segment::Field("ratio".to_string()),
                Op::Assign(crate::cbor::Value::from(2.5)),
            )])
        );
    }

    #[test]
    fn assign_string() {
        let m = mutation!({
            "name": "Alice"
        });
        assert_eq!(
            m,
            Mutation::Compose(vec![Mutation::Apply(
                Segment::Field("name".to_string()),
                Op::Assign(crate::cbor::Value::Text("Alice".to_string())),
            )])
        );
    }

    #[test]
    fn assign_bool() {
        let m = mutation!({
            "active": true
        });
        assert_eq!(
            m,
            Mutation::Compose(vec![Mutation::Apply(
                Segment::Field("active".to_string()),
                Op::Assign(crate::cbor::Value::Bool(true)),
            )])
        );
    }

    #[test]
    fn assign_null() {
        let m = mutation!({
            "empty": crate::cbor::Value::Null
        });
        assert_eq!(
            m,
            Mutation::Compose(vec![Mutation::Apply(
                Segment::Field("empty".to_string()),
                Op::Assign(crate::cbor::Value::Null),
            )])
        );
    }

    #[test]
    fn assign_cbor_map() {
        let m = mutation!({
            "metadata": cbor!({"key" => "value"}).unwrap()
        });
        assert_eq!(
            m,
            Mutation::Compose(vec![Mutation::Apply(
                Segment::Field("metadata".to_string()),
                Op::Assign(crate::cbor::Value::Map(vec![(
                    crate::cbor::Value::Text("key".to_string()),
                    crate::cbor::Value::Text("value".to_string()),
                )])),
            )])
        );
    }

    #[test]
    fn assign_cbor_array() {
        let m = mutation!({
            "items": cbor!([1, 2, 3]).unwrap()
        });
        assert_eq!(
            m,
            Mutation::Compose(vec![Mutation::Apply(
                Segment::Field("items".to_string()),
                Op::Assign(crate::cbor::Value::Array(vec![
                    crate::cbor::Value::Integer(1.into()),
                    crate::cbor::Value::Integer(2.into()),
                    crate::cbor::Value::Integer(3.into()),
                ])),
            )])
        );
    }

    #[test]
    fn delete_directive() {
        let m = mutation!({
            "field_a": @delete,
            "field_b": @delete
        });
        assert_eq!(
            m,
            Mutation::Compose(vec![
                Mutation::Apply(Segment::Field("field_a".to_string()), Op::Delete),
                Mutation::Apply(Segment::Field("field_b".to_string()), Op::Delete),
            ])
        );
    }

    #[test]
    fn nested_mutations() {
        let m = mutation!({
            "users": {
                "alice": {
                    "name": "Alice",
                    "age": @delete
                }
            }
        });
        assert_eq!(
            m,
            Mutation::Compose(vec![Mutation::Nested(
                Segment::Field("users".to_string()),
                vec![Mutation::Nested(
                    Segment::Field("alice".to_string()),
                    vec![
                        Mutation::Apply(
                            Segment::Field("name".to_string()),
                            Op::Assign(crate::cbor::Value::Text("Alice".to_string())),
                        ),
                        Mutation::Apply(Segment::Field("age".to_string()), Op::Delete),
                    ],
                )],
            )])
        );
    }

    #[test]
    fn macro_with_fractional_index_key() {
        let pid = PID::new(1u32).unwrap();
        let idx = FractionalIndex::between(None, None, pid);
        let expected_seg = Segment::FractionalIndex(idx.clone());
        let m = mutation!({
            "items": {
                idx: "Bob"
            }
        });
        assert_eq!(
            m,
            Mutation::Compose(vec![Mutation::Nested(
                Segment::Field("items".to_string()),
                vec![Mutation::Apply(
                    expected_seg,
                    Op::Assign(crate::cbor::Value::Text("Bob".to_string())),
                )],
            )])
        );
    }

    #[test]
    fn for_each_field_assign() {
        let m = mutation!({ "name": "Alice" });
        let mut results = Vec::new();
        m.for_each(|path, value| results.push((path, value)))
            .unwrap();

        let seg = Segment::Field("name".to_string());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, build_path(&[&seg]));
        assert_eq!(
            results[0].1,
            encode_cbor(&crate::cbor::Value::Text("Alice".to_string()))
        );
    }

    #[test]
    fn for_each_field_delete() {
        let m = mutation!({ "name": @delete });
        let mut results = Vec::new();
        m.for_each(|path, value| results.push((path, value)))
            .unwrap();

        let seg = Segment::Field("name".to_string());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, build_path(&[&seg]));
        assert!(results[0].1.is_empty());
    }

    #[test]
    fn for_each_fractional_index_assign() {
        let pid = PID::new(1u32).unwrap();
        let idx = FractionalIndex::between(None, None, pid);
        let seg = Segment::FractionalIndex(idx);
        let m = Mutation::Apply(seg.clone(), Op::Assign(crate::cbor::Value::from(42)));
        let mut results = Vec::new();
        m.for_each(|path, value| results.push((path, value)))
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, build_path(&[&seg]));
        assert_eq!(results[0].1, encode_cbor(&crate::cbor::Value::from(42)));
    }

    #[test]
    fn for_each_fractional_index_delete() {
        let pid = PID::new(1u32).unwrap();
        let idx = FractionalIndex::between(None, None, pid);
        let seg = Segment::FractionalIndex(idx);
        let m = Mutation::Apply(seg.clone(), Op::Delete);
        let mut results = Vec::new();
        m.for_each(|path, value| results.push((path, value)))
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, build_path(&[&seg]));
        assert!(results[0].1.is_empty());
    }

    #[test]
    fn for_each_nested_fields() {
        let m = mutation!({
            "users": {
                "name": "Alice"
            }
        });
        let mut results = Vec::new();
        m.for_each(|path, value| results.push((path, value)))
            .unwrap();

        let users = Segment::Field("users".to_string());
        let name = Segment::Field("name".to_string());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, build_path(&[&users, &name]));
        assert_eq!(
            results[0].1,
            encode_cbor(&crate::cbor::Value::Text("Alice".to_string()))
        );
    }

    #[test]
    fn for_each_nested_with_fractional_index() {
        let pid = PID::new(1u32).unwrap();
        let idx = FractionalIndex::between(None, None, pid);
        let idx_seg = Segment::FractionalIndex(idx);
        let items = Segment::Field("items".to_string());
        let m = Mutation::Nested(
            items.clone(),
            vec![Mutation::Apply(
                idx_seg.clone(),
                Op::Assign(crate::cbor::Value::from(99)),
            )],
        );
        let mut results = Vec::new();
        m.for_each(|path, value| results.push((path, value)))
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, build_path(&[&items, &idx_seg]));
        assert_eq!(results[0].1, encode_cbor(&crate::cbor::Value::from(99)));
    }

    #[test]
    fn for_each_compose_multiple_entries() {
        let m = mutation!({
            "name": "Alice",
            "age": @delete,
            "active": true
        });
        let mut results = Vec::new();
        m.for_each(|path, value| results.push((path, value)))
            .unwrap();

        let name = Segment::Field("name".to_string());
        let age = Segment::Field("age".to_string());
        let active = Segment::Field("active".to_string());

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, build_path(&[&name]));
        assert_eq!(
            results[0].1,
            encode_cbor(&crate::cbor::Value::Text("Alice".to_string()))
        );
        assert_eq!(results[1].0, build_path(&[&age]));
        assert!(results[1].1.is_empty());
        assert_eq!(results[2].0, build_path(&[&active]));
        assert_eq!(results[2].1, encode_cbor(&crate::cbor::Value::Bool(true)));
    }

    #[test]
    fn for_each_deep_nesting_mixed_ops() {
        let m = mutation!({
            "users": {
                "alice": {
                    "name": "Alice",
                    "age": @delete
                }
            }
        });
        let mut results = Vec::new();
        m.for_each(|path, value| results.push((path, value)))
            .unwrap();

        let users = Segment::Field("users".to_string());
        let alice = Segment::Field("alice".to_string());
        let name = Segment::Field("name".to_string());
        let age = Segment::Field("age".to_string());

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, build_path(&[&users, &alice, &name]));
        assert_eq!(
            results[0].1,
            encode_cbor(&crate::cbor::Value::Text("Alice".to_string()))
        );
        assert_eq!(results[1].0, build_path(&[&users, &alice, &age]));
        assert!(results[1].1.is_empty());
    }
}
