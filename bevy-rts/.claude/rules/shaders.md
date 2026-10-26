---
paths:
  - "**/*.wesl"
---

# Shader rules (spec §3)

- Bevy 0.20 custom shaders are **WESL only** (`.wesl`). GLSL was removed, and WGSL with naga_oil became WESL:
  `#import` is `import …;` and `#ifdef` is `@if`. Never add `.wgsl` or `.glsl` files.
- Resolve shader imports, bind groups and material APIs with the `bevy-api` skill (the local `bevy_shader`,
  `bevy_pbr` and `bevy_material` 0.20 sources and the migration guides), never from memory.
- Shaders are presentation only (fog of war, terrain, selection): nothing a shader computes feeds back into
  the sim.
