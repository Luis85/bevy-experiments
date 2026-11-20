# Player experience source review — 0.19 candidate

Reviewed 29 October 2026 against the attached 0.18 source. Scope: onboarding,
menu structure, recovery/error comprehension, keyboard/controller handoffs,
accessibility support and continuing-player guidance. This is a source and
contract review. No fresh native screenshots, physical input tests, contrast
measurements or uncoached sessions were available during this review. Existing
0.18 captures remain historical evidence.

## Journey and evidence

1. **Launch and choose a world:** a recent world, New world, Your worlds,
   Settings, Controls and Multiplayer have explicit actions. Saved-world actions
   bind their listing revision. `launcher/ui.rs:465–493` and
   `launcher/mod.rs:828` retain the reviewed-world boundary.
2. **Create a character and configure:** bounded editable names, visible source
   limitations and world review prevent geographic/scenario confusion. Source
   errors remain actionable, but error layout has the bounded-height defect below.
3. **Act in the world:** six goal choices and a private live commitments briefing
   give continuing goals without another completion ledger. Source coverage,
   physical arrival, input cost and gross reward remain distinct. Human clarity
   and balance need real playtests; scripted correctness cannot close G03/G04.
4. **Use keyboard/controller:** rebinding reserves interface shortcuts;
   management focus suppresses world controls, controller text uses a private
   draft, and standard-pad focus/device rearm requires neutral input. A source
   review did not reveal a new regression in those gates. Physical reconnect,
   assistive technology, wheel behavior and full keyboard journeys remain G08.
5. **Save, reopen, back up and recover:** checkpoint state is differentiated from
   diagnostic state, storage poison requires validated reopen, and recovery makes
   a separate named copy. An existing deliberate-restore action immediately
   discards later changes; pre-action review is a design opportunity below.

## Prioritized findings and bounded implementation

### UX19-01 — P2: error details can consume the launcher footer's space

`crates/terra-demo/src/launcher/ui.rs:465–471` places recovery/list errors inside
`scroll_panel`; `:615–618` places other screens' errors and the Home warning
outside it, immediately before the footer. Errors can include long filesystem
paths or failed-session output. The outer panel has a viewport-constrained
maximum height. At the minimum window and enlarged text, lengthy error content
therefore competes with the Back/Connect/Save/Create footer for fixed panel
height. The non-recovery error labels also omit character-level line breaking,
so an unbroken filename can exceed the text column. The report path in Settings
has the same wrapping gap.

Implement one shared bounded status area inside the existing scroll panel for
all screens. Keep notices, exact error details and relevant warnings; apply
WordOrCharacter wrapping to path-bearing status and report-path text. Keep
primary/footer controls outside the scrolling content and preserve their action
identities, focus and confirmation contracts. This is a layout correction, not a
new storage or gameplay behavior.

Acceptance: build the native crate; inspect Home, Join, World and Recovery review
with long unbroken paths and multiline messages at 1100×700/150% and reference
size. Read the complete message through scrolling; reach Back and the primary
action through Tab/controller navigation; confirm error details are neither
clipped nor duplicated. Source-structure checks can establish ancestry/wrapping,
but cannot replace actual rendering acceptance.

### UX19-02 — P2 design backlog: deliberate restore has no pre-action review

`interaction.rs:393` dispatches `SandboxIntent::Load` directly; paused
`flow.rs:506–516` does likewise. Documentation and menu labels disclose that
later changes are discarded, so this is not an undisclosed behavior. Nevertheless
an accidental F9 or Load activation has irreversible consequences for the current
unsaved session. A future common restore-review model should name the current
world, target checkpoint generation (or manual snapshot) and discard consequence,
with Cancel as initial focus and stale-world/generation rejection. This needs
both paused and world-panel paths; adding confirmation to only one leaves the
other entry point inconsistent. Do not claim closure without those routes and
regressions. Keep in backlog unless explicitly selected in the integration plan.

### UX19-03 — P1 acceptance: Windows and uncoached journeys remain unqualified

The user's Windows PC is the relevant target, but 0.18 establishes Linux review
only. Repeat documented first/returning Supply, Home/Community and Road/Company
journeys on the intended machine; record OS/GPU/input, text size, stalls and exact
save/reopen/recovery outcomes. A new Linux/source gate cannot close G02 or these
G03/G04/G08 acceptance conditions.

### UX19-04 — P2 acceptance: accessibility claims need device/user evidence

Keyboard focus, enlarged text, controller editing and neutral rearm are concrete
features. They do not establish screen-reader semantics, full contrast compliance,
motion comfort or localization. Keep those claims open; perform representative
assistive/device sessions before choosing supported accessibility guarantees.

## Recommended selected scope

Implement UX19-01 now with the incumbent visual design and exact errors retained.
Keep UX19-02 as a coordinated future change unless the integrator selects its
full surface area. Retain UX19-03/04 as explicit acceptance work rather than
claiming them closed through code changes.
