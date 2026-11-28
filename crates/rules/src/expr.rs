//! A tiny expression reader for class-name expressions: string literals,
//! templates, ternaries, `&&`/`||`/`??`, calls, arrays and objects. Anything
//! else is opaque. It is enough to list the class strings an attribute can
//! produce and to tell whether the expression is conditional.

use crate::markup::skip_group;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expr {
    Str(String),
    Template(Vec<Part>),
    Cond(Box<Expr>, Box<Expr>),
    Logical(Box<Expr>, Box<Expr>),
    Call(Vec<Expr>),
    Array(Vec<Expr>),
    Object(Vec<String>),
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Part {
    Text(String),
    Expr(Expr),
}

/// Byte offsets of the bytes of `s` that sit at nesting depth 0, outside any
/// string, template or comment. A group is represented by its opening byte.
fn top_level(s: &str) -> Vec<usize> {
    let bytes = s.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' | b'"' | b'`' | b'(' | b'[' | b'{' => {
                found.push(i);
                i = skip_group(s, i);
            }
            _ => {
                found.push(i);
                i += 1;
            }
        }
    }
    found
}

fn is_ternary_mark(bytes: &[u8], i: usize) -> bool {
    bytes[i] == b'?'
        && bytes.get(i + 1) != Some(&b'.')
        && bytes.get(i + 1) != Some(&b'?')
        && (i == 0 || bytes[i - 1] != b'?')
}

pub(crate) fn split_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    for i in top_level(s) {
        if s.as_bytes()[i] == b',' {
            parts.push(&s[start..i]);
            start = i + 1;
        }
    }
    parts.push(&s[start..]);
    parts
        .into_iter()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

fn unwrap_parens(s: &str) -> &str {
    let trimmed = s.trim();
    if trimmed.starts_with('(') && skip_group(trimmed, 0) == trimmed.len() {
        return unwrap_parens(&trimmed[1..trimmed.len() - 1]);
    }
    trimmed
}

fn whole_group(s: &str, open: u8, close: u8) -> Option<&str> {
    let bytes = s.as_bytes();
    (bytes.first() == Some(&open) && bytes.last() == Some(&close) && skip_group(s, 0) == s.len())
        .then(|| &s[1..s.len() - 1])
}

pub(crate) fn parse(source: &str) -> Expr {
    let s = unwrap_parens(source);
    let bytes = s.as_bytes();
    let tops = top_level(s);
    if let Some(&q) = tops.iter().find(|&&i| is_ternary_mark(bytes, i)) {
        let mut nested = 0usize;
        for &i in tops.iter().filter(|&&i| i > q) {
            if is_ternary_mark(bytes, i) {
                nested += 1;
            } else if bytes[i] == b':' {
                if nested == 0 {
                    return Expr::Cond(Box::new(parse(&s[q + 1..i])), Box::new(parse(&s[i + 1..])));
                }
                nested -= 1;
            }
        }
        return Expr::Cond(Box::new(parse(&s[q + 1..])), Box::new(Expr::Other));
    }
    for &i in &tops {
        let pair = bytes.get(i..i + 2);
        if matches!(pair, Some(b"&&" | b"||" | b"??")) {
            return Expr::Logical(Box::new(parse(&s[..i])), Box::new(parse(&s[i + 2..])));
        }
    }
    primary(s)
}

fn primary(s: &str) -> Expr {
    let bytes = s.as_bytes();
    let Some(&first) = bytes.first() else {
        return Expr::Other;
    };
    match first {
        b'\'' | b'"' if skip_group(s, 0) == s.len() => Expr::Str(s[1..s.len() - 1].to_string()),
        b'`' if skip_group(s, 0) == s.len() => Expr::Template(template_parts(&s[1..s.len() - 1])),
        b'[' => whole_group(s, b'[', b']').map_or(Expr::Other, |inner| {
            Expr::Array(split_commas(inner).into_iter().map(parse).collect())
        }),
        b'{' => whole_group(s, b'{', b'}').map_or(Expr::Other, |inner| {
            Expr::Object(
                split_commas(inner)
                    .into_iter()
                    .map(|entry| object_key(entry).to_string())
                    .collect(),
            )
        }),
        _ => call(s),
    }
}

fn object_key(entry: &str) -> &str {
    let key = top_level(entry)
        .into_iter()
        .find(|&i| entry.as_bytes()[i] == b':')
        .map_or(entry, |colon| &entry[..colon]);
    key.trim().trim_matches(['"', '\''])
}

