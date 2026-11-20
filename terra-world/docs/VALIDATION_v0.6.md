# Terra World 0.6 validation record

Validation date: **2026-10-27**. Source release **0.6.0**, wire protocol **4**,
domain save version **1**, regional database schema **1**. The release contains
**393 distinct passing Rust tests** with zero final failures and zero ignored
tests. This is correctness evidence for the bounded reference implementation.

The [machine-readable release report](release-verification-v0.6.json) links the
fresh automated, operational, native and storage evidence. Earlier records remain
in [v0.5 validation](VALIDATION_v0.5.md) and [v0.4 validation](VALIDATION_v0.4.md);
their timings and images are not presented as fresh measurements of this release.

## Automated gates

| Crate | Distinct passing tests |
|---|---:|
| `terra-cli` | 4 |
| `terra-core` | 19 |
| `terra-history` | 13 |
| `terra-net` | 79 |
| `terra-persistence` | 26 |
| `terra-sandbox` | 127 |
| `terra-stream` | 20 |
| `terra-bevy` | 63 |
| `terra-demo` | 42 |
| **Total** | **393** |

The Bevy total comprises 42 render-library cases and 21 integration cases. Its 23
headless-library cases were also run but overlap the render-library set and are
counted once. The non-Bevy gate has six empty documentation-test harnesses;
Bevy's documentation harness is also empty. Neither is counted as an executable
test. The native demo contributes 42 tests. Two finite examples, actual CLI
smoke scenarios, native actions and repeated targeted runs are separate evidence.

Final commands, using the committed lockfile:

```sh
cargo test --workspace --exclude terra-demo --exclude terra-bevy --locked
cargo test -p terra-bevy --no-default-features --locked
cargo test -p terra-bevy --lib --locked
cargo test -p terra-demo --locked
cargo run -p terra-bevy --no-default-features --example headless --locked
cargo run -p terra-bevy --no-default-features --example custom_provider --locked
cargo build -p terra-demo --locked
cargo fmt --all -- --check
```

The build used Rust 1.99.1 and pinned Bevy 0.20.0 on Linux x86_64. Local socket
tests initially encountered executor restrictions; they passed when local TCP
execution was permitted, without weakening the test or network implementation.
An early native compilation exposed two incorrect module import paths; these
were corrected before all final gates. Only successful complete final suites
contribute to the release count.

### Driving, economy and authority

- Seven shared vehicle tests cover acceleration/braking, reverse/steering,
  weather/mass, deterministic serialized dynamics, invalid input atomicity,
  subnormal timestep output and exact geographic center/support dimensions.
- Twenty transport tests cover finite value/fuel conservation, qualification and
  ownership, physical arrival, escrow/reservations, cargo returns, expiration,
  trailers, race order, privacy, older defaults, regional restoration, body caps
  and physical-time driver fatigue. They are part of 127 sandbox tests covering
  the pre-existing crafting, farming, factions, ecology, logistics and territories.
- Six actual TCP driving cases cover host-owned motion, replay rejection,
  generic-velocity bypass, timeout braking, rapid control transfer, appearance
  privacy, interest removal and durable reconnect. Seventeen older loopback
  gameplay cases and five delta cases also pass.
- Three new host regressions reject custom providers that bypass admission or
  return invalid support. Missing/invalid/forbidden corner coverage stops a
  vehicle without wear; a covered solid obstacle retains impact damage.
- Actual Bevy schedule tests cover absent-input braking versus true pause,
  missing-corner versus solid-obstacle behavior, one-time restored possession,
  and existing trade/production/delivery/restoration in a real App.

### Storage, security and operations

Twenty-six persistence cases cover store identity, validation, generation and
external-change guards, cold-page cache behavior, worker lifecycle, backup,
atomic failures and maintenance destinations. Twelve actual host regional cases
exercise durable admission/commands and continuous driving/focus/ping without a
transaction barrier per input. Nine Bevy regional integration cases exercise
hydration, source seeding, restoration, checkpoints and failure handling.

