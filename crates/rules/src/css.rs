//! CSS custom properties: parsing `--name: value;` declarations, resolving
//! `var()` chains, evaluating lengths (`px`, `rem`, `calc`) and reading colors.

use std::collections::HashMap;

const MAX_RESOLVE_DEPTH: usize = 16;
const REM_PX: f64 = 16.0;

#[derive(Debug, Clone)]
pub(crate) struct Token {
    pub value: String,
    pub line: usize,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Tokens {
    map: HashMap<String, Token>,
}

impl Tokens {
    pub(crate) fn get(&self, name: &str) -> Option<&Token> {
        self.map.get(name)
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&String, &Token)> {
        self.map.iter()
    }

    /// Adds every declaration of `css`; a later declaration of the same name wins.
    pub(crate) fn extend_from(&mut self, css: &str) {
        let clean = strip_comments(css);
        let bytes = clean.as_bytes();
        let mut line = 1;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\n' {
                line += 1;
            }
            let starts_name = bytes[i] == b'-'
                && bytes.get(i + 1) == Some(&b'-')
                && (i == 0 || !is_ident(bytes[i - 1]));
            if !starts_name {
                i += 1;
                continue;
            }
            let name_start = i + 2;
            let mut name_end = name_start;
            while name_end < bytes.len() && is_ident(bytes[name_end]) {
                name_end += 1;
            }
            let mut colon = name_end;
            while colon < bytes.len() && bytes[colon].is_ascii_whitespace() {
                colon += 1;
            }
            if name_end == name_start || bytes.get(colon) != Some(&b':') {
                i = name_end.max(i + 1);
                continue;
            }
            let value_start = colon + 1;
            let Some(offset) = clean[value_start..].find([';', '{', '}']) else {
                break;
            };
            let value_end = value_start + offset;
            if bytes[value_end] == b';' {
                self.map.insert(
                    clean[name_start..name_end].to_string(),
                    Token {
                        value: clean[value_start..value_end]
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" "),
                        line,
                    },
                );
            }
            line += clean[i..value_end].matches('\n').count();
            i = value_end;
        }
    }

    pub(crate) fn parse(css: &str) -> Self {
        let mut tokens = Self::default();
        tokens.extend_from(css);
        tokens
    }

    /// Follows `var(--x)` references until a concrete value (or a loop).
    pub(crate) fn resolve(&self, value: &str) -> Option<String> {
        let mut current = value.trim().to_string();
        for _ in 0..MAX_RESOLVE_DEPTH {
            let Some(name) = whole_var(&current) else {
                return Some(current);
            };
            current.clone_from(&self.map.get(name)?.value);
        }
        None
    }
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after.find("*/").map_or(after.len(), |e| e + 2);
        out.extend(
            after[..end.min(after.len())]
                .chars()
                .map(|c| if c == '\n' { '\n' } else { ' ' }),
        );
        out.push_str("  ");
        rest = &after[end.min(after.len())..];
    }
    out.push_str(rest);
    out
}

/// `var(--name)` or `var(--name, fallback)` as the whole value: the name.
fn whole_var(value: &str) -> Option<&str> {
    let inner = value.trim().strip_prefix("var(")?.strip_suffix(')')?;
    let name = inner.split(',').next()?.trim();
    name.strip_prefix("--")
}

/// Names (without `--`) of every `var(--name ...)` in `value`.
pub(crate) fn var_refs(value: &str) -> Vec<&str> {
    let mut refs = Vec::new();
    let mut rest = value;
    while let Some(at) = rest.find("var(") {
        let after = rest[at + 4..].trim_start();
        if let Some(name) = after.strip_prefix("--") {
            let end = name.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'));
            refs.push(&name[..end.unwrap_or(name.len())]);
        }
        rest = &rest[at + 4..];
    }
    refs
}