fn call(s: &str) -> Expr {
    let Some(paren) = top_level(s).into_iter().find(|&i| s.as_bytes()[i] == b'(') else {
        return Expr::Other;
    };
    if paren == 0 || skip_group(s, paren) != s.len() {
        return Expr::Other;
    }
    let callee = &s[..paren];
    if !callee
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$' | b'.' | b'?'))
    {
        return Expr::Other;
    }
    Expr::Call(
        split_commas(&s[paren + 1..s.len() - 1])
            .into_iter()
            .map(parse)
            .collect(),
    )
}

fn template_parts(body: &str) -> Vec<Part> {
    let bytes = body.as_bytes();
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'{') {
            let end = skip_group(body, i + 1);
            parts.push(Part::Text(std::mem::take(&mut text)));
            parts.push(Part::Expr(parse(&body[i + 2..end.saturating_sub(1)])));
            i = end;
        } else {
            let ch_len = body[i..].chars().next().map_or(1, char::len_utf8);
            text.push_str(&body[i..i + ch_len]);
            i += ch_len;
        }
    }
    parts.push(Part::Text(text));
    parts
}

fn tokens(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_string).collect()
}

fn combine(a: Vec<Vec<String>>, b: Vec<Vec<String>>) -> Vec<Vec<String>> {
    if a.is_empty() {
        return b;
    }
    if b.is_empty() {
        return a;
    }
    a.iter()
        .flat_map(|left| {
            b.iter().map(|right| {
                let mut merged = left.clone();
                merged.extend(right.iter().cloned());
                merged
            })
        })
        .collect()
}

fn product(items: &[Expr]) -> Vec<Vec<String>> {
    items.iter().fold(vec![Vec::new()], |acc, item| {
        combine(acc, alternatives(item))
    })
}

/// Every distinct list of class tokens the expression can produce: each
/// branch of a ternary is its own alternative, `&&`/`||` merge both sides.
pub(crate) fn alternatives(expr: &Expr) -> Vec<Vec<String>> {
    match expr {
        Expr::Str(s) => vec![tokens(s)],
        Expr::Template(parts) => parts.iter().fold(vec![Vec::new()], |acc, part| match part {
            Part::Text(text) => acc
                .into_iter()
                .map(|mut alt| {
                    alt.extend(tokens(text));
                    alt
                })
                .collect(),
            Part::Expr(inner) => combine(acc, alternatives(inner)),
        }),
        Expr::Cond(a, b) => {
            let mut all = alternatives(a);
            all.extend(alternatives(b));
            all
        }
        Expr::Logical(a, b) => {
            let mut merged: Vec<String> = alternatives(a).into_iter().flatten().collect();
            merged.extend(alternatives(b).into_iter().flatten());
            vec![merged]
        }
        Expr::Call(args) | Expr::Array(args) => {
            let merged = product(args);
            if merged == [Vec::<String>::new()] {
                Vec::new()
            } else {
                merged
            }
        }
        Expr::Object(keys) => vec![keys.iter().flat_map(|k| tokens(k)).collect()],
        Expr::Other => Vec::new(),
    }
}

/// A ternary or logical expression, or a call with such an argument.
pub(crate) fn is_conditional(expr: &Expr) -> bool {
    match expr {
        Expr::Cond(..) | Expr::Logical(..) => true,
        Expr::Call(args) => args.iter().any(is_conditional),
        _ => false,
    }
}

/// The text ends with a call to `.map(...)`.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
pub(crate) fn ends_with_map_call(s: &str) -> bool {
    let trimmed = s.trim();
    if !trimmed.ends_with(')') {
        return false;
    }
    let Some(open) = top_level(trimmed)
        .into_iter()
        .filter(|&i| trimmed.as_bytes()[i] == b'(')
        .rfind(|&i| skip_group(trimmed, i) == trimmed.len())
    else {
        return false;
    };
    let callee = trimmed[..open].trim_end();
    callee.ends_with(".map") || callee.ends_with("?.map")
}

/// The string literals an expression is made of: a string, a template (its
/// text parts joined with spaces) or the literals of an array.
pub(crate) fn literal_strings(expr: &Expr) -> Vec<String> {
    match expr {
        Expr::Str(s) => vec![s.clone()],
        Expr::Template(parts) => {
            let text: Vec<&str> = parts
                .iter()
                .filter_map(|p| match p {
                    Part::Text(t) => Some(t.as_str()),
                    Part::Expr(_) => None,
                })
                .collect();
            vec![text.join(" ")]
        }
        Expr::Array(items) => items.iter().flat_map(literal_strings).collect(),
        _ => Vec::new(),
    }
}

