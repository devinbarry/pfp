# Changelog

All notable changes to this project will be documented in this file.

## [0.8.0] - 2026-10-15

### Added

- `pfp plans` lists gateway plan records and their apply-run status, with JSON
  output available through `--json`.
- `pfp approve` and `pfp reject` can decide gateway plan records by full UUID.
  Paused flow runs remain supported; gateway policy denials exit with code 3.

### Changed

- `pfp policy` displays the active Cedar policy, grouped permissions and
  prohibitions, and proposed changes awaiting operator activation.
- Dependency checks use `cargo deny` over crates that can be compiled, alongside
  the existing CI vulnerability and secret scan.
- GitHub release tests and publication use the audited lockfile.
- Added CLI integration coverage for gateway commands, policy denials, lookup
  failures, and fallback to paused flow runs.

### Fixed

- `pfp cancel` requests cancellation through Prefect orchestration instead of
  forcing a terminal state while the worker may still be running. It reports
  rejected requests as errors and distinguishes acceptance from worker shutdown.
- API URLs ending in `/` now work for ordinary Prefect requests as well as
  gateway requests, including approval and rejection of paused flow runs.

## [0.7.3] - 2026-10-09

### Changed

- Release tooling only; 0.7.2 was tagged but never published. No change to the
  program.

## [0.7.2] - 2026-10-09

### Changed

- Release tooling only: the published repository history is cleaned more
  thoroughly. No change to the program.

## [0.7.1] - 2026-10-09

### Security

- Updated dependencies, clearing advisories in `rustls`, `rustls-webpki` and
  `h2`. `cargo audit` and a Trivy scan are clean.

### Changed

- Builds with Rust 1.97.1.
- The published crate contains only the source, manifest, lockfile, README and
  changelog.
- Examples and tests use neutral profile and deployment names.

## [0.7.0] - 2026-10-05

### Added

- `pfp approve <flow-run>` and `pfp reject <flow-run>` answer a flow run paused
  on an `approved: bool` input. They accept a UUID prefix and check Prefect's
  orchestration result, so answering a run that is not paused fails.
- Behind the Prefect gateway, a request its policy refuses exits with the new
  code **3** and prints the policy's reason. Agents stop and wait for the
  operator on exit code 3.
- `pfp policy` (and `--json`) shows the gateway policy that decides what agents
  may do.
- Inside an agent session every request carries `X-Agent-Id`, `X-Agent-Tool`,
  `X-Agent-Model` and `X-Agent-Session`, taken from `git config agent.id` and
  the command named by `git config agent.provenance`: the same source as agent
  commit trailers. Nothing is sent outside an agent session.

### Fixed

- Four logger tests that set the same environment variable no longer race when
  cargo runs them in parallel.

## [0.6.1] - 2026-08-23

### Added

- `pfp deployment delete <name>` permanently removes one uniquely resolved
  deployment registration. It uses the same substring matching as other
  deployment commands, rejects ambiguous matches before mutation, and accepts a
  full `flow/deployment` name for precise destructive use.

### Documentation

- The implementation plan for client-side parameter validation is now tracked
  alongside its design spec in `docs/superpowers/plans/`. The feature itself
  shipped in 0.4.0; only the plan document was missing from the repository.

## [0.6.0] - 2026-08-22

### Added

- `pfp deployment inspect <name>` reports what a deployment is actually
  registered as: its version, work pool and queue, entrypoint, tags, and the
  parameters saved at its last successful registration. A deployment keeps those
  values until something re-registers it, so a manifest in a repository is not
  evidence of live state.
- Each schedule is listed with its own active flag. A deployment can be active
  while every schedule attached to it is not, and the two states are reported
  separately rather than inferred from each other.
- Job variables, pull steps, and storage configuration are deliberately absent
  from both the table and `--json`: they carry credentials, and the deployment
  model stays narrow so unrecognised API fields cannot reach the output.

## [0.5.10] - 2026-08-09

### Added

- `pfp diagnose <flow-run-uuid>` prints an allowlisted operational summary of
  one exact flow run, including state, work pool, work queue, infrastructure
  identifier, attempts, and timing. It never serializes parameters, context,
  labels, job variables, state data, or the raw Prefect response.
- `pfp pool workers <exact-pool-name>` reports worker status and heartbeat
  information without requiring SSH access or process-list inspection. Its
  JSON output likewise discards unrecognised API fields.

## [0.5.9] - 2026-08-04

### Changed

- When `profiles.toml` contains more than one distinct Prefect API URL, every
  network command now requires `--server <PROFILE>` instead of silently using
  the active profile. This prevents reads from presenting stale data from the
  wrong cluster and prevents mutations such as `run`, `cancel`, and work-pool
  changes from targeting the wrong server.
- Single-server profile files, multiple aliases for the same API URL, and an
  explicit `PREFECT_API_URL` environment pair retain their existing behavior.
  Ambiguity errors list only profile names and never expose URLs or credentials.

## [0.5.8] - 2026-08-04

### Added

- Profiles may now carry `PREFECT_API_AUTH_STRING` beside
  `PREFECT_API_URL`. An explicit `--server <PROFILE>` selects both values as
  one atomic pair and ignores process-wide URL and authentication overrides,
  preventing one server's credential from being sent to another server.
- The active profile supplies its authentication when no environment override
  is present; existing environment-driven operation remains compatible.

## [0.5.7] - 2026-08-03

### Added

- Global `--server <PROFILE>` selection resolves the named profile's
  `PREFECT_API_URL` for every command. Unknown profiles fail without falling
  back to the environment or active profile; omitting the flag preserves the
  existing environment-first configuration behavior.
- Every flow run created by `pfp run` is tagged `manual`. Repeatable
  `--tag <TAG>` options add further run tags without replacing `manual`.

## [0.5.6] - 2026-07-25

### Added

- `pfp pool assert-idle <exact-pool-name>` fails with exit code 2 unless
  Prefect reports zero nonterminal flow runs for the whole work pool.
- `--json` returns stable `pool`, `idle`, and `nonterminal_run_count` fields
  for both idle and non-idle results without fetching or exposing run
  parameters.

## [0.5.5] - 2026-07-25

### Added

- `pfp pool status`, `pfp pool pause`, and `pfp pool resume` operate on one
  exact work-pool name. Mutations read the pool back and fail unless
  `is_paused` reaches the requested state.

### Fixed

- Work-pool names are encoded as one URL segment and names containing `/` are
  rejected before any request, preventing decoded paths from reaching a
  different Prefect endpoint.
- GitLab release pipelines no longer run duplicate jobs for the same release.

## [0.5.4] - 2026-07-24

### Fixed

- Deployment discovery now follows Prefect pagination instead of stopping after
  the first 100 deployments. Commands such as `pfp run`, `pfp runs`, `pfp pause`,
  and `pfp resume` can therefore resolve every registered deployment.

### Changed

- Repository guidance now uses `AGENTS.md` and explicitly requires the complete
  Justfile release workflow for every shipped change.

## [0.5.3] - 2026-07-19

### Added

- `pfp schedule-resume <query>` resolves one deployment and activates every attached schedule without changing the deployment-wide paused state. It fails when no schedules exist, verifies the complete final schedule set, and attempts to restore all initially inactive schedules if activation fails part way through.

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
