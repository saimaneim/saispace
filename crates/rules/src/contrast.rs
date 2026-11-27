//! WCAG contrast of text/background utility classes. A class `text-<name>` or
//! `bg-<name>` is looked up as the custom property `--color-<name>` (then
//! `--<name>`) of the configured CSS files; `/NN` opacity and `[#hex]`
//! arbitrary values are understood.

use crate::css::{Color, Tokens, blend, contrast_ratio, hex, parse_color};
use crate::ctx::{Cache, FileCtx, Hit, optional_regex, regex};
use crate::error::RulesResult;
use crate::expr;
use crate::markup::{Markup, skip_group};
use crate::model::{ContrastCheck, ContrastUnits, SiblingBackground};
use regex::Regex;
use std::fs;

#[derive(Clone)]
struct Layer {
    rgb: [u8; 3],
    opacity: f64,
    source: String,
}

#[derive(Clone, Default)]
struct Layers {
    bgs: Vec<Layer>,
    fg: Option<Layer>,
    svg_fg: Option<Layer>,
}

#[derive(Clone, Default)]
struct Ctx {
    base: Layers,
    hover: Layers,
    active: Layers,
    has_hover: bool,
    has_active: bool,
    large: bool,
}

struct Found {
    ratio: f64,
    required: f64,
    bg: [u8; 3],
    bg_source: String,
    fg: [u8; 3],
    fg_source: String,
    label: String,
}

pub(crate) struct ContrastSpec {
    units: ContrastUnits,
    colors: Vec<String>,
    default_fg: [u8; 3],
    default_bg: [u8; 3],
    min: f64,
    min_large: f64,
    min_state: f64,
    large: Option<Regex>,
    large_size: Option<Regex>,
    bold: Option<Regex>,
    emphasis: Option<Regex>,
    min_emphasis: Option<f64>,
    skip_open: Option<Regex>,
    sibling: Option<(Regex, Regex, Vec<String>)>,
    call: String,
    group: String,
    skip_prefixes: Vec<String>,
    max_variants: usize,
}

impl ContrastSpec {
    pub(crate) fn new(rule: &str, check: &ContrastCheck) -> RulesResult<Self> {
        let color = |value: &str| {
            parse_color(value).map(|c| c.rgb).ok_or_else(|| {
                crate::error::RulesError::Invalid(
                    rule.to_string(),
                    format!("default color '{value}' is not a color"),
                )
            })
        };
        Ok(Self {
            units: check.units,
            colors: check.colors.clone(),
            default_fg: color(&check.default_fg)?,
            default_bg: color(&check.default_bg)?,
            min: check.min,
            min_large: check.min_large,
            min_state: check.min_state,
            large: optional_regex(rule, check.large.as_deref())?,
            large_size: optional_regex(rule, check.large_size.as_deref())?,
            bold: optional_regex(rule, check.bold.as_deref())?,
            emphasis: optional_regex(rule, check.emphasis.as_deref())?,
            min_emphasis: check.min_emphasis,
            skip_open: optional_regex(rule, check.skip_open.as_deref())?,
            sibling: check
                .sibling_background
                .as_ref()
                .map(|s: &SiblingBackground| {
                    Ok((
                        regex(rule, &s.when_fg)?,
                        regex(rule, &s.sibling)?,
                        s.apply.split_whitespace().map(str::to_string).collect(),
                    ))
                })
                .transpose()?,
            call: check.call.clone(),
            group: check.group.clone(),
            skip_prefixes: check.skip_prefixes.clone(),
            max_variants: check.max_variants.max(1),
        })
    }

