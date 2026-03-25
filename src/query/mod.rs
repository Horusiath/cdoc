mod parse;

/// Structured query descriptor used to select fields from the document tree.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Query {
    selects: Vec<Select>,
}

impl Query {
    /// Creates a new query with the given field selectors.
    pub fn new(selects: Vec<Select>) -> Self {
        Query { selects }
    }

    pub fn parse(query: &str) -> crate::Result<Self, QueryError> {
        parse::Parser::new(query).parse_query()
    }
}

/// A single field selector within a [Query]. May carry an alias, filters, and nested sub-selects.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Select {
    name: crate::Segment,
    alias: Option<crate::Segment>,
    filters: Vec<Filter>,
    subselects: Vec<Select>,
}

impl Select {
    /// Creates a new field selector.
    pub fn new(
        name: crate::Segment,
        alias: Option<crate::Segment>,
        filters: Vec<Filter>,
        subselects: Vec<Select>,
    ) -> Self {
        Select {
            name,
            alias,
            filters,
            subselects,
        }
    }
}

/// Pagination/cursor filter applicable to collection fields inside a [Select].
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Filter {
    /// Skips a number of entries before returning results.
    Skip(usize),
    /// Returns at most a given number of entries.
    Take(usize),
    /// Sets the cursor at the entry identified by the segment and moves forward.
    After(crate::Segment),
    /// Sets the cursor at the entry identified by the segment and moves backward.
    Before(crate::Segment),
}

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    /// Found a character that doesn't belong at the given position.
    #[error("unexpected character '{0}' at position {1}")]
    UnexpectedChar(char, usize),
    /// Input ended before the query was complete.
    #[error("unexpected end of input")]
    UnexpectedEnd,
    /// A filter keyword was not recognised.
    #[error("unknown filter '{0}' at position {1}")]
    UnknownFilter(String, usize),
}

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
/// ```rust,ignore
/// use cdoc::*;
///
/// let db = Db::open(DbOptions::new("./path/to/db"))?;
/// let mut tx = db.begin_readonly()?;
///
/// tx.query(&query!({
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
#[macro_export]
macro_rules! query {
    // Entry point: wraps selects in a Query.
    ({ $($body:tt)* }) => {
        $crate::Query::new($crate::query!(@__selects [] $($body)*))
    };

    // --- Select list accumulation ---

    // Base case: all selects consumed.
    (@__selects [$($acc:expr),*]) => {
        ::std::vec![$($acc),*]
    };

    // Start parsing a select whose name is a bare identifier.
    (@__selects [$($acc:expr),*] $name:ident $($rest:tt)*) => {
        $crate::query!(@__alias [$($acc),*] [stringify!($name)] $($rest)*)
    };

    // Start parsing a select whose name is a quoted string.
    (@__selects [$($acc:expr),*] $name:literal $($rest:tt)*) => {
        $crate::query!(@__alias [$($acc),*] [$name] $($rest)*)
    };

    // --- Alias detection (optional `as` clause) ---

    // Alias is a bare identifier.
    (@__alias [$($acc:expr),*] [$name:expr] as $alias:ident $($rest:tt)*) => {
        $crate::query!(@__filters [$($acc),*] [$name] [Some($crate::Segment::from(stringify!($alias)))] $($rest)*)
    };

    // Alias is a quoted string.
    (@__alias [$($acc:expr),*] [$name:expr] as $alias:literal $($rest:tt)*) => {
        $crate::query!(@__filters [$($acc),*] [$name] [Some($crate::Segment::from($alias))] $($rest)*)
    };

    // No alias.
    (@__alias [$($acc:expr),*] [$name:expr] $($rest:tt)*) => {
        $crate::query!(@__filters [$($acc),*] [$name] [None] $($rest)*)
    };

    // --- Filter detection (optional `(...)` clause) ---

    // Parenthesised filter list present.
    (@__filters [$($acc:expr),*] [$name:expr] [$alias:expr] ($($fbody:tt)*) $($rest:tt)*) => {
        $crate::query!(@__subselects [$($acc),*] [$name] [$alias] [$crate::query!(@__filter_list [] $($fbody)*)] $($rest)*)
    };

    // No filters.
    (@__filters [$($acc:expr),*] [$name:expr] [$alias:expr] $($rest:tt)*) => {
        $crate::query!(@__subselects [$($acc),*] [$name] [$alias] [::std::vec![]] $($rest)*)
    };

    // --- Subselect detection (optional `{ ... }` block) ---

    // Brace-delimited subselects present.
    (@__subselects [$($acc:expr),*] [$name:expr] [$alias:expr] [$filters:expr] { $($inner:tt)* } $($rest:tt)*) => {
        $crate::query!(@__done [$($acc),*] [$name] [$alias] [$filters] [$crate::query!(@__selects [] $($inner)*)] $($rest)*)
    };

    // No subselects.
    (@__subselects [$($acc:expr),*] [$name:expr] [$alias:expr] [$filters:expr] $($rest:tt)*) => {
        $crate::query!(@__done [$($acc),*] [$name] [$alias] [$filters] [::std::vec![]] $($rest)*)
    };

    // --- Finalise one Select and continue ---

    // Comma separator followed by more selects.
    (@__done [$($acc:expr),*] [$name:expr] [$alias:expr] [$filters:expr] [$subs:expr] , $($rest:tt)*) => {
        $crate::query!(@__selects [
            $($acc,)*
            $crate::Select::new($crate::Segment::from($name), $alias, $filters, $subs)
        ] $($rest)*)
    };

    // Last select (no trailing comma).
    (@__done [$($acc:expr),*] [$name:expr] [$alias:expr] [$filters:expr] [$subs:expr]) => {
        $crate::query!(@__selects [
            $($acc,)*
            $crate::Select::new($crate::Segment::from($name), $alias, $filters, $subs)
        ])
    };

    // --- Filter list accumulation ---

    // Base case: all filters consumed.
    (@__filter_list [$($acc:expr),*]) => {
        ::std::vec![$($acc),*]
    };

    // skip filter with trailing filters.
    (@__filter_list [$($acc:expr),*] skip : $n:expr , $($rest:tt)*) => {
        $crate::query!(@__filter_list [$($acc,)* $crate::Filter::Skip($n)] $($rest)*)
    };
    // skip filter (last).
    (@__filter_list [$($acc:expr),*] skip : $n:expr) => {
        $crate::query!(@__filter_list [$($acc,)* $crate::Filter::Skip($n)])
    };

    // take filter with trailing filters.
    (@__filter_list [$($acc:expr),*] take : $n:expr , $($rest:tt)*) => {
        $crate::query!(@__filter_list [$($acc,)* $crate::Filter::Take($n)] $($rest)*)
    };
    // take filter (last).
    (@__filter_list [$($acc:expr),*] take : $n:expr) => {
        $crate::query!(@__filter_list [$($acc,)* $crate::Filter::Take($n)])
    };

    // after filter with trailing filters.
    (@__filter_list [$($acc:expr),*] after : $v:expr , $($rest:tt)*) => {
        $crate::query!(@__filter_list [$($acc,)* $crate::Filter::After($crate::Segment::from($v))] $($rest)*)
    };
    // after filter (last).
    (@__filter_list [$($acc:expr),*] after : $v:expr) => {
        $crate::query!(@__filter_list [$($acc,)* $crate::Filter::After($crate::Segment::from($v))])
    };

    // before filter with trailing filters.
    (@__filter_list [$($acc:expr),*] before : $v:expr , $($rest:tt)*) => {
        $crate::query!(@__filter_list [$($acc,)* $crate::Filter::Before($crate::Segment::from($v))] $($rest)*)
    };
    // before filter (last).
    (@__filter_list [$($acc:expr),*] before : $v:expr) => {
        $crate::query!(@__filter_list [$($acc,)* $crate::Filter::Before($crate::Segment::from($v))])
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Segment;

    fn field(name: &str) -> Segment {
        Segment::Field(name.to_string())
    }

    fn leaf(name: &str) -> Select {
        Select::new(field(name), None, vec![], vec![])
    }

    #[test]
    fn nested_structure() {
        let q = query!({ users { name, age } });
        assert_eq!(
            q,
            Query::new(vec![Select::new(
                field("users"),
                None,
                vec![],
                vec![leaf("name"), leaf("age")],
            )])
        );
    }

    #[test]
    fn deeply_nested_structure() {
        let q = query!({
            users {
                profile {
                    avatar {
                        url
                    }
                }
            }
        });
        assert_eq!(
            q,
            Query::new(vec![Select::new(
                field("users"),
                None,
                vec![],
                vec![Select::new(
                    field("profile"),
                    None,
                    vec![],
                    vec![Select::new(
                        field("avatar"),
                        None,
                        vec![],
                        vec![leaf("url")],
                    )],
                )],
            )])
        );
    }

    #[test]
    fn bare_ident_selectors() {
        let q = query!({
            name,
            age,
            active
        });
        assert_eq!(
            q,
            Query::new(vec![leaf("name"), leaf("age"), leaf("active")])
        );
    }

    #[test]
    fn quoted_string_selectors() {
        let q = query!({
            "first name",
            "last name"
        });
        assert_eq!(q, Query::new(vec![leaf("first name"), leaf("last name")]));
    }

    #[test]
    fn mixed_ident_and_quoted_selectors() {
        let q = query!({
            name,
            "home address"
        });
        assert_eq!(q, Query::new(vec![leaf("name"), leaf("home address")]));
    }

    #[test]
    fn alias_with_bare_idents() {
        let q = query!({
            name as first_name,
            age
        });
        assert_eq!(
            q,
            Query::new(vec![
                Select::new(field("name"), Some(field("first_name")), vec![], vec![]),
                leaf("age"),
            ])
        );
    }

    #[test]
    fn alias_with_quoted_strings() {
        let q = query!({
            "field" as "alias"
        });
        assert_eq!(
            q,
            Query::new(vec![Select::new(
                field("field"),
                Some(field("alias")),
                vec![],
                vec![],
            )])
        );
    }

    #[test]
    fn alias_mixed_ident_and_quoted() {
        let q = query!({
            name as "display name"
        });
        assert_eq!(
            q,
            Query::new(vec![Select::new(
                field("name"),
                Some(field("display name")),
                vec![],
                vec![],
            )])
        );
    }

    #[test]
    fn filter_skip() {
        let q = query!({
            items(skip: 5)
        });
        assert_eq!(
            q,
            Query::new(vec![Select::new(
                field("items"),
                None,
                vec![Filter::Skip(5)],
                vec![],
            )])
        );
    }

    #[test]
    fn filter_take() {
        let q = query!({
            items(take: 10)
        });
        assert_eq!(
            q,
            Query::new(vec![Select::new(
                field("items"),
                None,
                vec![Filter::Take(10)],
                vec![],
            )])
        );
    }

    #[test]
    fn filter_after() {
        let q = query!({
            items(after: "cursor_abc")
        });
        assert_eq!(
            q,
            Query::new(vec![Select::new(
                field("items"),
                None,
                vec![Filter::After(field("cursor_abc"))],
                vec![],
            )])
        );
    }

    #[test]
    fn filter_before() {
        let q = query!({
            items(before: "cursor_xyz")
        });
        assert_eq!(
            q,
            Query::new(vec![Select::new(
                field("items"),
                None,
                vec![Filter::Before(field("cursor_xyz"))],
                vec![],
            )])
        );
    }

    #[test]
    fn multiple_filters() {
        let q = query!({
            items(skip: 2, take: 5, after: "start", before: "end")
        });
        assert_eq!(
            q,
            Query::new(vec![Select::new(
                field("items"),
                None,
                vec![
                    Filter::Skip(2),
                    Filter::Take(5),
                    Filter::After(field("start")),
                    Filter::Before(field("end")),
                ],
                vec![],
            )])
        );
    }

    #[test]
    fn filters_with_subselects() {
        let q = query!({
            friends(skip: 1, take: 10) {
                name
            }
        });
        assert_eq!(
            q,
            Query::new(vec![Select::new(
                field("friends"),
                None,
                vec![Filter::Skip(1), Filter::Take(10)],
                vec![leaf("name")],
            )])
        );
    }

    #[test]
    fn alias_filters_and_subselects_combined() {
        let q = query!({
            users {
                name as first_name,
                age,
                friends(skip: 1, take: 10) {
                    name
                }
            }
        });
        assert_eq!(
            q,
            Query::new(vec![Select::new(
                field("users"),
                None,
                vec![],
                vec![
                    Select::new(field("name"), Some(field("first_name")), vec![], vec![]),
                    leaf("age"),
                    Select::new(
                        field("friends"),
                        None,
                        vec![Filter::Skip(1), Filter::Take(10)],
                        vec![leaf("name")],
                    ),
                ],
            )])
        );
    }

    #[test]
    fn trailing_comma_is_allowed() {
        let q = query!({
            name,
            age,
        });
        assert_eq!(q, Query::new(vec![leaf("name"), leaf("age")]));
    }
}
