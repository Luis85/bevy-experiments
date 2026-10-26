# Terra World

Terra is a reusable Rust/Bevy world SDK and a native playable reference game. It combines georeferenced streamed terrain, source features, persistent player changes, actor control and a bounded living settlement. Games can replace the renderer, providers and presentation while keeping the domain rules.

## Current iteration: inhabit and command

The player can explore a larger landscape, select owned actors and issue RTS orders, then possess an actor in third or first person. Building, gathering, farming, trading and production use a shared player inventory and wallet. Individual bodies have position, needs and orders. NPC residents, road traffic and couriers make the local settlement visibly active.

The first session should make five things immediately understandable: which control mode is active, which actor is selected, what it needs, which actions are available here, and whether the terrain is sourced or synthetic.

## Authority and truth

Source terrain, map features and political geometry retain provenance. Authored population and economy are a scenario layer; their placement does not claim that real named people or businesses were observed. The bundled historical scenarios remain fictional. Imported heightmaps must state geographic bounds, elevation range and attribution. Multiplayer keeps server ownership of mutable state.

## Supported experience

Native desktop, primarily mouse and keyboard, 1440×900 reference with 1100×700 minimum. Linux/X11 is verified. This iteration is a playable vertical slice, not a scientific economy, traffic or vehicle dynamics model. All limitations and exact validation evidence belong in the release docs.

## Priorities

1. A continuous playable loop across possession, needs, commerce and production.
2. Legible terrain, landmarks, actors and movement across a wider visible region.
3. Predictable controls, contextual actions and recoverable errors.
4. Reusable simulation contracts and bounded per-frame work.
5. Source provenance, durable ownership and verified persistence.
