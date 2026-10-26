# Terra World native interface

## Direction

Preserve the forest-green identity while making the rendered world the main surface. This is a desktop game with operational controls. The interface recedes during first- and third-person control, and expands for construction, trading or world inspection. No marketing copy, decorative metric cards or ornamental animations.

## Tokens

- Surface: deep forest `#101c1a`, high-opacity for legibility over changing terrain.
- Secondary surface: `#1b3029`; hover `#2a483b`; selected `#385b43`.
- Primary text: warm ivory `#eef1e4`.
- Secondary text: pale sage `#b2c4b5`.
- Action accent: meadow `#b3dc8e`; trade/warning accent: warm sand `#e6c38b`.
- Error: muted coral `#f09c89`, always accompanied by a written reason.
- Font: bundled DejaVu Sans. Main title 25px, section title18px, body13px, secondary12px; no text smaller than11px.
- Spacing: 4/8/12/16/24px. Buttons at least32px tall with8px horizontal padding. Tight related controls, larger section separation.
- Corners: 5px controls,8px panels. Use surface separation without stacked borders and shadows.

## Surface composition

The64px top bar exposes RTS, third-person and first-person modes, world drawer, atlas and inspector toggles. The256px world drawer is hidden initially. The300px inspector shows Command, Build, Market, Industry or Factions; only the chosen panel is present. A108px footer carries actionable status, key guidance and bounded diagnostics. Source attribution is available in the world drawer and the source label remains visible.

Default camera is an oblique settlement overview. Selection uses outlined ground rings, a clear current mode label, and a drag rectangle. First-person hides the possessed body's exterior mesh. Third-person follows with terrain clearance. Switching control does not replace player identity, inventory or ownership.

## Workflow rules

- RTS: left select, Shift add, drag select, right-click order; selected owned units are listed.
- F1 returns to RTS; F2/F3 enter third/first person on an owned selection; Escape releases/cancels; Tab focuses selection.
- While possessed: movement is view-relative; sprint uses energy; E opens contextual interaction; C switches camera.
- Build placement has pointer priority over unit selection and reports reach, surface and ownership failures.
- Trade exposes stock, buy/sell prices, distance and the player's wallet. Failed transactions explain the unmet condition.
- Needs are named food, water and energy, with numeric values; color is supplementary.
- Industry exposes recipe flow, finite stocks, jobs and delivery progress. Source versus authored placements are explicit.
- F5/F9 remain explicit local save/load. Network authority is named and server-owned controls are disabled.

## Verification

Inspect one batch covering RTS overview, possession, commerce, industry, atlas and the minimum window; fix material findings together and confirm once. Functional command, authority and persistence assertions use automated tests and saved state in addition to screenshots.