/// Evaluates a CSS length or number: `0.8125rem`, `12px`, `1.5`,
/// `calc(1.3 / 0.8125)`, `calc(var(--radius) - 4px)`, `var(--x)`. `rem` is 16px.
pub(crate) fn eval_length(value: &str, tokens: &Tokens) -> Option<f64> {
    let mut parser = LengthParser {
        chars: value.trim().as_bytes(),
        at: 0,
        tokens,
        depth: 0,
    };
    let result = parser.expr()?;
    parser.skip_ws();
    (parser.at == parser.chars.len()).then_some(result)
}

struct LengthParser<'a> {
    chars: &'a [u8],
    at: usize,
    tokens: &'a Tokens,
    depth: usize,
}

impl LengthParser<'_> {
    fn skip_ws(&mut self) {
        while self.chars.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        self.skip_ws();
        if self.chars.get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expr(&mut self) -> Option<f64> {
        let mut total = self.term()?;
        loop {
            if self.eat(b'+') {
                total += self.term()?;
            } else if self.eat(b'-') {
                total -= self.term()?;
            } else {
                return Some(total);
            }
        }
    }

    fn term(&mut self) -> Option<f64> {
        let mut total = self.factor()?;
        loop {
            if self.eat(b'*') {
                total *= self.factor()?;
            } else if self.eat(b'/') {
                let divisor = self.factor()?;
                if divisor == 0.0 {
                    return None;
                }
                total /= divisor;
            } else {
                return Some(total);
            }
        }
    }

    fn word(&mut self) -> &str {
        let start = self.at;
        while self
            .chars
            .get(self.at)
            .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'%'))
        {
            self.at += 1;
        }
        std::str::from_utf8(&self.chars[start..self.at]).unwrap_or_default()
    }

    fn factor(&mut self) -> Option<f64> {
        self.skip_ws();
        if self.eat(b'(') {
            let inner = self.expr()?;
            return self.eat(b')').then_some(inner);
        }
        let word = self.word().to_string();
        if self.chars.get(self.at) == Some(&b'(') {
            self.at += 1;
            return match word.as_str() {
                "calc" => {
                    let inner = self.expr()?;
                    self.eat(b')').then_some(inner)
                }
                "var" => self.var(),
                _ => None,
            };
        }
        number_with_unit(&word)
    }

    fn var(&mut self) -> Option<f64> {
        self.skip_ws();
        let start = self.at;
        let mut depth = 1usize;
        while let Some(&b) = self.chars.get(self.at) {
            match b {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            self.at += 1;
        }
        let inner = std::str::from_utf8(&self.chars[start..self.at]).ok()?;
        self.at += 1;
        let name = inner.split(',').next()?.trim().strip_prefix("--")?;
        if self.depth >= MAX_RESOLVE_DEPTH {
            return None;
        }
        let raw = self.tokens.get(name)?.value.clone();
        let mut nested = LengthParser {
            chars: raw.as_bytes(),
            at: 0,
            tokens: self.tokens,
            depth: self.depth + 1,
        };
        let value = nested.expr()?;
        nested.skip_ws();
        (nested.at == nested.chars.len()).then_some(value)
    }
}

fn number_with_unit(word: &str) -> Option<f64> {
    let split = word
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(word.len());
    let number: f64 = word[..split].parse().ok()?;
    match &word[split..] {
        "" | "px" => Some(number),
        "rem" | "em" => Some(number * REM_PX),
        _ => None,
    }
}

