//! A forgiving JSX/TSX scanner: it finds elements, their nesting and their
//! attributes without building a full JavaScript AST. Strings, template
//! literals, comments and braces are skipped so `>` or quotes inside them do
//! not confuse tag boundaries.

use std::ops::Range;

const MAX_DEPTH: usize = 200;

#[derive(Debug, Clone)]
pub(crate) struct Element {
    pub name: String,
    /// The opening tag, `<` through `>` (or `/>`).
    pub open: Range<usize>,
    /// Text between the opening and the closing tag; empty when self-closing.
    pub inner: Range<usize>,
    pub parent: Option<usize>,
    pub self_closing: bool,
    /// Direct child elements in document order (elements found inside an
    /// attribute value such as `render={<Button />}` are not children).
    pub children: Vec<usize>,
    /// Some direct child is text or a `{expression}` that renders text.
    pub has_text: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Markup {
    pub elements: Vec<Element>,
}

#[derive(Debug, Clone)]
pub(crate) struct Attr {
    pub name: String,
    /// Raw value including quotes or braces; `None` for a bare attribute.
    pub value: Option<Range<usize>>,
}

#[derive(Clone, Copy)]
struct Scope {
    parent: Option<usize>,
    in_attr: bool,
}

struct Parser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    elements: Vec<Element>,
    depth: usize,
}

pub(crate) fn parse(src: &str) -> Markup {
    let mut parser = Parser::new(src);
    parser.js(
        0,
        None,
        Scope {
            parent: None,
            in_attr: false,
        },
    );
    Markup {
        elements: parser.elements,
    }
}

/// Index just after the group that opens at `start` (`(`, `[`, `{` or a quote
/// or backtick). Returns the text length when it never closes.
pub(crate) fn skip_group(src: &str, start: usize) -> usize {
    let mut parser = Parser::new(src);
    let scope = Scope {
        parent: None,
        in_attr: true,
    };
    match src.as_bytes().get(start) {
        Some(b'\'' | b'"') => parser.quoted(start),
        Some(b'`') => parser.template(start, scope),
        Some(&open @ (b'(' | b'[' | b'{')) => parser.js(start + 1, Some(closer(open)), scope),
        _ => start + 1,
    }
}

fn closer(open: u8) -> u8 {
    match open {
        b'(' => b')',
        b'[' => b']',
        _ => b'}',
    }
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic()
}

