# `reqtrace` — CLI spec v0.3

A deterministic requirements-coverage checker. Reads a requirements inventory and a set of design docs, computes coverage arithmetic, reports findings, exits non-zero on failures. No AI, no network, no state.

This spec eats its own dogfood: normative requirements are EARS-formed with stable IDs (`cli~slug~rev`), so the implementation can carry `Covers:` annotations and be checked by the tool itself. Requirements changed since v0.1 carry bumped revs.

---

## 1. Scope

**v1 does:** `check`, implementing five checks (uncovered, orphaned, stale, unparented, stale-exception) plus the `malformed-annotation` finding emitted by the scanner. And `validate` (inventory + exception lint) as a free by-product of parsing.

**Non-goals for v1 (explicitly out):**

- Extraction, reconciliation, or anything touching an LLM — those are agent skills that *call* this tool.
- Cross-repo fetching. The inventory is a local path; CI does the download/checkout.
- Watch mode, dashboards, HTML reports. JSON out is the integration point.

---

## 2. CLI surface

```
reqtrace check --inventory <path> [--config <path>] [--json <path>] [<doc-paths>...]
reqtrace validate --inventory <path> [--config <path>]
```

| Flag | Meaning | Default |
|---|---|---|
| `--inventory` | file or directory of inventory YAML | required |
| `--config` | config file | nearest `.reqtrace.toml` (see `cli~config-discovery~1`), else built-in defaults |
| `--json` | write machine-readable report here | none (stdout is human-readable only) |
| `<doc-paths>` | files/dirs to scan for annotations | `[scan].globs` from config |

- `cli~exit-codes~1` — The tool shall exit with: `0` = no findings at fail severity; `1` = ≥1 finding at fail severity; `2` = usage, parse, or I/O error. (Warnings alone → exit 0.)
- `cli~error-lanes~1` — Errors in the tool's *own* inputs (inventory, exception file, config: unparseable, bad ID grammar, bad status, missing justification, duplicate rev) shall exit 2 — the run is unsound. Problems found in *scanned docs* (malformed annotations, orphaned links, …) shall be findings — the run is sound and completes.
- `cli~docpaths~1` — When `<doc-paths>` are given they shall replace `[scan].globs` entirely (no merging). If neither doc-paths nor config globs yield a scan set, the tool shall exit 2 (usage).
- `cli~config-discovery~1` — When `--config` is absent, the tool shall search for `.reqtrace.toml` in the current working directory, then each ancestor directory in order, using the first found; if none, built-in defaults apply.
- `cli~determinism~2` — Given identical inputs, the tool shall produce byte-identical JSON reports: findings sorted by check, then ID, then file, then line; paths reported relative to the current working directory with `/` separators.
- `cli~no-network~1` — The tool shall make no network calls.

---

## 3. Input formats

### 3.1 Inventory (YAML)

One file or a directory of `*.yaml` merged. Item schema:

```yaml
- id: req~login-throttling~2        # required — see ID grammar
  statement: "When a user fails login 5 times in 10 min, …"
  status: confirmed                  # required — see states
  rating: [H, M]                     # optional — [value, risk], H|M|L
  owner: pm-jane                     # optional
  # all other keys preserved but ignored by v1
```

