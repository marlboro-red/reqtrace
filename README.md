# reqtrace

A deterministic requirements-coverage checker. Reads a requirements inventory
(YAML) and a set of design docs or source files, verifies every requirement is
covered by a `Covers:` annotation, and exits non-zero when coverage regresses.
No AI, no network, no state. See [spec.md](spec.md) for the full specification.

```
reqtrace check --inventory requirements.yaml [--config .reqtrace.toml] [--json report.json] [<doc-paths>...]
reqtrace validate --inventory requirements.yaml
```

Exit codes: `0` clean (warnings allowed), `1` at least one fail-severity
finding, `2` usage/parse/I-O error.

New here? Start with the [worked example](examples/README.md) — a small
fictional service authored so every finding type fires exactly once, with
the expected output and a fix-it walkthrough.

## Annotations

Line-oriented; works in Markdown, YAML, and code comments:

```text
Covers: req~login-throttling~2, req~lockout-notice~1
Derived: dsn~retry-queue~1
```

Links attribute to the nearest preceding Markdown heading. Fenced code blocks
are ignored, so documenting the syntax (as this README just did) is safe.

## Checks

`uncovered`, `orphaned`, `stale`, `unparented` (off by default),
`stale-exception`, and `malformed-annotation` — severities configurable per
check in `.reqtrace.toml` (`fail` | `warn` | `off` | `by-rating`).

## GitHub Action

```yaml
- uses: marlboro-red/reqtrace@v1
  with:
    inventory: requirements.yaml
    json: reqtrace-report.json
```

Downloads the static musl binary from this repo's releases and runs `check`.

## Self-hosting

This repo eats its own dogfood: `requirements.yaml` mirrors the requirement
IDs in [spec.md](spec.md), the source carries `Covers:` annotations, and CI
runs `reqtrace check` on the implementation itself.