    pub(crate) fn hits(&self, ctx: &FileCtx<'_>, rule_index: usize) -> Vec<Hit> {
        let Some(repo) = ctx.repo else {
            return Vec::new();
        };
        let Some(cell) = repo.cache(rule_index) else {
            return Vec::new();
        };
        let Cache::Colors(tokens) = cell.get_or_init(|| {
            let mut tokens = Tokens::default();
            for file in &self.colors {
                if let Ok(css) = fs::read_to_string(repo.root().join(file)) {
                    tokens.extend_from(&css);
                }
            }
            Cache::Colors(tokens)
        }) else {
            return Vec::new();
        };
        let resolver = Resolver { spec: self, tokens };
        match self.units {
            ContrastUnits::Jsx => resolver.jsx(ctx),
            ContrastUnits::Variants => resolver.variants(ctx),
        }
    }
}

struct Resolver<'a> {
    spec: &'a ContrastSpec,
    tokens: &'a Tokens,
}

fn report(ctx: &FileCtx<'_>, offset: usize, found: &Found) -> Hit {
    Hit::with_snippet(
        ctx,
        offset,
        format!(
            "{} {:.2}:1 (min {}:1) | text {} ({}) on bg {} ({})",
            found.label,
            found.ratio,
            found.required,
            found.fg_source,
            hex(found.fg),
            found.bg_source,
            hex(found.bg)
        ),
    )
}

fn text_fg(layers: &Layers) -> Option<&Layer> {
    layers.fg.as_ref()
}

fn icon_fg(layers: &Layers) -> Option<&Layer> {
    layers.svg_fg.as_ref()
}

fn composite_bg(layers: &[Layer], fallback: [u8; 3]) -> ([u8; 3], String) {
    layers
        .iter()
        .fold((fallback, "default".to_string()), |(rgb, _), layer| {
            (blend(layer.rgb, rgb, layer.opacity), layer.source.clone())
        })
}

fn composite_fg(fg: Option<&Layer>, bg: [u8; 3], fallback: [u8; 3]) -> ([u8; 3], String) {
    fg.map_or((fallback, "default".to_string()), |layer| {
        (blend(layer.rgb, bg, layer.opacity), layer.source.clone())
    })
}

