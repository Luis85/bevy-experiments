# Native review — Terra World 0.22

Executed on Linux x86_64 in an isolated X11 display. The renderer reported
Vulkan **llvmpipe / LLVM 20.1.2**, a software CPU adapter. The display was
1440×900; the game and launcher were also operated at the declared minimum
**1100×700 with 150% interface text**. These observations are not physical GPU,
Windows, controller, screen-reader or uncoached-usability qualification.

## Method and provenance

The integrator operated the actual native executable with keyboard and mouse
and opened the captured images for visual inspection. `run_022.py` records
process command, PID, binary hash, source-input count, exit and log identity;
non-candidate phases also retain canonical source-input digests. `ui_022.py`
records each window PID before/after capture, its title and dimensions, and the
image's size/hash. The capture record's `release` identifies this review set;
the process phase and executable hash identify the binary, including baseline.

The managed launcher supervises a child game process. The candidate parent is
PID **207201**; the observed game window is PID **210688**. The separately
recorded `candidate-managed-child.json` verifies its parent, exact executable,
command, 150% preference handoff and source identity while running. Direct final
confirmation avoids that parent/child distinction. Access to the owned child
process identity required the normal reviewed execution permission path, as did
the isolated display. No browser, external service or user world was involved.

The text preference was prepared in a new isolated test-data directory. The
world itself was created through the native Home → Character → World → Review
→ Create path. Its reviewed settings were a synthetic frontier, Near/radius-one
coverage, noon at 1× time, normal vegetation and Performance graphics. The
Performance preset disables atmospheric rendering even when the stored sky
preference is on. All observed worlds and communities belong to this test.

## Fresh 0.21 baseline

The accepted 0.21 executable was run again for this review, with binary SHA-256
`209b5f2ee0d74d8b4776346297af6838dd9e05cf622af80645f63dedb0bc13d1`.
Captures `01`–`05` show RTS Activities, crew selection, pause and settings.
Activities omitted actor needs; the crew view showed several selection rings
while the footer still named one explorer. Save and quit completed.

The baseline started with the exact accepted 0.21 source-input identity
`345c5393749e43beba08a6e6cf12aa1127d1e21b341f8fe5399b564b2ca4b19f`.
Source editing began before that process ended, so its receipt correctly records
`source_inputs_unchanged=false`. It is baseline observation, not 0.22 acceptance.

## Candidate journey and observed outcomes

The candidate used binary SHA-256
`c85effd3dca3a374798e30f1b2e77dbab6515a748cf61e8fe6c8edb8d4904dda`.
The managed child observation recorded **378** inputs with canonical identity
`47fde098dafaa042fd8fd3a240714187fb7622620a4d4ab2a2b017f8736ef455`.
The supervised parent exited zero and retained unchanged source inputs through
its entire 21:01:27–21:20:32 UTC run. Final polished confirmation is separate.

| Evidence | Actual observation |
|---|---|
| `10`–`14`, `22`–`31` | Character/world screens retained fixed navigation at minimum size; scroll and keyboard focus reached settings. The final reviewed settings and regional save/F9 explanation were readable before creation. |
| `32`–`34` | Actual managed creation admitted regional progress. The first game capture explicitly still showed Loading with pending terrain; the later settled view rendered the world and the new Alex/ownership/needs summary during Activities. |
| `35`, `39` | Five owned selections were reported as five, with lowest needs across the three biological bodies. Command labels/bars and the compact footer agreed. |
| `36`, `38` | Clearing selection produced explicit no-selection guidance; no unrelated active actor's needs were borrowed. |
| `40`–`41` | Third- and first-person possession kept Alex's controlled identity and needs while side panels were hidden. |
| `43`–`47` | Command actions remained reachable by scrolling. The selected utility rover had no biological needs; actual entry showed Driver: Alex and needs matching the driving card. Release removed that association. |
| `48` | Resident 20 was Inspect only, with its own 84% food, 84% water and 79% energy. This also exposed the generic selection-toast defect below. |
| `50`–`55` | Created Review Guild #1 through native text input. The player remained Leader/sole member with 250.00 credits; the unused-faction review action was available. |
| `56`–`58` | The complete named consequences fit above Cancel at enlarged text size. F6 unexpectedly focused Confirm. The integrator deliberately clicked Cancel instead; the exact community remained and the UI reported no command sent. |
| `61`–`63` | Reopened the same named review, deliberately chose Confirm and observed the faction disappear. The wallet remained 250.00 credits. |
| `65`–`66` | Pause showed the managed world's checkpoint. Shift+Tab scrolled Save and quit into view; the explicit action completed the session. |
| `candidate-managed-save-evidence.json` | Closed-world read-only inspection returned SQLite integrity `ok`, committed generation **40**, 12 stored regions, `checkpoint_present=true`, no factions, next domain ID **2**, 25,000 minor currency units and the original player inventory. Profile, witness, lease and database were mode 0600. |

