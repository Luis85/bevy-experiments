# Terra World 0.23 — delivery evidence

## Release inputs

This handoff starts from the exact delivered 0.22.0 source ZIP, SHA-256
`54ba5420db937056ba62e1f7861308a4ea540112f52ae500af10e92e897a739e`,
containing 2,790 regular file members. The current release is 0.23.0.
The complete prior source, assets, datasets, authoring material, license notices
and historical evidence are preserved under the explicit
[preservation policy](../v0.23-review/preservation.md).

The source archive includes current implementation, tests, documentation,
research, dispatched plan/closeout, all 18 product gaps and actual native/automated
review records. Build caches, private test worlds and runtime/toolchain downloads
are excluded. Unix modes are retained. The exact previous current review,
validation and root manifest have named retained copies.

The source packer is [package_source_023.py](package_source_023.py). It refuses
missing, failed, partial, stale or mismatched terminal acceptance; checks the
canonical 44-gate roster and exact source/log/report/executable identities;
rechecks the complete baseline against named exceptions; and independently
rereads every final archive member. Two specialists reviewed its adaptation.
The actual preacceptance refusal created neither a ZIP nor a success report.

## Desktop contract

The desktop is **Linux x86_64**, built with the accepted native **dev profile at
opt-level 1**. It carries the exact accepted executable, the declared 180-entry
payload, source-free verification tools, launch helpers, current guides and
accepted evidence. It is not a separately optimized release-profile build or a
Windows executable. A graphical session and compatible system runtime/graphics
remain required as described in the [desktop guide](../../DESKTOP_DELIVERY.md).

The unchanged desktop packer validates the actual terminal receipt and source
before publishing a new archive without overwriting an existing path. Independent
verification uses a new extraction, checks ZIP/portable namespace/type/mode/byte
identity, runs the shipped verifier passively, then explicitly requests its
finite `--help` startup smoke. The extracted payload is checked again afterward.
The tested runtime is this Linux environment; clean-machine physical-device
qualification remains separate.

Passive verification executes no binary. Startup help does not establish rendered
play, while the [direct final native confirmation](../v0.23-native/README.md)
supplies the separately bound game-window and recovered-world evidence. Hash
consistency does not authenticate a release publisher.

## Final records

**Desktop packaging and independent fresh-extraction verification passed.**
The [package receipt](desktop-package.json),
[independent report](desktop-verification.md) and
[complete verification JSON](desktop-verification.json) bind the actual artifact.

| Property | Verified value |
|---|---|
| Archive | `terra-world-0.23.0-linux-x86_64.zip` |
| Compressed bytes | 39,669,704 |
| Regular archive/extracted entries | 266 |
| Declared runtime/guide payload entries | 180 |
| Exact packaged acceptance-evidence files | 82 |
| Unix modes | 264 files at 0644; executable and launcher at 0755 |
| Canonical acceptance | 44 gates; 1,070 distinct Rust tests; 1,171 executions; 50 Python tests |
| Passive verifier | Exit 0, passed; no binary executed |
| Explicit bounded help | Exit 0, passed; 3,227 output bytes; owned process group absent |
| Post-verification state | All extracted bytes, modes and inventory, and original ZIP unchanged |

Desktop archive SHA-256:
`e13a2b09dee1180ed65f2bfddccdb25cf9c144dba82250f26e73e74da3f8122f`.

Exact native executable SHA-256:
`432748a47f507d23e3efdbd2862f4e5beaae68c41801eeaf4a057392d4f3c99f`.

The actual [passive result](desktop-passive.json) and
[help result](desktop-smoke.json) come from the shipped verifier in a new private
extraction. Both stderr logs are empty. The current host resolved all linked
libraries; no wrapper or `LD_LIBRARY_PATH` override was used. This proves startup
on the observed Linux host, while physical graphics/input and clean-machine
qualification retain their documented scope.

The terminal receipt, current sources, all 180 declared payload bytes and all 82
packaged evidence bytes match their actual acceptance records. The ten final
native images have a separate [accepted provenance record](../v0.23-native/confirmation-provenance-accepted.json)
for the same executable. The [current Validation](../../VALIDATION.md) provides
complete automated and rendered-workflow results.

The complete source ZIP's final byte identity, member count and every-baseline
preservation outcome are retained in the separately delivered
`terra-world-0.23-source-verification.json`. Keeping that result outside its own
archive avoids a circular self-hash claim. The internal `DELIVERY_MANIFEST.json`
binds every included file, original-mode preservation and accepted inputs;
the source packer rereads the whole archive twice and rechecks current sources,
all baseline members, the actual terminal evidence and native executable.

## Download set

1. `terra-world-0.23.0.zip` — complete source and retained evidence.
2. `terra-world-0.23.0-linux-x86_64.zip` — accepted Linux desktop.
3. `terra-world-0.23-product-review.md` — comprehensive findings and decisions.
4. `terra-world-0.23-implementation-plan.md` — original plan plus execution ledger.
5. `terra-world-0.23-research.md` — primary sources and applied patterns.
6. `terra-world-0.23-gap-register.md` — every G01–G18 requirement and next proof.
7. `terra-world-0.23-validation.md` — actual acceptance and native/persistence scope.
8. `terra-world-0.23-source-verification.json` — full archive/baseline validation.
