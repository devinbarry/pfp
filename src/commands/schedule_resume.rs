use crate::client::PrefectClient;
use crate::error::{PfpError, Result};
use crate::models::DeploymentSchedule;
use crate::resolve;
use std::collections::HashSet;

pub async fn run(client: PrefectClient, query: String) -> Result<()> {
    let deployment = resolve::resolve_deployment(&client, &query).await?;
    let schedules = client.read_deployment_schedules(&deployment.id).await?;

    if schedules.is_empty() {
        return Err(PfpError::Validation(format!(
            "deployment '{}' has no schedules to activate",
            deployment.full_name()
        )));
    }

    let originally_inactive: Vec<DeploymentSchedule> = schedules
        .iter()
        .filter(|schedule| !schedule.active)
        .cloned()
        .collect();

    for schedule in &originally_inactive {
        if let Err(error) = client
            .set_deployment_schedule_active(&deployment.id, &schedule.id, true)
            .await
        {
            return Err(rollback_after_failure(
                &client,
                &deployment.id,
                &originally_inactive,
                error,
            )
            .await);
        }
    }

    match verify_all_schedules_active(&client, &deployment.id, &schedules).await {
        Ok(count) => {
            eprintln!(
                "Activated {} schedule(s): {}",
                count,
                deployment.full_name()
            );
            Ok(())
        }
        Err(error) => {
            Err(rollback_after_failure(&client, &deployment.id, &originally_inactive, error).await)
        }
    }
}

async fn verify_all_schedules_active(
    client: &PrefectClient,
    deployment_id: &str,
    expected: &[DeploymentSchedule],
) -> Result<usize> {
    let current = client.read_deployment_schedules(deployment_id).await?;
    let expected_ids: HashSet<&str> = expected
        .iter()
        .map(|schedule| schedule.id.as_str())
        .collect();
    let current_ids: HashSet<&str> = current
        .iter()
        .map(|schedule| schedule.id.as_str())
        .collect();

    if current_ids != expected_ids {
        return Err(PfpError::Api(
            "deployment schedules changed while they were being activated".to_string(),
        ));
    }

    let inactive: Vec<&str> = current
        .iter()
        .filter(|schedule| !schedule.active)
        .map(|schedule| schedule.id.as_str())
        .collect();
    if !inactive.is_empty() {
        return Err(PfpError::Api(format!(
            "schedule activation did not persist for: {}",
            inactive.join(", ")
        )));
    }

    Ok(current.len())
}

