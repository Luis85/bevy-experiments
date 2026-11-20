# Corrective pixel comparison — 0.24 native review

**The earlier disappearing-text findings are withdrawn.** Independent exact comparison of the original PNG regions found no changed RGB pixel in the disputed text. The screenshots do not establish the claimed renderer defect or a benefit from the experimental text refresh.

## Method and scope

Comparison completed at `2026-10-30T13:07:00.234156+00:00`. Read each original PNG and its separate capture receipt, checked the complete PNG SHA-256 and byte count against that receipt, and decoded it with Pillow. All six original images are RGB PNGs at 1100 × 700; no alpha conversion, resizing, filtering, OCR or image generation was involved. Crops use integer `(left, top, right, bottom)` coordinates, with right/bottom exclusive. Compared the complete row-major RGB byte strings and counted differing three-byte pixels. Reread every original PNG and receipt afterward; all retained bytes were unchanged.

Exact original crops of 209 and 220 were also inspected and contain their complete damage-review and Build-summary paragraphs. The equality results below are based on all decoded pixels in the specified regions, independently of the earlier full-frame interpretation.

## Exact regional results

| Original capture pair | Crop box | Crop dimensions | Pixels compared | Different pixels | All RGB bytes equal |
|---|---|---:|---:|---:|---|
| 208 / 209 | `(795, 210, 1073, 386)` | 278 × 176 | 48,928 | 0 | True |
| 219 / 220 | `(795, 213, 1073, 402)` | 278 × 189 | 52,542 | 0 | True |
| 220 / 310 | `(795, 213, 1073, 402)` | 278 × 189 | 52,542 | 0 | True |
| 310 / 311 | `(795, 213, 1073, 402)` | 278 × 189 | 52,542 | 0 | True |

Both 208/209 review crops have SHA-256 `193adaf1435e4a4a56e0af80da2164fc49bc57fb150dc8292232faf1c75735ae` over 146,784 RGB bytes.

All four 219/220/310/311 prose crops have SHA-256 `effd3954b39bec9c15f5730757302f7f167274c584458f9e3a91b037f97f50ef` over 157,626 RGB bytes. The full PNG files differ because these are different complete frames; that does not alter the exact equality of the specified text regions.

## Original PNG identities

| PNG | Bytes | SHA-256 |
|---|---:|---|
| `208-confirmation-damage-review.png` | 591,002 | `1f33bd1537a5d869ba55561513eb2bb553c3eb164b31ab0eeadd50b3ba176ed3` |
| `209-confirmation-cancel-focus.png` | 594,028 | `beb741e18c8038088a40cab53939675dfc867701e28ebffd36ac347bc9c1b844` |
| `219-confirmation-repeat-cancelled.png` | 578,307 | `67a1499bc4822851183712bc601a6f657b5898ab4f6afabff5bdbe9669f6c5fe` |
| `220-confirmation-repeat-focus-exited.png` | 575,952 | `43733ddf088495e445a04ef7f6f8fccea6c8a66b5f2f5329511538c1e5ff445e` |
| `310-confirmation-polish-focus-exited.png` | 575,627 | `f1bd37300bfba80a668abee78c356c339b746cc68929bd7f66f5c4085c12d0ad` |
| `311-confirmation-polish-idle-readback.png` | 575,682 | `3f04c49cc346f424c1268526ddedd63f170243d7c1fc5fc30a946f4d235132b2` |

Capture-receipt hashes, PIDs, timestamps and each full comparison record are retained in [pixel-comparison.json](pixel-comparison.json). Its SHA-256 is `8c041e0607b2360b816031b746d9f3a2159acd28244dca8b1f4321226224fbdd`.

## Review correction and acceptance boundary

The earlier assertions that 209 lost the named damage explanation, that 220 lost Build-body text, and that these frames proved a transition-sensitive rendering problem are incorrect. They are explicitly withdrawn at the top of [final-visual-review.md](final-visual-review.md); the original notes and capture files remain unchanged for auditability. No inference about the cause of the mistaken full-frame assessment is needed to establish this correction.

The experimental refresh cannot be justified as fixing a demonstrated visual defect from these images. The integrator directed its removal and a rebuild of the prior implementation while retaining the shorter labels. This comparison does not certify that revert, rebuild or final release; those require their actual records. No UI/process/Cargo action or helper/source edit was performed for this pixel comparison.

Equality in these regions is a precise local result. It does not establish correctness of every screen, keyboard/controller path, 200% text/reflow, assistive technology, platform or physical device.
