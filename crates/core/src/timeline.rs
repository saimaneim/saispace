//! Builds a project's readable history from the plan, recorded decisions and
//! the git log. Everything is deterministic: no network, no AI.

use crate::db::memory_graph::short_id;
use crate::git_history::{GitCommit, GitHistory, GitTag};
use crate::models::{
    Goal, GoalStatus, MemoryKind, Task, TaskStatus, TimelineDay, TimelineItem, TimelineKind,
};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// Commits read from git per request; older ones stay out of the timeline.
pub const COMMIT_LIMIT: usize = 300;
const MAX_FILES: usize = 40;
const MAX_DETAIL_CHARS: usize = 800;
/// Shared significant words needed to tie a commit to a goal by its text.
const MIN_SHARED_WORDS: usize = 2;

/// A recorded memory worth showing next to the changes (decisions and
/// architecture notes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineMemory {
    pub kind: MemoryKind,
    pub title: String,
    pub created_at: DateTime<Utc>,
}

/// Everything the timeline is built from.
#[derive(Debug, Clone, Default)]
pub struct TimelineSources {
    pub goals: Vec<Goal>,
    pub tasks: Vec<Task>,
    pub memories: Vec<TimelineMemory>,
    pub git: GitHistory,
}

/// A commit subject split into its Conventional Commit parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitTitle {
    pub kind: Option<String>,
    pub scope: Option<String>,
    pub breaking: bool,
    pub title: String,
    pub pr: Option<u32>,
}

/// Plain Spanish label of a Conventional Commit type.
#[must_use]
pub fn commit_label(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "feat" | "feature" => "Nuevo",
        "fix" | "hotfix" => "Arreglo",
        "perf" => "Más rápido",
        "refactor" => "Mejora interna",
        "docs" => "Documentación",
        "test" | "tests" => "Pruebas",
        "style" => "Estilo",
        "build" | "ci" | "chore" | "deps" => "Mantenimiento",
        "revert" => "Revertido",
        "security" => "Seguridad",
        _ => return None,
    })
}

fn split_trailing_pr(text: &str) -> (&str, Option<u32>) {
    let trimmed = text.trim_end();
    if let Some(open) = trimmed.rfind("(#")
        && trimmed.ends_with(')')
        && let Ok(number) = trimmed[open + 2..trimmed.len() - 1].parse::<u32>()
    {
        return (trimmed[..open].trim_end(), Some(number));
    }
    (trimmed, None)
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// Splits `type(scope)!: subject (#12)` into its parts. Subjects without a
/// Conventional Commit prefix keep their whole text as the title.
#[must_use]
pub fn parse_commit_title(subject: &str) -> CommitTitle {
    let (text, pr) = split_trailing_pr(subject.trim());
    let prefix = text.split_once(": ").filter(|(head, _)| {
        !head.is_empty()
            && head
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "()!-_/,. ".contains(c))
            && head.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && !head.contains(' ')
    });
    let Some((head, rest)) = prefix else {
        return CommitTitle {
            kind: None,
            scope: None,
            breaking: false,
            title: capitalize(text),
            pr,
        };
    };
    let breaking = head.ends_with('!');
    let head = head.trim_end_matches('!');
    let (kind, scope) = match head.split_once('(') {
        Some((kind, scope)) => (kind, Some(scope.trim_end_matches(')').to_string())),
        None => (head, None),
    };
    CommitTitle {
        kind: Some(kind.to_ascii_lowercase()),
        scope: scope.filter(|s| !s.is_empty()),
        breaking,
        title: capitalize(rest.trim()),
        pr,
    }
}

fn fold_char(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        other => other,
    }
}

const STOPWORDS: &[&str] = &[
    "para", "como", "esta", "este", "estos", "desde", "sobre", "entre", "cada", "todo", "todos",
    "pero", "tras", "hacer", "nuevo", "nueva", "with", "from", "into", "that", "this", "when",
    "then", "them", "also", "make", "more", "only", "each", "they", "have", "will",
];

/// Lowercased, accent-folded words of 4+ letters, with a plural `s` removed,
/// so "Exportar facturas" and "export factura" share "factura".
fn significant_words(text: &str) -> BTreeSet<String> {
    text.to_lowercase()
        .chars()
        .map(fold_char)
        .collect::<String>()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() >= 4 && !word.chars().all(|c| c.is_ascii_digit()))
        .filter(|word| !STOPWORDS.contains(word))
        .map(|word| {
            if word.len() > 4 && word.ends_with('s') {
                word[..word.len() - 1].to_string()
            } else {
                word.to_string()
            }
        })
        .collect()
}

struct GoalMatcher<'a> {
    goal: &'a Goal,
    ids: Vec<String>,
    word_sets: Vec<BTreeSet<String>>,
}