Four TLS integration tests verify trusted commands/resume, CA/name rejection,
plaintext rejection by a TLS listener and bounded configuration. Application
Hello/resume capabilities are withheld until verification and exact ALPN finish.
This does not provide a public account system.

The actual `terra-worldctl` subprocess test passes local/portable round trips and
no-overwrite guards. [Fresh server CLI evidence](server-operations-v0.6.json)
records exact JSON→new SQLite→JSON, import/export overwrite refusal, remote
plaintext guards and configured TLS listener startup. Listener startup is not
substituted for the separate handshake tests.

### Native flow regressions

The 42 native cases include source/preset validation, independent world folders,
character persistence, path protection, text/focus behavior, contextual Escape,
geographic coverage labels, local versus network pause, explicit save/restore,
affordable fuel, active-vehicle signal targeting, course-start navigation and
bounded original meshes. A regression actually calls `App::run` and proves the
post-checkpoint external handoff survives consumption of the live App.

## Fresh finite regional probe

The [0.6 regional report](regional-probe-v0.6.json) uses a synthetic world with
**239 complete / 59 active bodies** and **6,400 complete / 400 active resources**.
It runs 500 simulation steps (25 physical seconds, 250 calendar seconds), with no
sockets, imported collision or live providers. It is not an MMORPG load test.

| Measurement | Result |
|---|---:|
| Submission including snapshot clone, p50 / p95 | 0.687 / 0.991 ms |
| Background validation/commit, p50 / p95 | 47.622 / 73.985 ms |
| Maximum nonblocking poll (773 calls) | 0.063 ms |
| Explicit complete restore | 58.560 ms |
| Guarded cache hits / cold checkpoint decodes after initial load | 98 / 0 |
| Verified durable generations | 33 |

Every complete/worker checkpoint, round trip, online backup and restart matched
exactly; dormant resource state did not advance. The optimized development build
used SQLite 3.53.2 with debug assertions. These are shared-environment fixture
measurements, not capacity or latency guarantees.

Ordinary submission/polling no longer waits for the full transaction on the main
thread. **Authority still pauses while the worker reconstructs and validates the
bounded complete world.** Snapshot cloning and result installation remain main
thread work. Startup, explicit restore/export and final drain are synchronous.
The approximately 48–74 ms worker result is therefore a remaining simulation
latency concern, even though presentation may continue. Affected-dependency-group
transactions and realistic workload budgets are still required.

## Native interaction and visual confirmation

The [native evidence record](native-v6-verification.json) lists actual actions,
state checks and hashed captures. The shared review used Linux/X11 at 1440×900
and 1100×700 with Mesa software Vulkan. It is an interaction/rendering check, not
a gaming-GPU performance result or Windows/macOS certification.

The review and one combined confirmation cover menu transitions, named character
and appearance entry, heightmap/world configuration, review and creation, native
play, possession/camera switching, transport actions, garage controls, pause,
save-and-return and resume. The record distinguishes observed native behavior
from domain/network behavior established by automated tests. No generated imagery
is used as evidence. Prototype geometric assets remain visibly distinct from a
production AAA art library.

## Reproduction and scope

`scripts/verify.sh` contains the normal verification sequence. See
[GAME_FLOW.md](GAME_FLOW.md), [TRANSPORT.md](TRANSPORT.md),
[OPERATIONS.md](OPERATIONS.md) and [INTEGRATION.md](INTEGRATION.md) for entry
points. Source data and simulation content remain separate: the validation
heightmap and settlement are explicitly synthetic. Live-provider availability,
licensed global/historical coverage and source ingestion at production scale are
not established by an offline fixture.

The [product review](PRODUCT_REVIEW.md) records fixes and remaining risks.
[Motor Town coverage](MOTORTOWN_PARITY.md) and the
[MMORPG foundation guide](MMORPG_FOUNDATION.md) define the next acceptance gates.
The release does not claim complete Motor Town parity, AAA presentation,
MMORPG concurrency, full rigid-body contacts, client prediction, public account
hosting or distributed regional handoff.