/// `key: value` entries of an object literal written as `{ ... }`.
pub(crate) fn object_entries(object: &str) -> Option<Vec<(String, &str)>> {
    let inner = whole_group(object.trim(), b'{', b'}')?;
    Some(
        split_commas(inner)
            .into_iter()
            .filter_map(|entry| {
                let colon = top_level(entry)
                    .into_iter()
                    .find(|&i| entry.as_bytes()[i] == b':')?;
                Some((
                    entry[..colon].trim().trim_matches(['"', '\'']).to_string(),
                    entry[colon + 1..].trim(),
                ))
            })
            .collect(),
    )
}

/// Class text an expression writes literally: strings, the text parts of
/// templates, both branches of ternaries and logical operators, call arguments.
pub(crate) fn class_text(expr: &Expr) -> String {
    match expr {
        Expr::Str(s) => s.clone(),
        Expr::Template(parts) => parts
            .iter()
            .filter_map(|p| match p {
                Part::Text(t) => Some(t.as_str()),
                Part::Expr(_) => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
        Expr::Cond(a, b) | Expr::Logical(a, b) => format!("{} {}", class_text(a), class_text(b)),
        Expr::Call(items) | Expr::Array(items) => {
            items.iter().map(class_text).collect::<Vec<_>>().join(" ")
        }
        Expr::Object(_) | Expr::Other => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alts(source: &str) -> Vec<Vec<String>> {
        alternatives(&parse(source))
    }

    fn flat(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_ternary_yields_one_alternative_per_branch_and_cn_combines_with_the_rest() {
        assert_eq!(
            alts(r#"cn("a b", on ? "c" : "d")"#),
            vec![flat(&["a", "b", "c"]), flat(&["a", "b", "d"])]
        );
        assert_eq!(
            alts(r#"on ? "x" : off ? "y" : "z""#),
            vec![flat(&["x"]), flat(&["y"]), flat(&["z"])],
            "nested ternaries stay separate branches"
        );
    }

    #[test]
    fn logical_operators_merge_both_sides_and_templates_combine_text_with_expressions() {
        assert_eq!(alts(r#"on && "x" || "y""#), vec![flat(&["x", "y"])]);
        assert_eq!(
            alts(r#"`base ${on ? "p-1" : "p-2"} tail`"#),
            vec![
                flat(&["base", "p-1", "tail"]),
                flat(&["base", "p-2", "tail"])
            ]
        );
        assert_eq!(alts("someVariable"), Vec::<Vec<String>>::new());
        assert_eq!(alts(r#"{ "a b": on, c: 1 }"#), vec![flat(&["a", "b", "c"])]);
        assert_eq!(alts(r#"["a", on && "b"]"#), vec![flat(&["a", "b"])]);
    }

    #[test]
    fn conditional_means_ternary_logical_or_a_call_with_one() {
        let conditional = |s: &str| is_conditional(&parse(s));
        assert!(conditional(r#"on ? "a" : "b""#));
        assert!(conditional(r#"cn("a", on && "b")"#));
        assert!(conditional(r#"x ?? "a""#));
        assert!(!conditional(r#"cn("a", "b")"#));
        assert!(
            !conditional(r#"`a ${on ? "b" : "c"}`"#),
            "a template is not conditional"
        );
        assert!(
            !conditional("props?.className"),
            "optional chaining is not a ternary"
        );
    }

    #[test]
    fn class_text_ignores_template_expressions_but_keeps_both_branches() {
        assert_eq!(
            class_text(&parse(r#"cn("a", on ? "b" : "c", `d ${x ? "e" : "f"}`)"#))
                .split_whitespace()
                .collect::<Vec<_>>(),
            vec!["a", "b", "c", "d"]
        );
    }

    #[test]
    fn map_calls_are_recognised_only_when_they_end_the_expression() {
        assert!(ends_with_map_call("items.map((i) => <li key={i} />)"));
        assert!(ends_with_map_call("items.filter(Boolean).map(String)"));
        assert!(ends_with_map_call("items?.map(f)"));
        assert!(!ends_with_map_call("items.map(f).join(\",\")"));
        assert!(!ends_with_map_call("cond && items.length"));
    }

    #[test]
    fn object_entries_split_on_top_level_commas_and_colons() {
        let entries =
            object_entries(r#"{ a: "x, y", "b-c": cn("p", { q: 1 }), d: [1, 2] }"#).unwrap();
        let keys: Vec<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["a", "b-c", "d"]);
        assert_eq!(entries[0].1, r#""x, y""#);
        assert!(object_entries("not an object").is_none());
    }
}
