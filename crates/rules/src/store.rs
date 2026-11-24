use crate::error::{RulesError, RulesResult};
use crate::model::RuleDef;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const RULES_DIR: &str = ".saispace/rules";

#[must_use]
pub fn find_root(start: &Path) -> PathBuf {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(".saispace").is_dir() || dir.join(".git").exists() {
            return dir.to_path_buf();
        }
        current = dir.parent();
    }
    start.to_path_buf()
}

#[must_use]
pub fn rules_dir(root: &Path) -> PathBuf {
    root.join(RULES_DIR)
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> RulesError + '_ {
    move |source| RulesError::Io {
        path: path.to_path_buf(),
        source,
    }
}

pub fn load_rules(root: &Path) -> RulesResult<Vec<RuleDef>> {
    let dir = rules_dir(root);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(io_err(&dir))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    paths.sort();

    let mut seen: HashMap<String, PathBuf> = HashMap::new();
    let mut rules = Vec::with_capacity(paths.len());
    for path in paths {
        let text = fs::read_to_string(&path).map_err(io_err(&path))?;
        let rule: RuleDef = toml::from_str(&text).map_err(|source| RulesError::Parse {
            path: path.clone(),
            source,
        })?;
        rule.validate()?;
        if let Some(first) = seen.insert(rule.id.clone(), path.clone()) {
            return Err(RulesError::DuplicateDefinition {
                id: rule.id,
                first,
                second: path,
            });
        }
        rules.push(rule);
    }
    rules.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(rules)
}

pub fn write_rule(root: &Path, rule: &RuleDef, force: bool) -> RulesResult<PathBuf> {
    rule.validate()?;
    crate::engine::Engine::new(std::slice::from_ref(rule))?;
    let existing = load_rules(root)?;
    if !force && existing.iter().any(|r| r.id == rule.id) {
        return Err(RulesError::Duplicate(rule.id.clone()));
    }
    let dir = rules_dir(root);
    fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    let path = dir.join(format!("{}.toml", rule.id));
    let text = toml::to_string_pretty(rule).map_err(|source| RulesError::Serialize {
        id: rule.id.clone(),
        source,
    })?;
    let tmp = dir.join(format!(".{}.toml.tmp", rule.id));
    fs::write(&tmp, text).map_err(io_err(&tmp))?;
    fs::rename(&tmp, &path).map_err(io_err(&path))?;
    Ok(path)
}

pub fn remove_rule(root: &Path, id: &str) -> RulesResult<PathBuf> {
    let path = rules_dir(root).join(format!("{id}.toml"));
    if !path.is_file() {
        return Err(RulesError::NotFound(id.to_string()));
    }
    fs::remove_file(&path).map_err(io_err(&path))?;
    Ok(path)
}
