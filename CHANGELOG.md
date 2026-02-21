# Changelog

All notable changes to this project will be documented in this file.

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
