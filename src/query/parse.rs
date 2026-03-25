use super::{Filter, Query, QueryError, Select};

pub(super) struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    pub(super) fn new(input: &'a str) -> Self {
        Parser {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), QueryError> {
        self.skip_ws();
        match self.peek() {
            Some(b) if b == byte => {
                self.pos += 1;
                Ok(())
            }
            Some(b) => Err(QueryError::UnexpectedChar(b as char, self.pos)),
            None => Err(QueryError::UnexpectedEnd),
        }
    }

    fn try_byte(&mut self, byte: u8) -> bool {
        self.skip_ws();
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Tries to match a keyword at the current position (after whitespace).
    /// Only succeeds if the keyword is followed by a non-alphanumeric/non-underscore character
    /// (word boundary), preventing "as" from matching inside "assign".
    fn try_keyword(&mut self, kw: &str) -> bool {
        self.skip_ws();
        let end = self.pos + kw.len();
        if end > self.input.len() {
            return false;
        }
        if &self.input[self.pos..end] != kw.as_bytes() {
            return false;
        }
        // Check word boundary: next byte must not be alphanumeric or underscore.
        if let Some(&b) = self.input.get(end) {
            if b.is_ascii_alphanumeric() || b == b'_' {
                return false;
            }
        }
        self.pos = end;
        true
    }

    pub(super) fn parse_query(mut self) -> Result<Query, QueryError> {
        self.expect(b'{')?;
        let selects = self.parse_selects(b'}')?;
        self.expect(b'}')?;
        self.skip_ws();
        if self.pos != self.input.len() {
            return Err(QueryError::UnexpectedChar(
                self.input[self.pos] as char,
                self.pos,
            ));
        }
        Ok(Query::new(selects))
    }

    fn parse_selects(&mut self, terminator: u8) -> Result<Vec<Select>, QueryError> {
        let mut selects = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b) if b == terminator => break,
                None => break,
                _ => {}
            }
            selects.push(self.parse_select()?);
            // consume optional comma separator
            self.try_byte(b',');
        }
        Ok(selects)
    }

    fn parse_select(&mut self) -> Result<Select, QueryError> {
        let name = self.parse_name()?;

        // optional alias
        let alias = if self.try_keyword("as") {
            Some(self.parse_name()?)
        } else {
            None
        };

        // optional filters
        let filters = if self.try_byte(b'(') {
            let f = self.parse_filters()?;
            self.expect(b')')?;
            f
        } else {
            Vec::new()
        };

        // optional subselects
        let subselects = if self.try_byte(b'{') {
            let s = self.parse_selects(b'}')?;
            self.expect(b'}')?;
            s
        } else {
            Vec::new()
        };

        Ok(Select::new(name, alias, filters, subselects))
    }

    fn parse_name(&mut self) -> Result<crate::Segment, QueryError> {
        self.skip_ws();
        match self.peek() {
            Some(b'"') => self.parse_quoted_string(),
            Some(b) if b.is_ascii_alphabetic() || b == b'_' => self.parse_bare_word(),
            Some(b) => Err(QueryError::UnexpectedChar(b as char, self.pos)),
            None => Err(QueryError::UnexpectedEnd),
        }
    }

    fn parse_quoted_string(&mut self) -> Result<crate::Segment, QueryError> {
        debug_assert_eq!(self.peek(), Some(b'"'));
        self.pos += 1; // skip opening quote
        let start = self.pos;
        while self.pos < self.input.len() && self.input[self.pos] != b'"' {
            self.pos += 1;
        }
        if self.pos >= self.input.len() {
            return Err(QueryError::UnexpectedEnd);
        }
        let s = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|_| QueryError::UnexpectedChar('?', start))?;
        self.pos += 1; // skip closing quote
        Ok(crate::Segment::Field(s.to_string()))
    }

    fn parse_bare_word(&mut self) -> Result<crate::Segment, QueryError> {
        let start = self.pos;
        while self.pos < self.input.len() {
            let b = self.input[self.pos];
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let s = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|_| QueryError::UnexpectedChar('?', start))?;
        Ok(crate::Segment::Field(s.to_string()))
    }

    fn parse_number(&mut self) -> Result<usize, QueryError> {
        self.skip_ws();
        let start = self.pos;
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        if self.pos == start {
            return match self.peek() {
                Some(b) => Err(QueryError::UnexpectedChar(b as char, self.pos)),
                None => Err(QueryError::UnexpectedEnd),
            };
        }
        let s = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|_| QueryError::UnexpectedChar('?', start))?;
        s.parse::<usize>()
            .map_err(|_| QueryError::UnexpectedChar('?', start))
    }

    fn parse_filters(&mut self) -> Result<Vec<Filter>, QueryError> {
        let mut filters = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b')') | None => break,
                _ => {}
            }
            filters.push(self.parse_filter()?);
            self.try_byte(b',');
        }
        Ok(filters)
    }

    fn parse_filter(&mut self) -> Result<Filter, QueryError> {
        let kw_pos = self.pos;
        self.skip_ws();
        let start = self.pos;
        // read keyword
        while self.pos < self.input.len() {
            let b = self.input[self.pos];
            if b.is_ascii_alphabetic() || b == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let kw = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|_| QueryError::UnexpectedChar('?', start))?;

        self.expect(b':')?;

        match kw {
            "skip" => Ok(Filter::Skip(self.parse_number()?)),
            "take" => Ok(Filter::Take(self.parse_number()?)),
            "after" => Ok(Filter::After(self.parse_name()?)),
            "before" => Ok(Filter::Before(self.parse_name()?)),
            _ => Err(QueryError::UnknownFilter(kw.to_string(), kw_pos)),
        }
    }
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
    fn parse_nested_structure() {
        let q = Query::parse("{ users { name, age } }").unwrap();
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
    fn parse_deeply_nested_structure() {
        let q = Query::parse("{ users { profile { avatar { url } } } }").unwrap();
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
    fn parse_bare_ident_selectors() {
        let q = Query::parse("{ name, age, active }").unwrap();
        assert_eq!(
            q,
            Query::new(vec![leaf("name"), leaf("age"), leaf("active")])
        );
    }

    #[test]
    fn parse_quoted_string_selectors() {
        let q = Query::parse(r#"{ "first name", "last name" }"#).unwrap();
        assert_eq!(q, Query::new(vec![leaf("first name"), leaf("last name")]));
    }

    #[test]
    fn parse_mixed_ident_and_quoted_selectors() {
        let q = Query::parse(r#"{ name, "home address" }"#).unwrap();
        assert_eq!(q, Query::new(vec![leaf("name"), leaf("home address")]));
    }

    #[test]
    fn parse_alias_with_bare_idents() {
        let q = Query::parse("{ name as first_name, age }").unwrap();
        assert_eq!(
            q,
            Query::new(vec![
                Select::new(field("name"), Some(field("first_name")), vec![], vec![]),
                leaf("age"),
            ])
        );
    }

    #[test]
    fn parse_alias_with_quoted_strings() {
        let q = Query::parse(r#"{ "field" as "alias" }"#).unwrap();
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
    fn parse_alias_mixed_ident_and_quoted() {
        let q = Query::parse(r#"{ name as "display name" }"#).unwrap();
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
    fn parse_filter_skip() {
        let q = Query::parse("{ items(skip: 5) }").unwrap();
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
    fn parse_filter_take() {
        let q = Query::parse("{ items(take: 10) }").unwrap();
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
    fn parse_filter_after() {
        let q = Query::parse(r#"{ items(after: "cursor_abc") }"#).unwrap();
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
    fn parse_filter_before() {
        let q = Query::parse(r#"{ items(before: "cursor_xyz") }"#).unwrap();
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
    fn parse_multiple_filters() {
        let q =
            Query::parse(r#"{ items(skip: 2, take: 5, after: "start", before: "end") }"#).unwrap();
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
    fn parse_filters_with_subselects() {
        let q = Query::parse("{ friends(skip: 1, take: 10) { name } }").unwrap();
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
    fn parse_alias_filters_and_subselects_combined() {
        let q = Query::parse(
            "{ users { name as first_name, age, friends(skip: 1, take: 10) { name } } }",
        )
        .unwrap();
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
    fn parse_trailing_comma_is_allowed() {
        let q = Query::parse("{ name, age, }").unwrap();
        assert_eq!(q, Query::new(vec![leaf("name"), leaf("age")]));
    }

    #[test]
    fn parse_error_unexpected_char() {
        let err = Query::parse("{ @ }").unwrap_err();
        assert!(matches!(err, QueryError::UnexpectedChar('@', _)));
    }

    #[test]
    fn parse_error_unexpected_end() {
        let err = Query::parse("{ name").unwrap_err();
        assert!(matches!(err, QueryError::UnexpectedEnd));
    }

    #[test]
    fn parse_error_unknown_filter() {
        let err = Query::parse("{ items(limit: 5) }").unwrap_err();
        assert!(matches!(err, QueryError::UnknownFilter(kw, _) if kw == "limit"));
    }

    #[test]
    fn parse_error_missing_opening_brace() {
        let err = Query::parse("name, age").unwrap_err();
        assert!(matches!(err, QueryError::UnexpectedChar('n', 0)));
    }

    #[test]
    fn parse_error_trailing_content() {
        let err = Query::parse("{ name } extra").unwrap_err();
        assert!(matches!(err, QueryError::UnexpectedChar('e', _)));
    }
}
