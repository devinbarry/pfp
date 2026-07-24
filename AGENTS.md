# AGENTS.md

## What is pfp?

pfp (Prefect CLI) is a Rust CLI for managing Prefect deployments and flow runs. It replaces the official `prefect` CLI which has broken output truncation, unreliable exit codes, and awkward parameter syntax.

Talks directly to the Prefect REST API with Basic Auth.

## Build & Development Commands

```sh
just test           # cargo test
just lint           # cargo clippy -- -D warnings
just fmt            # cargo fmt
just install        # cargo install --path . --force (to ~/.cargo/bin)
```

## Release Process

Every change that is intended to ship must go through the complete two-step release
workflow in the justfile. Do not stop after an ordinary commit and push:

```sh
just release-prep patch   # (or minor/major) bump version, generate changelog draft
# Edit CHANGELOG.md with release notes
# Run just check and review the complete git diff
just release-finish       # commit, tag, push, install
```

Pipeline: GitLab CI (test + build) → sync to GitHub mirror → GitHub Actions publishes to crates.io on tag push.

## Architecture

```
main.rs (CLI parsing via clap derive -> routes to command handlers)
  └─ commands/{ls,run,runs,logs,pause,resume,cancel}.rs
       ├─ config.rs (reads ~/.prefect/profiles.toml + PREFECT_API_AUTH_STRING env)
       ├─ client.rs (async reqwest wrapper with Basic Auth)
       ├─ models.rs (Deployment, FlowRun, LogEntry structs)
       ├─ params.rs (dotted path -> nested JSON builder; --params-file <path|-> supplies the full params object as JSON, merged under --set)
       ├─ validate.rs (--set parameter validation against deployment OpenAPI schema)
       └─ output.rs (colored tables, JSON output, state->color mapping)
  └─ logger.rs (JSONL invocation logging to ~/.pfp/pfp.jsonl with rotation)
  └─ error.rs (PfpError enum via thiserror, Result<T> alias)
```

## Config

- API URL: from `~/.prefect/profiles.toml` (active profile's PREFECT_API_URL)
- Auth: from `PREFECT_API_AUTH_STRING` env var (format: "username:password", encoded as Basic Auth)

## Conventions

- Commit messages follow conventional commits (feat:, fix:, test:, chore:)
- All I/O is async (tokio + reqwest)
- Tests are inline `#[cfg(test)] mod tests` blocks
- HTTP mocking with mockito for client tests
- Exit codes: 0 = success, 1 = flow run failed, 2 = CLI/usage error
