use crate::client::PrefectClient;
use crate::error::{PfpError, Result};
use crate::models::Deployment;

/// Resolve a user query to a single deployment via unique substring match.
pub async fn resolve_deployment(client: &PrefectClient, query: &str) -> Result<Deployment> {
    let values = client.list_deployments().await?;
    let deployments: Vec<Deployment> = values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    let matches: Vec<&Deployment> = deployments
        .iter()
        .filter(|d| d.full_name().contains(query))
        .collect();

    match matches.len() {
        0 => Err(PfpError::NoMatch(format!(
            "no deployment matching '{}'",
            query
        ))),
        1 => {
            let idx = deployments
                .iter()
                .position(|d| d.full_name().contains(query))
                .unwrap();
            Ok(deployments.into_iter().nth(idx).unwrap())
        }
        _ => {
            let candidates = matches
                .iter()
                .map(|d| format!("  {}", d.full_name()))
                .collect::<Vec<_>>()
                .join("\n");
            Err(PfpError::AmbiguousMatch {
                query: query.to_string(),
                candidates,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::models::Deployment;
    use serde_json::json;

    fn sample_deployments() -> Vec<Deployment> {
        vec![
            serde_json::from_value(json!({
                "id": "1", "name": "happy-terraform-prod", "flow_name": "happy_terraform"
            }))
            .unwrap(),
            serde_json::from_value(json!({
                "id": "2", "name": "happy-ansible-prod", "flow_name": "happy_ansible"
            }))
            .unwrap(),
            serde_json::from_value(json!({
                "id": "3", "name": "hello_world-dev", "flow_name": "hello_world"
            }))
            .unwrap(),
        ]
    }

    fn find_match<'a>(deployments: &'a [Deployment], query: &str) -> Vec<&'a Deployment> {
        deployments
            .iter()
            .filter(|d| d.full_name().contains(query))
            .collect()
    }

    #[test]
    fn unique_substring_match() {
        let deps = sample_deployments();
        let matches = find_match(&deps, "happy-t");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].full_name(),
            "happy_terraform/happy-terraform-prod"
        );
    }

    #[test]
    fn ambiguous_match() {
        let deps = sample_deployments();
        let matches = find_match(&deps, "happy");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn no_match() {
        let deps = sample_deployments();
        let matches = find_match(&deps, "nonexistent");
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn exact_full_name_match() {
        let deps = sample_deployments();
        let matches = find_match(&deps, "happy_terraform/happy-terraform-prod");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn match_by_deployment_name_only() {
        let deps = sample_deployments();
        let matches = find_match(&deps, "hello_world-dev");
        assert_eq!(matches.len(), 1);
    }
}
