# Final delivery verification — 0.21.0

The canonical [current validation](../../VALIDATION.md) records 44 passing gates,
1,000 distinct Rust tests, 1,097 passing Rust executions and 34 Python tests.
The accepted executable is the same binary used in the final native confirmation.

## Linux desktop archive

`terra-world-0.21.0-linux-x86_64.zip` contains 255 files and is
39,537,078 bytes. SHA-256:

`26653e97a0885357f69f5ff1170fdead6392bc79035d2d53f1736accf5ad82f5`

The exact-receipt packager admitted every frozen input, gate log, report, test
identity and native binary, then read back the ZIP before publication.
[desktop-package.json](desktop-package.json) records its outcome.

An independent extraction checked all 254 manifest file hashes,
restored executable mode, matched the accepted native binary and ran `--help`
with exit zero. [desktop-smoke.json](desktop-smoke.json) and
[desktop-help.log](desktop-help.log) retain that bounded result. The extracted
folder was temporary and removed after the check.

This is an optimized dev-profile Linux x86_64 review package with system runtime
dependencies. The smoke check establishes command-line loading on this machine;
Windows, clean-machine graphics/input behavior and publisher signing remain
separate qualification.

## Complete source archive

[package_source_021.py](package_source_021.py) builds the full source handoff
only against the completed accepted receipt. It binds all evidence hashes and
exact test counters, compares every original ZIP entry, preserves original Unix
modes, and verifies every final ZIP byte hash and mode after writing.
The [cross-review](source-archive-cross-review.md) records corrected receipt,
mode-preservation and external-destination checks.

The finished source archive embeds `terra-world/DELIVERY_MANIFEST.json`, including
the complete file list, original preservation/change accounting, accepted receipt
identity and per-file SHA-256/modes. Its own hash is recorded in the external
`terra-world-0.21.0-source-verification.json`, avoiding a self-referential archive
hash. Both source and desktop archives exclude current test worlds, identities,
build caches and local Git metadata.

Integrity checks establish exact bytes and preservation, not publisher identity
or filesystem durability. Original historical evidence remains historical;
current execution lives in the separately named 0.21 acceptance/native folders.