fn is_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-' | b'$')
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            elements: Vec::new(),
            depth: 0,
        }
    }

    fn at(&self, i: usize) -> u8 {
        self.bytes.get(i).copied().unwrap_or(0)
    }

    fn quoted(&self, start: usize) -> usize {
        let quote = self.at(start);
        let mut i = start + 1;
        while i < self.bytes.len() {
            match self.bytes[i] {
                b'\\' => i += 2,
                b if b == quote => return i + 1,
                _ => i += 1,
            }
        }
        self.bytes.len()
    }

    fn template(&mut self, start: usize, scope: Scope) -> usize {
        let mut i = start + 1;
        while i < self.bytes.len() {
            match self.bytes[i] {
                b'\\' => i += 2,
                b'`' => return i + 1,
                b'$' if self.at(i + 1) == b'{' => i = self.js(i + 2, Some(b'}'), scope),
                _ => i += 1,
            }
        }
        self.bytes.len()
    }

    /// Scans JavaScript from `i` until the byte `close` ends the current
    /// group; returns the index after it. JSX found in expression position is
    /// recorded as elements.
    fn js(&mut self, mut i: usize, close: Option<u8>, scope: Scope) -> usize {
        if self.depth >= MAX_DEPTH {
            return self.bytes.len();
        }
        self.depth += 1;
        while i < self.bytes.len() {
            match self.bytes[i] {
                b'\'' | b'"' => i = self.quoted(i),
                b'`' => i = self.template(i, scope),
                b'/' if self.at(i + 1) == b'/' => {
                    i = self.src[i..].find('\n').map_or(self.bytes.len(), |n| i + n);
                }
                b'/' if self.at(i + 1) == b'*' => {
                    i = self.src[i + 2..]
                        .find("*/")
                        .map_or(self.bytes.len(), |n| i + 2 + n + 2);
                }
                b @ (b'(' | b'[' | b'{') => i = self.js(i + 1, Some(closer(b)), scope),
                b @ (b')' | b']' | b'}') => {
                    if Some(b) == close {
                        self.depth -= 1;
                        return i + 1;
                    }
                    i += 1;
                }
                b'<' if self.jsx_start(i) => i = self.element(i, scope),
                _ => i += 1,
            }
        }
        self.depth -= 1;
        self.bytes.len()
    }

    /// `<` begins JSX when a letter follows and the previous token cannot end
    /// an expression (so `a<b` and `Promise<void>` are not tags).
    fn jsx_start(&self, i: usize) -> bool {
        if !is_name_start(self.at(i + 1)) {
            return false;
        }
        let before = self.src[..i].trim_end();
        let Some(last) = before.bytes().last() else {
            return true;
        };
        if last.is_ascii_alphanumeric() || last == b'_' || last == b'$' {
            let word_start = before
                .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
                .map_or(0, |p| p + 1);
            return matches!(
                &before[word_start..],
                "return" | "yield" | "case" | "else" | "await"
            );
        }
        !matches!(last, b')' | b']')
    }

    /// Parses the element whose `<` is at `start`; returns the index to
    /// continue from. Not a real tag: returns `start + 1`.
    #[allow(clippy::too_many_lines)]
    fn element(&mut self, start: usize, scope: Scope) -> usize {
        let checkpoint = self.elements.len();
        let mut name_end = start + 1;
        while is_name_char(self.at(name_end)) {
            name_end += 1;
        }
        let name = self.src[start + 1..name_end].to_string();
        let index = self.elements.len();
        let parent = scope.parent;
        self.elements.push(Element {
            name,
            open: start..start,
            inner: start..start,
            parent,
            self_closing: false,
            children: Vec::new(),
            has_text: false,
        });
        let mut j = name_end;
        let own = Scope {
            parent,
            in_attr: true,
        };
        let open_end = loop {
            match self.at(j) {
                b'/' if self.at(j + 1) == b'>' => {
                    self.elements[index].open = start..j + 2;
                    self.elements[index].inner = j + 2..j + 2;
                    self.elements[index].self_closing = true;
                    self.register_child(index, scope);
                    return j + 2;
                }
                b'>' => break j + 1,
                b'{' => j = self.js(j + 1, Some(b'}'), own),
                b'\'' | b'"' => j = self.quoted(j),
                0 | b'<' | b';' => {
                    self.elements.truncate(checkpoint);
                    return start + 1;
                }
                _ => j += 1,
            }
        };
        self.elements[index].open = start..open_end;
        self.register_child(index, scope);
        let child_scope = Scope {
            parent: Some(index),
            in_attr: false,
        };
        let mut k = open_end;
        loop {
            match (self.at(k), self.at(k + 1)) {
                (0, _) => {
                    self.elements[index].inner = open_end..self.bytes.len();
                    return self.bytes.len();
                }
                (b'<', b'/') => {
                    let mut close_name_end = k + 2;
                    while is_name_char(self.at(close_name_end)) {
                        close_name_end += 1;
                    }
                    let close_name = &self.src[k + 2..close_name_end];
                    if close_name.is_empty() {
                        k = self.src[k..]
                            .find('>')
                            .map_or(self.bytes.len(), |n| k + n + 1);
                        continue;
                    }
                    self.elements[index].inner = open_end..k;
                    if close_name == self.elements[index].name {
                        return self.src[k..]
                            .find('>')
                            .map_or(self.bytes.len(), |n| k + n + 1);
                    }
                    return k;
                }
                (b'<', b'>') => k += 2,
                (b'<', c) if is_name_start(c) => {
                    let next = self.element(k, child_scope);
                    if next == k + 1 {
                        self.elements[index].has_text = true;
                    }
                    k = next;
                }
                (b'{', _) => {
                    let end = self.js(k + 1, Some(b'}'), child_scope);
                    let content = &self.src[k + 1..end.saturating_sub(1).max(k + 1)];
                    if renders_text(content) {
                        self.elements[index].has_text = true;
                    }
                    k = end;
                }
                (c, _) => {
                    if !c.is_ascii_whitespace() {
                        self.elements[index].has_text = true;
                    }
                    k += 1;
                }
            }
        }
    }

    fn register_child(&mut self, index: usize, scope: Scope) {
        if let Some(parent) = scope.parent
            && !scope.in_attr
        {
            self.elements[parent].children.push(index);
        }
    }
}

/// Whether the expression inside `{...}` can render visible text: empty
/// containers, comments, a lone element and `.map(...)` lists do not.
fn renders_text(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() || (trimmed.starts_with("/*") && trimmed.ends_with("*/")) {
        return false;
    }
    if trimmed.starts_with('<') {
        return false;
    }
    !crate::expr::ends_with_map_call(trimmed)
}