/// A color as 0-255 channels plus alpha in 0..=1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Color {
    pub rgb: [u8; 3],
    pub alpha: f64,
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn channel(value: f64) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> [u8; 3] {
    let hue = hue.rem_euclid(360.0);
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let second = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let offset = lightness - chroma / 2.0;
    let (red, green, blue) = match hue {
        h if h < 60.0 => (chroma, second, 0.0),
        h if h < 120.0 => (second, chroma, 0.0),
        h if h < 180.0 => (0.0, chroma, second),
        h if h < 240.0 => (0.0, second, chroma),
        h if h < 300.0 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    [
        channel((red + offset) * 255.0),
        channel((green + offset) * 255.0),
        channel((blue + offset) * 255.0),
    ]
}

fn percent_or_number(part: &str) -> Option<f64> {
    part.strip_suffix('%').map_or_else(
        || part.parse().ok(),
        |p| p.parse::<f64>().ok().map(|v| v / 100.0),
    )
}

fn function_args(value: &str) -> Option<(&str, Vec<&str>)> {
    let open = value.find('(')?;
    let inner = value[open + 1..].strip_suffix(')')?;
    let name = value[..open].trim();
    let args = inner
        .split(|c: char| c.is_whitespace() || c == ',' || c == '/')
        .filter(|p| !p.is_empty())
        .collect();
    Some((name, args))
}

pub(crate) fn parse_color(value: &str) -> Option<Color> {
    let v = value.trim().to_ascii_lowercase();
    if let Some(hex) = v.strip_prefix('#') {
        return parse_hex(hex);
    }
    match v.as_str() {
        "white" => {
            return Some(Color {
                rgb: [255; 3],
                alpha: 1.0,
            });
        }
        "black" => {
            return Some(Color {
                rgb: [0; 3],
                alpha: 1.0,
            });
        }
        "transparent" => {
            return Some(Color {
                rgb: [0; 3],
                alpha: 0.0,
            });
        }
        _ => {}
    }
    let (name, args) = function_args(&v)?;
    let alpha = args
        .get(3)
        .and_then(|a| percent_or_number(a))
        .unwrap_or(1.0);
    match name {
        "hsl" | "hsla" if args.len() >= 3 => {
            let hue: f64 = args[0].trim_end_matches("deg").parse().ok()?;
            let saturation = percent_or_number(args[1])?;
            let lightness = percent_or_number(args[2])?;
            Some(Color {
                rgb: hsl_to_rgb(hue, saturation, lightness),
                alpha,
            })
        }
        "rgb" | "rgba" if args.len() >= 3 => {
            let to_channel = |part: &str| {
                part.strip_suffix('%')
                    .map_or_else(
                        || part.parse().ok(),
                        |p| p.parse::<f64>().ok().map(|v| v * 2.55),
                    )
                    .map(channel)
            };
            Some(Color {
                rgb: [
                    to_channel(args[0])?,
                    to_channel(args[1])?,
                    to_channel(args[2])?,
                ],
                alpha,
            })
        }
        _ => None,
    }
}

fn parse_hex(hex: &str) -> Option<Color> {
    if !hex.is_ascii() {
        return None;
    }
    let digit = |s: &str| u8::from_str_radix(s, 16).ok();
    let double = |c: char| digit(&format!("{c}{c}"));
    let chars: Vec<char> = hex.chars().collect();
    let (rgb, alpha) = match chars.len() {
        3 | 4 => (
            [double(chars[0])?, double(chars[1])?, double(chars[2])?],
            match chars.get(3) {
                Some(&c) => Some(double(c)?),
                None => None,
            },
        ),
        6 | 8 => (
            [digit(&hex[0..2])?, digit(&hex[2..4])?, digit(&hex[4..6])?],
            (hex.len() == 8).then(|| digit(&hex[6..8])).flatten(),
        ),
        _ => return None,
    };
    Some(Color {
        rgb,
        alpha: alpha.map_or(1.0, |a| f64::from(a) / 255.0),
    })
}

fn linear(channel: u8) -> f64 {
    let s = f64::from(channel) / 255.0;
    if s <= 0.03928 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

pub(crate) fn luminance(rgb: [u8; 3]) -> f64 {
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
}

/// WCAG 2 contrast ratio between two opaque colors (1..=21).
pub(crate) fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `top` laid over `bottom` at `opacity`, rounded per channel.
pub(crate) fn blend(top: [u8; 3], bottom: [u8; 3], opacity: f64) -> [u8; 3] {
    if opacity >= 1.0 {
        return top;
    }
    let mix = |t: u8, b: u8| channel(f64::from(t) * opacity + f64::from(b) * (1.0 - opacity));
    [
        mix(top[0], bottom[0]),
        mix(top[1], bottom[1]),
        mix(top[2], bottom[2]),
    ]
}

pub(crate) fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_ratios_match_the_wcag_reference_values() {
        assert!((contrast_ratio([0; 3], [255; 3]) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio([255; 3], [255; 3]) - 1.0).abs() < 1e-9);
        let grey_on_white = contrast_ratio([0x76; 3], [255; 3]);
        assert!(
            (grey_on_white - 4.54).abs() < 0.01,
            "#767676 is the lightest grey that passes AA on white, got {grey_on_white}"
        );
    }

    #[test]
    fn hsl_hex_and_rgb_notations_agree() {
        let hsl = parse_color("hsl(0 72% 64%)").unwrap();
        assert_eq!(hsl.rgb, [229, 97, 97]);
        assert_eq!(parse_color("hsla(0, 0%, 100%, 0.25)").unwrap().alpha, 0.25);
        assert_eq!(parse_color("#0b0c0f").unwrap().rgb, [11, 12, 15]);
        assert_eq!(parse_color("#fff").unwrap().rgb, [255; 3]);
        assert_eq!(parse_color("#ffffff80").unwrap().alpha, 128.0 / 255.0);
        assert_eq!(parse_color("rgb(10 20 30 / 50%)").unwrap().alpha, 0.5);
        assert!(parse_color("oklch(0.5 0.1 20)").is_none());
        assert!(parse_color("#12345").is_none());
        assert!(parse_color("#é12").is_none());
    }

    #[test]
    fn blending_rounds_each_channel_like_the_original_checker() {
        assert_eq!(blend([0; 3], [255; 3], 0.5), [128; 3]);
        assert_eq!(blend([10, 20, 30], [200; 3], 1.0), [10, 20, 30]);
    }

    #[test]
    fn lengths_follow_calc_precedence_vars_and_reject_cycles() {
        let tokens = Tokens::parse(
            ":root { --radius: 0.625rem; --a: var(--b); --b: var(--a); --gap: calc(var(--radius) + 2px * 3); }\n.x { --ratio: calc(1.3 / 0.8125); }",
        );
        assert_eq!(eval_length("calc(var(--radius) - 4px)", &tokens), Some(6.0));
        assert_eq!(eval_length("var(--gap)", &tokens), Some(16.0));
        assert_eq!(eval_length("var(--ratio)", &tokens), Some(1.6));
        assert_eq!(
            eval_length("var(--a)", &tokens),
            None,
            "a loop never resolves"
        );
        assert_eq!(eval_length("calc(1 / 0)", &tokens), None);
        assert_eq!(eval_length("50%", &tokens), None);
        assert_eq!(eval_length("calc(1px +", &tokens), None);
    }

    #[test]
    fn tokens_keep_line_numbers_skip_comments_and_resolve_chains() {
        let tokens = Tokens::parse(
            ":root {\n  --a: var(--b);\n  /* --ghost: 1px; */\n  --b:\n    var(--c);\n  --c: red;\n}\n",
        );
        assert_eq!(tokens.get("a").unwrap().line, 2);
        assert_eq!(tokens.get("c").unwrap().line, 6);
        assert!(!tokens.contains("ghost"));
        assert_eq!(tokens.get("b").unwrap().value, "var(--c)");
        assert_eq!(tokens.resolve("var(--a)").as_deref(), Some("red"));
        assert_eq!(tokens.resolve("var(--nope)"), None);
        assert_eq!(var_refs("calc(var(--x) + var( --y, 1px))"), vec!["x", "y"]);
    }
}
