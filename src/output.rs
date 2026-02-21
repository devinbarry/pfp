use crate::models::{Deployment, FlowRun, LogEntry};
use colored::Colorize;

pub fn state_color(state: &str) -> colored::ColoredString {
    match state.to_uppercase().as_str() {
        "COMPLETED" | "ACTIVE" => state.green(),
        "FAILED" | "CRASHED" => state.red(),
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
}