- `inv~id-grammar~2` — The tool shall accept IDs matching `^(?<type>[a-z][a-z0-9]*)~(?<slug>[a-z0-9]+(?:-[a-z0-9]+)*)~(?<rev>[1-9][0-9]*)$` and reject all others with the offending file/line. (No leading, trailing, or consecutive hyphens in slugs.)
- `inv~status-set~1` — The tool shall accept statuses `extracted | assumed | confirmed | disputed | retired` and reject others.
- `inv~current-rev~1` — For each `type~slug`, the highest rev present shall be the *current* rev. Only the current rev participates in coverage arithmetic; lower revs exist solely so links to them can be detected as stale. (In particular: if the current rev is `retired`, the slug is out of the denominator regardless of lower revs' statuses.)
- `inv~coverage-denominator~2` — The tool shall include a `type~slug` in the coverage denominator iff its *current rev's* status is `confirmed` or `assumed`. (`extracted`/`disputed` are in flight; `retired` is gone.)
- `inv~dup-ids~2` — If two items share the same `type~slug` and the same `rev`, the tool shall exit 2.

### 3.2 Annotations (scanned from docs)

Line-oriented grammar, valid in Markdown, YAML, and code comments:

```
Covers: req~login-throttling~2, req~lockout-notice~1
Covers: `req~login-throttling~2`  # backticked — renders as a code span in Markdown
Derived: dsn~retry-queue~1        # declares a design item with no HLD parent
```

- `ann~grammar~3` — The tool shall recognize two line forms (`<ids>` = `<idt>(\s*,\s*<idt>)*`, where `<idt>` is an `<id>` per the ID grammar, optionally wrapped in single backticks — bare tildes trigger GFM strikethrough in rendered Markdown, backticked IDs render as code spans):
  - plain / line-comment: `^\s*(?:[#/*;-]+\s*)?(Covers|Derived):\s*<ids>\s*$` — the prefix class covers `#`, `//`, `///`, `*` (block-comment continuation), `--`, `;`, and Markdown list bullets.
  - HTML comment: `^\s*<!--\s*(Covers|Derived):\s*<ids>\s*-->\s*$` — opener and closer both required.

  Each link is attributed to the nearest preceding Markdown heading, or the file itself if none.
- `ann~malformed~2` — If a line, after stripping leading whitespace and any optional comment prefix per the grammar above, begins with `Covers:` or `Derived:` but the full line fails the grammar, the tool shall emit a `malformed-annotation` finding (default severity fail, configurable like any check) rather than silently skipping it.
- `ann~fences~1` — In Markdown files, lines inside fenced code blocks (` ``` ` or `~~~` delimited) shall be ignored for both annotation and heading scanning. (Without this, documenting the annotation syntax — as this spec does above — would create phantom links.)
- `ann~headings~1` — Heading tracking (`^#{1,6}\s`, outside fences) shall apply only to Markdown files (`.md`, `.markdown`); in all other files the section is the file itself. A line matching the annotation grammar is never treated as a heading.
- `ann~derived~1` — `Derived:` targets declare new IDs and shall be validated against the ID grammar only; they are exempt from the `orphaned` and `stale` checks. In v1 a `Derived:` line's sole effect beyond validation is to satisfy `unparented` for its section. Lists are permitted.

### 3.3 Exception file

```yaml
# not-in-scope.yaml
- id: req~mobile-push~1
  justification: "Mobile repo's responsibility, see design §4"
```

- `exc~justification~1` — The tool shall treat an exception entry without a non-empty `justification` as malformed (exit 2).
- `exc~matching~1` — Exceptions shall match on `type~slug`; the entry's rev is accepted by the grammar but ignored for matching. (An exception survives the requirement revving; `stale-exception` still fires when the slug leaves the inventory entirely.)

---

## 4. Checks

Let **R** = `type~slug`s in the denominator, **L** = `Covers:` links from docs, **E** = exception `type~slug`s.

- `chk~matching~1` — Coverage and exception matching shall be by `type~slug`; a link's rev is used only to detect staleness. (A slug covered only by a stale link is *covered* — it produces a `stale` finding, never an additional `uncovered` one.)

| # | Check | Logic | Default severity |
|---|---|---|---|
| 1 | `uncovered` | slug ∈ R, slug ∉ slugs(L), slug ∉ E | by-rating |
| 2 | `orphaned` | `Covers:` target's `type~slug` ∉ inventory at any rev | fail |
| 3 | `stale` | link rev < current rev of same `type~slug` | by-rating |
| 4 | `unparented` | doc section has content but no `Covers:`/`Derived:` line | off |
| 5 | `stale-exception` | slug ∈ E but slug ∉ inventory at any rev | warn |
| 6 | `malformed-annotation` | see `ann~malformed~2` | fail |

**by-rating:** rating value `H` → fail; `M`, `L`, or no rating → warn. (Unrated items warn rather than fail so a fresh, rating-less inventory doesn't instantly red-build; rate the important ones to arm them.)

Links whose target slug *is* in the inventory but outside the denominator (current rev `retired`, `extracted`, or `disputed`) are accepted silently in v1; a `covers-retired` warn check is deferred.

- `chk~uncovered~1` — When an item in the denominator has zero covering links and no exception, the tool shall report it with its rating and owner.
- `chk~stale~1` — When a link's rev is lower than the current rev, the tool shall report the link's file/section, the covered rev, and the current rev.
- `chk~severity-config~1` — Where a config file provides a severity policy, the tool shall apply it instead of defaults (this is the red-build-fatigue control; policy lives in config, never code). Every check in the table above, including `malformed-annotation` and `stale-exception`, is configurable.

---

## 5. Config (`.reqtrace.toml`)

```toml
[scan]
globs = ["docs/**/*.md", "adr/**/*.md"]
exceptions = "not-in-scope.yaml"    # path relative to the config file's directory

[severity]              # per check: "fail" | "warn" | "off" | "by-rating"
uncovered = "by-rating" # by-rating: H → fail; M, L, or unrated → warn
stale     = "by-rating"
orphaned  = "fail"
unparented = "off"
stale-exception = "warn"
malformed-annotation = "fail"
```

---

## 6. Output

**Human (stdout):** one line per finding, worst first, then a one-line summary.

```
FAIL uncovered  req~lockout-notice~1   [H,M] owner:pm-jane
WARN stale      req~login-throttling~2 covered rev 1, current rev 2
                → docs/design/rate-limiting.md § "Rate limiting design"
2 findings (1 fail, 1 warn) · 41/43 covered · 1 exception
```

**JSON (`--json`):**

```json
{
  "version": 1,
  "summary": { "denominator": 43, "covered": 41, "exceptions": 1,
               "findings": { "fail": 1, "warn": 1 } },
  "findings": [
    { "check": "uncovered", "id": "req~lockout-notice~1",
      "rating": ["H","M"], "owner": "pm-jane", "severity": "fail" },
    { "check": "stale", "id": "req~login-throttling~2",
      "covered_rev": 1, "current_rev": 2, "severity": "warn",
      "location": { "file": "docs/design/rate-limiting.md",
                    "section": "Rate limiting design", "line": 12 } }
  ]
}
```

- `out~json-stable~1` — The JSON schema shall be versioned (`version` field); breaking changes bump it.
- `out~summary~1` — `denominator` = |R|; `covered` = denominator slugs with ≥1 `Covers:` link at any rev; `exceptions` = exception entries that matched a denominator slug (applied, not merely listed). Invariant: `denominator = covered + exceptions + uncovered findings` (stale-but-linked items count as covered). Stale findings for slugs also covered elsewhere don't disturb the invariant.

---

## 7. Implementation notes (Rust)

- Edition 2021+. Crates: `clap` (derive) for CLI, `serde`/`serde_yaml` for inventory, `toml` for config, `regex` for annotations, `walkdir`+`globset` for scanning, `anyhow`/`thiserror` for errors. No async — it's a file scanner.
- Line-regex scanning, not full Markdown parsing. Three line regexes: annotation, heading (`^#{1,6}\s`, Markdown files only), and fence toggle (` ``` `/`~~~`, Markdown files only — flips an in-fence flag; annotation and heading matching are skipped while set). Keeps the tool format-agnostic (works on code comments too).
- Report findings sorted per `cli~determinism~2` before writing either output.
- Release: `x86_64-unknown-linux-musl` static binary built by the tag-push release workflow; that's what the composite GitHub Action downloads. (Hand-rolled workflow rather than `cargo-dist` — one target, fewer moving parts.)
- Perf target: 10k docs / 5k items < 1 s. Trivially met single-threaded; don't parallelize v1.

---

## 8. Milestones

1. **M0 — plumbing proof (~½ day):** `check` parses nothing, exits 0; workflow + composite action wired into one real repo pair. *Done when: a PR goes green through the action.*
2. **M1 — the arithmetic (~1–2 days):** inventory + annotation parsing, checks 1–3, human output, exit codes. *Done when: a deliberately dropped requirement turns a real PR red.*
3. **M2 — operability (~1 day):** config + severity policy, JSON report, exception file, `validate`. *Done when: an H item fails while an L item only warns, per config.*
4. **M3 — self-hosting (~½ day):** hand-maintain `requirements.yaml` mirroring this spec's requirement IDs (extraction is out of scope for v1, so the mirror is manual; drift shows up as `orphaned`/`uncovered` findings — the tool polices its own inventory). Annotate the tool's code with `Covers: cli~…` and run `reqtrace check` on itself in its own CI. *Done when: the spec's requirement IDs are enforced on the implementation.*

Deferred past v1: smarter `unparented` heuristics (the v1 check is a naive "section has prose but no annotation" test, shipped off by default), `covers-retired` check, incremental mode, SARIF output for GitHub code-scanning annotations, Windows builds.

---

## Changelog

**v0.3**
- IDs in annotation lines may be wrapped in single backticks (`ann~grammar~3`): bare `~` pairs trigger GFM strikethrough when docs render on GitHub, so visible `Covers:` lines should backtick their IDs; the HTML-comment form remains the invisible alternative.

**v0.2** — resolved audit findings against v0.1:
- Fence-aware scanning in Markdown (`ann~fences~1`); headings tracked only in Markdown, annotation grammar wins over heading regex (`ann~headings~1`).
- `Derived:` targets exempted from `orphaned`/`stale` (`ann~derived~1`).
- Matching semantics pinned: coverage and exceptions match on `type~slug`, rev only signals staleness (`chk~matching~1`, `exc~matching~1`).
- Current-rev rules pinned: highest rev wins, its status alone decides denominator membership (`inv~current-rev~1`, `inv~coverage-denominator~2`, `inv~dup-ids~2`).
- Check inventory reconciled (six finding types, all severity-configurable, incl. `malformed-annotation` and `stale-exception`); `by-rating` defined for unrated items (warn).
- Error lanes made explicit: tool-input errors exit 2, doc-side problems are findings (`cli~error-lanes~1`).
- `<doc-paths>` now optional and replaces config globs when given (`cli~docpaths~1`); config discovery defined (`cli~config-discovery~1`).
- Determinism sort key gains line tiebreaker + path normalization (`cli~determinism~2`).
- Annotation grammar split into balanced plain/HTML-comment forms, wider comment-prefix class, malformed trigger matches the prefixed forms (`ann~grammar~2`, `ann~malformed~2`).
- Slug grammar forbids leading/trailing/consecutive hyphens (`inv~id-grammar~2`).
- Summary fields defined with arithmetic invariant (`out~summary~1`); human-output example shows full ID.
- `validate` gains `--config` and lints the exception file too.
