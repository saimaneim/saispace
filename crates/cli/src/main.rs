#[macro_export]
macro_rules! out {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        if let Err(err) = writeln!(std::io::stdout().lock(), $($arg)*)
            && err.kind() == std::io::ErrorKind::BrokenPipe
        {
            std::process::exit(0);
        }
    }};
}

mod memory;
mod nudge;
mod rules;
mod secret;
mod setup;
mod skills;

use clap::{Parser, Subcommand};
use std::process::ExitCode;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error(transparent)]
    Core(#[from] saispace_core::CoreError),
    #[error(transparent)]
    Rules(#[from] saispace_rules::RulesError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Usage(String),
}

pub type CliResult<T> = Result<T, CliError>;

#[derive(Parser)]
#[command(
    name = "sai",
    version,
    about = "saispace: reglas verificables y memoria persistente para agentes de IA"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Ejecuta las comprobaciones de .agents/checks sobre el repositorio")]
    Check(rules::CheckArgs),
    #[command(about = "Crea, lista o elimina reglas")]
    #[command(subcommand)]
    Rule(rules::RuleCommand),
    #[command(about = "Muestra las reglas y guías que aplican a uno o más ficheros")]
    Context(rules::ContextArgs),
    #[command(about = "Memoria persistente compartida con el MCP y la app")]
    #[command(subcommand)]
    Memory(memory::MemoryCommand),
    #[command(about = "Claves guardadas: los agentes las usan con permiso, sin ver su valor")]
    #[command(subcommand)]
    Secret(secret::SecretCommand),
    #[command(about = "Skills de .agents/skills para los agentes que no las leen solos")]
    #[command(subcommand)]
    Skills(SkillsCommand),
    #[command(about = "Instala la integración de saispace en un agente")]
    #[command(subcommand)]
    Setup(setup::SetupCommand),
}

#[derive(Subcommand)]
enum SkillsCommand {
    #[command(
        about = "Enlaza .agents/skills en .claude/skills (Claude Code no lee .agents); no sobrescribe nada"
    )]
    Sync(skills::SyncArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Check(args) => rules::check(&args),
        Command::Rule(cmd) => rules::rule(cmd).map(|()| ExitCode::SUCCESS),
        Command::Context(args) => rules::context(&args).map(|()| ExitCode::SUCCESS),
        Command::Memory(cmd) => memory::run(cmd).map(|()| ExitCode::SUCCESS),
        Command::Secret(cmd) => secret::run(cmd),
        Command::Skills(SkillsCommand::Sync(args)) => skills::run(&args),
        Command::Setup(cmd) => setup::run(&cmd),
    };
    match result {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(2)
        }
    }
}