fn goal_matchers<'a>(goals: &'a [Goal], tasks: &[Task]) -> Vec<GoalMatcher<'a>> {
    goals
        .iter()
        .filter(|goal| goal.status != GoalStatus::Archived)
        .map(|goal| {
            let goal_tasks: Vec<&Task> = tasks
                .iter()
                .filter(|task| task.goal_id == Some(goal.id))
                .collect();
            let mut ids = vec![short_id(goal.id)];
            ids.extend(goal_tasks.iter().map(|task| short_id(task.id)));
            let mut word_sets = vec![significant_words(&goal.title)];
            word_sets.extend(goal_tasks.iter().map(|task| significant_words(&task.title)));
            GoalMatcher {
                goal,
                ids,
                word_sets,
            }
        })
        .collect()
}

/// The goal a commit most likely advanced: an explicit goal or task short id
/// in the message wins; otherwise the goal (or one of its tasks) sharing the
/// most significant words with the subject, if at least two.
fn match_goal<'a>(matchers: &[GoalMatcher<'a>], message: &str, subject: &str) -> Option<&'a Goal> {
    let lowered = message.to_lowercase();
    if let Some(explicit) = matchers
        .iter()
        .find(|m| m.ids.iter().any(|id| lowered.contains(id.as_str())))
    {
        return Some(explicit.goal);
    }
    let words = significant_words(subject);
    matchers
        .iter()
        .filter_map(|m| {
            let shared = m
                .word_sets
                .iter()
                .map(|set| set.intersection(&words).count())
                .max()
                .unwrap_or(0);
            (shared >= MIN_SHARED_WORDS).then_some((shared, m.goal))
        })
        .max_by(|(a, ga), (b, gb)| a.cmp(b).then(gb.position.cmp(&ga.position)))
        .map(|(_, goal)| goal)
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push('…');
    cut
}

const TRAILERS: &[&str] = &["co-authored-by:", "signed-off-by:", "reviewed-by:"];

/// Commit body without blank lines, trailers or `---` rules: squash bodies
/// list one change per line and read better compact.
fn clean_body(body: &str) -> Option<String> {
    let lines: Vec<&str> = body
        .lines()
        .map(str::trim_end)
        .filter(|line| {
            let trimmed = line.trim();
            let lowered = trimmed.to_lowercase();
            !trimmed.is_empty()
                && !trimmed.chars().all(|c| c == '-')
                && !TRAILERS.iter().any(|t| lowered.starts_with(t))
        })
        .collect();
    (!lines.is_empty()).then(|| truncate_chars(&lines.join("\n"), MAX_DETAIL_CHARS))
}

fn item(kind: TimelineKind, at: DateTime<Utc>, title: String) -> TimelineItem {
    TimelineItem {
        kind,
        at,
        title,
        detail: None,
        label: None,
        goal: None,
        link: None,
        hash: None,
        pr: None,
        files: Vec::new(),
        file_count: 0,
    }
}

/// `Merge pull request #12 from owner/branch` keeps the PR title from the
/// body; `Merge branch 'x' into y` and remote-tracking merges are syncs, not
/// changes, and are skipped.
fn commit_item(
    commit: &GitCommit,
    web_url: Option<&str>,
    matchers: &[GoalMatcher<'_>],
) -> Option<TimelineItem> {
    let subject = commit.subject.as_str();
    let mut body = commit.body.as_str();
    let (parsed, merged_pr) = if let Some(rest) = subject.strip_prefix("Merge pull request #") {
        let number = rest.split_whitespace().next().and_then(|n| n.parse().ok());
        let (first_line, remaining) = body.split_once('\n').unwrap_or((body, ""));
        let title_source = if first_line.trim().is_empty() {
            rest.split(" from ").nth(1).unwrap_or(rest)
        } else {
            body = remaining;
            first_line
        };
        (parse_commit_title(title_source), number)
    } else if subject.starts_with("Merge remote-tracking branch")
        || (subject.starts_with("Merge branch ") && subject.contains(" into "))
    {
        return None;
    } else if let Some(branch) = subject.strip_prefix("Merge branch ") {
        (parse_commit_title(branch.trim_matches('\'')), None)
    } else {
        (parse_commit_title(subject), None)
    };

    let pr = parsed.pr.or(merged_pr);
    let mut entry = item(TimelineKind::Commit, commit.at, parsed.title.clone());
    entry.label = parsed
        .kind
        .as_deref()
        .and_then(commit_label)
        .map(str::to_string);
    entry.detail = clean_body(body);
    entry.goal = match_goal(
        matchers,
        &format!("{subject}\n{}", commit.body),
        &format!("{} {}", parsed.scope.unwrap_or_default(), parsed.title),
    )
    .map(|goal| goal.title.clone());
    entry.link = web_url.map(|base| match pr {
        Some(number) => format!("{base}/pull/{number}"),
        None => format!("{base}/commit/{}", commit.hash),
    });
    entry.hash = Some(commit.hash.chars().take(7).collect());
    entry.pr = pr;
    entry.file_count = commit.files.len();
    entry.files = commit.files.iter().take(MAX_FILES).cloned().collect();
    Some(entry)
}

fn release_item(tag: &GitTag, web_url: Option<&str>) -> TimelineItem {
    let mut entry = item(TimelineKind::Release, tag.at, tag.name.clone());
    entry.label = Some("Versión".to_string());
    entry.detail.clone_from(&tag.message);
    entry.link = web_url.map(|base| format!("{base}/releases/tag/{}", tag.name));
    entry
}

fn memory_label(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Decision => "Decisión",
        MemoryKind::Architecture => "Arquitectura",
        _ => "Nota",
    }
}

