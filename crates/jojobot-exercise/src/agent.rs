//! **The agent is the thing under test, so the agent is not ours.**
//!
//! An earlier design drove the model through a loop written here, over an API
//! client written here, reaching the room through an MCP client written here.
//! That instrument cannot answer this tier's question: it measures our agent
//! loop against our surface, and the model in the middle is only a text
//! generator inside a harness we built. The oracle would have been the thing
//! under test.
//!
//! So a run shells out to the shipped agent CLI, headless, with the room as its
//! MCP server. Its own client, its own tool loop, its own system prompt — the
//! way any real session meets jojobot. Nothing here re-implements any of it,
//! and the only thing this module owns is how that process is started.
//!
//! **Auth is the CLI's, not ours.** A person is already logged in to it. There
//! is no key read here, no key checked here and no key printed here.

use anyhow::{Context, Result};

/// What the CLI is called, and what a run defaults to. The model is a
/// parameter everywhere else — this is only the value a caller who said
/// nothing gets.
pub const CLI: &str = "claude";
pub const DEFAULT_MODEL: &str = "sonnet";

/// **The name a room's MCP server is given inside the CLI's config.** It is
/// arbitrary and it is deliberately not the name of anything the operator has
/// connected: a run reaches one server, and that server is the room.
const ROOM: &str = "jojobot-room";

/// Which conversation a phase belongs to.
///
/// The id is minted here rather than read back out of the CLI's output: a run
/// names its own conversations, so chaining a phase onto the last one needs no
/// parsing of anything the CLI printed and cannot drift when that output
/// changes shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conversation {
    /// A session of this run's own, starting from nothing.
    Start(String),
    /// The session started under this id, carried on.
    Carry(String),
}

impl Conversation {
    /// A fresh conversation nobody has used.
    pub fn fresh() -> Conversation {
        Conversation::Start(uuid())
    }

    /// The same conversation, for the phase after this one.
    pub fn carried(&self) -> Conversation {
        match self {
            Conversation::Start(id) | Conversation::Carry(id) => Conversation::Carry(id.clone()),
        }
    }
}

/// A command line, built and not yet run — the whole of what this module
/// decides, and therefore the whole of what its tests can hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    /// **Where the CLI is started from, and it is a directory with nothing in
    /// it.** The CLI discovers instruction files by walking up from its working
    /// directory, and `--strict-mcp-config` says nothing about those: it scopes
    /// the server map and no more. So a run that inherited the caller's
    /// directory would be started inside this repository, whose own
    /// instructions name the verbs and the properties the suite measures — and
    /// a model coached that way produces a transcript that reads like a product
    /// which works.
    pub cwd: std::path::PathBuf,
}

/// What one invocation produced.
pub struct Worked {
    /// Everything it printed.
    pub output: String,
    /// Whether the CLI itself reported success. A phase told to continue a
    /// conversation the CLI could not find is the case this exists for.
    pub ran: bool,
}

/// The shipped agent, driven headless.
pub struct Agent {
    model: String,
    cwd: std::path::PathBuf,
}

