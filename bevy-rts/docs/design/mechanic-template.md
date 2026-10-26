# Mechanic spec template

One mechanic, one file: `docs/design/mechanics/<AREA>-<name>.md` (ADR-0022, spec §9.2 C1). `docs/design/GDD.md`
indexes every spec in its mechanic registry. The files under `docs/design/mechanics/` are protected: an author writes
`<AREA>-<name>.md.new`, and the lead blesses it (`cargo xtask bless docs/design/mechanics --reason "…"`).

`cargo xtask spec-lint` parses this format and `cargo xtask trace` maps `// @req <rule id>` tags in tests to its rules
(Task 9). Keep to the format exactly: the header lines, the section headings and the line shapes below are what the
parser reads. Prose inside a section is free unless a line shape is given for it.

## The format

**Header (lines 1 to 3, in this order, nothing before them):**

```text
# <AREA.NAME> — <title>
status: <state>[ (<approver>, <ruling>)] · spec-version: <n> · crate: <module path> · milestone: M<n>
Depends on: <spec ids, comma-separated, or none> · Emits journal records: <journal kinds, comma-separated, or none>
```

- `<AREA.NAME>` is the spec id: upper-case letters, a dot, upper-case letters or `_` (`MOVE.SPEED`, `PATH.QUEUE`).
  `AREA` names the file (`MOVE-speed.md`); `NAME` names the mechanic inside the area.
- `<state>` is one of `draft`, `approved`, `implemented`, `deprecated`:
  - `draft`: being written; acceptance criteria may name tests that do not exist yet, and `trace` ignores it.
  - `approved`: the requirements are agreed; every rule must be traced to a test (`trace` fails an untraced rule).
  - `implemented`: the code does what the rules say; traced as for `approved`.
  - `deprecated`: kept for history; not traced.
- The optional note `(<approver>, <ruling>)` records who approved the spec and under which ruling, for example
  `approved (lead, R148)`. The parser reads the state word and keeps the note as text.
- `spec-version` starts at 1 and rises by one with every blessed change to a rule, a formula or a parameter row (not
  for wording fixes). The `## Changelog` section lists every version.
- `crate` is the Rust module path that owns the mechanic (`fl_sim::movement`, `fl_feat_ballistics::hit`).
- `milestone` is the milestone that first delivered (or will deliver) the mechanic.
- `Depends on` lists the spec ids whose rules this one relies on; `Emits journal records` lists the journal kinds the
  mechanic writes (each must be registered: `spec-lint` checks the `"journal_records"` table).

**Sections, in this order, each a level-2 heading spelled exactly as here:**

1. `## Intent`: two to five sentences. What the mechanic is for and what a player sees.
2. `## Rules`: one requirement per line, `- <AREA.NAME.N>  <EARS sentence>` (two spaces after the id). Ids are
   `<AREA.NAME>.<N>`, `N` from 1 upwards, unique across all specs and never reused once blessed (a dropped rule
   leaves a gap). Each sentence uses one EARS pattern, with "the sim" (or a named system) as the subject:
   - ubiquitous: `The <system> shall <response>.`
   - event-driven: `When <trigger>, the <system> shall <response>.`
   - state-driven: `While <state>, the <system> shall <response>.`
   - unwanted behaviour: `If <condition>, then the <system> shall <response>.`
   - optional feature: `Where <feature is present>, the <system> shall <response>.`

   A rule states one observable behaviour (state, journal, counters, reject codes), never an implementation
   detail: two responses to different triggers are two rules, so each can be traced to its own test. The subject
   is always the system (`the sim shall …`), never the command or the content.
   Content values are named by their path key (below) or rules field (`Movement.arrive_m`).
3. `## Formulas`: the exact arithmetic, in a fenced block or a list. Name the number type (`Fx`, integer), the order
   of operations where rounding depends on it, the rounding (floor, ceiling, nearest) and every clamp.
