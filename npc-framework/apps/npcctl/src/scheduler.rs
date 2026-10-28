//! Operator-only links between game actors and resumable local AI CLI sessions.
use clap::{Args, Subcommand, ValueEnum};
use npc_framework::agent_io::{
    http::RpcClient,
    scheduler::{self, RunnerBinding, RunnerProvider},
};
use npc_framework::tools::output::{Failure, Out};
use serde_json::json;
use std::{fmt, path::PathBuf};
use testbed_content::explorer::{home, host::RunFile};

#[derive(Subcommand, Debug)]
pub enum SchedulerCmd {
    /// Link an already-connected game actor to one exact CLI session. No AI is launched by this command.
    Bind(BindArgs),
    /// Remove a link; the host stops dispatching further continuation turns.
    Unbind(IdentityArgs),
    /// Read the host supervisor's current actor/activity states (never session UUIDs or credentials).
    Status {
        #[arg(long,default_value=crate::agent::DEFAULT_URL)]
        url: String,
    },
}
#[derive(Args, Clone, Debug)]
pub struct IdentityArgs {
    /// Explorer agents folder. Together with --agent, resolves its registered controller identity.
    #[arg(long, requires = "agent")]
    home: Option<PathBuf>,
    #[arg(long, requires = "home", conflicts_with = "controller")]
    agent: Option<String>,
    /// Explicit controller id for a non-explorer scene.
    #[arg(long, conflicts_with = "agent")]
    controller: Option<String>,
    /// Operator-owned links file; default HOME/.ai-scheduler/bindings.json, else NPC_AI_SCHEDULER_BINDINGS.
    #[arg(long)]
    bindings: Option<PathBuf>,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum Provider {
    Codex,
    Claude,
    Custom,
}
#[derive(Args, Clone)]
pub struct BindArgs {
    #[command(flatten)]
    identity: IdentityArgs,
    #[arg(long, value_enum, default_value = "codex")]
    provider: Provider,
    /// The exact previous conversation UUID. Global --last and ambiguous thread names are not accepted.
    #[arg(long)]
    session_id: String,
    /// Override the actor id, or discover it from the running game's scheduler/status response.
    #[arg(long)]
    unit: Option<u32>,
    #[arg(long)]
    url: Option<String>,
    /// Existing AI workspace (defaults to the explorer agent folder).
    #[arg(long)]
    cwd: Option<PathBuf>,
    #[arg(long)]
    program: Option<PathBuf>,
    /// Structured argv for Custom; {session_id} is substituted. Prompt always arrives on stdin.
    #[arg(long, allow_hyphen_values = true)]
    runner_arg: Vec<String>,
    /// Original one-shot CLI process. Nudging waits for it to exit; it is never killed.
    #[arg(long, conflicts_with = "turn_finished")]
    client_pid: Option<u32>,
    /// Confirm the original CLI turn has already finished (alternative to --client-pid).
    #[arg(long, conflicts_with = "client_pid")]
    turn_finished: bool,
}
impl fmt::Debug for BindArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BindArgs")
            .field("identity", &self.identity)
            .field("provider", &self.provider)
            .field("session_id", &"[configured]")
            .finish_non_exhaustive()
    }
}
fn identity(args: &IdentityArgs) -> Result<(String, PathBuf, Option<home::AgentDir>), Failure> {
    let (controller, dir) = if let (Some(root), Some(name)) = (&args.home, &args.agent) {
        if home::name_problem(name).is_some() {
            return Err(Failure::usage("invalid agent name"));
        }
        let dir = home::AgentDir {
            dir: root.join(home::slug(name)),
        };
        let meta = home::read_meta(&dir)
            .map_err(|_| Failure::usage("cannot read the selected agent metadata"))?;
        (meta.agent_id, Some(dir))
    } else {
        (
            args.controller.clone().ok_or_else(|| {
                Failure::usage("give --home with --agent, or an explicit --controller")
            })?,
            None,
        )
    };
    let path = args
        .bindings
        .clone()
        .or_else(|| {
            args.home
                .as_ref()
                .map(|p| p.join(".ai-scheduler").join(scheduler::BINDINGS_FILE))
        })
        .unwrap_or_else(scheduler::default_bindings_path);
    Ok((controller, path, dir))
}
fn client(url: &str) -> Result<RpcClient, Failure> {
    RpcClient::new(url)
        .map(|c| c.with_token(std::env::var("NPC_SCHEDULER_BEARER_TOKEN").ok()))
        .map_err(|_| Failure::usage("invalid game URL"))
}
pub fn run(cmd: &SchedulerCmd, out: &mut Out) -> Result<u8, Failure> {
    match cmd {
        SchedulerCmd::Status { url } => {
            let result = client(url)?
                .call("scheduler/status", serde_json::Value::Null)
                .map_err(|_| Failure::usage("cannot read the running game's scheduler status"))?;
            out.json(&result)?;
        }
        SchedulerCmd::Unbind(args) => {
            let (controller, path, _) = identity(args)?;
            scheduler::remove_binding(&path, &controller).map_err(Failure::usage)?;
            out.json(&json!({"linked":false,"controller":controller,"bindings":path}))?;
        }
        SchedulerCmd::Bind(args) => {
            if args.client_pid.is_none() && !args.turn_finished {
                return Err(Failure::usage(
                    "give --client-pid for the original one-shot CLI, or --turn-finished once its turn has ended",
                ));
            }
            let (controller, path, dir) = identity(&args.identity)?;
            let cwd = args
                .cwd
                .clone()
                .or_else(|| dir.as_ref().map(|d| d.dir.clone()))
                .ok_or_else(|| Failure::usage("give the AI workspace with --cwd"))?
                .canonicalize()
                .map_err(|_| Failure::usage("AI workspace does not exist"))?;
            let inferred_url = dir
                .as_ref()
                .and_then(|d| std::fs::read_to_string(RunFile::path(d)).ok())
                .and_then(|s| serde_json::from_str::<RunFile>(&s).ok())
                .map(|r| format!("http://127.0.0.1:{}", r.port));
            let unit = match args.unit {
                Some(id) => id,
                None => {
                    let url = args
                        .url
                        .clone()
                        .or(inferred_url)
                        .unwrap_or_else(|| crate::agent::DEFAULT_URL.into());
                    let status=client(&url)?.call("scheduler/status",serde_json::Value::Null).map_err(|_|Failure::usage("connect the AI and take control first, then link it; or provide --unit explicitly"))?;
                    status["agents"].as_array().into_iter().flatten().find(|r|r["controller"]==controller).and_then(|r|r["unit"].as_u64()).and_then(|v|u32::try_from(v).ok()).ok_or_else(||Failure::usage("this controller is not attached to an actor; let the agent connect and take control first"))?
                }
            };
            let binding = RunnerBinding {
                controller: controller.clone(),
                unit,
                provider: match args.provider {
                    Provider::Codex => RunnerProvider::Codex,
                    Provider::Claude => RunnerProvider::Claude,
                    Provider::Custom => RunnerProvider::Custom,
                },
                session_id: args.session_id.clone(),
                cwd,
                program: args.program.clone(),
                args: args.runner_arg.clone(),
                client_pid: args.client_pid,
                revision: 0,
            };
            scheduler::write_binding(&path, binding.clone()).map_err(Failure::usage)?;
            if args.turn_finished {
                scheduler::clear_stale_runner_lock(&path, &binding).map_err(Failure::usage)?;
            }
            out.json(&json!({"linked":true,"controller":controller,"unit":unit,"provider":format!("{:?}",args.provider).to_lowercase(),"session":"[configured]","bindings":path}))?;
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn debug_does_not_print_conversation_identity() {
        let a = BindArgs {
            identity: IdentityArgs {
                home: None,
                agent: None,
                controller: Some("a".into()),
                bindings: None,
            },
            provider: Provider::Codex,
            session_id: "private-conversation-marker".into(),
            unit: Some(1),
            url: None,
            cwd: None,
            program: None,
            runner_arg: vec![],
            client_pid: None,
            turn_finished: true,
        };
        assert!(!format!("{a:?}").contains("private-conversation-marker"));
    }
}