impl Resolver<'_> {
    fn lookup(&self, name: &str) -> Option<Color> {
        [format!("color-{name}"), name.to_string()]
            .iter()
            .find_map(|key| {
                let token = self.tokens.get(key)?;
                parse_color(&self.tokens.resolve(&token.value)?)
            })
    }

    /// Color and opacity a class value (`primary/90`, `[#fff]`, `red-500`) stands for.
    fn color_of(&self, value: &str) -> Option<(Color, f64)> {
        let (base, opacity) = value
            .rsplit_once('/')
            .filter(|(_, pct)| !pct.is_empty() && pct.bytes().all(|b| b.is_ascii_digit()))
            .map_or((value, 1.0), |(base, pct)| {
                (base, pct.parse::<f64>().unwrap_or(100.0) / 100.0)
            });
        let color = if let Some(inner) = base.strip_prefix('[').and_then(|b| b.strip_suffix(']')) {
            parse_color(inner).filter(|_| inner.starts_with('#'))?
        } else {
            self.lookup(base)?
        };
        (color.alpha > 0.0).then_some((color, opacity))
    }

    fn is_large(&self, class: &str) -> bool {
        self.spec
            .large
            .as_ref()
            .is_some_and(|re| re.is_match(class))
    }

    fn detect_large(&self, tokens: &[String]) -> bool {
        let stripped: Vec<&str> = tokens
            .iter()
            .map(|t| t.rsplit(':').next().unwrap_or(t))
            .collect();
        stripped.iter().any(|c| self.is_large(c))
            || matches!((&self.spec.large_size, &self.spec.bold), (Some(size), Some(bold))
                if stripped.iter().any(|c| size.is_match(c)) && stripped.iter().any(|c| bold.is_match(c)))
    }

    fn apply(&self, tokens: &[String], ctx: &mut Ctx) {
        for raw in tokens {
            let mut parts: Vec<&str> = raw.split(':').collect();
            let class = parts.pop().unwrap_or_default();
            let prefixes = parts;
            if prefixes.iter().any(|p| {
                self.spec
                    .skip_prefixes
                    .iter()
                    .any(|skip| p.starts_with(skip.as_str()))
                    || (p.starts_with("[&") && !p.contains("svg"))
            }) {
                continue;
            }
            let is_hover = |p: &str| p.starts_with("hover") || p.starts_with("group-hover");
            let is_active = |p: &str| p.starts_with("active");
            let direct = prefixes.iter().all(|p| {
                is_hover(p)
                    || is_active(p)
                    || p.contains("svg")
                    || matches!(*p, "sm" | "md" | "lg" | "xl" | "2xl")
            });
            if !direct {
                continue;
            }
            if self.is_large(class) {
                ctx.large = true;
            }
            let Some((role, value)) = class.split_once('-') else {
                continue;
            };
            let is_bg = matches!(role, "bg" | "from");
            if !is_bg && role != "text" {
                continue;
            }
            let Some((color, opacity)) = self.color_of(value) else {
                continue;
            };
            let layer = Layer {
                rgb: color.rgb,
                opacity: opacity * color.alpha,
                source: raw.clone(),
            };
            let active = prefixes.iter().any(|p| is_active(p));
            let hover = prefixes.iter().any(|p| is_hover(p));
            if active {
                ctx.has_active = true;
            } else if hover {
                ctx.has_hover = true;
            }
            let target = if active {
                &mut ctx.active
            } else if hover {
                &mut ctx.hover
            } else {
                &mut ctx.base
            };
            if !is_bg && prefixes.iter().any(|p| p.contains("svg")) {
                target.svg_fg = Some(layer);
            } else if is_bg {
                target.bgs.push(layer);
            } else {
                target.fg = Some(layer);
            }
        }
    }

    fn pair(
        &self,
        layers: &Layers,
        fallback: &Layers,
        use_fg: fn(&Layers) -> Option<&Layer>,
    ) -> ([u8; 3], String, [u8; 3], String) {
        let bgs = if layers.bgs.is_empty() {
            &fallback.bgs
        } else {
            &layers.bgs
        };
        let (bg, bg_source) = composite_bg(bgs, self.spec.default_bg);
        let fg_layer = use_fg(layers).or_else(|| use_fg(fallback));
        let (fg, fg_source) = composite_fg(fg_layer, bg, self.spec.default_fg);
        (bg, bg_source, fg, fg_source)
    }

    fn found(
        label: &str,
        (bg, bg_source, fg, fg_source): ([u8; 3], String, [u8; 3], String),
        required: f64,
    ) -> Option<Found> {
        let ratio = contrast_ratio(bg, fg);
        (ratio < required).then(|| Found {
            ratio,
            required,
            bg,
            bg_source,
            fg,
            fg_source,
            label: label.to_string(),
        })
    }

    fn jsx(&self, ctx: &FileCtx<'_>) -> Vec<Hit> {
        if !ctx.text.contains('<') {
            return Vec::new();
        }
        let markup = ctx.markup();
        let mut hits = Vec::new();
        for index in 0..markup.elements.len() {
            let element = &markup.elements[index];
            if !element.has_text || element.self_closing {
                continue;
            }
            let mut chain: Vec<usize> = markup.ancestors(index).collect();
            if self.spec.skip_open.as_ref().is_some_and(|re| {
                re.is_match(markup.open_text(ctx.text, index))
                    || chain
                        .iter()
                        .any(|&a| re.is_match(markup.open_text(ctx.text, a)))
            }) {
                continue;
            }
            chain.reverse();
            chain.push(index);
            let mut variants = self.variants_for(ctx.text, markup, &chain);
            self.apply_sibling_background(ctx.text, markup, index, &mut variants);
            let required = if variants.iter().any(|v| v.large) {
                self.spec.min_large
            } else {
                self.spec.min
            };
            let label = format!("<{}>", element.name);
            let worst = variants
                .iter()
                .flat_map(|v| self.jsx_findings(v, &label, required))
                .min_by(|a, b| a.ratio.total_cmp(&b.ratio));
            if let Some(found) = worst {
                hits.push(report(ctx, element.open.start, &found));
            }
        }
        hits
    }

    fn apply_sibling_background(
        &self,
        src: &str,
        markup: &Markup,
        index: usize,
        variants: &mut [Ctx],
    ) {
        let Some((when_fg, sibling, apply)) = &self.spec.sibling else {
            return;
        };
        let Some(parent) = markup.elements[index].parent else {
            return;
        };
        let has_sibling = markup.elements[parent]
            .children
            .iter()
            .filter(|&&c| c != index)
            .flat_map(|&c| class_alternatives(src, markup, c))
            .flatten()
            .any(|token| sibling.is_match(&token));
        if !has_sibling {
            return;
        }
        for variant in variants.iter_mut() {
            if variant
                .base
                .fg
                .as_ref()
                .is_some_and(|fg| when_fg.is_match(&fg.source))
            {
                self.apply(apply, variant);
            }
        }
    }

    fn variants_for(&self, src: &str, markup: &Markup, chain: &[usize]) -> Vec<Ctx> {
        let mut variants = vec![Ctx::default()];
        for &ancestor in chain {
            let alternatives = class_alternatives(src, markup, ancestor);
            if alternatives.is_empty() {
                continue;
            }
            let mut next = Vec::new();
            'outer: for variant in &variants {
                for tokens in &alternatives {
                    let mut clone = variant.clone();
                    self.apply(tokens, &mut clone);
                    if self.detect_large(tokens) {
                        clone.large = true;
                    }
                    next.push(clone);
                    if next.len() >= self.spec.max_variants {
                        break 'outer;
                    }
                }
            }
            variants = next;
            if variants.len() >= self.spec.max_variants {
                break;
            }
        }
        variants
    }

    fn jsx_findings(&self, ctx: &Ctx, label: &str, required: f64) -> Vec<Found> {
        let mut found = Vec::new();
        let base = &ctx.base;
        found.extend(Self::found(label, self.pair(base, base, text_fg), required));
        let state_min = self.spec.min_state;
        if ctx.has_hover {
            let mut pair = self.pair(&ctx.hover, base, text_fg);
            pair.1 = format!("{} [hover]", pair.1);
            pair.3 = format!("{} [hover]", pair.3);
            found.extend(Self::found(label, pair, state_min));
        }
        if ctx.has_active {
            let mut pair = self.pair(&ctx.active, base, text_fg);
            pair.1 = format!("{} [active]", pair.1);
            pair.3 = format!("{} [active]", pair.3);
            found.extend(Self::found(label, pair, state_min));
        }
        found
    }

    fn variants(&self, ctx: &FileCtx<'_>) -> Vec<Hit> {
        let call = format!("{}(", self.spec.call);
        let mut hits = Vec::new();
        let mut from = 0;
        while let Some(at) = ctx.text[from..].find(&call) {
            let start = from + at;
            from = start + call.len();
            let before = &ctx.text[..start];
            if before
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '.')
            {
                continue;
            }
            let paren = start + call.len() - 1;
            let end = skip_group(ctx.text, paren);
            let inner = &ctx.text[paren + 1..end.saturating_sub(1).max(paren + 1)];
            let name = declared_name(before).unwrap_or_else(|| self.spec.call.clone());
            for (variant, tokens) in self.call_variants(inner, &name) {
                for found in self.evaluate_variant(&tokens, &variant) {
                    hits.push(report(ctx, start, &found));
                }
            }
        }
        hits
    }

    /// `(label, tokens)` for each variant of a `cva(base, config)` call.
    fn call_variants(&self, args: &str, name: &str) -> Vec<(String, Vec<String>)> {
        let args = expr::split_commas(args);
        let base: Vec<String> = args
            .first()
            .map_or_else(Vec::new, |a| expr::literal_strings(&expr::parse(a)))
            .iter()
            .flat_map(|s| s.split_whitespace().map(str::to_string))
            .collect();
        let alone = || vec![(format!("<{name}>"), base.clone())];
        let Some(config) = args.get(1).and_then(|c| expr::object_entries(c)) else {
            return alone();
        };
        let Some((_, variants)) = config.iter().find(|(k, _)| k == "variants") else {
            return alone();
        };
        let Some(groups) = expr::object_entries(variants) else {
            return alone();
        };
        let Some((_, entries)) = groups.iter().find(|(k, _)| *k == self.spec.group) else {
            return Vec::new();
        };
        expr::object_entries(entries)
            .unwrap_or_default()
            .into_iter()
            .map(|(variant, value)| {
                let mut tokens = base.clone();
                tokens.extend(
                    expr::literal_strings(&expr::parse(value))
                        .iter()
                        .flat_map(|s| s.split_whitespace().map(str::to_string)),
                );
                (format!("<{name}.{variant}>"), tokens)
            })
            .collect()
    }

    fn evaluate_variant(&self, tokens: &[String], label: &str) -> Vec<Found> {
        let mut ctx = Ctx::default();
        self.apply(tokens, &mut ctx);
        if self.detect_large(tokens) {
            ctx.large = true;
        }
        let emphasized = ctx.large
            || self
                .spec
                .emphasis
                .as_ref()
                .is_some_and(|re| tokens.iter().any(|t| re.is_match(t)));
        let required = if emphasized {
            self.spec.min_emphasis.unwrap_or(self.spec.min)
        } else {
            self.spec.min
        };
        let state_min = self.spec.min_state;
        let base = &ctx.base;
        let mut found = Vec::new();
        found.extend(Self::found(label, self.pair(base, base, text_fg), required));
        if base.svg_fg.is_some() {
            found.extend(Self::found(
                &format!("{label} [icon]"),
                self.pair(base, base, icon_fg),
                state_min,
            ));
        }
        if ctx.has_hover {
            found.extend(Self::found(
                &format!("{label} [hover]"),
                self.pair(&ctx.hover, base, text_fg),
                state_min,
            ));
            if ctx.hover.svg_fg.is_some() || base.svg_fg.is_some() {
                found.extend(Self::found(
                    &format!("{label} [hover:icon]"),
                    self.pair(&ctx.hover, base, icon_fg),
                    state_min,
                ));
            }
        }
        if ctx.has_active {
            found.extend(Self::found(
                &format!("{label} [active]"),
                self.pair(&ctx.active, base, text_fg),
                state_min,
            ));
            if ctx.active.svg_fg.is_some() || base.svg_fg.is_some() {
                found.extend(Self::found(
                    &format!("{label} [active:icon]"),
                    self.pair(&ctx.active, base, icon_fg),
                    state_min,
                ));
            }
        }
        found
    }
}

/// Class token lists the `className` of an element can produce.
fn class_alternatives(src: &str, markup: &Markup, index: usize) -> Vec<Vec<String>> {
    let Some(range) = markup
        .attrs(src, index)
        .into_iter()
        .find(|a| a.name == "className" || a.name == "class")
        .and_then(|a| a.value)
    else {
        return Vec::new();
    };
    let raw = src[range].trim();
    if let Some(inner) = raw.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        return expr::alternatives(&expr::parse(inner));
    }
    let text = raw.trim_matches(['"', '\'']);
    vec![text.split_whitespace().map(str::to_string).collect()]
}

/// `const name = ` right before a call, if any.
fn declared_name(before: &str) -> Option<String> {
    let trimmed = before.trim_end().strip_suffix('=')?.trim_end();
    let start = trimmed
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
        .map_or(0, |p| p + 1);
    let name = &trimmed[start..];
    (!name.is_empty()).then(|| name.to_string())
}