/// Builds the timeline, newest day first, items newest first within a day.
/// Days are calendar days in `tz`.
#[must_use]
pub fn build_timeline<Tz: TimeZone>(sources: &TimelineSources, tz: &Tz) -> Vec<TimelineDay> {
    let goal_titles: BTreeMap<Uuid, &str> = sources
        .goals
        .iter()
        .map(|goal| (goal.id, goal.title.as_str()))
        .collect();
    let matchers = goal_matchers(&sources.goals, &sources.tasks);
    let web_url = sources.git.web_url.as_deref();

    let mut items: Vec<TimelineItem> = Vec::new();
    for goal in &sources.goals {
        if let (GoalStatus::Completed, Some(at)) = (goal.status, goal.completed_at) {
            let mut entry = item(TimelineKind::GoalCompleted, at, goal.title.clone());
            entry.detail.clone_from(&goal.criteria);
            items.push(entry);
        }
    }
    for task in &sources.tasks {
        if let (TaskStatus::Done, Some(at)) = (task.status, task.completed_at) {
            let mut entry = item(TimelineKind::TaskCompleted, at, task.title.clone());
            entry.goal = task
                .goal_id
                .and_then(|id| goal_titles.get(&id))
                .map(|title| (*title).to_string());
            items.push(entry);
        }
    }
    for memory in &sources.memories {
        let mut entry = item(
            TimelineKind::Memory,
            memory.created_at,
            memory.title.clone(),
        );
        entry.label = Some(memory_label(memory.kind).to_string());
        items.push(entry);
    }
    items.extend(
        sources
            .git
            .tags
            .iter()
            .map(|tag| release_item(tag, web_url)),
    );
    items.extend(
        sources
            .git
            .commits
            .iter()
            .filter_map(|commit| commit_item(commit, web_url, &matchers)),
    );

    items.sort_by(|a, b| b.at.cmp(&a.at).then(a.kind.cmp(&b.kind)));

    let mut days: Vec<TimelineDay> = Vec::new();
    for entry in items {
        let date: NaiveDate = entry.at.with_timezone(tz).date_naive();
        match days.last_mut() {
            Some(day) if day.date == date => day.items.push(entry),
            _ => days.push(TimelineDay {
                date,
                items: vec![entry],
            }),
        }
    }
    days
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_conventional_titles() {
        let parsed = parse_commit_title("feat(desktop)!: premium UI pass (#10)");
        assert_eq!(parsed.kind.as_deref(), Some("feat"));
        assert_eq!(parsed.scope.as_deref(), Some("desktop"));
        assert!(parsed.breaking);
        assert_eq!(parsed.title, "Premium UI pass");
        assert_eq!(parsed.pr, Some(10));

        let plain = parse_commit_title("Update README.md");
        assert_eq!(plain.kind, None);
        assert_eq!(plain.title, "Update README.md");

        let colon_in_text = parse_commit_title("Release notes: see below");
        assert_eq!(colon_in_text.kind, None);
        assert_eq!(colon_in_text.title, "Release notes: see below");
    }

    #[test]
    fn squash_bodies_become_one_change_per_line() {
        let body = "* feat: a\n\n* fix: b\n\n---------\n\nCo-authored-by: X <x@y.z>\n";
        assert_eq!(clean_body(body).as_deref(), Some("* feat: a\n* fix: b"));
        assert_eq!(clean_body("\n\n"), None);
    }

    #[test]
    fn words_fold_accents_and_plurals() {
        let words = significant_words("Exportación de facturas en PDF");
        assert!(words.contains("exportacion"));
        assert!(words.contains("factura"));
        assert!(!words.contains("de"));
    }
}
