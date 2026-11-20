# Independent diagnostic — interrupted 0.22 acceptance

**Cause: unknown.** Read-only inspection found no substantiated application
failure, configured deadline expiry or code defect explaining why the verifier
and workload disappeared. This report preserves the observation boundary; it
does not promote the incomplete run into acceptance or assign an unrecorded
infrastructure cause.

The diagnostic inspected the interrupted `v0.22-confirmed-acceptance` evidence,
runner/workload source and the delivered baseline. It made no code or prior
evidence edits, ran no tests or Cargo commands, and took no action against any
active process. The replacement `v0.22-release-acceptance` run was left untouched.
This separate report was subsequently authorized as documentation only.

## Retained observations

- `gates.json` remains `running`, with **42 completed gates**, all recording exit
  code zero. Its active gate is `regional-sessions`.
- The outer runner log ends at `[regional-sessions] running`. Its inspected hash
  matches the hash in `acceptance-interruption.json`.
- No `regional-sessions.log` or `completed-gates.json` was present in the
  interrupted directory at inspection. The interruption record reports no
  surviving verifier/workload process after scoped process inspection; this
  diagnostic did not repeat or alter those processes.
- The regional sidecar exists and contains the workload's final JSON report.
  The interruption record states that the **378 frozen implementation inputs**
  matched current inputs. That complete-input comparison belongs to the
  integrator's recorded observation; the diagnostic separately compared the two
  relevant source files to their exact baseline ZIP members.

| Recorded event | UTC |
|---|---|
| Regional gate began | 2026-10-29 21:57:33.200583 |
| Regional sidecar modification | 2026-10-29 22:00:55.410917 |
| Missing-process observation in interruption record | 2026-10-29 22:04:00.952450 |

The sidecar appeared **202.210334 seconds** after gate startup. The recorded
missing-process observation was **387.751867 seconds** after startup. The gate's
configured timeout was **3,600 seconds**, so these retained timestamps do not
support normal deadline expiry as the explanation. The recorded cgroup counters
show `oom 0`, `oom_kill 0` and `oom_group_kill 0`; they do not identify another
cause.

## Runner and workload boundary

`scripts/verify.py::run_logged` directs child output into an anonymous temporary
file. It publishes the visible gate log in `finally`, after process completion
or handled interruption. The timeout branch stops the owned process group and
adds a timeout explanation; handled exceptions also pass through log publication.
The main runner records a gate result only after that function returns.

An abrupt loss of the runner can bypass Python cleanup and leave no published
gate log. That is an observability limit of this capture design, not evidence
that a particular signal, tool-session event or host action occurred here.
The retained record does not identify one.

`regional_session_load.rs` writes its JSON sidecar immediately before printing
the report and returning. Consequently, the sidecar establishes that execution
reached that write; it does **not** independently establish successful child
exit, completed destructor/cleanup work, captured stdout or the runner's
acceptance of that report. A normal child failure would still ordinarily return
to the runner's log/result handling. No such result was retained in this run.

Both inspected source files are byte-identical to the corresponding members of
`/workspace/scratch/96f1d170dd13/deliverables/terra-world-0.21.0.zip`, under the
`terra-world/` archive prefix. The diagnostic read those actual members; it did
not infer equality from Git history or rehash the entire baseline archive.

## Exact inspected hashes

Paths below are relative to the Terra source root. SHA-256 values were computed
from the inspected bytes, without modifying the files.

| Path | Bytes | SHA-256 |
|---|---:|---|
| `docs/verification/v0.22-review/acceptance-interruption.json` | 2,349 | `97db41e90ad2b72456b8bda6b2d226674de4a13eb5461e555570b02e1393d986` |
| `docs/verification/v0.22-preflight/confirmed-acceptance-runner.log` | 3,284 | `c26dc6c7725778a5948d49c1cec1ea6edc876d7f6f224bbcdc6ded461ab91ab9` |
| `docs/verification/v0.22-confirmed-acceptance/gates.json` | 33,350 | `8b38834391387ea48d30e70c83a22ca338c9b6eb62cae71a28d2fde1b880ffb4` |
| `docs/verification/v0.22-confirmed-acceptance/source-hashes.json` | 44,268 | `cd417e9c9cfaff341d86908467aa975e8f134df85e21c6724d01e5e739b61ff7` |
| `docs/verification/v0.22-confirmed-acceptance/regional-sessions.json` | 30,520 | `de95bd467661ff0f37db444a7ffc5eb317c5ec1b912ea03262239de886619180` |
| `scripts/verify.py` — also exact baseline member | 68,798 | `177c424019de79fcb26755599f06641d4137886a72da6a7b7731c39bf8beed6e` |
| `crates/terra-net/examples/regional_session_load.rs` — also exact baseline member | 40,623 | `45e9ea3ff56c1e254c04b448a037e91b515283720b9eea19a9f643d72aa9ff3b` |

## Disposition

Retain the interrupted directory unchanged. Its sidecar and 42 successful gate
results cannot substitute for a complete canonical terminal receipt. The fresh
run must execute all 44 gates and bind its own final source, logs, reports and
native executable before packaging is authorized. This diagnostic does not
establish whether the interruption could recur or justify changing frozen code
on the basis of an unproven cause.
