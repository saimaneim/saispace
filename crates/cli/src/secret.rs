use crate::{CliError, CliResult};
use clap::{Args, Subcommand};
use saispace_core::models::{GrantStatus, SecretGrant};
use saispace_core::{CoreError, Db, Vault, default_db_path, detect_agent_name, vault_key_path_for};
use std::io::{BufRead, BufReader, IsTerminal, Read, Write};
use std::process::{ExitCode, Stdio};

const MIN_MASK_LEN: usize = 4;
const EXIT_NO_ACCESS: u8 = 3;

#[derive(Subcommand)]
pub enum SecretCommand {
    #[command(about = "Lista las claves guardadas (solo nombres) y tu permiso sobre cada una")]
    List {
        #[arg(long)]
        json: bool,
    },
    #[command(about = "Pide permiso al usuario para usar una clave; se aprueba en la app")]
    Request {
        name: String,
        #[arg(long, default_value = "")]
        reason: String,
    },
    #[command(about = "Ejecuta un comando con claves como variables de entorno, sin mostrarlas")]
    Run(RunArgs),
    #[command(about = "Guarda o reemplaza una clave leyendo el valor de stdin")]
    Set {
        name: String,
        #[arg(long, default_value = "")]
        description: String,
    },
    #[command(about = "Elimina una clave y sus permisos")]
    Rm { name: String },
}

#[derive(Args)]
pub struct RunArgs {
    #[arg(
        short = 's',
        long = "secret",
        required = true,
        help = "NOMBRE o NOMBRE=VARIABLE; repetible"
    )]
    secrets: Vec<String>,
    #[arg(
        long,
        default_value = "",
        help = "Motivo que verá el usuario si falta permiso"
    )]
    reason: String,
    #[arg(last = true, required = true, help = "Comando tras --")]
    command: Vec<String>,
}

struct Context {
    db: Db,
    vault: Vault,
    agent: String,
}

async fn open() -> CliResult<Context> {
    let db_path = default_db_path()?;
    let db = Db::open(&db_path).await?;
    let vault = Vault::open(&vault_key_path_for(&db_path))?;
    Ok(Context {
        db,
        vault,
        agent: detect_agent_name("sai-cli"),
    })
}

fn status_label(grant: Option<&SecretGrant>) -> &'static str {
    match grant.map(|g| g.status) {
        Some(GrantStatus::Approved) => "aprobado",
        Some(GrantStatus::Pending) => "pendiente",
        Some(GrantStatus::Denied) => "denegado",
        None => "sin pedir",
    }
}

pub fn run(cmd: SecretCommand) -> CliResult<ExitCode> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_async(cmd))
}

async fn run_async(cmd: SecretCommand) -> CliResult<ExitCode> {
    let ctx = open().await?;
    match cmd {
        SecretCommand::List { json } => list(&ctx, json).await?,
        SecretCommand::Request { name, reason } => {
            let grant = ctx
                .db
                .request_secret_access(&name, &ctx.agent, &reason)
                .await?;
            crate::out!(
                "{}: {} para {}. El usuario lo aprueba en saispace → Global → Claves.",
                grant.secret_name,
                status_label(Some(&grant)),
                grant.agent
            );
        }
        SecretCommand::Run(args) => return run_with_secrets(&ctx, args).await,
        SecretCommand::Set { name, description } => {
            let value = read_value()?;
            let meta = ctx
                .db
                .set_secret(&ctx.vault, &name, &value, &description)
                .await?;
            crate::out!("{} guardada", meta.name);
        }
        SecretCommand::Rm { name } => {
            ctx.db.delete_secret(&name).await?;
            crate::out!("{name} eliminada");
        }
    }
    Ok(ExitCode::SUCCESS)
}

async fn list(ctx: &Context, json: bool) -> CliResult<()> {
    let secrets = ctx.db.list_secrets().await?;
    let grants = ctx.db.list_secret_grants().await?;
    let mine = |name: &str| {
        grants
            .iter()
            .find(|g| g.secret_name == name && g.agent == ctx.agent)
    };
    if json {
        let rows: Vec<_> = secrets
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "description": s.description,
                    "access": mine(&s.name).map(|g| g.status),
                })
            })
            .collect();
        crate::out!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if secrets.is_empty() {
        crate::out!("No hay claves. El usuario las añade en saispace → Global → Claves.");
        return Ok(());
    }
    crate::out!("Agente: {}", ctx.agent);
    for s in &secrets {
        crate::out!(
            "{:<28} {:<10} {}",
            s.name,
            status_label(mine(&s.name)),
            s.description
        );
    }
    Ok(())
}

fn read_value() -> CliResult<String> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        eprint!("Valor (Enter para terminar): ");
        std::io::stderr().flush()?;
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        return Ok(line.trim_end_matches(['\r', '\n']).to_string());
    }
    let mut value = String::new();
    stdin.lock().read_to_string(&mut value)?;
    Ok(value.trim_end_matches(['\r', '\n']).to_string())
}

