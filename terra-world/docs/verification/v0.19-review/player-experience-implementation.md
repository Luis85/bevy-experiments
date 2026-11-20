# Player experience implementation — 0.19

Implemented UX19-01 from the specialist source review.

- All launcher errors now live inside the existing bounded scroll content,
  including Home, Join, World setup, Settings and Controls. Long details cannot
  become a fixed sibling that consumes footer space.
- Notices/progress remain in the same scroll area. Warnings retain the incumbent
  screen visibility policy. No message is truncated, duplicated or hidden by
  this change.
- Launcher text uses WordOrCharacter wrapping, including report paths and status
  text without spaces. Existing palette, controls, action identities, focus
  ordering, quote/recovery review and world authority are retained.
- Added a headless structural/action regression across all twelve launcher
  screens. It uses 1024-character unbroken status/path components, proves each
  relevant message appears once under the bounded ScrollArea, checks path wrap,
  and proves footer controls remain outside that scroll, retain Tab focus and
  activate the exact expected action. This establishes tree/action contracts;
  it does not claim a native rendered-layout or physical-device test.

Focused gate to execute centrally:

```text
cargo test --locked --offline -p terra-demo status_layout_tests -- --nocapture
```

Formatting completed with the pinned rustfmt. Root owns fresh compilation and
execution receipts. Actual 1100×700/150% rendered error layouts, native focus
reachability, Windows and assistive/device qualification remain acceptance work
unless separately executed and recorded by the integrator.

UX19-02 (review before deliberate F9/Load discard) remains backlog: complete
closure needs both world-panel and paused entry points plus stale-checkpoint and
restore-context validation. It is deliberately excluded from this isolated
launcher correction to retain the current documented restore contract.

Cross-review correction: delivery found that the pre-existing button-specific
TextLayout would duplicate the shared text helper's new layout component.
Removed that redundant nested component before acceptance and audited all
launcher text/layout call sites. Native editable fields retain their separate
no-wrap widget bundle. The regression constructs real button bundles across all
twelve screens and exercises activation rather than only checking helper output.
