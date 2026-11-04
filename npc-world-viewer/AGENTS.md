# NPC World Viewer

Independent npm project; run Node commands here and Cargo commands in `../npc-framework`.
Read `README.md`, `docs/world-state.md`, `PRODUCT.md` and `DESIGN.md` before editing.

- Read-only visualizer: no authoritative gameplay, prediction, autonomous decisions or gameplay proxy.
- Keep integer millimetres and SimIds at the boundary. Validate full replacements; retain the last valid
  view on failure; allow lower ticks on reset. Keep renderer state separate.
- Read only the operator-configured file. Never expose arbitrary paths, private saves or credentials.
  Bind to loopback by default.
- Pixel-art graphics are original code-generated rasters. Preserve nearest-neighbor filtering.
- `npm ci`, `npm test`, `npm run build`; browser-test visible changes with live Rust output. Exporter
  edits must follow `../npc-framework/AGENTS.md`, including the full Rust gate.
- Ignore node_modules, dist, runtime projections, screenshots and logs.
