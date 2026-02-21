# pfp

A fast CLI for managing [Prefect 3](https://docs.prefect.io/) deployments and flow runs, built for both human and AI agent use.

## What it does

pfp talks directly to the Prefect REST API, bypassing the official CLI's limitations: truncated output, unreliable exit codes, and awkward parameter syntax.

- **Substring matching** — `pfp run happy-t` finds `happy_terraform/happy-terraform-prod`
- **Correct exit codes** — 0 for success, 1 for flow failure, 2 for CLI errors
- **`--watch` that works** — polls until completion with state change reporting
- **Dotted path parameters** — `--set config.action=destroy` builds nested JSON
- **`--json` on everything** — structured output for programmatic consumption
- **Full deployment names** — no truncation, ever

## Installation

From [crates.io](https://crates.io/crates/pfp):

```bash
cargo install pfp
```

From source:

```bash
git clone https://github.com/devinbarry/pfp.git
cd pfp
cargo install --path .
```

## Configuration

pfp reads your existing Prefect configuration. No extra config files needed.

**API URL** is resolved from `~/.prefect/profiles.toml`:

```toml
active = "self-hosted"

[profiles.self-hosted]
PREFECT_API_URL = "https://prefect.example.com/api"
```

The `PREFECT_API_URL` environment variable takes priority if set.

**Authentication** is optional. If your server requires it, set `PREFECT_API_AUTH_STRING` with a `username:password` value — pfp encodes it as HTTP Basic Auth:

```bash
export PREFECT_API_AUTH_STRING="admin:secret"
```

## Usage

### pfp ls

List all deployments:

```
$ pfp ls
DEPLOYMENT                                         STATUS   WORK POOL
happy_ansible/happy-ansible-prod                   ACTIVE   docker-prod
happy_terraform/happy-terraform-prod               ACTIVE   docker-prod
hello_world/hello_world-dev                        ACTIVE   docker-dev
update_hosts/update_hosts-prod                     ACTIVE   docker-prod
```

```bash
pfp ls --json    # JSON array of deployment objects
```

### pfp run

Run a deployment by substring match:

```bash
pfp run happy-t                          # create flow run and exit
pfp run happy-t --watch                  # poll until completion
pfp run happy-t --set config.action=plan # override parameters
```

Combining `--watch` with parameters:

```
$ pfp run happy-t --watch --set config.action=apply --set config.auto_approve=true
Resolved: happy_terraform/happy-terraform-prod
Created flow run 'venomous-alligator' (171a3f55-...)

17:34:00 | Pending
17:34:05 | Running
17:35:27 | Completed
```

With `--watch`, the exit code reflects the flow run outcome: 0 for Completed, 1 for Failed/Cancelled/Crashed.

```bash
pfp run happy-t --json              # JSON output of created flow run
pfp run happy-t --watch --json      # JSON object per state change
```

### pfp runs

Show recent flow runs for a deployment:

```
$ pfp runs happy-t
FLOW RUN                   STATE        STARTED              DURATION   ID
production-apply           COMPLETED    2026-02-21 17:34     45s        e130c152
production-destroy         COMPLETED    2026-02-21 17:34     8s         171a3f55
production-plan            COMPLETED    2026-02-21 00:05     3s         7137cfe7
```

```bash
pfp runs happy-t --json    # JSON array of flow run objects
```

### pfp logs

Show logs for a flow run (requires full UUID):

```
$ pfp logs e130c152-db01-428a-9698-e8404cd2c5d3
2026-02-21T17:34:36 | INFO     | Worker submitting flow run 'e130c152-...'
2026-02-21T17:34:41 | INFO     | Beginning flow run 'production-apply' for flow 'happy_terraform'
2026-02-21T17:34:41 | INFO     | Action: apply
2026-02-21T17:35:27 | INFO     | Flow run completed successfully
```

Get the flow run UUID from `pfp runs <query> --json`.

```bash
pfp logs e130c152-db01-428a-9698-e8404cd2c5d3 --json    # JSON array of log entries
```

### pfp pause / pfp resume

```bash
pfp pause happy-t     # pause the deployment
pfp resume happy-t    # resume it
```

### pfp cancel

```bash
pfp cancel e130c152-db01-428a-9698-e8404cd2c5d3    # cancel a running flow run
```

## Substring matching

All commands that take a deployment name use unique substring matching against the full `flow_name/deployment_name` identifier:

| Matches | Behavior |
|---------|----------|
| 0 | Error: `No deployment matching 'query'` |
| 1 | Uses the match |
| 2+ | Error: `Ambiguous match 'query', candidates:` with list |

Use `pfp ls` to discover available deployment names and find a unique substring.

## Parameters

The `--set` flag builds nested JSON from dotted paths:

```bash
--set config.action=destroy --set config.auto_approve=true
```

Produces:

```json
{"config": {"action": "destroy", "auto_approve": true}}
```

Values are auto-typed:

| Input | Type |
|-------|------|
| `true` / `false` | boolean |
| `42` | integer |
| `3.14` | float |
| anything else | string |

Parameters from `--set` are merged with the deployment's defaults. Explicit values override defaults.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success — command completed, flow run finished (if `--watch`) |
| 1 | Flow failure — flow run ended in Failed, Cancelled, or Crashed (only with `--watch`) |
| 2 | CLI error — bad arguments, no match, ambiguous match, API unreachable |

## License

MIT