Screenshot filenames describe intended review actions, not assertions. Early
setting attempts `15`–`20` occurred across native scroll repositioning; they do
not prove their named intended setting was selected. The later inspected
`22`, `24`, `27`, `29` and `30` establish the actual final configuration.

The candidate's incomplete terrain capture was taken while Loading explicitly
reported pending work. Later frames rendered terrain and actors. No first-frame
timing, shader-warmup cause or physical-GPU performance result is inferred.

## One correction batch

Two material findings and one small copy issue were corrected after the whole
candidate journey, without changing command authority or saved schemas:

1. The old selection toast advertised F2 possession for an inspect-only resident.
   It now counts selected ownership for the current principal and advertises
   possession only when the actual primary actor is owned. Mixed/non-owned
   selection receives an accurate next step.
2. Management navigation explicitly preferred Confirm when entering a fresh
   review. It now enters on Cancel, waits if Cancel is not ready, consumes the
   transition's acceptance and never falls back to a committing action. Actual
   F6 capture/navigation/activation regression covers this, alongside deliberate
   later confirmation, expiry and return-to-origin coverage.
3. Community summary nouns now use “1 member” and “1 invitation.”

The final preflight demo suite passed **227 tests**, zero failed or ignored.
The initial 226-test candidate result preceded this batch. No redundant
copy-only test was added; rendered confirmation checks the actual wording.

## Final rebuilt confirmation

The direct confirmation reopened the same managed profile with binary SHA-256
`34dc6fe5c906d556fcf182968df0a92742079008c5eb26cc94d652a69a041cd6`.
PID **212865** ran from **21:27:12 to 21:33:39 UTC** and exited **0**. Its **378**
implementation inputs retained canonical identity
`ec3fb5123a82cd1d4e6f5351d8f04503d305359628a44f61d8e032d9eca5902d`
through execution. After this native check, canonical acceptance exposed two
old protocol-10 literals in a wire-format test. They were corrected to 11 inside
`#[cfg(test)]`; the exact native-confirmed file is retained as
`native-confirmed-protocol.rs`. Independent comparison in
[test-only source review](../v0.22-review/test-only-source-review.md) confirms
that only those two bytes changed, all production-prefix bytes are identical,
and this is the sole changed input. The completed canonical release run rebuilt
and verified the **exact same executable hash**. The executed
[association check](confirmation-input-association.json) binds its accepted
receipt, both source-map identities, the exact two-byte test correction and the
identical native/accepted/current executable. Its accepted source-map identity is
`261b8840abe316cb873cb9561e52115b453fe4826a430a609b20166351cf4510`.
No runtime correction followed confirmation.

| Final capture / record | Observed result |
|---|---|
| `70`–`71` | Same managed world reopened at reference and minimum sizes with the retained paused clock, utility-rover selection and 250.00-credit wallet. |
| `72` | Selecting Resident 06 shows “Inspect only” and “Select an owned actor for control or orders.” The corrected toast and compact summary agree and fit. |
| `74` | The earlier Review Guild remained absent after reopen. |
| `77` | Native creation produced Confirm Guild **#2**, with correct “1 member” wording. The removed #1 was not reused. |
| `79`–`80` | Full named consequences remain readable. F6 now highlights **Cancel community change** by default, without scrolling the review's name away. |
| `81` | Enter on the default Cancel retains Confirm Guild #2 and reports that no command was sent. |
| `84`–`86` | A separately opened review again enters on Cancel. Only a subsequent deliberate Tab moves focus to Confirm and scrolls it into view. |
| `87` | Enter on that deliberately selected Confirm removes the named community; the wallet remains 250.00 credits. |
| `89`–`90` | Pause and keyboard focus expose Save and quit at minimum size. The explicit action completes the direct process successfully. |
| `confirmation-managed-save-evidence.json` | Read-only closed-world inspection returns SQLite integrity `ok`, generation **60**, no factions and next domain ID **3**. Wallet, inventory, witness and lease-marker hashes/values remain unchanged from the candidate checkpoint; progress generation increases. Private files retain mode 0600. |

The final process and image identities are retained in
`confirmation-managed-reopen-process.json` and the matching capture records.
The saved-state comparison is observed evidence from this isolated journey;
the broader conservation and stale-review cases remain separately automated.

After all owned native processes exited, the isolated X11 display was stopped
at **21:38:46 UTC**. The [cleanup record](display-cleanup.json) retains the
display identity and the already-exited process IDs.

## Limits

This is a bounded developer-operated native workflow and visible layout check,
supported by independent specialist image review and separately named automated
fault regressions. It does not measure novice comprehension, sustained careers,
all localized names, all displays, physical input devices, assistive access,
Windows, WAN behavior, disk latency tails or power-loss survival. The source
handoff retains all images, process/capture records, candidate faults and final
evidence; the current Validation report names the accepted release identity.
