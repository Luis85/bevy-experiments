# Native review — 0.21 candidate and final confirmation

Executed 29 October 2026 on Linux x86_64 in an isolated X11 virtual display.
The renderer reported Vulkan **llvmpipe / LLVM 20.1.2**, a software CPU adapter.
Display resolution was 1440×900; windows were also inspected at the declared
minimum 1100×700. The tested settings used 150% interface text.

## Method and provenance

The integrator launched the actual compiled native executable, operated its
native keyboard and mouse inputs, and captured the real window. `run_021.py`
records command, PID, binary hash, source-input identity, exit code and log hash.
`ui_021.py` records window PID before/after capture, title, exact dimensions,
image bytes and SHA-256. Images were opened and visually inspected; no mockup or
historical screenshot substitutes for this run.

Default sandbox execution could not create the local display socket. The normal
reviewed execution permission path allowed the isolated display and local test
processes. The display was stopped after the review. There was no browser or
external service interaction.

## Candidate findings and corrections

The initial launcher showed stable footer actions and scrolling content at
1440×900 and 1100×700 with 150% text. F4 opened the private text draft; Tab moved
the visible accent outline to Delete inside the modal. Escape discarded it and
returned to the unchanged setup screen without activating the parent action.

The Space key wrapped to `Spa` / `ce` at large text. The correction gives only
that character key a modestly larger flex weight, preserving text size. The
launch review also used older direct-restore wording; it now reuses the shared
four-context save description. This was one narrow correction batch.

The native launcher then created a separate synthetic radius-one managed world.
Its regional checkpoint and immutable launch record appeared; the paused menu
showed the saved generation. At 1100×700, Shift+Tab scrolled Save and quit fully
into view with its focus outline. Enter completed a clean save and process exit.
The profile retained `checkpoint_present=true`, the witness existed and SQLite
integrity was `ok` in `managed-candidate-save-evidence.json`.

The candidate's initial gameplay image caught a dark/partially drawn scene during
early rendering, despite logical Region ready. Subsequent captured frames showed
the world; these captures alone do not determine the cause. This is retained
evidence, not an accepted first-visible-frame or GPU-loading measurement;
startup presentation timing and shader warmup remain G07 investigation work.

The candidate process spans the small source correction, so its receipt correctly
records `source_inputs_unchanged=false`. It is not used as final frozen evidence.

## Final rebuilt confirmation

Both direct confirmation processes used executable SHA-256:

`209b5f2ee0d74d8b4776346297af6838dd9e05cf622af80645f63dedb0bc13d1`

Both recorded 373 implementation inputs with canonical identity
`53339826e1360eb8567b793dc534bc1d10d302e493d57a3a18bf986bcb19b395`, unchanged
through execution, and both exited successfully. After these captures, only
`scripts/desktop-payload.json` changed among the frozen implementation inputs:
the curated desktop allowlist gained the current 0.21 handoff and preserved
0.20 documents. Runtime code/configuration/assets stayed unchanged. Final
acceptance must verify the exact same executable and record that one input
difference before associating these captures with delivery.

| Capture / record | Observed result |
|---|---|
| `confirmation-keyboard-1100-150.png` | Single-line Space; draft, all character rows, five editing actions and complete instructions visible inside the modal. Independently inspected by the player specialist. |
| `confirmation-managed-copy-1100-150.png` | Setup screen with Review world focused. The name records intended check, not successful navigation; this image does not verify save copy. |
| `confirmation-review-copy-1100-150.png` | Ready to begin and fixed footer visible; save paragraph below the scroll position. Corrected F9 wording is established by shared-source/test evidence, not this image. |
| `confirmation-managed-playing-1440-150.png` | Final binary reopened the same managed world with Performance graphics, no atmospheric sky and camp view; terrain, actors, roads, activities and checkpoint status rendered. |
| `confirmation-managed-paused-1440-150.png` | Same title and saved checkpoint shown in the native session menu; final Save and quit completed. |
| `managed-confirmation-save-evidence.json` | Witness hash unchanged, played profile retained, database integrity `ok`, committed generation 15 after reopen/save. |

The player specialist inspected the final editor and the candidate paused/final
focus endpoints. The integrator executed the key sequence; this distinction is
preserved in `../v0.21-review/player-experience.md`.

## Limits

This establishes the named native rendering, logical keyboard/focus, minimum
window/large-text and bounded create/save/reopen journey on Linux software
rendering. It does not establish physical controllers/wheels, screen readers,
assistive access, Windows, physical GPU performance, all display scales, native
network cancellation, or every storage/recovery failure path. Those failure
paths have their separately named automated regressions and remaining criteria.

No simulated user-study finding, latency target or sustained-play conclusion is
derived from these captures. The complete archived source retains the images,
logs, per-process receipts and per-capture identities.
