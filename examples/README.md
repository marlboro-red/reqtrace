# Worked example

A tiny fictional login service, deliberately authored so that **every finding
type fires exactly once**. Use it to see what reqtrace reports and why.

```
examples/
  .reqtrace.toml        config: scan globs, exceptions path, severity policy
  requirements.yaml     the inventory (8 items, 5 in the denominator)
  not-in-scope.yaml     justified exceptions
  docs/                 design docs carrying Covers:/Derived: annotations
  src/throttle.rs       annotations in code comments work too
```

## Run it

From this directory (the config is discovered automatically):

```sh
reqtrace check --inventory requirements.yaml
# or, from a source checkout of this repo:
cargo run --quiet --manifest-path ../Cargo.toml -- check --inventory requirements.yaml
```

Expected output (byte-stable — the tool is deterministic):

```text
FAIL malformed-annotation                        line fails the annotation grammar
                → docs/session-handling.md § "Login flow"
FAIL orphaned   req~session-idle~1     not in inventory at any rev
                → docs/session-handling.md § "Sticky sessions"
FAIL uncovered  req~lockout-notice~1   [H,M] owner:pm-jane
WARN stale      req~login-throttling~2 covered rev 1, current rev 2
                → docs/rate-limiting.md § "Rate limiting"
WARN stale-exception req~legacy-export~1    exception target not in inventory
5 findings (3 fail, 2 warn) · 3/5 covered · 1 exception
```

Exit code is `1` (at least one fail-severity finding). Add
`--json report.json` for the machine-readable version.

## Why each finding fires — and how to fix it

| Finding | Cause | Fix |
|---|---|---|
| `malformed-annotation` | `Covers: req~Login-Throttling~2` — uppercase breaks the ID grammar, and reqtrace refuses to silently skip a line that starts like an annotation | lowercase it: `req~login-throttling~2` |
| `orphaned` | `req~session-idle~1` isn't in the inventory at any rev (the item is called `session-timeout`) | point the link at the real slug |
| `uncovered` | `req~lockout-notice~1` is in the denominator with no covering link and no exception; its value rating is `H`, so by-rating makes it a fail | add `Covers: req~lockout-notice~1` where the notice flow is designed |
| `stale` | `docs/rate-limiting.md` covers rev 1 of `login-throttling` but the inventory's current rev is 2 | re-review the doc against rev 2, then bump the link |
| `stale-exception` | `req~legacy-export~1` is excepted but no longer exists in the inventory | delete the entry from `not-in-scope.yaml` |

Apply all five fixes and the run goes green, exit 0:

```text
0 findings (0 fail, 0 warn) · 4/5 covered · 1 exception
```

(4/5, not 5/5 — the excepted `mobile-push` item is accounted for as an
exception, not as covered.)

## Arithmetic worth noticing

- **Denominator is 5, not 8.** `audit-log` is `extracted` (in flight) and
  `old-captcha` is `retired`; only `confirmed`/`assumed` items at their
  current rev count. `login-throttling` appears at revs 1 and 2 but is one
  slug — the highest rev is current.
- **The stale item still counts as covered** (`3/5`, not `2/5`). Coverage
  matches on `type~slug`; the rev only signals staleness. You get one
  `stale` finding, never a double-report with `uncovered`.
- **The exception is "applied"** because `mobile-push` is in the denominator.
  The invariant always holds: denominator = covered + exceptions + uncovered
  findings (5 = 3 + 1 + 1).
- **The fenced-block trap doesn't exist**: annotation examples inside
  triple-backtick fences in scanned Markdown are ignored, so documenting
  the syntax can't create phantom links.
- **`Derived: dsn~retry-queue~1`** declares a design item with no parent
  requirement — it's grammar-checked but exempt from orphaned/stale.
