# Incomplete capture attempt

The first fresh 0.11 gate invocation correctly finalized as failed because the
Bevy command returned zero while its retained log lacked test output. The core
501-test gate and exact career gate have complete logs; the Bevy result is not
accepted as evidence. The command's observed procedural test process does not
substitute for a complete output record.

The verifier now captures child output in a private temporary file and publishes
the completed log atomically. The final release runs every gate again under
`../v0.11/`; it does not reuse this attempt's test results.
