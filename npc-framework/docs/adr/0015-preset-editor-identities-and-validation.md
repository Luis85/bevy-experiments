# ADR 0015: Preset editor identity, persistence and validation

- Status: Accepted (actor presets plan, Task 14)
- Date: 2026-10-27

## Context

Preset IDs are filenames, while display names are editable. The shipped `human.json` is named "Human scout";
always deriving its path from the name would silently change the default ID just to edit a stat. Editors also
need to handle incomplete or extreme values without passing unsafe arithmetic to the authoritative rules.

## Decision

- Ordinary edits whose display name is unchanged retain the selected file path/ID. New, duplicate and renamed
  drafts use `slug(name).json`, preserving the original. The UI displays the actual destination as the name changes.
- The draft and its loaded-byte version come from one read. Save rechecks the bytes; a collision or external
  change requires an explicit confirmation of that version. New-file publication cannot overwrite a filename
  that appeared meanwhile. All complete writes use a same-folder temporary file and atomic publication.
- Revert restores the disk version of a saved preset or the pristine baseline of an unsaved new/duplicate draft.
  New/selection/close operations protect dirty drafts with explicit discard confirmation.
- Shared content validation checks numeric ranges/names before calling core point costs, checks intermediate
  integer bounds, and adds banked points in `i64`. `sheet_point_total` is host-side validation, not new sim state.
- The editor uses a frozen, unticked validation app for registries. Its preview has its own cameras and visual
  entities, reusing the actual actor models; it never adds a simulation driver or writes sim components.

## Consequences

Existing agent references and saved preset copies remain stable. Editor errors cannot overflow the rules core;
valid presets retain the same point values, state hashes and content hashes. Editing and previewing do not run
the world. The same validated files are consumed by both windowed and detached explorer hosts.
