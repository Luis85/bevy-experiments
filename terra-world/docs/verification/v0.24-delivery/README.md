# Terra World 0.24 — delivery record

## Scope and admission

The deliverables are the complete preserved source archive, the exact accepted Linux x86_64 desktop with its original runtime content, and six standalone review/plan/research/gap/validation/verification reports. Source history is included; this is not a patch-only handoff.

The source packer requires the actual successful 44-gate terminal receipt, current implementation and executable identity, complete bounded execution metadata and unchanged gate/evidence hashes. It independently checks every regular member of the pinned delivered 0.23 archive, exact retained current reports and manifest, original modes and prior release entries. It rereads the complete current source and baseline and verifies every ZIP member twice.

The desktop packer uses the same terminal receipt and exact native executable. `verify_desktop_archive.py` validates the actual ZIP's bounded namespace, types and expansion budget, extracts privately into a fresh directory, preserves modes, verifies passively before optional help execution, and rereads the archive and extracted tree afterwards. Ordinary process/output limits and explicit platform scope remain part of its report.

## Acceptance and desktop result

The [canonical terminal receipt](../v0.24-release-acceptance/completed-gates.json) passed all **44 gates** with **1,099 distinct Rust tests**, **1,200 Rust test executions** and a separate **72-test Python gate**. All 390 implementation inputs remained unchanged. The [accepted native audit](../v0.24-native/confirmation-provenance-accepted.json) passed for two clean processes and 18 inspected captures, bound to that exact terminal receipt and executable.

The independent [desktop archive verification](desktop-archive-verification.json) **passed** on the normal Linux host. It freshly extracted the actual delivered ZIP into a private directory, verified 271 payload files plus the manifest, checked every regular-file mode, ran bounded `--help` successfully after passive verification, and confirmed that both the archive and extracted tree remained unchanged. The help process exited 0 and its ordinary owned process group was absent. This is command-line startup evidence; the separately recorded native sessions provide rendered interaction evidence.

| Desktop artifact | Actual result |
|---|---|
| Filename | `terra-world-0.24.0-linux-x86_64.zip` |
| Package root | `terra-world-0.24.0-linux-x86_64/` |
| Archive bytes / entries | 39,734,961 / 272 |
| Expanded bytes | 172,925,602 |
| Archive SHA-256 | `8ec346e51b065d8779bb62bb59535ef14f3ebcd216164c2ce1066ad170c2c81c` |
| Native executable SHA-256 | `509d831c5c6ff1c6619a6ed7fc9feb92ed24dbd737048200f5aca6473ed147e6` |
| Accepted terminal receipt SHA-256 | `8c386ac3ef5f1bead91f808db90b1301a6b6b834096a36c2cc654abd8f776d81` |
| Build / platform | Development profile / Linux x86_64 |

## Complete source and WP12 completion evidence

The source artifact is `terra-world-0.24.0.zip`, rooted at `terra-world/`. Its final byte identity and full readback result belong to the companion **`terra-world-0.24-source-verification.json`**, outside the archive. The packer emits that success report only after both complete ZIP readbacks, repeated source/baseline/evidence/executable checks, and explicit preservation-policy validation. Keeping this final result external avoids a circular archive self-hash.

WP01–10 are covered by the implementation, focused regressions, rotated review and native record; WP11 is covered by the successful canonical receipt. WP12 is evidenced by the actual desktop verification above and the external source-verification report. The five standalone Markdown reports are exact copies of the current product review, implementation plan, research, gap register and validation document. The sixth standalone report is the final source-verification JSON. Full logs, captures, failed attempts and independent reviews remain in the complete source artifact.

## Limits

The desktop is a Linux development-profile executable qualified here by native software-Vulkan interaction and bounded command-line startup. It is not a signed publisher release or a physical GPU, Windows, controller, clean-machine or deployment-capacity qualification. The complete G01–G18 register carries those separate criteria. No private full save payload is included.