4. `## Parameters`: one table with exactly these columns:

   ```text
   | Name | Path key | Unit | Default | Valid range | Provenance | Confidence |
   ```

   - **Path key** (ADR-0020): `<doc>/<field path>`, where `doc` is `rules` or `<kind>:<id>` (`unit:base.de.pz3`,
     `terrain:base.forest`) and the field path is dot-separated, map keys as written, sequence indices as numbers:
     `rules/Movement.arrive_m`, `unit:base.de.pz3/components.Hull.footprint.0`. A `*` stands for exactly one id or
     one path segment: `terrain:*/pass`, `rules/Movement.max_slope_m.*`. Every key must match at least one key of
     the provenance ledger.
   - **Default**: the base mod's value; `per unit` or `per terrain` (with the base range) for a per-object value.
   - **Valid range**: the bounds the link pass (`validate`) enforces; values outside are content errors (R104). It
     takes exactly one of these forms:
     - an interval or bound: `[a, b]`, `(a, b]`, `[a, b)`, `(a, b)`, `≥ a`, `> a`, `≤ b`, `< b`, or an integer range
       `a..=b`; a bound may name another parameter (`≥ flow_cache_max`), and one further bound may follow after
       `and` (`(0, 1] and ≥ 100 / 65535`);
     - a map or sequence: `<interval or bound> for each entry; one per <key set>` (`≥ 0 for each entry; one per
       class`), for a key whose `*` runs over map keys or sequence indices;
     - an enum list: the allowed variants, backticked and comma-separated (`` `Foot`, `Wheeled` ``);
     - an integer type: `any <type>` (`` any `u32` ``) when the link pass checks nothing beyond the type.
   - **Shared parameters**: a key may appear in several specs' tables, but one spec owns it (the spec whose rules
     it chiefly drives, and which defines its guess). A copy in another spec writes `<name> (owned by <SPEC.ID>)` in
     the Name column and must agree with the owner's row in every other column; when a pattern in one row matches
     keys of another spec's row (`footprint.0` inside `footprint.*`), the narrower row is the copy.
   - **Provenance**: the ledger class of every key the pattern matches (`Original`, `Manual`, `ResearchDerived`,
     `OursScaled`, `Guess`, `Tuned`); `spec-lint` checks it against the ledger.
   - **Confidence**: `High`, `Medium` or `Low`, as the ledger records it.
5. `## Acceptance criteria`: one test per line,
   `- AC-<n> (<rule ids or ranges>): <criterion> → <crates/…/tests/….rs>::<test fn>`.
   - `<n>` counts from 1 inside the spec.
   - The parenthesis lists the rules the test verifies, comma-separated; a range `AREA.NAME.a-b` covers rules `a` to
     `b`.
   - `<criterion>` says what the test observes, in one sentence.
   - The test is an integration test file path from the repository root and the test function's name. Unless the
     spec is `draft`, the file must contain `fn <test fn>`.
   - **Open criteria.** When no test yet checks a rule in isolation, write
     `- AC-<n> (<rule ids>): <criterion> → open: <why no test covers it yet>`. Never point a criterion at a test that
     does not check what it claims. `spec-lint` accepts an open criterion in any status, but it names no test, so no
     `@req` tag is backfilled for it: `trace` reports its rules untraced, and an `approved` or `implemented` spec
     fails `trace` until a test is added and the criterion names it.
   - Every rule must appear in at least one acceptance criterion (open or not).
6. `## Non-goals · Open questions · Guesses`: free prose and lists for non-goals and open questions, and one line per
   guess: `- Guess G-<AREA>-<n>: <text>`. A guess id is unique across all specs and `GDD.md` §Guesses; the
   provenance ledger's `Guess(id: …)` entries must name a defined one. The text says which value is guessed, what it
   is and what would replace it (a source, a measurement, a tuning sweep).
7. `## Changelog`: one line per spec version, `- v<n> (<date>, <who>): <change>`; a version that changes behaviour
   names its MCR (`docs/design/changelog-mechanics.md`).

## Skeleton

```markdown
# AREA.NAME — Title in sentence case
status: draft · spec-version: 1 · crate: fl_sim::module · milestone: M2
Depends on: none · Emits journal records: none

## Intent

What the mechanic is for, in two to five sentences.

## Rules

- AREA.NAME.1  The sim shall …
- AREA.NAME.2  When …, the sim shall …
- AREA.NAME.3  If …, then the sim shall …

## Formulas

    x = clamp(a × b, lo, hi)          (Fx; the product floors)

## Parameters

| Name | Path key | Unit | Default | Valid range | Provenance | Confidence |
|---|---|---|---|---|---|---|
| a | `rules/Section.a` | m | 1.0 | (0, 10] | Guess | Low |

## Acceptance criteria

- AC-1 (AREA.NAME.1): what the test observes → crates/fl_sim/tests/file.rs::test_name
- AC-2 (AREA.NAME.2-3): what the test observes → crates/fl_sim/tests/file.rs::other_test
- AC-3 (AREA.NAME.3): what a test should observe → open: no isolated test yet

## Non-goals · Open questions · Guesses

- Non-goal: …
- Open question: …
- Guess G-AREA-1: `rules/Section.a` = 1.0 is a placeholder until a tuning sweep.

## Changelog

- v1 (YYYY-MM-DD, author): first version.
```
