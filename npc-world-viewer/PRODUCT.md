# NPC World Viewer

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

TypeScript and ExcaliburJS, as requested. Local Node server and Vite for development.

## Product Purpose

Project NPC Framework world-state JSON into an interactive 2D browser world. The first increment showcases the island-village demo.

## Operating Context

An independent sibling project beside the four Rust workspaces. NPC Framework periodically exports a configurable file; the viewer follows it and replaces its projection.

## Capabilities and Constraints

Read-only simulation observer. Show terrain, workplaces/housing, villagers and the played character. Never simulate NPC behavior or send gameplay commands. Preserve integer positions and stable IDs in the transport. Export no private agent files or credentials.

## Brand Commitments

The user selected pixel-art style. Native crisp pixels and distinct village landmarks guide the map.

## Open Decisions

Assumption: developers/operators are the initial audience. Pan, zoom, entity inspection and connection status support the requested interactive demonstration. Hosting and remote access are outside this increment.
