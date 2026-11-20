# Independent native provenance audit review — 0.24

The player specialist reviewed `v0.24-native/audit_native_024.py` independently
before any actual final registry or accepted result was created. Its use of the
complete current `package_source_024.accepted_state` validator, exact source and
binary binding, two window PID observations, direct clean process records and
confirmation-only registry matched the intended acceptance contract.

Three additional malformed-record cases were reproduced with temporary fixtures:

| Finding | Correction |
|---|---|
| A Boolean registry schema passed Python's equality comparison with integer 1. | Registry and process-start schema fields now require the actual integer type. Process-terminal and capture schema fields already did so. |
| A 50-second wall-clock lifetime could claim a 0.000001-second elapsed duration. | Recorded monotonic duration must agree with the UTC start/finish difference within two seconds. |
| A future terminal timestamp passed the native portion of pending mode. | Native and canonical completion timestamps must be no later than the audit's captured start time. |

The pending result was already non-authorizing and required the real canonical
receipt to become accepted. These corrections tighten the native observations
themselves in both modes rather than depending on later canonical chronology to
reject inconsistent records.

The duration check explicitly assumes UTC did not step or drift relative to the
monotonic clock by more than two seconds during a recorded process lifetime. It
rejects larger disagreement; it does not infer a different clock correction or
rewrite historical timestamps. The tolerance is returned in the audit report.

Focused fixtures reject all three findings, test both boundaries of the allowed
duration tolerance, and retain the earlier missing-canonical, baseline/candidate,
PID, image, source, log, binary, changed-during-read and no-overwrite checks.
Temporary synthetic fixture records are removed after testing. They never stand
in for an actual native launch, inspected image, approved registry or completed
canonical receipt. The first 10-test run is retained unchanged; the corrected
fixture result is recorded separately as
`../v0.24-preflight/native-audit-fixtures-02.json` and its hashed log. The corrected
run passed all **13 tests**, with no failures or skips, in 0.093 seconds of
unittest time (0.237 seconds including the bounded runner). It completed at
`2026-10-30T11:51:17.651650+00:00`, exit 0. No implementation input, native process,
actual capture registry or canonical receipt was changed by this follow-up.