impl Markup {
    pub(crate) fn open_text<'a>(&self, src: &'a str, index: usize) -> &'a str {
        &src[self.elements[index].open.clone()]
    }

    pub(crate) fn inner_text<'a>(&self, src: &'a str, index: usize) -> &'a str {
        &src[self.elements[index].inner.clone()]
    }

    /// Ancestors from the nearest to the root.
    pub(crate) fn ancestors(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(self.elements[index].parent, |&p| self.elements[p].parent)
    }

    pub(crate) fn attrs(&self, src: &str, index: usize) -> Vec<Attr> {
        let element = &self.elements[index];
        let tag = &src[element.open.clone()];
        let bytes = tag.as_bytes();
        let mut attrs = Vec::new();
        let mut i = 1 + element.name.len();
        while i < bytes.len() {
            match bytes[i] {
                b'/' | b'>' => break,
                b'{' => i = skip_group(tag, i),
                b if is_name_start(b) => {
                    let name_start = i;
                    while i < bytes.len() && is_name_char(bytes[i]) {
                        i += 1;
                    }
                    let name = tag[name_start..i].to_string();
                    let mut k = i;
                    while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                        k += 1;
                    }
                    if bytes.get(k) == Some(&b'=') {
                        k += 1;
                        while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                            k += 1;
                        }
                        let value_start = k;
                        if matches!(bytes.get(k), Some(b'"' | b'\'' | b'{')) {
                            k = skip_group(tag, k);
                        } else {
                            while k < bytes.len()
                                && !bytes[k].is_ascii_whitespace()
                                && bytes[k] != b'>'
                            {
                                k += 1;
                            }
                        }
                        let base = element.open.start;
                        attrs.push(Attr {
                            name,
                            value: Some(base + value_start..base + k),
                        });
                        i = k;
                    } else {
                        attrs.push(Attr { name, value: None });
                    }
                }
                _ => i += 1,
            }
        }
        attrs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(markup: &Markup) -> Vec<&str> {
        markup.elements.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn nesting_children_and_ancestors_follow_the_tags() {
        let src = "const a = (\n  <Card>\n    <p>one</p>\n    <Row><Icon /></Row>\n    {ok && <span>x</span>}\n  </Card>\n);\n";
        let markup = parse(src);
        assert_eq!(names(&markup), vec!["Card", "p", "Row", "Icon", "span"]);
        assert_eq!(markup.elements[0].children, vec![1, 2, 4]);
        assert_eq!(markup.ancestors(3).collect::<Vec<_>>(), vec![2, 0]);
        assert_eq!(markup.inner_text(src, 1), "one");
        assert!(markup.elements[3].self_closing);
    }

    #[test]
    fn jsx_inside_an_attribute_is_not_a_child_but_keeps_its_ancestors() {
        let src = "<Menu render={<Button size=\"lg\" />}><Item /></Menu>";
        let markup = parse(src);
        assert_eq!(names(&markup), vec!["Menu", "Button", "Item"]);
        assert_eq!(markup.elements[0].children, vec![2]);
        assert_eq!(markup.elements[1].parent, None);
    }

    #[test]
    fn attributes_are_read_with_their_raw_values() {
        let src =
            "<Input a=\"x > y\" b={cn(\"p\", { q: '>' })} disabled c='z' {...rest} d=plain />";
        let markup = parse(src);
        let attrs = markup.attrs(src, 0);
        let shown: Vec<(&str, Option<&str>)> = attrs
            .iter()
            .map(|a| (a.name.as_str(), a.value.clone().map(|r| &src[r])))
            .collect();
        assert_eq!(
            shown,
            vec![
                ("a", Some("\"x > y\"")),
                ("b", Some("{cn(\"p\", { q: '>' })}")),
                ("disabled", None),
                ("c", Some("'z'")),
                ("d", Some("plain")),
            ]
        );
    }

    #[test]
    fn has_text_tells_text_from_empty_maps_and_lone_elements() {
        let src = "<a><b>text</b><c>{}</c><d>{/* note */}</d><e>{list.map((i) => <i key={i} />)}</e><f>{<g />}</f><h>{label}</h><i> </i><j>{ok && <k />}</j></a>";
        let markup = parse(src);
        let has_text = |name: &str| {
            markup
                .elements
                .iter()
                .find(|e| e.name == name)
                .map(|e| e.has_text)
        };
        assert_eq!(has_text("b"), Some(true));
        assert_eq!(has_text("c"), Some(false));
        assert_eq!(has_text("d"), Some(false));
        assert_eq!(
            has_text("e"),
            Some(false),
            "a .map list renders elements, not text"
        );
        assert_eq!(has_text("f"), Some(false));
        assert_eq!(has_text("h"), Some(true));
        assert_eq!(has_text("i"), Some(false));
        assert_eq!(
            has_text("j"),
            Some(true),
            "`cond && <el/>` is an expression like the original checker counts"
        );
    }

    #[test]
    fn unclosed_tags_and_deep_nesting_terminate() {
        let broken = "<div><span>never closed";
        assert_eq!(names(&parse(broken)), vec!["div", "span"]);
        let deep = format!("{}x{}", "<a>".repeat(500), "</a>".repeat(500));
        assert!(!parse(&deep).elements.is_empty());
        let braces = "{".repeat(5000);
        assert!(parse(&braces).elements.is_empty());
    }
}