impl Agent {
    pub fn new(model: &str) -> Agent {
        Agent {
            model: model.to_string(),
            cwd: unbriefed_dir(),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// **The command line for one phase.**
    ///
    /// Pure, and separate from running it, because this is where the decisions
    /// are: which model, which conversation, and — the one that matters most —
    /// that the agent reaches this room and nothing else.
    pub fn invocation(
        &self,
        endpoint: &str,
        conversation: &Conversation,
        prompt: &str,
    ) -> Invocation {
        let config = serde_json::json!({
            "mcpServers": {ROOM: {"type": "http", "url": endpoint}},
        })
        .to_string();
        let mut args = vec!["--print".to_string()];
        match conversation {
            Conversation::Start(id) => {
                args.push("--session-id".to_string());
                args.push(id.clone());
            }
            Conversation::Carry(id) => {
                args.push("--resume".to_string());
                args.push(id.clone());
            }
        }
        args.extend([
            "--model".to_string(),
            self.model.clone(),
            "--mcp-config".to_string(),
            config,
            // **The room and nothing else.** Without this the agent would also
            // load whatever the person running it has connected, and a run
            // would reach somebody's real memory, calendar and mail from inside
            // a test. It is the single most load-bearing flag on this line.
            "--strict-mcp-config".to_string(),
            // A room is a throwaway instance on loopback with one server
            // attached, and a headless run that stops to ask about every call
            // is a run that never finishes.
            "--permission-mode".to_string(),
            "bypassPermissions".to_string(),
        ]);
        args.push(prompt.to_string());
        Invocation {
            program: CLI.to_string(),
            args,
            cwd: self.cwd.clone(),
        }
    }

    /// Run one phase and hand back everything it printed.
    ///
    /// **Whatever the CLI says is the transcript.** It is not parsed, scored or
    /// summarized here: what the agent reached for is evidence a person reads,
    /// and the assertions are over the room's state rather than over any of it.
    pub async fn work(
        &self,
        endpoint: &str,
        conversation: &Conversation,
        prompt: &str,
    ) -> Result<Worked> {
        let invocation = self.invocation(endpoint, conversation, prompt);
        std::fs::create_dir_all(&invocation.cwd).with_context(|| {
            format!(
                "making the directory the agent is started in at {}",
                invocation.cwd.display(),
            )
        })?;
        let done = tokio::process::Command::new(&invocation.program)
            .args(&invocation.args)
            .current_dir(&invocation.cwd)
            .output()
            .await
            .with_context(|| {
                format!(
                    "running {} — the agent CLI is the operator's own tool, the same one they are \
                     already logged in to, and this project neither ships nor installs it: it has \
                     to be on PATH before a run starts",
                    invocation.program,
                )
            })?;
        let mut said = String::from_utf8_lossy(&done.stdout).to_string();
        if !done.status.success() {
            said.push_str(&format!(
                "\n[the agent exited {}]\n{}",
                done.status,
                String::from_utf8_lossy(&done.stderr),
            ));
        }
        Ok(Worked {
            output: said,
            // **A refused resume is the failure that looks like success.** The
            // CLI is asked to carry a conversation on; if it cannot, an
            // invocation that came back non-zero is the one signal the harness
            // has without spending anything, and a phase that lost its memory
            // and carried on regardless produces a transcript a reader would
            // pass.
            ran: done.status.success(),
        })
    }
}

impl Drop for Agent {
    /// The directory exists only to be empty, so it goes when the agent does.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.cwd);
    }
}

/// A directory of this agent's own, outside any tree that briefs whoever runs
/// in it — see [`Invocation::cwd`].
fn unbriefed_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("jojobot-agent-{}-{}", std::process::id(), uuid()))
}

