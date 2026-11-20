# Recovery naming increment — 0.16

Status: implementation complete; automated and native confirmation pending.

## Evidence and change

A fresh baseline native capture (`/tmp/terra-native/v16-baseline-recovery-worlds.png`) shows two `Recovery valley (recovered)` entries with identical character/source/open labels. Accepted 0.15 binary SHA256 `c12970d499bd2f9045cad0f9c106cdc870dddd64ef7804fd496d8b37c96cc504`. This reproduces UX-01/G16 without relying only on historical captures.

The recovery review now offers a bounded editable recovered-world name. Your worlds offers Rename for both checkpointed and unplayed managed profiles, plus creation time to seconds in UTC. Save name rechecks the entire selected profile and its direct managed path, rejects aliases/stale context, atomically changes only the title, then pins and focuses the renamed entry within the 24-entry list. No world/character/economy/save schema changes. Cancellation does not write. Duplicate descriptive names remain allowed: world identity is its separate folder.

A discovered pinning assumption has also been corrected: the completed-entry refresh derives actual checkpoint-file presence rather than forcing true, preserving the Finish first launch state for renamed unplayed worlds.

## Planned acceptance

- Two restored copies with identical creation timestamps retain distinct chosen titles after reconstructing the list.
- Rename changes only profile bytes/title; all progress/source/backup bytes and profile settings remain exact.
- Invalid titles, changed profiles, unmanaged targets, symbolic/hard-linked files, cancelled and stale queued actions perform no mutation.
- A renamed unplayed world stays unplayed and receives no launch request.
- Native minimum-window enlarged-text review covers editing, cancellation, save, navigation and reentry.

## Boundary

This closes copy identification/renaming in the supported local menu workflow. It does not add retention, offsite copies, encryption, automatic migrations, remote operator management or recovery for unsupported sources. Filesystem ancestors must remain quiescent during a metadata operation, as for existing recovery.
