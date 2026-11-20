"""Exercise command deadlines, retained output and owned child cleanup."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import sys
import subprocess
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

import verify


class GateExecutionTests(unittest.TestCase):
    def test_completed_command_retains_exact_output_and_exit_status(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "complete.log"
            command = [
                sys.executable, "-u", "-c",
                "import sys; print('stdout'); print('stderr', file=sys.stderr); sys.exit(3)",
            ]
            result, captured, timed_out = verify.run_logged(command, os.environ.copy(), log, 5)
            self.assertEqual(result.returncode, 3)
            self.assertFalse(timed_out)
            self.assertIn(b"stdout", captured)
            self.assertIn(b"stderr", captured)
            self.assertEqual(log.read_bytes(), captured)

    def test_success_at_output_limit_retains_exact_bytes_and_completion_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "exact.log"
            result, captured, timed_out = verify.run_logged(
                [sys.executable, "-u", "-c", "import sys;sys.stdout.buffer.write(b'x'*4096)"],
                os.environ.copy(), log, 5, 4096,
            )
            self.assertEqual(result.returncode, 0)
            self.assertFalse(timed_out)
            self.assertEqual(captured, b"x" * 4096)
            self.assertEqual(log.read_bytes(), captured)
            self.assertEqual(result.execution["status"], "passed")
            self.assertTrue(result.execution["output_complete"])
            self.assertFalse(result.execution["output_overflow"])
            self.assertTrue(result.execution["leader_exit_observed"])
            if os.name == "posix":
                self.assertEqual(result.execution["descendant_check"]["status"], "owned_process_group_absent")

    def test_noisy_command_is_stopped_and_retained_output_is_bounded(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "noisy.log"
            started = time.monotonic()
            result, captured, timed_out = verify.run_logged(
                [sys.executable, "-u", "-c", "import sys,time\nwhile True: sys.stdout.buffer.write(b'x'*65536);sys.stdout.flush()"],
                os.environ.copy(), log, 5, 4096,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(timed_out)
            self.assertLess(time.monotonic() - started, 5)
            self.assertEqual(result.execution["failure_reason"], "output_limit")
            self.assertTrue(result.execution["output_overflow"])
            self.assertFalse(result.execution["output_complete"])
            self.assertEqual(result.execution["raw_output_bytes_observed"], 4097)
            self.assertLessEqual(len(captured), 4096 + verify.MAX_CAPTURE_TRAILER_BYTES)
            self.assertIn(b"exceeded its 4096-byte", captured)
            self.assertEqual(log.read_bytes(), captured)

    def test_child_holding_output_cannot_turn_leader_exit_into_success(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            ready = root / "ready"
            child = "import pathlib,sys,time;pathlib.Path(sys.argv[1]).write_text('ready');time.sleep(2)"
            leader = (
                "import subprocess,sys,time,pathlib;"
                "subprocess.Popen([sys.executable,'-u','-c',sys.argv[1],sys.argv[2]]);"
                "deadline=time.monotonic()+3\n"
                "while not pathlib.Path(sys.argv[2]).exists() and time.monotonic()<deadline: time.sleep(.01)\n"
                "print('leader done',flush=True)"
            )
            result, captured, timed_out = verify.run_logged(
                [sys.executable, "-u", "-c", leader, child, str(ready)],
                os.environ.copy(), root / "held.log", 0.7, 4096,
            )
            self.assertTrue(ready.exists(), "the child must actually run")
            self.assertTrue(timed_out)
            self.assertEqual(result.returncode, 124)
            self.assertIn(b"leader done", captured)
            self.assertEqual(result.execution["failure_reason"], "timeout")
            # Windows taskkill cannot guarantee cleanup once the leader exits;
            # fixtures there use a bounded child lifetime rather than leak work.
            if os.name != "posix":
                time.sleep(1.4)

    def test_redirected_child_completion_is_reported_with_actual_platform_scope(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            ready, escaped = root / "ready", root / "escaped"
            child = (
                "import pathlib,sys,time;pathlib.Path(sys.argv[1]).write_text('ready');"
                "time.sleep(1);pathlib.Path(sys.argv[2]).write_text('escaped')"
            )
            leader = (
                "import subprocess,sys,time,pathlib;"
                "subprocess.Popen([sys.executable,'-u','-c',sys.argv[1],sys.argv[2],sys.argv[3]],"
                "stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);"
                "deadline=time.monotonic()+3\n"
                "while not pathlib.Path(sys.argv[2]).exists() and time.monotonic()<deadline: time.sleep(.01)\n"
                "print('leader done',flush=True)"
            )
            result, captured, timed_out = verify.run_logged(
                [sys.executable, "-u", "-c", leader, child, str(ready), str(escaped)],
                os.environ.copy(), root / "redirected.log", 5, 4096,
            )
            self.assertTrue(ready.exists())
            self.assertFalse(timed_out)
            if os.name == "posix":
                self.assertEqual(result.returncode, 125)
                self.assertEqual(result.execution["failure_reason"], "owned_descendants")
                self.assertEqual(result.execution["descendant_check"]["status"], "owned_process_group_present")
                self.assertIn(b"process-group completion was not confirmed", captured)
            else:
                self.assertEqual(result.returncode, 0)
                self.assertEqual(result.execution["descendant_check"]["status"], "unconfirmed")
            time.sleep(1.1)
            self.assertEqual(escaped.exists(), os.name != "posix")

    def test_cleanup_has_no_unbounded_leader_wait_or_unearned_exit_claim(self):
        process = Mock()
        process.poll.return_value = None
        process.wait.side_effect = subprocess.TimeoutExpired("fixture", 2)
        with patch.object(verify.os, "name", "other"):
            result = verify.stop_process_tree(process)
        process.wait.assert_called_once_with(timeout=2)
        self.assertFalse(result["leader_exit_observed"])
        self.assertEqual(result["descendant_check"]["status"], "unconfirmed")

    def test_stalled_command_times_out_and_retains_partial_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "timeout.log"
            started = time.monotonic()
            command = [
                sys.executable, "-u", "-c",
                "import time; print('before stall', flush=True); time.sleep(60)",
            ]
            result, captured, timed_out = verify.run_logged(command, os.environ.copy(), log, 0.5)
            self.assertTrue(timed_out)
            self.assertEqual(result.returncode, 124)
            self.assertLess(time.monotonic() - started, 10)
            self.assertIn(b"before stall", captured)
            self.assertIn(b"timed out after 0.5 seconds", captured)
            self.assertEqual(log.read_bytes(), captured)

    def test_timeout_stops_descendants_before_they_can_publish_late_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            ready, escaped = root / "child-ready", root / "escaped"
            child = (
                "from pathlib import Path; import sys,time;"
                "Path(sys.argv[1]).write_text('ready');"
                "time.sleep(1.5);Path(sys.argv[2]).write_text('late output')"
            )
            parent = (
                "import subprocess,sys,time; from pathlib import Path;"
                "subprocess.Popen([sys.executable,'-u','-c',sys.argv[1],sys.argv[2],sys.argv[3]]);"
                "print('parent ready',flush=True);time.sleep(60)"
            )
            result, captured, timed_out = verify.run_logged(
                [sys.executable, "-u", "-c", parent, child, str(ready), str(escaped)],
                os.environ.copy(), root / "tree.log", 1.0,
            )
            self.assertTrue(ready.exists(), "the descendant must actually start")
            self.assertTrue(timed_out)
            self.assertEqual(result.returncode, 124)
            self.assertIn(b"parent ready", captured)
            time.sleep(1.0)
            self.assertFalse(escaped.exists(), "a timed-out descendant escaped termination")
            self.assertEqual((root / "tree.log").read_bytes(), captured)

    def test_deadline_rejects_zero_negative_nonfinite_and_nonnumeric_values(self):
        for value in ["0", "-1", "nan", "inf", "-inf", "not-a-number"]:
            with self.subTest(value=value), self.assertRaises(argparse.ArgumentTypeError):
                verify.positive_timeout(value)
        self.assertEqual(verify.positive_timeout("0.25"), 0.25)
        self.assertEqual(verify.positive_timeout("3600"), 3600)

    def test_output_limit_rejects_unbounded_noninteger_or_nonpositive_values(self):
        for value in ["0", "-1", "nan", "inf", "1.5", str(verify.MAX_GATE_OUTPUT_LIMIT_BYTES + 1)]:
            with self.subTest(value=value), self.assertRaises(argparse.ArgumentTypeError):
                verify.positive_output_limit(value)
        self.assertEqual(verify.positive_output_limit("4096"), 4096)

    def test_startup_timeout_retains_failed_receipt_and_stops_real_descendant(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            ready, escaped = root / "ready", root / "escaped"
            child = (
                "from pathlib import Path; import sys,time;"
                "Path(sys.argv[1]).write_text('ready');"
                "time.sleep(1.5);Path(sys.argv[2]).write_text('escaped startup')"
            )
            probe = root / "stalled-probe.py"
            probe.write_text(
                "import subprocess,sys,time\n"
                f"subprocess.Popen([sys.executable,'-u','-c',{child!r},{str(ready)!r},{str(escaped)!r}])\n"
                "print('metadata partial output',flush=True)\n"
                "time.sleep(60)\n"
            )
            original = verify.run_logged

            def stalled_probe(command, env, log, timeout, output_limit):
                self.assertEqual(command[:2], ["cargo", "metadata"])
                return original([sys.executable, "-u", str(probe)], env, log, timeout, output_limit)

            evidence = root / "evidence"
            with patch.object(verify, "ROOT", root), patch.object(verify, "run_logged", side_effect=stalled_probe), \
                 patch.object(sys, "argv", ["verify.py", "--gate-timeout-seconds", "1", "--report-dir", str(evidence)]):
                self.assertEqual(verify.main(), 1)
            self.assertTrue(ready.exists(), "the startup descendant must actually run")
            time.sleep(1)
            self.assertFalse(escaped.exists())
            report = json.loads((evidence / "completed-gates.json").read_text())
            self.assertEqual(report["status"], "failed")
            self.assertEqual(report["gates"], [])
            self.assertEqual(len(report["startup_probes"]), 1)
            result = report["startup_probes"][0]
            self.assertTrue(result["timed_out"])
            self.assertEqual(result["exit_code"], 124)
            retained = (evidence / result["log"]).read_bytes()
            self.assertIn(b"metadata partial output", retained)
            self.assertIn(b"timed out", retained)
            self.assertEqual(report["evidence_artifacts"][result["log"]], verify.digest(evidence / result["log"]))

    def test_startup_overflow_has_a_failed_terminal_receipt_with_bounded_hashed_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original = verify.run_logged

            def noisy_probe(command, env, log, timeout, output_limit):
                return original([sys.executable, "-u", "-c", "print('x'*20000)"], env, log, timeout, output_limit)

            evidence = root / "evidence"
            with patch.object(verify, "ROOT", root), patch.object(verify, "run_logged", side_effect=noisy_probe), \
                 patch.object(sys, "argv", ["verify.py", "--gate-output-limit-bytes", "4096", "--report-dir", str(evidence)]):
                self.assertEqual(verify.main(), 1)
            report = json.loads((evidence / "completed-gates.json").read_text())
            self.assertEqual(report["status"], "failed")
            self.assertEqual(report["gates"], [])
            result = report["startup_probes"][0]
            self.assertEqual(result["execution"]["failure_reason"], "output_limit")
            self.assertNotEqual(result["exit_code"], 0)
            retained = (evidence / result["log"]).read_bytes()
            self.assertLessEqual(len(retained), 4096 + verify.MAX_CAPTURE_TRAILER_BYTES)
            self.assertEqual(report["evidence_artifacts"][result["log"]], verify.digest(evidence / result["log"]))


if __name__ == "__main__":
    unittest.main()
