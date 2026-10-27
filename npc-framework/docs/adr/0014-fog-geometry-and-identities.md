# ADR 0014: Fog geometry and object identity coverage

- Status: Accepted (actor presets and robot sensors plan, Task 13)
- Date: 2026-10-27
- Source: `docs/superpowers/specs/2026-10-25-agent-notes-and-workspace-design.md`, G8–G10; ADR 0013

## Context

LIDAR reports distances without identities. Treating every cell reached by a beam as ordinary vision would
show live items and door states that the robot's camera, radar and reach have not sensed. The existing view
uses one visible-cell mask for terrain and named static objects, while units already require sensed percepts.

## Decision

- `VisibleCells.bits` covers geometry: vision/camera cones with LOS, reach, LIDAR beam cells through their
  first hit, and radar target cells. LIDAR uses the same endpoint rounding, obstacle mask and conservative
  supercover as its distance scanner; noise never extends fog through a wall.
- `VisibleCells.identified: Option<Vec<u64>>` records coverage that can reveal live named static objects.
  Robot fog fills it before adding LIDAR geometry. `None` means the ordinary visible bits apply, preserving
  existing vision/reach actors and old snapshots. Views use `is_visible` for terrain and `identifies` for
  named statics. Previously identified objects outside identity coverage retain their last-seen state.
- Units and vehicles still require current sensed percepts (R14); LIDAR never adds such percepts.
- These masks remain snapshot-only. They change no authoritative state or content hash. Runtime caches
  include exact robot position, facing, sensor parameters, grid revision, obstacle cells and radar target cells.

## Consequences

The existing snapshot component gains a backward-compatible optional field. Robot save/resume tests compare
both coverage masks and last-seen statics, as well as sensor readings and authoritative hashes. A shared bitmap
beam walker supplies range hits and fog geometry, avoiding repeated grid lookups while preserving complete scans.
