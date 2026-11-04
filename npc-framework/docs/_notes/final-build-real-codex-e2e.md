# Final-build real Codex playtest

Result: **all requested functional checks verified, with harness limitations and
one extra interrupted dispatch recorded**. This is a qualified runtime result,
not a claim that every automation attempt completed cleanly.

The game used the installed subscription-authenticated Codex CLI through native
Start Codex and New AI session actions. The game supplied a bounded engineering
mission automatically. No prompt was entered into a Codex terminal. No fake
metrics or transcripts were inserted, and no provider history, authentication
files or raw provider arguments were read.

## Build and scope

The initial preflight matched all 501 files in the final green candidate,
fingerprint `cc24f5fd46140bf0a22ed0ec291194281cda1f613fb14e23b20e6b7ccd3b743a`.
The baseline gate was `ok:true`, with 1,695 tests passed. SHA-256 values for
`actor_testbed.exe`, `game_menu.exe`, `npcctl.exe` and
`ai-agent-game-client.exe` remained unchanged throughout this playtest. No
application source, dependency, global CLI setting or application binary was
changed by this runtime test. A separately authorized subsequent simulation
increment began source work after the binary/source preflight; this result
describes the frozen final executables, not that later increment.

The disposable Fixed Island Village fixture used a ten-second idle threshold,
five-second cooldown, three-dispatch hourly cap and 240-second provider timeout.
These are clearly labeled test policies, not default survival behavior. The
mission asked the actor to register, enter, take control, observe, speak, write a
physical diary page and add a private notebook note, then finish its turn.

## Verified checks

| Requirement | Actual evidence |
|---|---|
| Actor control | Actual server-reported `actor_controlled:true`, native controls showing confirmed control, and successful game-tool actions in both conversations |
| Automatic continuation | One native Start completed two turns in the same exact provider conversation and metrics record; no second Start or manual AI prompt triggered turn two |
| Live Markdown logs in game | 20 changing native log-body snapshots while the first turn ran; concurrent Markdown growth recorded without pressing Refresh |
| Live metrics in game | The visible first conversation updated from one completed turn/357,727 input/1,292 output to two completed turns/888,129 input/2,343 output without Refresh; values matched actual JSON exports |
| Native log-folder preference | Entered `c:/projects/qa-final-9b2764` in the real Settings field with direct native keystrokes and clicked Save; verified the persisted preference and unchanged JSON metrics folder |
| Next session destination | The fresh conversation wrote its Markdown log in the saved folder. The original folder retained its earlier log; the new folder retained both the failed-setup diagnostic log and the actual fresh-run log |
| Real gameplay | Saved world at tick 4,005 contains one physical diary page each for initial, automatic-continuation and fresh-session objectives; the notebook contains the three corresponding notes |

The Settings boundary required resuming the same saved world after a desktop
availability timeout. This is not an uninterrupted-game-process reload claim.
The new session still launched through the game, using the exact saved Settings
file. Existing focused launch tests separately verify rereading that file on each
Start without restarting the game.

## Actual usage and outcomes

| Conversation | Started | Completed | Interrupted | Reported input | Reported output | Cached input |
|---|---:|---:|---:|---:|---:|---:|
| Initial plus automatic continuation | 2 | 2 | 0 | 888,129 | 2,343 | 833,536 |
| Fresh conversation after Settings save | 2 | 1 | 1 | 524,536 | 1,483 | 482,176 |

Thus **four model dispatches occurred: three completed and one was interrupted**.
No provider turn was recorded as failed. A separate metrics-setup attempt stopped
before any model dispatch and retained its zero-start metrics record and Markdown
diagnostic log. Usage shown above covers reports received from completed turns;
the interrupted attempt did not provide numeric trailing usage or monetary cost.

Codex CLI 0.145.0 used verified cumulative-session accounting. Requested selection
was `gpt-5.6-sol` with effort `high`, taken from the advertised default/CLI config.
Actual model and effort were not reported and remained unavailable. Monetary cost
also remained unavailable, not a fabricated zero-dollar subscription charge.

## Automation findings and preserved failures

1. Computer Use detected concurrent user input while Settings was covered by
   other work. Input paused and the user made the desktop available. The first
   bounded Settings wait expired; all owned resources stopped and its failed
   report was preserved. The two successful model turns were not rerun.
2. Clipboard-based `type_text` restored the user's previous clipboard before the
   application's asynchronous paste read completed. The unrelated clipboard
   value was not saved. Direct native keystrokes, including the active German
   layout's punctuation, followed by the real Save button succeeded. This result
   does not claim reliable clipboard-based automation.
3. One fresh-client setup failed while the Python collector read a metrics file.
   A standalone probe using the same Rust atomic replacement pattern reproduced
   Windows error 5 with a non-delete-sharing reader and succeeded with
   `FILE_SHARE_DELETE`. The collector was corrected to permit atomic replacement;
   the application binaries were not changed. This exposes an external-reader
   file-sharing limitation worth hardening in a future implementation pass. The
   exact cause of the original setup failure is inferred from the timing and
   deterministic reproduction, not an exported raw OS error from the application.
4. The failed setup left an empty metrics record. The harness selected that old
   record and matched a stale client PID during its subsequent fresh-run watch.
   That monitoring error let one extra automatic continuation start before
   cleanup. It was interrupted, not silently counted as a completed turn. An
   independent native observer selected the actual fresh record and verified its
   visible values against the real export. The fresh record was first observed
   after completion, so the first conversation supplies the before/after live
   metrics proof. No further model call or provider-failure retry was made.

The failed harness reports remain failed and unchanged. The final verification
record is a separate evidence audit of the real metrics, native observations,
saved settings, logs and game artifacts; it does not relabel those failed
automation runs as clean passes.

## Evidence and cleanup

Fixture: `target/final-build-codex-9b2764c88734/`.

- `final-playtest-verification.json`: authoritative functional-check audit,
  actual outcomes, usage, binary hashes and explicit limitations.
- `summary.json`, `summary-final.json`, `summary-verified.json`: preserved
  harness interruption/failure reports.
- `fresh-native-observer.json`: actual fresh-record native metrics verification.
- `settings-ui-proof.json`: real Settings Save and unchanged metrics destination.
- `reader-sharing-proof.json`: controlled Rust/Windows replacement reproduction.
- `game-artifact-proof.json`: three physical pages, three notebook notes and tick.
- `evidence-sha256.json`: integrity hashes for reports, actual logs/metrics and PNGs.
- `npc-out/shots/`: confirmed actor control, real log content and metrics before
  and after automatic continuation.
- `npc-out-resumed/shots/fresh-actual-metrics-completed.png`: actual fresh metrics.

All five principal gameplay/metrics PNGs were opened and inspected. Settings Save
was also observed through Computer Use. All owned game, menu and dedicated-client
resources stopped; the final public status is stopped/disconnected with actor
control cleared, its actual client PID is no longer live, and the game run marker
was removed. Existing user Codex processes were untouched. Release binaries and
GPU surfaces were then released to the next authorized simulation increment.
