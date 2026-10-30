//! The Bash hook refuses expensive local commands before they can execute.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

mod support;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn invoke(hook: &mut Command, command: &str) -> Output {
    let input = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": command }
    });
    let mut child = support::fenced_spawn(
        hook.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .expect("start the hook");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn direct(command: &str) -> Output {
    invoke(
        Command::new("python3").arg(root().join("scripts/dev-command-gate")),
        command,
    )
}

fn refused(out: &Output) -> bool {
    let text = String::from_utf8_lossy(&out.stderr);
    out.status.code() == Some(2)
        && out.stdout.is_empty()
        && text.lines().count() == 1
        && ["make dev", "make narrow", "CRATE=", "FILTER="]
            .iter()
            .all(|form| text.contains(form))
}

#[test]
fn dev_command_gate_refuses_local_build_bypasses() {
    let commands = [
        "cargo test -p x",
        "cargo build",
        "cargo run -p jojobot",
        "cargo check",
        "nix develop -c cargo test -p x",
        "make check",
        "make report",
        "make test",
        "make",
        "make dev check",
        "git worktree add /tmp/alpha",
        "git -C /tmp/alpha worktree add /tmp/beta",
        "git clone /tmp/alpha /tmp/beta",
        "export CARGO_TARGET_DIR=/tmp/alpha",
        "CARGO_TARGET_DIR=/tmp/alpha make dev",
        "env CARGO_TARGET_DIR=/tmp/alpha make narrow CRATE=jojobot-bar FILTER=foo",
        "nix develop -c env CARGO_TARGET_DIR=/tmp/alpha make dev",
        "make narrow CRATE=jojobot-bar FILTER='foo bar'",
        "make narrow CRATE=jojobot-bar FILTER=foo FILTER=bar",
        "make narrow CRATE=jojobot-bar",
        "git status && cargo test",
        "git status\nmake report",
        "scripts/sabotage file before after -- make check",
        "make check > /tmp/alpha.log 2>&1",
        "make narrow CRATE=jojobot-bar FILTER=foo >/tmp/alpha.log;cargo test",
        "make narrow CRATE=jojobot-bar FILTER=foo > \"/tmp/alpha log\"; make check",
        "make check &> /tmp/alpha.log",
        "make narrow CRATE=jojobot-bar FILTER=foo > a\\\"; cargo test; echo \"x\"",
        "make narrow CRATE=jojobot-bar FILTER=foo > a\\'; cargo test; echo 'x'",
        "make narrow CRATE=jojobot-bar FILTER=foo > a\\\"; make check; echo \"x\"",
        "make narrow CRATE=jojobot-bar FILTER=foo > \"$(cargo test)\"",
        "make narrow CRATE=jojobot-bar FILTER=foo > \"`cargo test`\"",
        "make narrow CRATE=jojobot-bar FILTER=foo > /tmp/a\\ b; cargo test",
        "make narrow CRATE=jojobot-bar FILTER=foo > \"/tmp/a\"b; cargo test",
        "echo a \\> b; cargo test",
        "cargo test -p x 2>&1 | tail -5",
        "make narrow CRATE=jojobot-bar FILTER=foo CARGO_TARGET_DIR=/tmp/alpha > /tmp/alpha.log",
        // A backslash and a newline join two lines, so these are `cargo test`.
        "cargo \\\ntest -p x",
        "make \\\ncheck",
        // A shell keyword or a wrapper leads to the command that follows it.
        "if true; then cargo test; fi",
        "while true; do make check; done",
        "for i in 1 2; do cargo build; done",
        "{ cargo test; }",
        "! cargo test",
        "time cargo test",
        "time -p cargo test",
        "nohup cargo test",
        "nohup make check > /tmp/alpha.log 2>&1 &",
        "timeout 5 cargo test",
        "timeout -k 1 5 cargo test",
        "xargs cargo test",
        "xargs -n 1 -I x cargo test x",
        "nice -n 5 cargo test",
        "sudo -u alpha cargo test",
        "env -i cargo test",
        "env -u HOME make check",
        "command -p cargo test",
        // A string handed to a shell, or evaluated, is read as a command.
        "eval cargo test",
        "eval 'make check'",
        "sh -c 'cargo test'",
        "bash -c \"make check\"",
        "bash -lc 'cargo build'",
        "sh -c 'sh -c \"cargo test\"'",
        "xargs sh -c 'cargo test'",
        // A command substitution runs, in quotes or in backquotes.
        "echo \"$(cargo test)\"",
        "echo `cargo test`",
        "echo \"`make check`\"",
        "echo \"a $(echo $(cargo test)) b\"",
        "echo `echo \\`cargo test\\``",
        // An even run of backslashes before a newline joins nothing: the newline ends
        // the command, and the next line is a command of its own.
        "echo x \\\\\ncargo test",
        // A word is cut at whitespace only, so these stay one word each.
        "xargs -I{} cargo test {}",
        "sudo -u user@host cargo test",
        "nice -n+5 cargo test",
        "echo a#b; cargo test",
        // A quoted program name is still the program.
        "\"time\" cargo test",
        "'nohup' make check",
        // `env -S` runs the string it is given.
        "env -S 'cargo test'",
        "env --split-string='make check'",
        // Cargo reads its short aliases, and nix runs a command after --command.
        "cargo t -p x",
        "cargo b",
        "cargo r",
        "cargo c",
        "nix develop --command cargo test",
        "nix shell nixpkgs#alpha -c cargo test",
        "nix shell nixpkgs#alpha --command make check",
        // Nesting past four levels is refused (four are allowed below).
        "eval eval eval eval eval make dev",
        // A comment ends at its newline, and what follows is a command. A comment
        // is dropped only where the shell reads one: not inside backquotes, `${}`
        // or `$'..'`, and not after an escaped space.
        "echo a #b\ncargo test",
        "echo a;# note\ncargo test",
        "echo a\\ #b; cargo test",
        "echo ${x:- #y}; cargo test",
        "echo ${x:-a #y}; cargo test",
        "echo `echo a #b`; cargo test",
        "x=`echo a #b`; cargo test",
        "echo $'\\' #'; cargo test",
        // A backslash at the end of a comment line joins nothing: the comment ends
        // at the newline.
        "echo a # c\\\ncargo test",
        "echo x #\\\ncargo test",
        // `env -S` in an option cluster.
        "env -iS 'cargo test'",
        "env -i0S 'make check'",
        // Cargo reads its subcommand, past its own options and a toolchain.
        "cargo +nightly t",
        "cargo --locked test",
        "cargo --color always build",
        "cargo -Z unstable-options check",
        // A shell string after a script name is the script's argument, but a string
        // after the options is run.
        "bash -x -c 'cargo test'",
        // An apostrophe in a comment opens no quote: the substitution after it runs.
        "echo # it's\necho \"$(cargo test)\"",
        // A backquote value with a space is one word of the assignment, and the
        // command after the assignment is `cargo test`.
        "FOO=`echo x` cargo test",
        // An option that starts with `+` is still an option, so -c still follows it.
        "bash +x -c \"cargo test\"",
        "bash +o noglob -c 'make check'",
    ];
    let failures: Vec<_> = commands
        .iter()
        .filter_map(|command| {
            let out = direct(command);
            (!refused(&out)).then(|| format!("{command}: {out:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn dev_command_gate_allows_narrow_and_read_commands() {
    let commands = [
        "make dev",
        "make narrow CRATE=jojobot-domain FILTER=foo",
        "nix develop -c make narrow CRATE=jojobot-bar FILTER=foo",
        "scripts/sabotage file before after -- make narrow CRATE=jojobot-domain FILTER=foo",
        "scripts/sabotage file 'cargo test' 'make check' -- make narrow CRATE=jojobot-bar FILTER=foo",
        "make narrow CRATE=jojobot-domain FILTER=foo > /tmp/alpha.log 2>&1",
        "make narrow CRATE=jojobot-domain FILTER=foo >/tmp/alpha.log 2>/dev/null",
        "make narrow CRATE=jojobot-domain FILTER=foo >> /tmp/alpha.log 2>&1",
        "make narrow CRATE=jojobot-domain FILTER=foo 2>&1",
        "make narrow CRATE=jojobot-domain FILTER=foo < /dev/null",
        "make narrow CRATE=jojobot-domain FILTER=foo &> /tmp/alpha.log",
        "make narrow CRATE=jojobot-domain FILTER=foo > \"/tmp/alpha log\" 2>&1",
        "echo 'a > b'",
        "echo a \\> b",
        "scripts/sabotage file before after -- nix develop -c make narrow CRATE=jojobot-domain FILTER=foo > /tmp/alpha.log 2>&1; echo exit=$?",
        "git status",
        "git log --oneline -3",
        "git diff --stat",
        "git show HEAD:Makefile",
        "git fetch forgejo",
        "git switch -c dev-alpha-work",
        "git -C /tmp/alpha status",
        "git log --grep 'cargo test'",
        "echo cargo test",
        "echo \"$CARGO_TARGET_DIR\"",
        "printf '%s\\n' 'make check'",
        "make narrow \\\nCRATE=jojobot-domain FILTER=foo",
        "if true; then echo ok; fi",
        "while read x; do echo $x; done < /tmp/alpha.log",
        "time make narrow CRATE=jojobot-domain FILTER=foo",
        "nohup make narrow CRATE=jojobot-domain FILTER=foo > /tmp/alpha.log 2>&1 &",
        "timeout 300 make narrow CRATE=jojobot-domain FILTER=foo",
        "xargs echo",
        "nice -n 5 make narrow CRATE=jojobot-domain FILTER=foo",
        "env -i make dev",
        "eval echo ok",
        "sh -c 'make narrow CRATE=jojobot-domain FILTER=foo'",
        "bash -c \"git status\"",
        "echo \"$(date)\"",
        "echo `date`",
        "echo \"$(git rev-parse HEAD)\"",
        "git log --grep '$(cargo test)'",
        "make narrow CRATE=jojobot-domain FILTER=a::b",
        "make narrow CRATE=jojobot-domain FILTER=foo # a note",
        "make narrow CRATE=jojobot-bar FILTER=foo # > \"x",
        "echo a#b",
        "eval eval eval eval make dev",
        "command -v cargo",
        "nix develop --command make dev",
        "env -S 'make dev'",
        "echo `echo \\`date\\``",
        // A comment where the shell reads one is dropped, and one inside a
        // substitution stays inside it.
        "echo a #b",
        "echo ${x:- #y}",
        "echo `echo a #b`",
        "echo a\\ #b",
        "make narrow CRATE=jojobot-bar FILTER=foo # a note\nmake dev",
        // Cargo subcommands that only carry a crate named like a short alias.
        "cargo add b",
        "cargo new r",
        "cargo fmt",
        // Words that only look at or print a command or a script.
        "command -v make",
        "bash ./run.sh -c 'cargo test'",
        "make -j 8 narrow CRATE=jojobot-domain FILTER=foo",
        "make -j narrow CRATE=jojobot-domain FILTER=foo",
        // The same three shapes with a harmless command.
        "echo # it's\necho \"$(date)\"",
        "FOO=`echo x` make dev",
        "bash +x -c 'make dev'",
    ];
    let failures: Vec<_> = commands
        .iter()
        .filter_map(|command| {
            let out = direct(command);
            (!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty()))
                .then(|| format!("{command}: {out:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn dev_command_gate_project_settings_run_the_guard() {
    let settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root().join(".claude/settings.json")).unwrap())
            .unwrap();
    let groups = settings["hooks"]["PreToolUse"]
        .as_array()
        .expect("project PreToolUse hooks");
    let bash = groups
        .iter()
        .find(|group| group["matcher"] == "Bash")
        .expect("Bash matcher");
    let hook = &bash["hooks"][0];
    assert_eq!(hook["type"], "command");
    let mut command = Command::new("sh");
    command
        .args(["-c", hook["command"].as_str().unwrap()])
        .env("CLAUDE_PROJECT_DIR", root())
        .current_dir(root().join("crates/jojobot-bar"));
    let out = invoke(&mut command, "cargo test");
    assert!(refused(&out), "{out:?}");
    let out = invoke(&mut command, "make dev");
    assert!(out.status.success(), "{out:?}");
    assert!(out.stdout.is_empty() && out.stderr.is_empty(), "{out:?}");
}
