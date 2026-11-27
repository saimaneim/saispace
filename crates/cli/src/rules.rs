use crate::{CliError, CliResult};
use clap::{Args, Subcommand, ValueEnum};
use saispace_rules::{
    Check, Engine, Finding, RuleDef, Severity, find_root, load_rules, remove_rule, write_rule,
};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Args)]
pub struct CheckArgs {
    #[arg(help = "Solo estas rutas (ficheros o carpetas)")]
    pub paths: Vec<PathBuf>,
    #[arg(help = "Solo estas reglas")]
    #[arg(long = "rule")]
    pub rules: Vec<String>,
    #[arg(help = "Salida JSON para agentes")]
    #[arg(long)]
    pub json: bool,
    #[arg(help = "Falla también con avisos")]
    #[arg(long)]
    pub strict: bool,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum SeverityArg {
    Error,
    Warn,
}

#[derive(Args)]
pub struct AddArgs {
    #[arg(help = "Identificador en minúsculas con guiones")]
    #[arg(long)]
    pub id: String,
    #[arg(help = "Mensaje que verá quien rompa la regla")]
    #[arg(long)]
    pub description: String,
    #[arg(help = "Globs de ficheros a los que aplica (repetible)")]
    #[arg(long = "files", required = true)]
    pub files: Vec<String>,
    #[arg(help = "Globs excluidos (repetible)")]
    #[arg(long = "exclude")]
    pub exclude: Vec<String>,
    #[arg(help = "Regex prohibida por línea")]
    #[arg(long, conflicts_with_all = ["require", "max_lines"])]
    pub forbid: Option<String>,
    #[arg(
        help = "Deja de buscar a partir de la línea que cumpla esta regex (p. ej. módulos de test)"
    )]
    #[arg(long, requires = "forbid")]
    pub stop_at: Option<String>,
    #[arg(help = "Regex que debe aparecer en cada fichero")]
    #[arg(long, conflicts_with = "max_lines")]
    pub require: Option<String>,
    #[arg(help = "Máximo de líneas por fichero")]
    #[arg(long)]
    pub max_lines: Option<usize>,
    #[arg(help = "Cómo arreglarlo")]
    #[arg(long)]
    pub fix: Option<String>,
    #[arg(help = "Guía de contexto (sin verificación) para `sai context`")]
    #[arg(long)]
    pub guide: Option<String>,
    #[arg(long, value_enum, default_value = "error")]
    pub severity: SeverityArg,
    #[arg(help = "Reemplaza una regla con el mismo id")]
    #[arg(long)]
    pub force: bool,
}

#[derive(Subcommand)]
pub enum RuleCommand {
    #[command(about = "Añade una regla y muestra cuántas violaciones encuentra ya")]
    Add(Box<AddArgs>),
    #[command(about = "Lista las reglas")]
    List {
        #[arg(long)]
        json: bool,
    },
    #[command(about = "Muestra una regla en TOML")]
    Show { id: String },
    #[command(about = "Elimina una regla")]
    Rm { id: String },
}

#[derive(Args)]
pub struct ContextArgs {
    pub files: Vec<PathBuf>,
    #[arg(long)]
    pub json: bool,
}

fn root() -> CliResult<PathBuf> {
    Ok(find_root(&std::env::current_dir()?))
}

fn absolute(root: &Path, path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_or_else(|_| root.join(path), |cwd| cwd.join(path))
    };
    joined.canonicalize().unwrap_or(joined)
}

fn print_findings(findings: &[Finding]) {
    for f in findings {
        crate::out!(
            "{}:{}:{}  {}  [{}] {}",
            f.path,
            f.line,
            f.column,
            f.severity.label(),
            f.rule,
            f.message
        );
        if !f.snippet.is_empty() {
            crate::out!("    {}", f.snippet);
        }
        if let Some(fix) = &f.fix {
            crate::out!("    arreglo: {fix}");
        }
    }
}

