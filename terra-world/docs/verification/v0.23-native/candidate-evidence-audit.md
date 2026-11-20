# Independent candidate native evidence audit

Reviewer: reliability and delivery specialist, 30 October 2026. This review
reread the retained candidate evidence and inspected the seven rename, backup
and recovery screenshots numbered 55–61. It did not rerun the game, change the
implementation or alter existing capture/process records.

**Result: all 52 candidate images and sidecars passed the integrity checks;
gameplay process association has an explicit evidence limit.** This is candidate
evidence, not final-binary native acceptance or canonical release acceptance.
The complete machine-readable observations, per-image and per-sidecar hashes,
dimensions, timestamps and limitations are in
[candidate-evidence-audit.json](candidate-evidence-audit.json).

## Capture integrity

The audit found exactly captures **10–61**, without a missing or repeated index.
For every capture it independently checked:

- The sidecar and image are regular files, and neither changed during its read.
- The expected image name, SHA-256 and byte length match the actual PNG.
- The PNG verifies and its dimensions match the recorded window dimensions.
- Capture timestamps are ordered and fall within the recorded launcher lifetime.
- The before/after window PID agrees with the non-null recorded PID and the
  sidecar reports an available, unchanged observation.

There are **36 captures at 1440×900** and **16 at 1100×700**. These checks establish
consistency of the observed captures. A matching PNG hash does not independently
establish what input caused the displayed state or authenticate the capture's
publisher.

## Process and binary association

| Captures | Count | Window PID | What the retained evidence establishes |
| --- | ---: | ---: | --- |
| 10–28 and 52–61 | 29 | 11234 | Direct match to the recorded candidate launcher PID; all captures lie within its recorded lifetime. |
| 29–51 | 23 | 12147 | Stable observed gameplay-window PID before and after each capture. No separately retained process/binary/exit receipt or parent-log child-PID entry establishes the child's parentage, binary identity or exit status independently. |

The launcher start and terminal records agree in all shared fields. They name
the candidate executable SHA-256
`700120e44255afe4c482b3416bdcedaad4200d930012418aa0c36cda0281c7f0`,
start at **08:03:23 UTC** and finish at **08:25:19 UTC** with **exit code 0**.
The [parent process log](candidate-menu.log) independently hashes to its recorded
SHA-256
`3b5fd009a9e9b7aca28a58dc0e2e8f7bc2b3fc994927be30186ee5bc88864fff`.

The parent log does not contain PID 12147. The coordinator's earlier suggestion
that it recorded the child association was checked and corrected during this
review. No substitute parent/child link was reconstructed from application
behavior. **The parent's exit code is not recorded as the gameplay child's exit
code.** Likewise, the absence of a parent-log panic or ERROR entry is not a claim
that every child log was retained or inspected.

The process record reports **381 unchanged implementation inputs** over the
candidate run. It retains neither the full map nor its digest, so this audit
cannot independently reconstruct that exact candidate map or equate it with a
later corrected source tree. The recorded candidate binary hash remains a
start-time observation. A later executable rebuilt at the same pathname must
not be hashed and mislabeled as the candidate executable.

The log identifies Vulkan **llvmpipe**, a CPU/software renderer using
LLVM 20.1.2 and Mesa 25.2.8. Retained warnings concern software rendering,
XSETTINGS and XRandR physical-size metadata. This environment is suitable for
the bounded native inspection performed here; it does not establish physical
GPU performance or Windows/device compatibility.

## Visual review of rename, backup and recovery

All seven inspected screens are 1440×900. The following observations come from
the actual images, not inferred screenshots or fixture rendering.

| Image | Visible result |
| --- | --- |
| [55 — entered name](55-candidate-rename-entered.png) | The original world identity, new `My frontier Review23` name, explanation, Back and Save name controls are readable. The field and action row remain within the panel. |
| [56 — renamed world](56-candidate-renamed.png) | The success message names the renamed world, and the existing-world row retains its character/source identity and Make backup action. |
| [57 — backup created](57-candidate-backup.png) | Success feedback names the world and checkpoint 23 and directs the player to review the recovery copy. |
| [58 — backup list](58-candidate-backup-list.png) | The named checkpoint can be verified. External-folder verification, private-data guidance and the need for a separate protected copy are readable. |
| [59 — recovery review](59-candidate-backup-verified.png) | The review names the backup, checkpoint, recovered-copy title, character and source. It clearly describes creating a separate world and retaining the existing worlds and backup. Back and Recover as new world are unobstructed. |
| [60 — recovered copy](60-candidate-recovered.png) | Success feedback accompanies two distinct rows: the recovered copy and original world. Each has its own Rename and Make backup controls. |
| [61 — return to menu](61-candidate-menu-final.png) | Continue names the recovered copy. The longer status and name wrap within the panel; the remaining menu actions, Quit and keyboard guidance stay visible. |

No clipped text, overlapping controls or missing primary action was found in
these seven views. This visual result applies to the displayed names, content
and dimensions; it does not qualify these specific backup screens at 1100×700,
every possible long world name or assistive-technology interaction.

The images establish a visible rename → backup → review → separate-copy listing
flow. Exact equality of original, backup and recovered durable state belongs to
the separate world-state audit. The menu offering Continue is not evidence that
the recovered world has already been reopened and played.

## Required final association

The planned confirmation should launch the corrected executable directly with a
retained start/terminal process record, capture its same PID, retain a digest of
the complete frozen implementation map, and record its actual exit status.
Opening the recovered managed world can then establish the separate reopen
continuation. Final canonical acceptance and the independently extracted desktop
artifact must be associated with that final executable and source identity.

Those later records complement this candidate evidence. They must not rewrite
the candidate receipts or silently promote the unrecorded child association to
a verified one.
