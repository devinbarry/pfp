use crate::client::PrefectClient;
use crate::error::Result;
use crate::output;

/// List plan records on the Prefect gateway: what is waiting for approval and how approved plans ended.
pub async fn run(client: PrefectClient, json: bool) -> Result<()> {
    let plans = client.list_gateway_plans().await?;
    if json {
        output::print_json(&serde_json::Value::Array(plans));
        return Ok(());
    }
    print!("{}", render(&plans));
    Ok(())
}

fn render(plans: &[serde_json::Value]) -> String {
    if plans.is_empty() {
        return "No plans.\n".to_string();
    }
    let mut out = format!(
        "{:<36}  {:<10} {:<11} {:>9}  {:<20} {}\n",
        "PLAN RECORD", "ROUTER", "STATUS", "LINES", "RECEIVED", "APPLY"
    );
    for plan in plans {
        let apply = plan["apply_state"]["name"]
            .as_str()
            .map(str::to_string)
            .or_else(|| {
                plan["apply_flow_run_id"]
                    .as_str()
                    .map(|id| id.chars().take(8).collect())
            })
            .unwrap_or_else(|| "-".to_string());
        out.push_str(&format!(
            "{:<36}  {:<10} {:<11} {:>9}  {:<20} {}\n",
            plan["id"].as_str().unwrap_or("?"),
            plan["target"].as_str().unwrap_or("?"),
            plan["status"].as_str().unwrap_or("?"),
            format!("+{}/-{}", plan["added"], plan["removed"]),
            plan["received_at"]
                .as_str()
                .unwrap_or("-")
                .chars()
                .take(19)
                .collect::<String>(),
            apply,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_one_row_per_plan() {
        let text = render(&[serde_json::json!({
            "id": "66666666-7777-8888-9999-000000000000", "target": "protectli", "status": "pending",
            "added": 3, "removed": 1, "received_at": "2026-10-14T22:30:00.000000+00:00", "apply_flow_run_id": null
        })]);
        let row = text.lines().nth(1).unwrap();
        assert!(
            row.starts_with("66666666-7777-8888-9999-000000000000  protectli  pending"),
            "{row}"
        );
        assert!(
            row.contains("+3/-1") && row.contains("2026-10-14T22:30:00") && row.ends_with('-'),
            "{row}"
        );
    }

    #[test]
    fn says_when_there_are_no_plans() {
        assert_eq!(render(&[]), "No plans.\n");
    }
}
