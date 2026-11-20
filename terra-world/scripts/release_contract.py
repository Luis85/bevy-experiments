"""Canonical local acceptance roster shared by execution and desktop admission.

Changing release gates requires updating this roster together with the runner.
This structural contract does not authenticate an evidence publisher.
"""
from __future__ import annotations

from verify_desktop import validate_command_execution


GATE_NAMES = (
    'compatibility',
    'desktop-packaging-tests',
    'format',
    'headless',
    'career-report',
    'purchased-career-report',
    'novice-career-report',
    'traffic-recovery',
    'navigation-cluster',
    'navigation-geometry',
    'guidance-continuity',
    'bevy-headless',
    'bevy-render',
    'demo',
    'launcher-recovery',
    'launcher-world-names',
    'demo-check',
    'headless-example',
    'custom-provider-example',
    'asset-pack',
    'asset-inspect',
    'asset-build',
    'asset-authored-validate',
    'vehicle-generator',
    'vehicle-inspect',
    'vehicle-pack',
    'vehicle-demo-pack',
    'vehicle-asset-build',
    'vehicle-authored-validate',
    'character-generator',
    'character-inspect',
    'character-pack',
    'character-demo-pack',
    'character-asset-build',
    'character-authored-validate',
    'source-inspect',
    'source-export',
    'source-install',
    'source-verify',
    'recovery-probe',
    'recovery-bundle',
    'session-probe-checks',
    'regional-sessions',
    'native-build',
)


def validate_gate_names(names: list[str]) -> None:
    """Require each known acceptance gate exactly once, in execution order."""
    if tuple(names) != GATE_NAMES:
        raise ValueError("Acceptance gate roster is incomplete, duplicated, reordered or unknown")


RUST_TEST_GATE_NAMES = (
    'headless', 'career-report', 'purchased-career-report', 'novice-career-report',
    'traffic-recovery', 'navigation-cluster', 'navigation-geometry', 'guidance-continuity',
    'bevy-headless', 'bevy-render', 'demo', 'launcher-recovery', 'launcher-world-names',
    'session-probe-checks',
)


def validate_rust_test_gate_names(names: list[str]) -> None:
    if tuple(names) != RUST_TEST_GATE_NAMES:
        raise ValueError("Rust test gate selections differ from the canonical acceptance roster")


def validate_gate_results(gates: list[dict], *, require_execution: bool = False,
                          platform: str | None = None, output_limit_bytes: int | None = None) -> int:
    """Validate typed local gate results and return total Rust test executions."""
    validate_gate_names([gate.get('name') for gate in gates])
    total = 0
    for gate in gates:
        if gate.get('timed_out', False) is not False:
            raise ValueError("A timed-out acceptance gate cannot authorize publication")
        validate_command_execution(gate, required=require_execution, platform=platform,
                                   output_limit_bytes=output_limit_bytes)
        if type(gate.get('exit_code')) is not int or gate['exit_code'] != 0:
            raise ValueError("Acceptance gate requires an integer zero exit code")
        for field in ('passed_test_executions', 'failed_tests', 'ignored_tests'):
            count = gate.get(field)
            if type(count) is not int or count < 0:
                raise ValueError(f"Acceptance gate requires nonnegative integer {field}")
        if gate['failed_tests'] != 0 or gate['ignored_tests'] != 0:
            raise ValueError("An acceptance gate was not fully successful")
        passed = gate['passed_test_executions']
        if gate['name'] in RUST_TEST_GATE_NAMES:
            if passed == 0:
                raise ValueError("Every Rust test gate requires executed passing tests")
        elif passed != 0:
            raise ValueError("A non-test gate cannot record Rust test executions")
        total += passed
    return total