async fn rollback_after_failure(
    client: &PrefectClient,
    deployment_id: &str,
    originally_inactive: &[DeploymentSchedule],
    cause: PfpError,
) -> PfpError {
    let mut rollback_errors = Vec::new();
    for schedule in originally_inactive {
        if let Err(error) = client
            .set_deployment_schedule_active(deployment_id, &schedule.id, false)
            .await
        {
            rollback_errors.push(format!("{}: {}", schedule.id, error));
        }
    }

    let state = client.read_deployment_schedules(deployment_id).await;
    let expected_inactive: HashSet<&str> = originally_inactive
        .iter()
        .map(|schedule| schedule.id.as_str())
        .collect();

    match state {
        Ok(current) => {
            let still_active: Vec<&str> = current
                .iter()
                .filter(|schedule| expected_inactive.contains(schedule.id.as_str()) && schedule.active)
                .map(|schedule| schedule.id.as_str())
                .collect();
            if rollback_errors.is_empty() && still_active.is_empty() {
                PfpError::Api(format!(
                    "schedule activation failed ({cause}); rollback restored the original inactive state"
                ))
            } else {
                let active_detail = if still_active.is_empty() {
                    "none".to_string()
                } else {
                    still_active.join(", ")
                };
                let error_detail = if rollback_errors.is_empty() {
                    "none".to_string()
                } else {
                    rollback_errors.join("; ")
                };
                PfpError::Api(format!(
                    "schedule activation failed ({cause}); rollback incomplete; originally inactive schedules still active: {active_detail}; rollback errors: {error_detail}"
                ))
            }
        }
        Err(verify_error) => PfpError::Api(format!(
            "schedule activation failed ({cause}); rollback state could not be verified ({verify_error}); rollback errors: {}",
            if rollback_errors.is_empty() {
                "none".to_string()
            } else {
                rollback_errors.join("; ")
            }
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn test_client(server: &mockito::Server) -> PrefectClient {
        PrefectClient::new(Config {
            api_url: server.url(),
            auth_header: Some("Basic dGVzdDp0ZXN0".to_string()),
        })
    }

    async fn mock_resolution(server: &mut mockito::Server) -> (mockito::Mock, mockito::Mock) {
        let deployments = server
            .mock("POST", "/deployments/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"id":"dep-1","name":"backup-prod","flow_id":"flow-1","flow_name":"backup"}]"#,
            )
            .expect(1)
            .create_async()
            .await;
        let flows = server
            .mock("POST", "/flows/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"flow-1","name":"backup"}]"#)
            .expect(1)
            .create_async()
            .await;
        (deployments, flows)
    }

    #[tokio::test]
    async fn activates_every_schedule_and_verifies_final_state() {
        let mut server = mockito::Server::new_async().await;
        let (deployments, flows) = mock_resolution(&mut server).await;
        let initial = server
            .mock("GET", "/deployments/dep-1/schedules")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"schedule-1","active":false},{"id":"schedule-2","active":true}]"#)
            .expect(1)
            .create_async()
            .await;
        let activate = server
            .mock("PATCH", "/deployments/dep-1/schedules/schedule-1")
            .match_body(mockito::Matcher::JsonString(
                serde_json::json!({"active": true}).to_string(),
            ))
            .with_status(204)
            .expect(1)
            .create_async()
            .await;
        let verified = server
            .mock("GET", "/deployments/dep-1/schedules")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"schedule-1","active":true},{"id":"schedule-2","active":true}]"#)
            .expect(1)
            .create_async()
            .await;

        run(test_client(&server), "backup-prod".to_string())
            .await
            .unwrap();

        deployments.assert_async().await;
        flows.assert_async().await;
        initial.assert_async().await;
        activate.assert_async().await;
        verified.assert_async().await;
    }

    #[tokio::test]
    async fn fails_without_mutation_when_deployment_has_no_schedules() {
        let mut server = mockito::Server::new_async().await;
        let (deployments, flows) = mock_resolution(&mut server).await;
        let schedules = server
            .mock("GET", "/deployments/dep-1/schedules")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("[]")
            .expect(1)
            .create_async()
            .await;

        let error = run(test_client(&server), "backup-prod".to_string())
            .await
            .unwrap_err();

        assert!(error.to_string().contains("has no schedules to activate"));
        deployments.assert_async().await;
        flows.assert_async().await;
        schedules.assert_async().await;
    }

    #[tokio::test]
    async fn rolls_back_all_initially_inactive_schedules_after_partial_failure() {
        let mut server = mockito::Server::new_async().await;
        let (_deployments, _flows) = mock_resolution(&mut server).await;
        let initial = server
            .mock("GET", "/deployments/dep-1/schedules")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"id":"schedule-1","active":false},{"id":"schedule-2","active":false},{"id":"schedule-3","active":true}]"#,
            )
            .expect(1)
            .create_async()
            .await;
        let activate_first = server
            .mock("PATCH", "/deployments/dep-1/schedules/schedule-1")
            .match_body(mockito::Matcher::JsonString(
                serde_json::json!({"active": true}).to_string(),
            ))
            .with_status(204)
            .expect(1)
            .create_async()
            .await;
        let activate_second = server
            .mock("PATCH", "/deployments/dep-1/schedules/schedule-2")
            .match_body(mockito::Matcher::JsonString(
                serde_json::json!({"active": true}).to_string(),
            ))
            .with_status(500)
            .with_body("boom")
            .expect(1)
            .create_async()
            .await;
        let rollback_first = server
            .mock("PATCH", "/deployments/dep-1/schedules/schedule-1")
            .match_body(mockito::Matcher::JsonString(
                serde_json::json!({"active": false}).to_string(),
            ))
            .with_status(204)
            .expect(1)
            .create_async()
            .await;
        let rollback_second = server
            .mock("PATCH", "/deployments/dep-1/schedules/schedule-2")
            .match_body(mockito::Matcher::JsonString(
                serde_json::json!({"active": false}).to_string(),
            ))
            .with_status(204)
            .expect(1)
            .create_async()
            .await;
        let restored = server
            .mock("GET", "/deployments/dep-1/schedules")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"id":"schedule-1","active":false},{"id":"schedule-2","active":false},{"id":"schedule-3","active":true}]"#,
            )
            .expect(1)
            .create_async()
            .await;

        let error = run(test_client(&server), "backup-prod".to_string())
            .await
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("rollback restored the original inactive state"));
        initial.assert_async().await;
        activate_first.assert_async().await;
        activate_second.assert_async().await;
        rollback_first.assert_async().await;
        rollback_second.assert_async().await;
        restored.assert_async().await;
    }

    #[tokio::test]
    async fn reports_exact_schedule_when_rollback_is_incomplete() {
        let mut server = mockito::Server::new_async().await;
        let (_deployments, _flows) = mock_resolution(&mut server).await;
        let initial = server
            .mock("GET", "/deployments/dep-1/schedules")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"schedule-1","active":false}]"#)
            .expect(1)
            .create_async()
            .await;
        let activation = server
            .mock("PATCH", "/deployments/dep-1/schedules/schedule-1")
            .match_body(mockito::Matcher::JsonString(
                serde_json::json!({"active": true}).to_string(),
            ))
            .with_status(500)
            .with_body("lost response")
            .expect(1)
            .create_async()
            .await;
        let rollback = server
            .mock("PATCH", "/deployments/dep-1/schedules/schedule-1")
            .match_body(mockito::Matcher::JsonString(
                serde_json::json!({"active": false}).to_string(),
            ))
            .with_status(500)
            .with_body("rollback failed")
            .expect(1)
            .create_async()
            .await;
        let final_state = server
            .mock("GET", "/deployments/dep-1/schedules")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"schedule-1","active":true}]"#)
            .expect(1)
            .create_async()
            .await;

        let error = run(test_client(&server), "backup-prod".to_string())
            .await
            .unwrap_err();
        let message = error.to_string();

        assert!(message.contains("rollback incomplete"));
        assert!(message.contains("still active: schedule-1"));
        assert!(message.contains("rollback failed"));
        initial.assert_async().await;
        activation.assert_async().await;
        rollback.assert_async().await;
        final_state.assert_async().await;
    }
}
