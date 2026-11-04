"""Local report/privacy regression tests. Never starts a game or provider."""
import json
from pathlib import Path
import tempfile
import unittest

import client_smoke


class FinalStatusTests(unittest.TestCase):
    def snapshot(self, public):
        fixture = tempfile.TemporaryDirectory(prefix="npc-smoke-report-")
        self.addCleanup(fixture.cleanup)
        workspace = Path(fixture.name)
        (workspace / ".ai-client").mkdir()
        (workspace / ".ai-client/status.json").write_text(
            json.dumps(public), encoding="utf-8")
        return workspace

    def test_guard_failure_is_captured_from_final_stopped_status(self):
        for code in ("mcp_startup", "game_tool_cancelled", "game_access_revoked",
                     "game_tools_unavailable", "game_permission_denied"):
            with self.subTest(code=code):
                workspace = self.snapshot({"state": "stopped", "turns_completed": 0,
                                           "failures": 1, "last_failure_code": code,
                                           "message": "private-token-marker",
                                           "session": "private-session-marker"})
                report = {"turns_completed": 0, "provider_failures": 0}
                self.assertTrue(client_smoke.refresh_report_status(report, workspace))
                self.assertEqual(report, {"turns_completed": 0, "provider_failures": 1,
                                          "last_failure_code": code})
                self.assertNotIn("private-token-marker", json.dumps(report))
                self.assertNotIn("private-session-marker", json.dumps(report))

    def test_earlier_public_schema_does_not_invent_a_failure_category(self):
        workspace = self.snapshot({"state": "stopped", "turns_completed": 0,
                                   "failures": 1})
        report = {"turns_completed": 0, "provider_failures": 0}
        self.assertTrue(client_smoke.refresh_report_status(report, workspace))
        self.assertEqual(report["provider_failures"], 1)
        self.assertNotIn("last_failure_code", report)

    def test_unknown_diagnostic_strings_and_invalid_counters_never_enter_report(self):
        workspace = self.snapshot({"turns_completed": "private-count-marker",
                                   "failures": True,
                                   "last_failure_code": "authentication-private-token-marker"})
        report = {"turns_completed": 0, "provider_failures": 0}
        self.assertFalse(client_smoke.refresh_report_status(report, workspace))
        self.assertEqual(report, {"turns_completed": 0, "provider_failures": 0})


if __name__ == "__main__":
    unittest.main()
