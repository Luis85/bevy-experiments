# Native AI session views — 2026-11-01

The explorer's F10 controls now open a native Sessions reader. The game menu
offers Logs & metrics for saved agents and detached worlds. The native Settings
form independently configures the private Markdown log folder and JSON metrics
folder, preserving older settings files and applying saved changes at the next
normal/fresh Start through the exact menu settings file.

Both views use background reads, cached pages, history navigation, refresh,
live following and focused keyboard scrolling. Back/Escape preserves the previous
controls, notebook drafts, focus and intentional pause state. Metrics distinguish
actual/requested model and effort, unavailable costs and accounting coverage.
Logs use the existing licensed embedded Barlow font for accented text and the
em dash emitted by client headers; the surrounding controls retain their font.

A private workspace owner/index records exact output paths across folder changes.
The reader never scans shared external folders or reads provider history, auth
configuration or private client state. Compatible default files and canonical
metrics ledgers remain visible. The catalog is bounded to 256 recent entries;
12 KiB pages allow reading full log files without modifying or deleting them.
Older externally stored logs from before indexing remain in their original
folders and are not automatically discovered.

## Checks

- Initial explorer package filter: 95 passed, one ignored.
- Initial complete game-menu test binary: 135 passed, one ignored.
- Native menu viewer regressions: six passed, including stale worker errors,
  cross-agent reopening, linked metrics, stable scrolling, native screen/widget
  construction and unique keyboard order.
- Shared history reader: eight passed, including malformed ownership/schema/path
  rejection, cross-folder isolation, legacy canonical fallback, nullable metadata
  and costs, UTC date bounds, lossless UTF-8 paging and exact LF/CRLF headers.
- Actor controls/history: 15 passed, including focus/pause/draft preservation,
  stale replies, DPI-aware keyboard scroll bounds and embedded-font assignment.
- Client binary: 68 passed, including owned indexing across output folder changes.
- Scoped three-app/all-target Clippy: passed with `-D warnings` after ordinary
  style fixes, without lint suppression. See `target/native-sessions-clippy-crlf.log`.
- Font cmap inspection confirms the existing default subset lacks U+00E9/U+2014
  and the embedded Barlow asset supports both:
  `target/native-sessions-font-coverage.json`.
- Release builds passed for `game_menu`, `actor_testbed` and `npcctl`; final
  menu polish rebuilt the existing three-app release configuration in 1m14s.
  Evidence: `target/native-sessions-release-crlf.log` and
  `target/native-sessions-menu-polish-release-hot.log`.
- Native screenshot batch: nine captures, owned processes stopped, no provider
  started, no client state created and saved tick unchanged at zero. The actor
  views at logical widths 1280/760/480, actor metrics, empty menu state and narrow
  Settings field passed review. The three populated menu views exposed an
  overflowing middle panel; one polish batch bounded its flex basis/minimum
  height and protected the header/footer from shrinking.
- The three affected menu views were recaptured once at 960 and 480 widths.
  Parent and independent product reviewer opened all three and returned SHIP:
  Back and paging stay visible, content scrolls inside its bounds, and accented
  text/log tails remain readable. No further polishing or captures were needed.
  Evidence: `target/native-sessions-f2575e04ef37/report.json` and
  `target/native-sessions-confirm-b17e6415a481/report.json`.
- **Final combined `cargo xtask verify`: `ok:true`, all seven steps passed.**
  **1,585/1,585** Nextest tests passed, with one slow test, five leaky-test
  classifications and twelve skipped tests. Doc tests: one passed, zero failed,
  ten ignored. Format and workspace/all-target Clippy passed with no warnings;
  seven architecture checks, eight protected replays and six goldens passed,
  with no pending snapshots (`INSTA_UPDATE=no`). Dependency advisories, bans,
  licenses and sources all passed. No optional tool was skipped.
  Evidence: `target/npc-life-verify-final.json`.
  Final release builds of all five maintained apps also passed in 3m00s:
  `target/npc-life-release-final.log`. The integration limitation below is closed;
  the native UI source and reviewed captures remained unchanged.
- Historical first combined `cargo xtask verify` returned **`ok:false`**. **1,582/1,583** tests
  passed, with one slow test, two leaky-test warnings and twelve skipped tests;
  doc tests: one passed, zero failed, ten ignored. Format, workspace Clippy,
  seven architecture checks, eight replays, six goldens and dependency policy
  all passed; no optional tool was skipped. The sole failing test was
  `npcctl::island_playtest registered_cold_income_needs_and_save_resume`, while
  approaching a moving trader. The parallel NPC-life implementation corrected
  stale-offer/search assumptions without removing the original budgets or outcome
  assertions, and added read-only public workplace data with privacy regressions.
  The three focused E2E tests passed in 7.98s and nine directory/privacy tests
  passed in 0.19s before the final green gate. No client/session-viewer test failed.
  This older failed verdict is retained as history, not the current validation
  status. Evidence: `target/npc-life-registered-workplace-search.log` and the
  parallel NPC-life closeout record.
  Evidence: `target/npc-life-verify.json`.

The first capture setup produced no PNGs: Windows Python converted its synthetic
Markdown header to CRLF, which the first reader rejected. Its failure report is
preserved in `target/native-sessions-ca9824d5b847/report.json`; owned processes
stopped and no provider session started. The exact-header CRLF regression and
compatibility fix passed before retrying. This is not visual validation evidence.

Concurrent NPC-life work owns separate simulation files. Its temporary missing
module/BTreeSet compile errors briefly blocked the UI iteration chain; its owner
corrected them. The source-specific final gate is coordinated with that chat so
the final verdict covers the combined checkout.

The implementation's initial screenshot matrix used no real provider inference,
provider-history read or physical OS input. Its synthetic session values are UI
evidence, not observed billing or gameplay.

## Subsequent real Codex playtest

The user's separately requested live playtest completed two real Codex sessions
through native Start/Fresh controls. It observed 19 live log updates, native
metrics changing without Refresh, and the next session writing into a folder
saved through the actual Settings UI while preserving the first log. Both runs
created physical diary pages and notebook notes. All owned processes stopped.
See [the real-playtest record](2026-11-01-real-native-codex-playtest.md) for actual
token counts, evidence, the desktop-availability interruption and automation-input
limitations. No additional application source/build changes were made.
