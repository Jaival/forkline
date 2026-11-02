//! Search query parser: free text plus `is:`, `author:`, `label:`, `repo:`,
//! `assignee:` operators with `-` negation. Never fails: anything that is not
//! a valid operator is free text.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    IsPr,
    IsIssue,
    State(&'static str),
    Author(String),
    Label(String),
    /// `owner`, `name`
    Repo(String, String),
    Assignee(String),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    /// (filter, negated)
    pub filters: Vec<(Filter, bool)>,
    /// Free text terms: (text, is_phrase, negated).
    pub terms: Vec<(String, bool, bool)>,
}

/// Split on whitespace; a `"` starts a quoted run (also right after `key:`).
/// Returns (text, had_quotes) per token.
fn tokens(input: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let (mut cur, mut quoted, mut in_quote, mut any) = (String::new(), false, false, false);
    for c in input.chars() {
        match c {
            '"' => {
                in_quote = !in_quote;
                quoted = true;
                any = true;
            }
            c if c.is_whitespace() && !in_quote => {
                if any {
                    out.push((std::mem::take(&mut cur), quoted));
                }
                quoted = false;
                any = false;
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push((cur, quoted));
    }
    out
}

fn filter(key: &str, value: &str) -> Option<Filter> {
    if value.is_empty() {
        return None;
    }
    Some(match key.to_ascii_lowercase().as_str() {
        "is" => match value.to_ascii_lowercase().as_str() {
            "pr" => Filter::IsPr,
            "issue" => Filter::IsIssue,
            "open" => Filter::State("open"),
            "closed" => Filter::State("closed"),
            "merged" => Filter::State("merged"),
            _ => return None,
        },
        "author" => Filter::Author(value.to_owned()),
        "assignee" => Filter::Assignee(value.to_owned()),
        "label" => Filter::Label(value.to_owned()),
        "repo" => {
            let (o, n) = value.split_once('/')?;
            if o.is_empty() || n.is_empty() {
                return None;
            }
            Filter::Repo(o.to_owned(), n.to_owned())
        }
        _ => return None,
    })
}

pub fn parse(input: &str) -> Parsed {
    let mut p = Parsed::default();
    for (tok, quoted) in tokens(input) {
        let (neg, body) = match tok.strip_prefix('-') {
            Some(rest) if !rest.is_empty() => (true, rest),
            _ => (false, tok.as_str()),
        };
        if let Some((k, v)) = body.split_once(':')
            && let Some(f) = filter(k, v)
        {
            p.filters.push((f, neg));
            continue;
        }
        // A bare quoted token is a phrase; keep the minus as text if not negation.
        p.terms.push((body.to_owned(), quoted, neg));
    }
    p
}

/// Build the FTS5 MATCH expression from the free text, or `None` when there
/// is nothing searchable. Every term is a quoted string, so user text can
/// never be read as FTS syntax. The last plain term matches as a prefix.
pub fn fts_match(terms: &[(String, bool, bool)]) -> Option<String> {
    let quote = |t: &str| format!("\"{}\"", t.replace('"', "\"\""));
    // FTS5 splits on non-alphanumerics, so a term with none would be empty.
    let usable = |t: &str| t.chars().any(char::is_alphanumeric);
    let pos: Vec<_> = terms.iter().filter(|(t, _, n)| !n && usable(t)).collect();
    if pos.is_empty() {
        return None;
    }
    let last = pos.len() - 1;
    let positive = pos
        .iter()
        .enumerate()
        .map(|(i, (t, phrase, _))| {
            let q = quote(t);
            if i == last && !phrase { format!("{q}*") } else { q }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let neg: Vec<_> = terms
        .iter()
        .filter(|(t, _, n)| *n && usable(t))
        .map(|(t, _, _)| quote(t))
        .collect();
    Some(if neg.is_empty() {
        positive
    } else {
        format!("({positive}) NOT ({})", neg.join(" OR "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_operators_and_text() {
        let p = parse(r#"crash is:pr -is:closed author:octocat label:"good first issue" repo:o/r panic"#);
        assert_eq!(
            p.filters,
            vec![
                (Filter::IsPr, false),
                (Filter::State("closed"), true),
                (Filter::Author("octocat".into()), false),
                (Filter::Label("good first issue".into()), false),
                (Filter::Repo("o".into(), "r".into()), false),
            ]
        );
        assert_eq!(p.terms.len(), 2);
        assert_eq!(p.terms[0].0, "crash");
    }

    #[test]
    fn invalid_operators_are_text() {
        for q in ["repo:nowhere", "is:nonsense", "author:", "color:red", "-"] {
            let p = parse(q);
            assert!(p.filters.is_empty(), "{q}");
            assert_eq!(p.terms.len(), 1, "{q}");
        }
        assert_eq!(parse("-foo").terms, vec![("foo".into(), false, true)]);
        assert_eq!(parse("-").terms[0].0, "-");
    }

    #[test]
    fn unterminated_quote_takes_the_rest() {
        let p = parse(r#"label:"a b"#);
        assert_eq!(p.filters, vec![(Filter::Label("a b".into()), false)]);
    }

    #[test]
    fn fts_expression_is_always_quoted() {
        let m = |q: &str| fts_match(&parse(q).terms);
        assert_eq!(m("foo bar").as_deref(), Some(r#""foo" "bar"*"#));
        assert_eq!(m(r#""foo bar""#).as_deref(), Some(r#""foo bar""#));
        assert_eq!(m("foo -bar").as_deref(), Some(r#"("foo"*) NOT ("bar")"#));
        assert_eq!(m("NEAR(a b) OR AND *").as_deref(), Some(r#""NEAR(a" "b)" "OR" "AND"*"#));
        assert_eq!(m("*** ^ ()"), None);
        assert_eq!(m("-only"), None);
        assert_eq!(m(r#"a"b"#).as_deref(), Some(r#""ab""#));
    }
}
