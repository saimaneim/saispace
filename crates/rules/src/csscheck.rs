//! Checks on CSS custom properties (design tokens): minimum sizes, line
//! heights, scales, contrast between color tokens and dangling `var()`s.

use crate::css::{Tokens, blend, contrast_ratio, eval_length, parse_color, var_refs};
use crate::ctx::{FileCtx, Hit, optional_regex, regex};
use crate::error::RulesResult;
use crate::model::{Check, ColorPair};
use regex::Regex;

pub(crate) enum CssSpec {
    Length {
        names: Regex,
        min_px: f64,
    },
    LineHeight {
        names: Regex,
        suffix: String,
        min_ratio: f64,
        skip_from_px: Option<f64>,
    },
    Scale {
        names: Regex,
        exclude: Option<Regex>,
        min_step_px: f64,
    },
    Contrast {
        pairs: Vec<ColorPair>,
    },
    Refs {
        names: Regex,
    },
}

impl CssSpec {
    pub(crate) fn new(rule: &str, check: &Check) -> RulesResult<Option<Self>> {
        Ok(Some(match check {
            Check::CssLength { names, min_px } => Self::Length {
                names: regex(rule, names)?,
                min_px: *min_px,
            },
            Check::CssLineHeight {
                names,
                suffix,
                min_ratio,
                skip_from_px,
            } => Self::LineHeight {
                names: regex(rule, names)?,
                suffix: suffix.clone(),
                min_ratio: *min_ratio,
                skip_from_px: *skip_from_px,
            },
            Check::CssScale {
                names,
                exclude,
                min_step_px,
            } => Self::Scale {
                names: regex(rule, names)?,
                exclude: optional_regex(rule, exclude.as_deref())?,
                min_step_px: *min_step_px,
            },
            Check::CssContrast { pairs } => Self::Contrast {
                pairs: pairs.clone(),
            },
            Check::CssRefs { names } => Self::Refs {
                names: regex(rule, names)?,
            },
            _ => return Ok(None),
        }))
    }

    pub(crate) fn hits(&self, ctx: &FileCtx<'_>) -> Vec<Hit> {
        let tokens = Tokens::parse(ctx.text);
        let at_line = |line: usize, snippet: String| Hit {
            line,
            column: 1,
            snippet,
        };
        let mut named: Vec<(&String, usize, &str)> = tokens
            .iter()
            .map(|(name, token)| (name, token.line, token.value.as_str()))
            .collect();
        named.sort_by_key(|(name, line, _)| (*line, (*name).clone()));
        match self {
            Self::Length { names, min_px } => named
                .iter()
                .filter(|(name, ..)| names.is_match(name))
                .filter_map(|(name, line, value)| {
                    let px = eval_length(value, &tokens)?;
                    (px < *min_px).then(|| at_line(*line, format!("--{name}: {value} = {px}px")))
                })
                .collect(),
            Self::LineHeight {
                names,
                suffix,
                min_ratio,
                skip_from_px,
            } => named
                .iter()
                .filter(|(name, ..)| names.is_match(name))
                .filter_map(|(name, _, value)| {
                    let px = eval_length(value, &tokens)?;
                    let line_height = tokens.get(&format!("{name}{suffix}"))?;
                    let ratio = eval_length(&line_height.value, &tokens)?;
                    let skipped = skip_from_px.is_some_and(|limit| px >= limit);
                    (ratio < *min_ratio && !skipped).then(|| {
                        at_line(
                            line_height.line,
                            format!("--{name}{suffix}: {ratio:.3}x of {px}px"),
                        )
                    })
                })
                .collect(),
            Self::Scale {
                names,
                exclude,
                min_step_px,
            } => scale_hits(&named, &tokens, names, exclude.as_ref(), *min_step_px)
                .into_iter()
                .map(|(line, snippet)| at_line(line, snippet))
                .collect(),
            Self::Contrast { pairs } => pairs
                .iter()
                .filter_map(|pair| contrast_hit(&tokens, pair).map(|(line, s)| at_line(line, s)))
                .collect(),
            Self::Refs { names } => named
                .iter()
                .filter(|(name, ..)| names.is_match(name))
                .flat_map(|(name, line, value)| {
                    var_refs(value)
                        .into_iter()
                        .filter(|reference| !tokens.contains(reference))
                        .map(move |reference| {
                            (
                                *line,
                                format!("--{name} uses var(--{reference}) that is not declared"),
                            )
                        })
                })
                .map(|(line, snippet)| at_line(line, snippet))
                .collect(),
        }
    }
}

fn scale_hits(
    entries: &[(&String, usize, &str)],
    tokens: &Tokens,
    names: &Regex,
    exclude: Option<&Regex>,
    min_step_px: f64,
) -> Vec<(usize, String)> {
    let mut hits = Vec::new();
    let mut steps: Vec<(&str, usize, f64)> = Vec::new();
    for (name, line, value) in entries {
        if !names.is_match(name) || exclude.is_some_and(|e| e.is_match(name)) {
            continue;
        }
        match eval_length(value, tokens) {
            Some(px) => steps.push((name.as_str(), *line, px)),
            None => hits.push((*line, format!("--{name}: cannot resolve '{value}' to px"))),
        }
    }
    steps.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(b.0)));
    for pair in steps.windows(2) {
        let ((prev, _, prev_px), (curr, line, curr_px)) = (pair[0], pair[1]);
        if (curr_px - prev_px).abs() < f64::EPSILON {
            hits.push((line, format!("--{prev} and --{curr} are both {curr_px}px")));
        } else if curr_px - prev_px < min_step_px {
            hits.push((
                line,
                format!(
                    "--{prev} to --{curr} differ by {}px (min {min_step_px}px)",
                    curr_px - prev_px
                ),
            ));
        }
    }
    hits
}

fn contrast_hit(tokens: &Tokens, pair: &ColorPair) -> Option<(usize, String)> {
    let read = |name: &str| {
        let token = tokens.get(name)?;
        let color = parse_color(&tokens.resolve(&token.value)?)?;
        Some((token.line, color))
    };
    let (Some((line, fg)), Some((_, bg))) = (read(&pair.fg), read(&pair.bg)) else {
        let line = tokens.get(&pair.fg).map_or(1, |t| t.line);
        return Some((
            line,
            format!("{}: cannot read --{} / --{}", pair.label, pair.fg, pair.bg),
        ));
    };
    let bg_rgb = blend(bg.rgb, [0, 0, 0], bg.alpha);
    let fg_rgb = blend(fg.rgb, bg_rgb, fg.alpha);
    let ratio = contrast_ratio(fg_rgb, bg_rgb);
    (ratio < pair.min).then(|| {
        (
            line,
            format!("{}: {ratio:.2}:1 (min {}:1)", pair.label, pair.min),
        )
    })
}
