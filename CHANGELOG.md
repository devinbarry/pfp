# Changelog

All notable changes to this project will be documented in this file.

## [0.5.2] - 2026-07-19

### Added

- `pfp inspect <flow-run-uuid> [--json]` fetches one exact flow run directly by its full UUID. It is not limited to recent runs and deliberately rejects prefixes, making terminal-state checks reliable under concurrent run volume.

## [0.5.1] - 2026-07-08

### Added

- `pfp logs` now accepts `--tail` as an alias for `--follow`/`-f`, matching the more familiar `tail -f` terminology.

### Fixed

- `pfp runs` no longer shows a stuck `0s` duration for actively running flow runs. Prefect only persists `total_run_time` once a run leaves the `RUNNING` state; the duration column now uses `estimated_run_time`, which the server computes live, so elapsed time updates on each check.

## [0.5.0] - 2026-07-06

### Added

- **JSON payload input for `pfp run`** — new `--params-file <PATH>` flag supplies the full flow-run parameters object as JSON. Ideal for large or deeply-nested parameters (e.g. a `vault_secrets` array of objects) that are awkward to express with repeated `--set` flags.
- `--params-file -` reads the payload from stdin.
- Merge precedence: deployment defaults < `--params-file` < `--set`, so individual fields can still be overridden with `--set` on top of a payload.
- The payload is validated against the deployment's OpenAPI schema (same validation as `--set`). An unreadable file, malformed JSON, or a non-object top-level fails fast with exit code 2 before any API call.
- The full payload is recorded in the JSONL invocation log.
- 9 new tests covering merge precedence, stdin handling, and array-of-objects payloads (179 total)

## [0.4.0] - 2026-04-13

### Added

- **Client-side parameter validation** — `--set` parameters are validated against the deployment's OpenAPI schema before creating flow runs. Typos are caught immediately with "did you mean?" suggestions via Levenshtein distance.
- Supports Pydantic v1 (`definitions`) and v2 (`$defs`) schemas
- Handles `allOf`, `anyOf`, `oneOf` composition (model inheritance, Optional fields, discriminated unions)
- Handles `additionalProperties` (Dict fields skip validation)
- Recursion guard for self-referential Pydantic models
- Validation errors exit with code 2 (CLI/usage error)
- 75 new tests including real production schema fixtures (166 total)

## [0.3.1] - 2026-04-11

### Fixed

- `--set` flag now supports JSON arrays and objects as values (e.g., `--set 'field=["a","b"]'`). Previously these were treated as plain strings, causing Prefect API validation failures for `list` and `dict` parameter types.

### Changed

- 4 new params tests (88 total)

## [0.3.0] - 2026-04-03

### Added

- JSONL invocation logging — every CLI command is logged to `~/.pfp/pfp.jsonl` with version, timestamp, subcommand, args, outcome, error message, and duration
- Automatic log rotation at 25 MB with 10 rotated files retained
- `PFP_LOG_MAX_BYTES` env var to configure rotation threshold
- 6 new logger tests (84 total)

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
