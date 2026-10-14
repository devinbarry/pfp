use crate::client::PrefectClient;
use crate::error::Result;
use crate::output;

/// Show the Prefect gateway policy that decides what agents may do, and any change waiting for activation.
pub async fn run(client: PrefectClient, json: bool) -> Result<()> {
    let policy = client.get_gateway_policy().await?;
    if json {
        output::print_json(&policy);
        return Ok(());
    }
    print!("{}", render(&policy));
    Ok(())
}

/// Whether a described policy belongs in one of the printed groups.
type Membership = fn(&serde_json::Value) -> bool;

fn short(value: &serde_json::Value, length: usize) -> String {
    value
        .as_str()
        .map(|text| text.chars().take(length).collect())
        .unwrap_or_else(|| "-".to_string())
}

/// The enforced Cedar policy as the gateway describes it: one sentence per policy, grouped by what it does.
fn render(policy: &serde_json::Value) -> String {
    let mut out = String::new();
    let active = &policy["active"];
    if active.is_object() {
        out.push_str(&format!(
            "Enforcing policy {} from commit {}, activated by {} at {}\n",
            short(&active["digest"], 12),
            short(&active["commit"], 10),
            active["activated_by"].as_str().unwrap_or("-"),
            active["activated_at"].as_str().unwrap_or("-"),
        ));
    } else {
        out.push_str("No policy has been activated yet; agents may only read.\n");
    }
    out.push_str("Anything no policy allows is denied; a forbid always wins.\n");
    let empty = Vec::new();
    let policies = policy["policies"].as_array().unwrap_or(&empty);
    let groups: [(&str, Membership); 3] = [
        ("Never allowed", |p| {
            p["effect"] == "forbid" && p["template"] != true
        }),
        ("Allowed", |p| {
            p["effect"] == "permit" && p["template"] != true
        }),
        ("Can be granted", |p| p["template"] == true),
    ];
    for (title, belongs) in groups {
        let members: Vec<_> = policies.iter().filter(|p| belongs(p)).collect();
        if members.is_empty() {
            continue;
        }
        out.push_str(&format!("\n{title}:\n"));
        for member in members {
            out.push_str(&format!(
                "  {:<16} {}\n",
                member["id"].as_str().unwrap_or("?"),
                member["description"].as_str().unwrap_or(""),
            ));
        }
    }
    let proposal = &policy["proposal"];
    if proposal.is_object() {
        out.push_str(&format!(
            "\nWaiting for the operator: policy {} from commit {}",
            short(&proposal["digest"], 12),
            short(&proposal["commit"], 10),
        ));
        if let Some(error) = proposal["error"].as_str() {
            out.push_str(&format!(" cannot be enforced: {error}\n"));
        } else if proposal["rejection"].is_object() {
            out.push_str(" was rejected.\n");
        } else {
            out.push_str(" is waiting for activation.\n");
            for change in proposal["changes"].as_array().unwrap_or(&empty) {
                out.push_str(&format!(
                    "  {:<8} {:<16} {}\n",
                    change["change"].as_str().unwrap_or("?"),
                    change["id"].as_str().unwrap_or("?"),
                    change["description"].as_str().unwrap_or(""),
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn reads_the_policy_beside_the_api() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/gateway/policy")
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"active":null,"policies":[],"proposal":null}"#)
            .create_async()
            .await;
        let client = PrefectClient::new(Config {
            api_url: format!("{}/api/", server.url()),
            auth_header: Some("Basic dGVzdDp0ZXN0".to_string()),
        });
        run(client, true).await.unwrap();
        mock.assert_async().await;
    }

    #[test]
    fn renders_the_enforced_policy_by_group_and_the_waiting_change() {
        let policy = serde_json::json!({
            "active": {"digest": "abcdef0123456789", "commit": "1234567890abcdef", "activated_by": "devin",
                       "activated_at": "2026-10-14T22:00:00Z"},
            "policies": [
                {"id": "agents-read", "description": "Agents may read Prefect state.", "effect": "permit",
                 "template": false},
                {"id": "never-other", "description": "Never: other.", "effect": "forbid", "template": false},
                {"id": "grant-approve", "description": "Temporary approval.", "effect": "permit", "template": true}
            ],
            "proposal": {"digest": "ffff000011112222", "commit": "aaaabbbbccccdddd", "rejection": null, "error": null,
                         "changes": [{"id": "agents-read", "change": "changed", "description": "Agents read."}]}
        });
        let text = render(&policy);
        assert!(text.starts_with(
            "Enforcing policy abcdef012345 from commit 1234567890, activated by devin"
        ));
        let never = text.find("Never allowed:").unwrap();
        let allowed = text.find("Allowed:").unwrap();
        let granted = text.find("Can be granted:").unwrap();
        assert!(never < allowed && allowed < granted, "{text}");
        assert!(text.contains("  never-other      Never: other."), "{text}");
        assert!(text.contains(
            "Waiting for the operator: policy ffff00001111 from commit aaaabbbbcc is waiting"
        ));
        assert!(
            text.contains("  changed  agents-read      Agents read."),
            "{text}"
        );
    }

    #[test]
    fn says_agents_only_read_before_the_first_activation() {
        let text = render(&serde_json::json!({"active": null, "policies": [], "proposal": null}));
        assert!(text.starts_with("No policy has been activated yet; agents may only read."));
    }

    #[tokio::test]
    async fn an_api_url_without_api_suffix_is_a_config_error() {
        let client = PrefectClient::new(Config {
            api_url: "https://prefect.example".to_string(),
            auth_header: None,
        });
        let err = run(client, false).await.unwrap_err();
        assert!(err.to_string().contains("does not end in /api"), "{err}");
    }
}
