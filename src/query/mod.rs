#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Query {
    selects: Vec<Select>,
}

impl Query {
    pub fn parse(query: &str) -> crate::Result<Self, QueryError> {
        todo!()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Select {
    name: crate::Segment,
    alias: Option<crate::Segment>,
    filters: Vec<Filter>,
    subselects: Vec<Select>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Filter {
    Skip(usize),
    Take(usize),
    After(crate::Segment),
    Before(crate::Segment),
}

#[derive(Debug, thiserror::Error)]
pub enum QueryError {}

/// Macro used to build queries at compile time, without the need to parse strings. Its grammar
/// roughly resembles a subset of GraphQL query syntax:
/// - Nesting is expressed as `{}` parentheses.
/// - Field names can be provided with or without string quotes `"`.
/// - It supports aliasing, ex. `{field} as {field_alias}`.
/// - Fields representing collections can have filters attaches. Currently supported filters:
///   - `skip: {number}` skips a number of entries before moving on.
///   - `take: {number}` returns at most a number of entries.
///   - `after: {string|fractional_index}` puts a cursor position at a given entry defined by either
///      string field or [FractionalIndex] and defines direction as moving forward.
///   - `before: {string|fractional_index}` puts a cursor position at a given entry defined by either
///      string field or [FractionalIndex] and defines direction as moving backward.
///
/// # Example
///
/// ```rust
/// use cdoc::*;
///
/// let db = Db::open(DbOptions::new("./path/to/db"))?;
/// let mut tx = db.begin_readonly()?;
///
/// tx.query(query!({
///   users {
///     name as first_name,
///     age,
///     friends(skip: 1, take: 10) {
///       name
///     }
///   }
/// }))?;
/// tx.commit()?;
/// ```
macro_rules! query {
    () => {};
}
