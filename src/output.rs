use crate::models::{Deployment, FlowRun, FlowRunDiagnostic, LogEntry, WorkPool, WorkerDiagnostic};
use colored::Colorize;

pub fn state_color(state: &str) -> colored::ColoredString {
    match state.to_uppercase().as_str() {
        "COMPLETED" | "ACTIVE" | "ONLINE" | "READY" => state.green(),
        "FAILED" | "CRASHED" | "OFFLINE" => state.red(),
        "RUNNING" => state.blue(),
        "PENDING" | "SCHEDULED" => state.yellow(),
        "CANCELLED" | "PAUSED" => state.magenta(),
        _ => state.normal(),
    }
}

pub fn print_deployments_table(deployments: &[Deployment]) {
    println!("{:<50} {:<8} WORK POOL", "DEPLOYMENT", "STATUS");
    for d in deployments {
        let status = state_color(d.status_str().to_uppercase().as_str());
        println!(
            "{:<50} {:<8} {}",
            d.full_name(),
            status,
            d.work_pool_name.as_deref().unwrap_or("-"),
        );
    }
}

pub fn print_flow_runs_table(runs: &[FlowRun]) {
    println!(
        "{:<26} {:<12} {:<20} {:<10} ID",
        "FLOW RUN", "STATE", "STARTED", "DURATION"
    );
    for r in runs {
        let state = state_color(&r.state_type);
        println!(
            "{:<26} {:<12} {:<20} {:<10} {}",
            truncate(&r.name, 26),
            state,
            r.start_time_short(),
            r.duration_str(),
            r.short_id(),
        );
    }
}

pub fn print_work_pool(pool: &WorkPool) {
    let status = pool.status.as_deref().unwrap_or("UNKNOWN");
    let status = state_color(status);
    println!(
        "{:<30} {:<12} {:<8} {}",
        pool.name,
        status,
        if pool.is_paused { "yes" } else { "no" },
        pool.r#type.as_deref().unwrap_or("-"),
    );
}

pub fn print_worker_diagnostics(workers: &[WorkerDiagnostic]) {
    println!(
        "{:<36} {:<10} {:<27} HEARTBEAT INTERVAL",
        "WORKER", "STATUS", "LAST HEARTBEAT"
    );
    for worker in workers {
        let status = if worker.status.is_empty() {
            "UNKNOWN"
        } else {
            &worker.status
        };
        println!(
            "{:<36} {:<10} {:<27} {}",
            truncate(&worker.name, 36),
            state_color(status),
            worker.last_heartbeat_time.as_deref().unwrap_or("-"),
            worker
                .heartbeat_interval_seconds
                .map(|seconds| format!("{seconds}s"))
                .unwrap_or_else(|| "-".to_string()),
        );
    }
}

pub fn print_flow_run_diagnostic(run: &FlowRunDiagnostic) {
    let state = run
        .state_name
        .as_deref()
        .or(run.state_type.as_deref())
        .unwrap_or("UNKNOWN");
    println!("Flow run:            {}", run.name);
    println!("ID:                  {}", run.id);
    println!("State:               {}", state_color(state));
    println!(
        "Deployment ID:       {}",
        run.deployment_id.as_deref().unwrap_or("-")
    );
    println!(
        "Deployment version:  {}",
        run.deployment_version.as_deref().unwrap_or("-")
    );
    println!(
        "Work pool:           {}",
        run.work_pool_name.as_deref().unwrap_or("-")
    );
    println!(
        "Work queue:          {}",
        run.work_queue_name.as_deref().unwrap_or("-")
    );
    println!(
        "Infrastructure PID:  {}",
        run.infrastructure_pid.as_deref().unwrap_or("-")
    );
    println!("Run count:           {}", run.run_count);
    println!(
        "Expected start:      {}",
        run.expected_start_time.as_deref().unwrap_or("-")
    );
    println!(
        "Next scheduled:      {}",
        run.next_scheduled_start_time.as_deref().unwrap_or("-")
    );
    println!(
        "Started:             {}",
        run.start_time.as_deref().unwrap_or("-")
    );
    println!(
        "Ended:               {}",
        run.end_time.as_deref().unwrap_or("-")
    );
    println!(
        "Run time:            {}",
        format_seconds(run.estimated_run_time.max(run.total_run_time))
    );
    println!(
        "Start delay:         {}",
        format_seconds(run.estimated_start_time_delta)
    );
}

pub fn print_logs(logs: &[LogEntry]) {
    for log in logs {
        let ts = if log.timestamp.len() >= 19 {
            &log.timestamp[..19]
        } else {
            &log.timestamp
        };
        let name = log.level_name();
        let level = match name {
            "ERROR" | "CRITICAL" => name.red(),
            "WARNING" => name.yellow(),
            "INFO" => name.blue(),
            _ => name.dimmed(),
        };
        println!("{} | {:<8} | {}", ts, level, log.message);
    }
}

pub fn print_watch_state(state_name: &str, timestamp: &str) {
    let ts = if timestamp.len() >= 19 {
        &timestamp[11..19]
    } else {
        timestamp
    };
    let state = state_color(state_name);
    println!("{} | {}", ts, state);
}

pub fn print_json<T: serde::Serialize>(value: &T) {
    println!("{}", serde_json::to_string_pretty(value).unwrap());
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn format_seconds(seconds: f64) -> String {
    let seconds = seconds.max(0.0).round() as u64;
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Deref;

    #[test]
    fn state_color_completed() {
        assert_eq!(state_color("COMPLETED").deref(), "COMPLETED");
    }

    #[test]
    fn state_color_case_insensitive() {
        assert_eq!(state_color("completed").deref(), "completed");
    }

    #[test]
    fn truncate_short_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_adds_ellipsis() {
        assert_eq!(truncate("hello world!", 8), "hello...");
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn format_seconds_handles_minutes() {
        assert_eq!(format_seconds(125.4), "2m 05s");
    }
}
