//! Read-only view of a repository's history through the `git` binary.
//!
//! Only the first-parent line of `HEAD` is read: with squash or merge PRs
//! that is one commit per PR, so the history reads like a changelog instead
//! of every intermediate commit.

use chrono::{DateTime, Utc};
use std::path::Path;
use std::process::Command;

const RECORD: char = '\u{1e}';
/// Variables that point git at another repository. Git exports them to
/// hooks (`GIT_DIR` in worktrees), and they would override `-C <path>`.
pub const REPO_ENV_VARS: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
];
const FIELD: char = '\u{1f}';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommit {
    pub hash: String,
    pub at: DateTime<Utc>,
    pub subject: String,
    pub body: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTag {
    pub name: String,
    pub at: DateTime<Utc>,
    /// Message of an annotated tag; lightweight tags have none.
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitHistory {
    pub commits: Vec<GitCommit>,
    pub tags: Vec<GitTag>,
    /// `https://github.com/owner/repo` when `origin` points to GitHub.
    pub web_url: Option<String>,
}

fn git(path: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new("git");
    for var in REPO_ENV_VARS {
        command.env_remove(var);
    }
    let output = command
        .arg("-C")
        .arg(path)
        .args(["-c", "core.quotepath=off", "--no-pager"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Reads up to `limit` first-parent commits, every tag and the GitHub URL.
/// A folder that is not a repository (or has no commits) yields an empty
/// history rather than an error: the timeline still shows the plan.
#[must_use]
pub fn read_git_history(path: &Path, limit: usize) -> GitHistory {
    let limit_arg = format!("--max-count={limit}");
    let format = format!("--format={RECORD}%H{FIELD}%cI{FIELD}%s{FIELD}%b{FIELD}");
    let commits = git(
        path,
        &[
            "log",
            "--first-parent",
            "--diff-merges=first-parent",
            "--name-only",
            &limit_arg,
            &format,
        ],
    )
    .map_or_else(Vec::new, |raw| parse_log(&raw));

    let tag_format = format!(
        "--format=%(refname:short){FIELD}%(creatordate:iso-strict){FIELD}%(objecttype){FIELD}%(contents:subject){RECORD}"
    );
    let tags = git(path, &["for-each-ref", "refs/tags", &tag_format])
        .map_or_else(Vec::new, |raw| parse_tags(&raw));

    let web_url = git(path, &["remote", "get-url", "origin"])
        .and_then(|remote| github_web_url(remote.trim()));

    GitHistory {
        commits,
        tags,
        web_url,
    }
}

/// Parses `git log` output written with the record/field separators used by
/// [`read_git_history`], followed by `--name-only` file lists.
#[must_use]
pub fn parse_log(raw: &str) -> Vec<GitCommit> {
    raw.split(RECORD)
        .filter_map(|record| {
            let mut fields = record.splitn(5, FIELD);
            let hash = fields.next()?.trim().to_string();
            let at = DateTime::parse_from_rfc3339(fields.next()?.trim())
                .ok()?
                .with_timezone(&Utc);
            let subject = fields.next()?.trim().to_string();
            let body = fields.next()?.trim().to_string();
            let files = fields
                .next()
                .unwrap_or_default()
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect();
            (!hash.is_empty()).then_some(GitCommit {
                hash,
                at,
                subject,
                body,
                files,
            })
        })
        .collect()
}

fn parse_tags(raw: &str) -> Vec<GitTag> {
    raw.split(RECORD)
        .filter_map(|record| {
            let mut fields = record.trim().splitn(4, FIELD);
            let name = fields.next()?.trim().to_string();
            let at = DateTime::parse_from_rfc3339(fields.next()?.trim())
                .ok()?
                .with_timezone(&Utc);
            let annotated = fields.next()? == "tag";
            let subject = fields.next().unwrap_or_default().trim();
            let message = (annotated && !subject.is_empty()).then(|| subject.to_string());
            (!name.is_empty()).then_some(GitTag { name, at, message })
        })
        .collect()
}

/// Turns an `origin` remote into its GitHub web URL. Other hosts return
/// `None`: their link formats differ and a wrong link is worse than none.
#[must_use]
pub fn github_web_url(remote: &str) -> Option<String> {
    let rest = remote
        .strip_prefix("git@github.com:")
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))
        .or_else(|| remote.strip_prefix("https://github.com/"))
        .or_else(|| remote.strip_prefix("http://github.com/"))?;
    let repo = rest.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = repo.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(format!("https://github.com/{owner}/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_urls_from_common_remote_forms() {
        for remote in [
            "git@github.com:acme/app.git",
            "https://github.com/acme/app",
            "https://github.com/acme/app.git/",
            "ssh://git@github.com/acme/app.git",
        ] {
            assert_eq!(
                github_web_url(remote).as_deref(),
                Some("https://github.com/acme/app"),
                "{remote}"
            );
        }
        assert_eq!(github_web_url("https://gitlab.com/acme/app.git"), None);
        assert_eq!(github_web_url("https://github.com/acme"), None);
    }

    #[test]
    fn parses_log_records_with_bodies_and_files() {
        let raw = format!(
            "{RECORD}abc{FIELD}2026-11-20T10:00:00+01:00{FIELD}feat: add x (#3){FIELD}line one\n\nline two{FIELD}\n\nsrc/a.rs\nsrc/b.rs\n{RECORD}def{FIELD}2026-11-19T09:00:00Z{FIELD}fix: y{FIELD}{FIELD}\n"
        );
        let commits = parse_log(&raw);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].subject, "feat: add x (#3)");
        assert_eq!(commits[0].body, "line one\n\nline two");
        assert_eq!(commits[0].files, vec!["src/a.rs", "src/b.rs"]);
        assert_eq!(commits[0].at.to_rfc3339(), "2026-11-20T09:00:00+00:00");
        assert_eq!(commits[1].files, Vec::<String>::new());
    }
}