fn parse_binding(raw: &str) -> (&str, &str) {
    raw.split_once('=').unwrap_or((raw, raw))
}

async fn run_with_secrets(ctx: &Context, args: RunArgs) -> CliResult<ExitCode> {
    let mut env = Vec::new();
    let mut blocked = Vec::new();
    for raw in &args.secrets {
        let (name, var) = parse_binding(raw);
        if var.is_empty() {
            return Err(CliError::Usage(format!("variable vacía en '{raw}'")));
        }
        match ctx.db.reveal_secret(&ctx.vault, name, &ctx.agent).await {
            Ok(value) => env.push((var.to_string(), value)),
            Err(CoreError::SecretAccess { secret, status, .. }) => {
                if !args.reason.is_empty() {
                    ctx.db
                        .request_secret_access(&secret, &ctx.agent, &args.reason)
                        .await?;
                }
                blocked.push(format!("{secret} ({status})"));
            }
            Err(e) => return Err(e.into()),
        }
    }
    if !blocked.is_empty() {
        eprintln!(
            "Sin permiso para {} como agente '{}'. Pide al usuario que lo apruebe en saispace → Global → Claves y vuelve a ejecutar.",
            blocked.join(", "),
            ctx.agent
        );
        return Ok(ExitCode::from(EXIT_NO_ACCESS));
    }
    let Some((program, rest)) = args.command.split_first() else {
        return Err(CliError::Usage("falta el comando tras --".to_string()));
    };
    let mut child = std::process::Command::new(program)
        .args(rest)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    // Longest first, so a secret that contains another is masked whole.
    let mut masks: Vec<Vec<u8>> = env
        .iter()
        .map(|(_, v)| v.as_bytes().to_vec())
        .filter(|v| v.len() >= MIN_MASK_LEN)
        .collect();
    masks.sort_by_key(|m| std::cmp::Reverse(m.len()));
    let out_pipe = child.stdout.take();
    let err_pipe = child.stderr.take();
    let out_masks = masks.clone();
    let out_thread = std::thread::spawn(move || {
        if let Some(pipe) = out_pipe {
            pump(pipe, &mut std::io::stdout(), &out_masks);
        }
    });
    if let Some(pipe) = err_pipe {
        pump(pipe, &mut std::io::stderr(), &masks);
    }
    let _ = out_thread.join();
    let status = child.wait()?;
    Ok(ExitCode::from(exit_code(status)))
}

/// Mirrors the shell: a child killed by a signal exits as 128 + signal.
fn exit_code(status: std::process::ExitStatus) -> u8 {
    if let Some(code) = status.code() {
        return u8::try_from(code).unwrap_or(1);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return u8::try_from(128 + signal).unwrap_or(1);
        }
    }
    1
}

const MASK: &[u8] = "••••••".as_bytes();

/// Replaces every secret in `line` byte by byte, so binary output (an image
/// piped through `curl`, a tarball) passes through untouched.
fn mask(line: &[u8], masks: &[Vec<u8>]) -> Vec<u8> {
    let mut out = line.to_vec();
    for secret in masks {
        if secret.is_empty() || !out.windows(secret.len()).any(|w| w == secret.as_slice()) {
            continue;
        }
        let mut replaced = Vec::with_capacity(out.len());
        let mut i = 0;
        while i < out.len() {
            if out[i..].starts_with(secret) {
                replaced.extend_from_slice(MASK);
                i += secret.len();
            } else {
                replaced.push(out[i]);
                i += 1;
            }
        }
        out = replaced;
    }
    out
}

fn pump(pipe: impl Read, sink: &mut impl Write, masks: &[Vec<u8>]) {
    let mut reader = BufReader::new(pipe);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if sink
                    .write_all(&mask(&line, masks))
                    .and_then(|()| sink.flush())
                    .is_err()
                {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn mask_keeps_binary_bytes() {
        let line = b"abc\x00\xff\xfeDEF";
        assert_eq!(mask(line, &[b"zzzz".to_vec()]), line.to_vec());
    }

    #[test]
    fn mask_replaces_every_occurrence_next_to_invalid_utf8() {
        let out = mask(b"\xffsecret-secret\n", &[b"secret".to_vec()]);
        assert_eq!(
            out,
            [b"\xff".as_slice(), MASK_BYTES, b"-", MASK_BYTES, b"\n"].concat()
        );
    }

    #[test]
    fn mask_longest_secret_first() {
        let mut masks = vec![b"abcd".to_vec(), b"abcdefgh".to_vec()];
        masks.sort_by_key(|m| std::cmp::Reverse(m.len()));
        assert_eq!(
            mask(b"xabcdefghx", &masks),
            [b"x".as_slice(), MASK_BYTES, b"x"].concat()
        );
    }

    const MASK_BYTES: &[u8] = super::MASK;
}
