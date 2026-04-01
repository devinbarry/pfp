# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] - 2026-04-02

### Added

- `pfp logs --follow` / `-f` — continuously poll for new log entries, like `tail -f`
- Automatically exits when the flow run reaches a terminal state (completed, failed, cancelled, crashed)
- Drains any straggler logs after terminal detection so nothing is lost
- 3 new follow-mode integration tests (73 total)

### Changed

- `get_flow_run_logs` now accepts a `start_offset` parameter for incremental fetching

## [0.1.3] - 2026-02-23

### Added

- UUID prefix support in `pfp logs` and `pfp cancel` — short IDs from `pfp runs` output now work directly (e.g., `pfp logs 9d9ca60c`)
- Case-insensitive prefix matching
- Helpful error message when prefix not found, suggesting full UUID
- Integration test infrastructure with `assert_cmd` and CLI smoke tests
- 24 new tests (69 total)

### Changed

- `NoMatch` error is now generic, supporting both deployment and flow run resolution contexts

## [0.1.2] - 2026-02-22

### Fixed

- Pinned Rust toolchain to 1.93.1 (`rust-toolchain.toml`) and CI images to prevent formatting drift between local and CI environments
- Pre-commit hook now runs `cargo fmt --check` before clippy, catching format issues before push

### Added

- `just check` recipe that mirrors the full CI pipeline locally (fmt, clippy, test)

## [0.1.1] - 2026-02-22

### Added

- `pfp logs` now fetches all log entries via automatic pagination (previously capped at 200)
- `--limit <N>` flag on `pfp logs` to cap the number of entries fetched
- 10,000-entry safety cap with stderr warning when hit

### Changed

- Standardized install target to `~/.cargo/bin` (removed `install-local`)

## [0.1.0] - 2026-02-21

Initial release.

### Added

- `pfp ls` — list all deployments with full `flow_name/deployment_name` display
- `pfp run <query> [--watch] [--set key=val]` — run deployment with substring matching, optional polling, dotted path parameters
- `pfp runs <query>` — recent flow run history for a deployment
- `pfp logs <flow-run-id>` — flow run log viewer
- `pfp pause/resume <query>` — pause and resume deployments
- `pfp cancel <flow-run-id>` — cancel a running flow run
- `--json` output mode on ls, runs, and logs commands
- Unique substring matching for deployment names (ambiguous matches show candidates)
- Dotted path parameter builder (`--set config.action=destroy` → nested JSON)
- Config from `~/.prefect/profiles.toml` with optional `PREFECT_API_AUTH_STRING` auth
- Correct exit codes: 0=success, 1=flow failure, 2=CLI error
- 42 unit tests with mockito HTTP mocking
