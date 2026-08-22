use crate::client::PrefectClient;
use crate::error::Result;
use crate::models::{Deployment, DeploymentSchedule};
use crate::output;
use crate::resolve;
use serde::Serialize;

/// What one deployment is registered as, gathered into a single object so
/// `--json` is one document rather than two calls' worth of fragments.
#[derive(Serialize)]
struct DeploymentDetail<'a> {
    #[serde(flatten)]
    deployment: &'a Deployment,
    schedules: &'a [DeploymentSchedule],
}

pub async fn inspect(client: PrefectClient, query: String, json: bool) -> Result<()> {
    let deployment = resolve::resolve_deployment(&client, &query).await?;
    let schedules = client.read_deployment_schedules(&deployment.id).await?;

    if json {
        output::print_json(&DeploymentDetail {
            deployment: &deployment,
            schedules: &schedules,
        });
    } else {
        output::print_deployment_detail(&deployment, &schedules);
    }

    Ok(())
}

pub async fn delete(client: PrefectClient, query: String) -> Result<()> {
    let deployment = resolve::resolve_deployment(&client, &query).await?;
    client.delete_deployment(&deployment.id).await?;
    eprintln!("Deleted: {}", deployment.full_name());
    Ok(())
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

    fn deployment_page(id: &str) -> String {
        format!(
            r#"[{{"id":"{id}","name":"ansible-deploy-prod","flow_id":"f1","paused":false,
                 "version":"1.2.3","entrypoint":"flows/x/src/flow.py:flow",
                 "work_pool_name":"host-general","work_queue_name":"default",
                 "enforce_parameter_schema":true,
                 "parameters":{{"deployment_name":"host","playbook_name":"deploy_host"}}}}]"#
        )
    }

    async fn mount(server: &mut mockito::Server, id: &str) -> Vec<mockito::Mock> {
        vec![
            server
                .mock("POST", "/deployments/filter")
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(deployment_page(id))
                .create_async()
                .await,
            server
                .mock("POST", "/flows/filter")
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(r#"[{"id":"f1","name":"ansible_deploy"}]"#)
                .create_async()
                .await,
            server
                .mock("GET", format!("/deployments/{id}/schedules").as_str())
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(
                    r#"[{"id":"s1","active":true,"slug":"nightly",
                         "schedule":{"cron":"0 3 * * *","timezone":"UTC"}}]"#,
                )
                .expect(1)
                .create_async()
                .await,
        ]
    }

    #[tokio::test]
    async fn reads_the_deployment_and_its_schedules() {
        let mut server = mockito::Server::new_async().await;
        let id = "11111111-2222-3333-4444-555555555555";
        let mocks = mount(&mut server, id).await;

        inspect(test_client(&server), "ansible-deploy".to_string(), true)
            .await
            .unwrap();

        for mock in mocks {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn a_name_matching_nothing_is_an_error_not_an_empty_report() {
        let mut server = mockito::Server::new_async().await;
        let _deployments = server
            .mock("POST", "/deployments/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("[]")
            .create_async()
            .await;

        let error = inspect(test_client(&server), "absent".to_string(), true)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("absent"), "{error}");
    }

    #[tokio::test]
    async fn deletes_only_the_resolved_deployment() {
        let mut server = mockito::Server::new_async().await;
        let id = "11111111-2222-3333-4444-555555555555";
        let mut mocks = mount(&mut server, id).await;
        mocks.pop();
        let delete_mock = server
            .mock("DELETE", format!("/deployments/{id}").as_str())
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .with_status(204)
            .expect(1)
            .create_async()
            .await;

        delete(test_client(&server), "ansible-deploy-prod".to_string())
            .await
            .unwrap();

        for mock in mocks {
            mock.assert_async().await;
        }
        delete_mock.assert_async().await;
    }

    #[tokio::test]
    async fn ambiguous_delete_fails_before_sending_delete() {
        let mut server = mockito::Server::new_async().await;
        let deployments = server
            .mock("POST", "/deployments/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"id":"dep-1","name":"prod","flow_id":"f1"},
                     {"id":"dep-2","name":"prod","flow_id":"f2"}]"#,
            )
            .expect(1)
            .create_async()
            .await;
        let flows = server
            .mock("POST", "/flows/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"id":"f1","name":"ansible_deploy"},
                     {"id":"f2","name":"terraform_deploy"}]"#,
            )
            .expect(1)
            .create_async()
            .await;
        let no_delete = server
            .mock("DELETE", mockito::Matcher::Any)
            .expect(0)
            .create_async()
            .await;

        let error = delete(test_client(&server), "prod".to_string())
            .await
            .unwrap_err();

        assert!(
            error.to_string().contains("Ambiguous match 'prod'"),
            "{error}"
        );
        deployments.assert_async().await;
        flows.assert_async().await;
        no_delete.assert_async().await;
    }
}