/// A version-4 UUID, from OS entropy. The CLI takes one to name a
/// conversation; nothing here needs it to be anything more than unique.
fn uuid() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("OS entropy");
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flag_after(invocation: &Invocation, flag: &str) -> Option<String> {
        let at = invocation.args.iter().position(|a| a == flag)?;
        invocation.args.get(at + 1).cloned()
    }

    /// **A run reaches the room and nothing else.**
    ///
    /// The hazard this holds off is not hypothetical: a person running this has
    /// their own connectors configured, and an agent that loaded them would
    /// reach real memory, real mail and a real calendar from inside a test that
    /// believes it is in a throwaway room. Both halves in one case — the room
    /// is configured, and the flag that excludes everything else is on the line
    /// — because either alone passes on a build that has the other wrong.
    #[test]
    fn a_phase_reaches_the_room_and_nothing_the_operator_has_connected() {
        let line = Agent::new("sonnet").invocation(
            "http://127.0.0.1:31000/mcp",
            &Conversation::fresh(),
            "do the thing",
        );
        let config = flag_after(&line, "--mcp-config").expect("a config is passed");
        let parsed: serde_json::Value = serde_json::from_str(&config).expect("it is JSON");
        let servers = parsed["mcpServers"].as_object().expect("servers");
        assert_eq!(
            servers.len(),
            1,
            "a run configures exactly one server: {config}",
        );
        assert_eq!(
            servers[ROOM]["url"], "http://127.0.0.1:31000/mcp",
            "the one server is this room: {config}",
        );
        assert!(
            line.args.iter().any(|a| a == "--strict-mcp-config"),
            "without this the agent also loads whatever else is configured: {:?}",
            line.args,
        );
    }

    /// **The playbook decides how many conversations a run uses.** A phase that
    /// says it must start fresh gets an id nobody has used; one that does not
    /// carries the last one on. Both directions, because a build that always
    /// started fresh would pass a check that only ever looked at the fresh one.
    #[test]
    fn a_fresh_phase_starts_a_conversation_and_a_carried_one_resumes_it() {
        let agent = Agent::new("sonnet");
        let first = Conversation::fresh();
        let opened = agent.invocation("http://room", &first, "first");
        let started = flag_after(&opened, "--session-id").expect("a fresh phase names its session");
        assert!(
            flag_after(&opened, "--resume").is_none(),
            "a fresh phase resumed something: {:?}",
            opened.args,
        );

        let carried = agent.invocation("http://room", &first.carried(), "second");
        assert_eq!(
            flag_after(&carried, "--resume").as_deref(),
            Some(started.as_str()),
            "a carried phase must resume the conversation before it: {:?}",
            carried.args,
        );

        let elsewhere = agent.invocation("http://room", &Conversation::fresh(), "third");
        assert_ne!(
            flag_after(&elsewhere, "--session-id"),
            Some(started),
            "two fresh phases were given one conversation",
        );
    }

    /// The model is a parameter, and the default is the cheap one.
    #[test]
    fn the_model_is_whatever_the_run_was_told() {
        let named = Agent::new("opus").invocation("http://room", &Conversation::fresh(), "go");
        assert_eq!(flag_after(&named, "--model").as_deref(), Some("opus"));
        assert_eq!(
            Agent::new(DEFAULT_MODEL).model(),
            "sonnet",
            "the default a caller gets when they say nothing",
        );
    }

    /// **The harness may furnish the room; it may not coach the occupant.**
    ///
    /// The CLI finds instruction files by walking up from where it was
    /// started, and this repository's own root carries a set that names the
    /// verbs and the properties this suite exists to measure. An agent started
    /// inside that tree is scored on how well it read our house rules, and a
    /// run coached that way looks exactly like a product that works.
    ///
    /// The positive it rests on is in the same case: the instruction file it is
    /// being kept away from is looked up rather than assumed, so a repository
    /// that stopped carrying one fails here instead of passing vacuously.
    #[test]
    fn the_agent_is_started_outside_the_tree_that_carries_our_instructions() {
        let line = Agent::new("sonnet").invocation("http://room", &Conversation::fresh(), "go");
        let coaching = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|dir| dir.join("CLAUDE.md").is_file())
            .expect("this repository carries instructions, which is the whole reason for the case");
        assert!(
            !line.cwd.starts_with(coaching),
            "the agent is started under {}, which carries the instructions it must not read: {}",
            coaching.display(),
            line.cwd.display(),
        );
    }

    /// Headless, or the run waits for somebody who is not there.
    #[test]
    fn a_run_is_headless() {
        let line = Agent::new("sonnet").invocation("http://room", &Conversation::fresh(), "go");
        assert!(
            line.args.iter().any(|a| a == "--print"),
            "an interactive agent in a make target is a make target that hangs: {:?}",
            line.args,
        );
        assert_eq!(
            line.args.last().map(String::as_str),
            Some("go"),
            "the prompt is the last thing on the line: {:?}",
            line.args,
        );
    }
}