pub fn check(args: &CheckArgs) -> CliResult<ExitCode> {
    let root = root()?;
    let mut defs = load_rules(&root)?;
    if !args.rules.is_empty() {
        for id in &args.rules {
            if !defs.iter().any(|d| &d.id == id) {
                return Err(CliError::Usage(format!("rule {id} not found")));
            }
        }
        defs.retain(|d| args.rules.contains(&d.id));
    }
    let engine = Engine::new(&defs)?;
    let only: Vec<PathBuf> = args.paths.iter().map(|p| absolute(&root, p)).collect();
    let report = engine.check_tree(&root, &only);

    if args.json {
        crate::out!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_findings(&report.findings);
        let status = if report.findings.is_empty() {
            "ok"
        } else {
            "fallos"
        };
        crate::out!(
            "\n{status}: {} errores, {} avisos · {} reglas · {} ficheros",
            report.errors(),
            report.warnings(),
            report.rules,
            report.files_scanned
        );
    }
    let failed = report.errors() > 0 || (args.strict && report.warnings() > 0);
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

pub fn rule(cmd: RuleCommand) -> CliResult<()> {
    let root = root()?;
    match cmd {
        RuleCommand::Add(args) => add(&root, *args),
        RuleCommand::List { json } => {
            let rules = load_rules(&root)?;
            if json {
                crate::out!("{}", serde_json::to_string_pretty(&rules)?);
            } else if rules.is_empty() {
                crate::out!(
                    "sin reglas en {}",
                    saispace_rules::rules_dir(&root).display()
                );
            } else {
                for r in rules {
                    let kind = match &r.check {
                        Some(Check::Forbid { .. }) => "prohíbe",
                        Some(Check::Require { .. }) => "exige",
                        Some(Check::MaxLines { .. }) => "máx. líneas",
                        Some(Check::RefsExist { .. }) => "rutas válidas",
                        None => "guía",
                    };
                    crate::out!(
                        "{:<28} {:<5} {:<11} {}",
                        r.id,
                        r.severity.label(),
                        kind,
                        r.description
                    );
                }
            }
            Ok(())
        }
        RuleCommand::Show { id } => {
            let rules = load_rules(&root)?;
            let rule = rules
                .iter()
                .find(|r| r.id == id)
                .ok_or(saispace_rules::RulesError::NotFound(id))?;
            crate::out!("{}", serde_json::to_string_pretty(rule)?);
            Ok(())
        }
        RuleCommand::Rm { id } => {
            let path = remove_rule(&root, &id)?;
            crate::out!("eliminada {}", path.display());
            Ok(())
        }
    }
}

fn add(root: &Path, args: AddArgs) -> CliResult<()> {
    let check = match (args.forbid, args.require, args.max_lines) {
        (Some(pattern), None, None) => Some(Check::Forbid {
            pattern,
            stop_at: args.stop_at,
            multiline: false,
            unless_line: None,
            except: None,
            within: None,
            unless_span: None,
        }),
        (None, Some(pattern), None) => Some(Check::Require { pattern }),
        (None, None, Some(max)) => Some(Check::MaxLines { max }),
        (None, None, None) => None,
        _ => {
            return Err(CliError::Usage(
                "use only one of --forbid, --require or --max-lines".to_string(),
            ));
        }
    };
    let rule = RuleDef {
        id: args.id,
        description: args.description,
        severity: match args.severity {
            SeverityArg::Error => Severity::Error,
            SeverityArg::Warn => Severity::Warn,
        },
        files: args.files,
        exclude: args.exclude,
        only_if: None,
        unless: None,
        check,
        fix: args.fix,
        guide: args.guide,
    };
    let path = write_rule(root, &rule, args.force)?;
    let report = Engine::new(std::slice::from_ref(&rule))?.check_tree(root, &[]);
    crate::out!("guardada {}", path.display());
    crate::out!(
        "aplica a {} ficheros · {} violaciones actuales",
        report.files_scanned,
        report.findings.len()
    );
    print_findings(&report.findings.iter().take(10).cloned().collect::<Vec<_>>());
    Ok(())
}

pub fn context(args: &ContextArgs) -> CliResult<()> {
    if args.files.is_empty() {
        return Err(CliError::Usage("indica al menos un fichero".to_string()));
    }
    let root = root()?;
    let rules = load_rules(&root)?;
    let engine = Engine::new(&rules)?;
    let mut out = Vec::new();
    for file in &args.files {
        let abs = absolute(&root, file);
        let rel = abs
            .strip_prefix(&root)
            .unwrap_or(&abs)
            .to_string_lossy()
            .replace('\\', "/");
        let applying: Vec<&RuleDef> = engine.rules_for(&rel);
        out.push(serde_json::json!({ "file": rel, "rules": applying }));
        if !args.json {
            crate::out!("{rel}");
            if applying.is_empty() {
                crate::out!("  sin reglas");
            }
            for r in applying {
                crate::out!("  [{}] {}", r.id, r.description);
                if let Some(guide) = &r.guide {
                    for line in guide.lines() {
                        crate::out!("      {line}");
                    }
                }
            }
        }
    }
    if args.json {
        crate::out!("{}", serde_json::to_string_pretty(&out)?);
    }
    Ok(())
}
