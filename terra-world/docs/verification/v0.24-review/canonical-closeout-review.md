# Independent canonical closeout review — 0.24

**Result: passed independent readback; no blocking discrepancy found.** Reviewed
the actual terminal receipt, all bound logs/artifacts, current implementation
inputs, native executable and regional-session report. The independent readback
ran on 30 October 2026 from **13:40:57.891700 to 13:40:58.093244 UTC**. It launched
no Cargo/native process and changed no existing source or evidence.

## Actual canonical result

The [terminal receipt](../v0.24-release-acceptance/completed-gates.json) records
**44 successful gates**, from **13:13:54.011453 to 13:37:40.812703 UTC**:
**1,426.801250 seconds**, or **23 minutes 46.801 seconds**. The environment was
Linux x86_64, Rust 1.99.1, Python 3.12.14, offline Cargo with three build jobs.

Independent parsing of actual Cargo harness headers and individual passing-test
lines reconstructed **1,099 distinct Rust test identities** and **1,200 passing
Rust executions**, matching every summary and the terminal receipt. The extra
101 executions are repeated feature/scenario runs, not additional unique tests.
All recorded Rust failures and ignored tests are zero. The Python tooling gate
separately reports **72 tests, OK**; those are not part of either Rust total.

| Crate | Distinct passing Rust tests |
|---|---:|
| terra-sandbox | 393 |
| terra-demo | 250 |
| terra-bevy | 180 |
| terra-net | 128 |
| terra-persistence | 53 |
| terra-stream | 45 |
| terra-core | 33 |
| terra-history | 13 |
| terra-cli | 4 |
| **Total** | **1,099** |

The gate roster/order is exact. Shared admission validators accepted all 44
current execution records and startup probes using actual bound log sizes. They
record no timeout/overflow, complete output, leader completion and absence of
the owned POSIX process group. The configured limits were 3,600 seconds and
67,108,864 output bytes per gate. This is validation of retained execution
evidence, not a new observation of each historical child process.

## Source, executable and evidence identity

The terminal receipt and `gates.json` alias are byte-identical. Receipt SHA-256:
`8c386ac3ef5f1bead91f808db90b1301a6b6b834096a36c2cc654abd8f776d81`.

All **36 artifact hashes and 44 gate-log hashes** match their current files; all
80 bound files and both receipt files were read again before this review closed.
JSON duplicate keys were rejected during parsing. The distinct-identity artifact
matches the independently reconstructed set and per-crate totals.

The actual current implementation namespace contains the same **390 inputs**,
and every file matches the frozen manifest. Its raw manifest hash is
`1b7dc0264668315eba39d8e10acbb9fcb34a9282263413db03085c06c5117407`;
its canonical map-content digest is
`37d6c0e5a9e5170959980c86134501c1693a270d6c375253fa94bf642c834aaf`.
These are different encodings of the same recorded map. Documentation and
historical evidence outside the implementation map remain subject to the
separate preservation/package checks.

The actual current native executable was streamed and hashed independently:
**166,055,296 bytes**, SHA-256
`509d831c5c6ff1c6619a6ed7fc9feb92ed24dbd737048200f5aca6473ed147e6`.
It matches the terminal receipt and the sole native compiler-artifact path in
the captured build log. Regular-file identities remained stable during reads.

## Regional-session workload and observed timing

The [regional-session sidecar](../v0.24-release-acceptance/regional-sessions.json)
is exactly the JSON report captured in its bound command log. Its workload was
**16 real local TLS clients**, eight churn cycles, 400 active and 200 offline
ticks per cycle, plus unavailable/restored-source phases: **4,840 fixed steps**
across 18 measured phases. It used 25 prepared source chunks, 182 living bodies
(80 owned), and 1,024 cold overlay regions containing 4,096 fixture resources.
Measured workload wall time was **225.069706 seconds**; gate time including
command overhead was **225.446 seconds**.

| Observation | Samples | p50 ms | p95 ms | p99 ms | Maximum ms |
|---|---:|---:|---:|---:|---:|
| Host update | 7,460 | 11.106 | 91.037 | 110.569 | 158.379 |
| Advancing update | 4,840 | 26.371 | 97.493 | 115.104 | 158.379 |
| Durable command acknowledgement | 128 | 84.396 | 97.221 | 99.581 | 100.489 |
| Client ping round trip | 6,464 | 37.752 | 157.126 | 188.511 | 349.180 |
| Storage-wait update | 2,541 | 0.001 | 0.549 | 6.079 | 27.480 |

All recorded state/recovery assertions passed: 64 offline owner checks, 64 resume
checks, 128 restart checks, 16 authenticated recovered-owner checks, exact cargo,
craft inventory, command sequences, cold records and backup/private metadata.
One deliberately injected storage failure was recovered atomically with exact
retry; it is an expected workload condition, not an unresolved gate failure.
Backup generation 165 agrees with its recovery receipt. Every client received
2,020 actor snapshots and 2,020 motion states.

## Limits and final handoff

These timings come from a finite accelerated debug workload in a shared Linux
container, with the host and client fixtures in the same process. RSS samples
range from 37,072 to 101,056 KiB; they do not prove a long-duration memory plateau.
Storage paused authority on 2,541 updates. Cold overlay fixtures do not extend
the prepared terrain or establish populated simulation in every cold region.

There was no Internet delay/loss, renderer/GPU load, live provider, distributed
authority or production identity enrollment in this workload. Sixteen local
clients do not establish a deployment's supported concurrent-player capacity.
Windows, physical devices, hardware performance and uncoached usability remain
separate qualifications.

Canonical acceptance is complete for these exact implementation inputs and this
executable. The integrator's accepted native image/process audit and final
source/desktop archive preservation/readback are separate handoff gates. This
review does not claim those results before their own evidence is published.
