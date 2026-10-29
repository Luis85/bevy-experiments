# Island Village review and polish — 2026-10-29

Scope: review the shipped village's gameplay boundaries, persistent lives, scheduler feedback and native
scene/diary experience. Preserve the existing KayKit art, native Bevy controls, numerical model and
protected oracles. This is a refinement of the incumbent scene.

## Findings and corrections

| Finding | Correction | Evidence |
|---|---|---|
| Successors chose the first ActorPresets entry, allowing an unrelated preset to change their body/speed | Retain the scenario's selected preset as immutable construction data; remove old control only after spawning succeeds | Regression reproduced 99 instead of 70 mm/tick before the fix; selected-body and future-birth snapshot/resume tests |
| Read-only scheduler status retained cached “ready” despite a current pause/death; first-request waiting hid stronger eligibility reasons | Overlay fresh eligibility on cached timing data without running the dispatcher; prioritize exclusions and pause/control reasons | Red/green pause, disable and death test with unchanged simulation hash; owned fake-runner tests |
| World captions overlapped each other, clipped at the viewport and sat behind the village HUD | Place measured captions in stable priority/SimId order, keep them near their anchors and outside village panels; suppress excess captions and unselected old corpse labels | Native before/after matrix and bounded placement regression |
| Diary order depended on ECS iteration, unavailable navigation stayed enabled, and a new page retained the previous scroll offset | Sort by SimId, retain the selected book by identity, disable boundary controls and reset scroll on explicit navigation | Native reader and stable-order/page-boundary/scroll tests |
| One scheduler control ambiguously advertised both actions | Use “Pause nudges” / “Resume nudges” with matching accessible labels; show permanent death immediately in the HUD | Native controls and semantic caption test |

The suspected carried-purse cleanup defect did **not** reproduce: existing entity hooks already remove
the inventory entry. Its conservation/ownership regression is retained; no redundant inventory change
was made. Native explorer fog correctly limits actor/static disclosure and remains unchanged.

The birth-preset helper is rebuilt from immutable scenario data, like ActorPresets. It adds no sim
component/resource registration or hash encoding and reads no host preset files during play/resume.
Normal island saves retain their selected body. No private credentials, conversations or real provider
sessions are used by the checks.

## Inspection and verification

Initial standalone matrix: `target/island-polish-before.json`, six opened PNGs under
`target/island-polish-42d6b6c25127/npc-out/shots/` (normal, narrow, scheduler, death and diary states).
The standalone's fog toggle has no initial fog actor, so it is not used as fog evidence. Persistent
explorer evidence is the opened capture from `target/island-polish-explorer-35f754205944/`, after closing
its start prompt; only currently perceived actors and known static sites appear.

Focused checks so far:

- `cargo test --locked -p testbed_content --test island_village`: **19 passed**
  (`target/island-polish-content-final.log`).
- `cargo test --locked -p npc_agent_io --lib scheduler`: **7 passed**
  (`target/island-polish-scheduler.log`).
- `cargo test --locked -p testbed_view --lib`: **72 passed, 1 ignored**
  (`target/island-polish-view2.log`).
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed
  (`target/island-polish-clippy2.log`).

Native confirmation: `target/island-polish-after.json` returned **ok: true**, with six opened PNGs under
`target/island-polish-6ff019715ac2/npc-out/shots/`. In every captured state, measured captions have
**zero** overlapping pairs, intersections with village panels or viewport overflows. Normal day keeps
22 captions visible; the 480-logical-pixel view keeps 14. Excess captions are suppressed near their
anchors rather than stacked far away. Death identifies the current corpse explicitly; the successor
reader no longer carries the old corpse caption into the same position. Boundary controls are disabled.

The opened persistent explorer capture from `target/island-polish-explorer-35b951121f4d/` confirms fog
disclosure is preserved. Both disposable game hosts exited. `target/island-polish-scheduler-e2e.json`
returned **ok: true** for a real headless host with an owned fake CLI: exact conversation/prompt,
pause suppression, reset recovery, redacted status and unlinking. No real provider ran.

All five release apps built successfully with `--locked --release --bins`
(`target/island-polish-release.log`).

## Full gate

`cargo xtask verify` returned **`"ok": true`**, exit **0** (`target/island-polish-verify.json`,
`target/island-polish-verify.exit`). No concurrent Cargo build or disposable game host ran beside it.

| Check | Result |
|---|---|
| Format / Clippy | Formatted; no warnings |
| Workspace tests | 1,313 passed (3 leaky), 12 skipped |
| Doc tests | 1 passed, 10 ignored |
| Architecture | 7 passed |
| Replays | 8 verified |
| Goldens | 6 snapshots checked; none pending |
| Dependency policy | Advisories, bans, licenses and sources passed |

No verification tool was skipped. Existing test skips/ignored cases and nextest's leaky classifications
are retained; no protected file, assertion, snapshot, replay or budget was relaxed.

Local integration and the final main release build are recorded below when complete.
Native Rust has no HTML/CSS detector verdict; manual native inspection and semantic/geometry tests are
the applicable checks. Physical input, OS screen-reader announcements and authenticated provider
continuation remain outside this automated pass.
