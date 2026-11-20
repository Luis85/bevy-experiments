# Independent recovery review — 0.16

Reviewer: guidance increment subagent. Scope: current launcher naming, recovery
handoff, metadata persistence, ownership of selected files, bounded work and
documented user expectations. This is a source review; fresh rendered interaction
is recorded separately by the parent native acceptance pass.

## Findings and correction

The restore confirmation accepted an edited destination name, but its in-progress
message named only the original backup. That could make a player unsure whether
the name they had just entered was applied. The corrected message begins with the
chosen, trimmed recovered-world name, then identifies the original backup and
checkpoint. This changes presentation only.

The existing queued-selection/cancelled-confirmation regression now also exercises
a successful native `RestoreBackup` action with an edited name. It checks the
in-progress message, joins the real background restore result, reads the resulting
profile title and verifies that no world-launch request was produced. The parent
will run this with the integrated terra-demo suite.

## Reviewed contracts

- Rename rechecks the complete selected profile, direct managed-world location,
  regular private file and path aliases before replacing only profile metadata.
- The name edit retains character, source, creation/last-played dates, checkpoint
  state, gameplay data and backups. Renaming an unplayed profile does not invent
  a checkpoint.
- Screen/review/list revisions reject cancelled and stale queued selections.
  The active recovery worker excludes conflicting rename/launch/write actions.
- Recovery validates the chosen title before allocating a new output and uses a
  separate unique folder. A title is descriptive; it is not the world identity.
- The recovered/renamed result uses the existing bounded recent-list pin and
  focus, preserving its reachable Open action.
- Duplicate names and equal creation timestamps remain allowed and documented;
  explicit naming and renaming provide the user's means of distinction.

No blocking source defect was found in these reviewed contracts. Native minimum
window text entry, keyboard focus, cancellation, scrolling and reentry remain
interaction acceptance work. The existing quiescent-filesystem requirement and
unsupported-source/offsite boundaries remain explicit.
