use crate::client::PrefectClient;
use crate::error::Result;
use crate::output;

/// Show the Prefect gateway policy that decides what agents may do.
pub async fn run(client: PrefectClient, json: bool) -> Result<()> {
    let policy = client.get_gateway_policy().await?;
    if json {
        output::print_json(&policy);
        return Ok(());
    }

    println!(
        "Policy version {} (saved {} by {})",
        policy["version"],
        policy["updated_at"].as_str().unwrap_or("-"),
        policy["updated_by"].as_str().unwrap_or("-"),
    );
    println!("First enabled, unexpired matching rule decides; no match denies.\n");
    let empty = Vec::new();
    for (index, rule) in policy["rules"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .enumerate()
    {
        let list = |key: &str, default: &str| match rule[key].as_array() {
            Some(items) => items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()
                .join(","),
            None => default.to_string(),
        };
        let enabled = rule["enabled"].as_bool().unwrap_or(true);
        println!(
            "{index:>2}  {:<5} {:<8} actions={} agents={} deployments={}{}",
            rule["effect"].as_str().unwrap_or("?"),
            if enabled { "" } else { "disabled" },
            list("actions", "*"),
            list("agents", "*"),
            list("deployments", "any"),
            rule["until"]
                .as_str()
                .map(|until| format!(" until={until}"))
                .unwrap_or_default(),
        );
        if let Some(reason) = rule["reason"].as_str().filter(|reason| !reason.is_empty()) {
            println!("    {reason}");
        }
    }
    Ok(())
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
            .with_body(r#"{"version":3,"rules":[{"effect":"allow","actions":["read"]}]}"#)
            .create_async()
            .await;
        let client = PrefectClient::new(Config {
            api_url: format!("{}/api/", server.url()),
            auth_header: Some("Basic dGVzdDp0ZXN0".to_string()),
        });
        run(client, true).await.unwrap();
        mock.assert_async().await;
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
