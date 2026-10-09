//! Agent identity for the Prefect gateway.
//!
//! The identity comes from the same git configuration that writes agent commit
//! trailers: `git config agent.id` names the agent (set only when an agent's
//! `GIT_CONFIG_GLOBAL` is active), and the optional `agent.provenance` command
//! prints `TOOL=`, `MODEL=` and `SESSION=` lines. Nothing is sent when no agent
//! identity is configured, so human use of pfp is unchanged.

use std::process::{Command, Stdio};

const MAX_VALUE_LEN: usize = 200;

/// Headers describing the agent running pfp, or none outside an agent session.
pub fn agent_headers() -> Vec<(&'static str, String)> {
    let Some(agent) = git_config("agent.id") else {
        return Vec::new();
    };
    let provenance = git_config("agent.provenance")
        .and_then(|command| run(&command))
        .unwrap_or_default();
    headers(&agent, &provenance)
}

fn git_config(key: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["config", "--get", key])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (output.status.success() && !value.is_empty()).then_some(value)
}

fn run(command: &str) -> Option<String> {
    let output = Command::new(command)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Build the header list from an agent id and `KEY=value` provenance lines.
pub fn headers(agent: &str, provenance: &str) -> Vec<(&'static str, String)> {
    let mut headers: Vec<(&'static str, String)> = Vec::new();
    let mut push = |name: &'static str, value: &str| {
        let value = value.trim();
        let printable = value.chars().all(|c| c.is_ascii_graphic() || c == ' ');
        if !value.is_empty() && printable && !headers.iter().any(|(existing, _)| *existing == name)
        {
            headers.push((name, value.chars().take(MAX_VALUE_LEN).collect()));
        }
    };
    push("X-Agent-Id", agent);
    for line in provenance.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "TOOL" => push("X-Agent-Tool", value),
            "MODEL" => push("X-Agent-Model", value),
            "SESSION" => push("X-Agent-Session", value),
            _ => {}
        }
    }
    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_headers_from_provenance_lines() {
        let provenance = "TOOL=claude-code 2.1.300\nMODEL=claude-opus-5-5\nSESSION=s-0001\n";
        assert_eq!(
            headers("claude", provenance),
            vec![
                ("X-Agent-Id", "claude".to_string()),
                ("X-Agent-Tool", "claude-code 2.1.300".to_string()),
                ("X-Agent-Model", "claude-opus-5-5".to_string()),
                ("X-Agent-Session", "s-0001".to_string()),
            ]
        );
    }

    #[test]
    fn keeps_values_containing_equals_and_ignores_unknown_or_malformed_lines() {
        let provenance = "noise\nOTHER=x\nMODEL=openai/gpt=6\nMODEL=second\n";
        assert_eq!(
            headers("opencode", provenance),
            vec![
                ("X-Agent-Id", "opencode".to_string()),
                ("X-Agent-Model", "openai/gpt=6".to_string()),
            ]
        );
    }

    #[test]
    fn drops_values_that_cannot_be_header_values() {
        let provenance = "TOOL=caf\u{e9}\nMODEL=\nSESSION=a\tb\n";
        assert_eq!(
            headers("codex", provenance),
            vec![("X-Agent-Id", "codex".to_string())]
        );
        assert!(headers("\u{1f916}", "").is_empty());
    }

    #[test]
    fn truncates_long_values() {
        let long = "x".repeat(500);
        let built = headers("claude", &format!("SESSION={long}"));
        assert_eq!(built[1].1.len(), MAX_VALUE_LEN);
    }

    use crate::client::PrefectClient;
    use crate::config::Config;
    use std::os::unix::fs::PermissionsExt;

    /// Build a client while GIT_CONFIG_GLOBAL points at `gitconfig`, exactly as an
    /// agent session sets it, then restore the variable.
    fn client_under(gitconfig: &std::path::Path, server: &mockito::Server) -> PrefectClient {
        let previous = std::env::var_os("GIT_CONFIG_GLOBAL");
        std::env::set_var("GIT_CONFIG_GLOBAL", gitconfig);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        let client = PrefectClient::new(Config {
            api_url: server.url(),
            auth_header: Some("Basic dGVzdDp0ZXN0".to_string()),
        });
        match previous {
            Some(value) => std::env::set_var("GIT_CONFIG_GLOBAL", value),
            None => std::env::remove_var("GIT_CONFIG_GLOBAL"),
        }
        std::env::remove_var("GIT_CONFIG_NOSYSTEM");
        client
    }

    #[tokio::test]
    #[serial_test::serial(git_agent_config)]
    async fn requests_carry_the_git_agent_identity_and_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("agent-provenance");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf 'TOOL=claude-code 2.1.300\\nMODEL=claude-opus-5-5\\nSESSION=s-1\\n'\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let gitconfig = dir.path().join("claude.gitconfig");
        std::fs::write(
            &gitconfig,
            format!(
                "[agent]\n\tid = claude\n\tprovenance = {}\n",
                script.display()
            ),
        )
        .unwrap();

        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/flow_runs/x")
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .match_header("x-agent-id", "claude")
            .match_header("x-agent-tool", "claude-code 2.1.300")
            .match_header("x-agent-model", "claude-opus-5-5")
            .match_header("x-agent-session", "s-1")
            .with_status(200)
            .with_body("{}")
            .create_async()
            .await;
        let client = client_under(&gitconfig, &server);
        let _: serde_json::Value = client.get("/flow_runs/x").await.unwrap();
        mock.assert_async().await;
    }

    #[tokio::test]
    #[serial_test::serial(git_agent_config)]
    async fn no_identity_headers_outside_an_agent_session() {
        let dir = tempfile::tempdir().unwrap();
        let gitconfig = dir.path().join("human.gitconfig");
        std::fs::write(&gitconfig, "[user]\n\tname = someone\n").unwrap();

        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/flow_runs/x")
            .match_header("x-agent-id", mockito::Matcher::Missing)
            .with_status(200)
            .with_body("{}")
            .create_async()
            .await;
        let client = client_under(&gitconfig, &server);
        let _: serde_json::Value = client.get("/flow_runs/x").await.unwrap();
        mock.assert_async().await;
    }

    #[tokio::test]
    #[serial_test::serial(git_agent_config)]
    async fn a_failing_provenance_command_still_sends_the_agent_id() {
        let dir = tempfile::tempdir().unwrap();
        let gitconfig = dir.path().join("codex.gitconfig");
        std::fs::write(
            &gitconfig,
            "[agent]\n\tid = codex\n\tprovenance = /nonexistent/agent-provenance\n",
        )
        .unwrap();

        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/flow_runs/x")
            .match_header("x-agent-id", "codex")
            .match_header("x-agent-session", mockito::Matcher::Missing)
            .with_status(200)
            .with_body("{}")
            .create_async()
            .await;
        let client = client_under(&gitconfig, &server);
        let _: serde_json::Value = client.get("/flow_runs/x").await.unwrap();
        mock.assert_async().await;
    }
}
