# Independent cross-review — launcher layout and source-bound meshes

Reviewed 29 October 2026. Scope: current implementation and tests in `crates/terra-demo/src/launcher/ui.rs`, its shared `flow::scroll_panel`, and `crates/terra-bevy/src/render.rs`. No peer source was edited and no Cargo command was run by this reviewer.

## Launcher: blocker found and corrected by the owner

The first reviewed version added `TextLayout` to the reusable `text()` bundle while `button()` still nested `text(...)` alongside another explicit `TextLayout`. Bevy bundles cannot contain the same component twice; every launcher button construction could consequently panic. This was reported immediately to the integrator and player-experience owner. The owner removed the redundant button component and the redundant recovery-path insertion; a fresh source scan confirms the helper is now the only wrapping declaration for its labels. The separate `EditableText` field's `TextLayout::no_wrap()` is in its own standalone bundle and does not duplicate the helper.

## Launcher: resulting structure and acceptance limits

All general notices, progress and errors are now children of the existing `ScrollArea`; warnings keep their pre-existing screen policy. The wrapper and scroll nodes both have a viewport-height ceiling and zero minimum height, so the content can shrink inside the panel. Footer rows remain outside the scroll content, with `flex_shrink: 0`; action closures still retain their exact `Action`, and the wrapping change does not replace IDs or alter review revisions. The existing post-layout focus observer continues to issue `ScrollIntoView` for focused descendants.

The twelve-screen regression rebuilds actual menu trees, checks each expected message appears once under a bounded scroll ancestor, checks WordOrCharacter wrapping for unbroken paths, and triggers real button activation to compare exact queued actions. It should also catch the corrected duplicate-component bundle panic when executed. The fixture has no window/layout/font/accessibility plugin and does not run at 150% scale. `TabIndex` presence is a focusability contract, not proof of actual Tab traversal or native pixel reachability. A 1100×700/150% rendered session, keyboard/controller tab journeys and device qualification remain open. The implementation note correctly states this rendered acceptance boundary; avoid summarizing this structural test as measured footer reachability.

## Mesh completion: no new blocker found

The retained `Arc<WorldChunk>` binds a job to its actual immutable source allocation. `Arc::ptr_eq` prevents an old resident page's successfully completed mesh from integrating over a replacement, even if both have the same key, revision, LOD or value-equivalent contents. Retaining that Arc also prevents address reuse while the job is tracked. Rejection retains existing residency/revision/desired-LOD admission; a current resident page is requeued through the existing bounded two-job nearest-first path, while an evicted tile is discarded. A disconnected stale-source completion also reaches the source mismatch requeue path. Authority, prepared-source bytes and physics queries are unaffected.

The four injected-completion tests exercise the actual `render_chunks` system, not just a duplicate comparison helper. They cover current acceptance, changed-source rejection, current-source requeue followed by accepted current-height vertices, value-equivalent replacement rejection, stale revision/LOD and actual eviction. Replacing the asynchronously spawned retry with a ready injected job makes outcome independent of worker timing; the dropped receiver causes the finite detached original retry's send to fail harmlessly. No execution result is claimed here.

The known disconnected-current-source and poisoned receiver cases remain WP02: matching existing geometry can lose dirty work, or a poisoned receiver can keep a slot. These are explicitly documented pre-existing reliability gaps, not established closures. WP03 broad invalidation and actual GPU tails also remain open.

## Verdict

After the owner-corrected duplicate bundle, no additional source-level blocker was identified in the scoped launcher or mesh admission changes. Central compilation/test execution is still required, and native/hardware acceptance cannot be inferred from these source inspections.
