"""One authorized real Codex rehearsal through the dedicated AI game client.

This script creates a disposable Herp/Island explorer, appends a clearly labeled
engineering fixture to its generated prompts/current.md, and starts the shipped
ai-agent-game-client with --provider codex --max-turns 2
--max-runtime-seconds 600. It never launches Codex directly, builds binaries,
changes global provider model settings/permissions/login, reads private conversation history,
or retries a failed provider turn. Stop at the first failure; do not rerun this
paid/subscription rehearsal without separate operator authorization.

--approve-game-tools is a separate, explicit opt-in to the client's bounded
NPC-tool approvals for its owned Codex processes. Use it only when the operator
has also authorized that scope; ordinary disposable-session authorization alone
does not grant it. No global, shell, sandbox or other-server policy is changed.

From npc-framework, after the release binaries have already been built:
    python scripts/playtest/client_smoke.py --help
    python scripts/playtest/client_smoke.py --authorize-disposable

Optional --bin-dir selects an already-built directory containing BOTH npcctl and
ai-agent-game-client. Defaults to target/release. Output is a sanitized summary
only, also saved as summary.json under target/playtest-client-smoke-*/.

This is an engineering fixture, not unaided survival, GUI, or billing evidence.
The operator injects one recorded own-actor need threshold after turn one. Idle
recovery is configured beyond the entire rehearsal, so the second-turn check
exercises event continuation. Provider output and conversation identifiers are
deliberately not captured; exact continuity is checked privately in the owned
disposable client's state, alongside its two successful turn completions.
"""

import argparse
import json
import os
from pathlib import Path
import queue
import re
import signal
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request


ROOT = Path(__file__).resolve().parents[2]
FLAGS = getattr(subprocess, "CREATE_NO_WINDOW", 0)
WALL_SECONDS = 600
MAX_JSON = 4 * 1024 * 1024
INITIAL_TITLE = "Client smoke initial"
RESUMED_TITLE = "Client smoke continuation"
FAILURE_CODES = frozenset({
    "authentication", "rate_limit", "model_unavailable", "mcp_startup",
    "provider_config", "network", "output_limit", "missing_terminal",
    "nonzero_exit", "game_tool_cancelled", "game_access_revoked",
    "game_tools_unavailable", "game_permission_denied", "unknown",
})

FIXTURE = """

## Bounded engineering fixture: dedicated AI-Agent-Game-Client smoke

This is an explicitly authorized disposable engineering rehearsal, not an
open-ended survival run or repository coding task. The client owns exactly one
initial turn and one continuation of THIS conversation. Use the existing CLI
login and the model and local-game permission policy selected by the dedicated
client for its owned CLI process. Do not grant or change permissions yourself.
Do not launch agents or CLI processes,
edit configuration, advance ticks, reveal credentials, or use shell commands
for game actions. Read only this workspace's AGENTS.md and use its registered
npc MCP tools. If a tool/setup requirement blocks you, finish the turn with that
blocker; do not substitute another interface or attempt another conversation.

On this INITIAL turn: read AGENTS.md, register, enter_world and take_control.
Observe your assigned actor. Use the advertised act_and_wait tool/schema to
talk once with the public line "Hello Island. I am Herp. Engineering initial."
Write one physical diary page using the normal write_diary action, with title
"Client smoke initial" and a short engineering-fixture body. Add one Diary
notebook note with notes_add, book diary, title "Client smoke initial" and a
short body. These are TWO different game features; do both. Wait for the game
actions to finish. Observe LAST so completed actions/events are acknowledged,
then FINISH YOUR AI TURN immediately. Do not keep polling or wait for a reply.

When THIS SAME conversation is continued once by the dedicated client:
read AGENTS.md and privately register once on the fresh npc MCP connection, call enter_world and take_control if needed, then observe current state and fresh events; if access is revoked, stop without reconnecting or replaying a mutation. Talk once with the public line
"Herp resumed. Engineering continuation." Write one physical write_diary page
and one Diary notebook notes_add note, BOTH titled "Client smoke continuation".
Use short bodies identifying this as the resumed engineering fixture. Wait for
those actions to finish, observe LAST, then FINISH YOUR AI TURN immediately.
Do not gather, travel, start long work or request any additional AI turns.
"""


class SmokeFailure(Exception):
    """Only closed, non-sensitive reason codes are raised publicly."""


def read_json(path, limit=MAX_JSON):
    with path.open("rb") as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise SmokeFailure("owned_fixture_json_too_large")
    return json.loads(data.decode("utf-8-sig"))


def rpc(url, method, params=None):
    request = urllib.request.Request(
        url,
        json.dumps({"jsonrpc": "2.0", "id": 1, "method": method,
                    "params": params or {}}).encode(),
        {"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=5) as response:
            data = response.read(MAX_JSON + 1)
        if len(data) > MAX_JSON:
            raise SmokeFailure("owned_game_response_too_large")
        value = json.loads(data)
        if "error" in value:
            raise SmokeFailure("owned_game_rpc_rejected")
        return value["result"]
    except (OSError, urllib.error.URLError, ValueError, KeyError):
        raise SmokeFailure("owned_game_rpc_unavailable") from None


def creation_options():
    if os.name == "nt":
        return {"creationflags": FLAGS}
    return {"start_new_session": True}


def readiness(stream, output):
    """Keep only the public loopback readiness address; discard all host logs."""
    for line in stream:
        if len(line) > 128 * 1024:
            continue
        try:
            value = json.loads(line)
            url = value.get("listening", "")
            if isinstance(url, str) and re.fullmatch(r"http://127\.0\.0\.1:[0-9]+/?", url):
                output.put_nowait(url)
        except (ValueError, AttributeError, queue.Full):
            pass


def status(workspace):
    path = workspace / ".ai-client/status.json"
    try:
        return read_json(path, 16 * 1024)
    except (OSError, ValueError):
        return {}


def refresh_report_status(report, workspace):
    """Sample final public counters after cleanup, copying only closed diagnostics.

    The independent budget guard can observe a failure before the main loop's
    next sample. Shutdown must refresh that sample rather than report stale zero
    failures. Provider output, arbitrary messages and private state are excluded.
    """
    try:
        public = status(workspace)
    except (OSError, ValueError, TypeError, SmokeFailure):
        return False
    if not isinstance(public, dict):
        return False
    sampled = False
    for source, target in (("turns_completed", "turns_completed"),
                           ("failures", "provider_failures")):
        value = public.get(source)
        if isinstance(value, int) and not isinstance(value, bool) and value >= 0:
            report[target] = max(report.get(target, 0), value)
            sampled = True
    code = public.get("last_failure_code")
    if isinstance(code, str) and code in FAILURE_CODES:
        report["last_failure_code"] = code
    return sampled


def guard_budget(workspace, deadline, finished, failures):
    """Stop independently of game RPC latency, before provider failure retries."""
    while not finished.wait(0.1):
        reason = None
        try:
            public = status(workspace)
            if int(public.get("failures", 0)) > 0:
                reason = "first_provider_failure_no_retry_authorized"
            elif public.get("state") == "error":
                reason = "client_setup_or_turn_failed_no_retry_authorized"
        except (SmokeFailure, ValueError, TypeError):
            reason = "owned_client_status_invalid"
        if time.monotonic() >= deadline:
            reason = reason or "authorized_wall_budget_exhausted_no_retry"
        if reason:
            # This is the dedicated client's ordinary cooperative stop channel,
            # confined to the newly generated, owned disposable workspace.
            try:
                (workspace / ".ai-client/stop").write_text("stop\n", encoding="utf-8")
            except OSError:
                pass
            try:
                failures.put_nowait(reason)
            except queue.Full:
                pass
            return


def private_state(workspace):
    """Read only this newly created fixture, never provider history/global state."""
    value = read_json(workspace / ".ai-client/state.json", 256 * 1024)
    session = value.get("session")
    if not isinstance(session, str) or not re.fullmatch(
            r"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}", session):
        raise SmokeFailure("owned_client_session_not_established")
    return value


def notebook_counts(workspace):
    result = {INITIAL_TITLE: 0, RESUMED_TITLE: 0}
    for path in (workspace / "diary").glob("*.md"):
        if path.stat().st_size > 256 * 1024:
            raise SmokeFailure("owned_notebook_note_too_large")
        text = path.read_text(encoding="utf-8-sig")
        header = text.split("---", 2)
        if len(header) < 3:
            continue
        for line in header[1].splitlines():
            if line.startswith("title:"):
                title = line.partition(":")[2].strip().strip("\"'")
                if title in result:
                    result[title] += 1
                break
    return result


def physical_counts(url, unit):
    entities = rpc(url, "world/state").get("entities", [])
    actor = next((entity for entity in entities if entity.get("sim_id") == unit), None)
    if actor is None:
        raise SmokeFailure("owned_actor_unavailable")
    pages = actor.get("components", {}).get("island_diary", {}).get("pages", [])
    return {title: sum(page.get("title") == title for page in pages)
            for title in (INITIAL_TITLE, RESUMED_TITLE)}


def journal_counts(url, unit):
    entries = rpc(url, "npc/journal", {"unit": unit,
                  "kinds": ["Effect", "NeedLevel"], "limit": 1000})
    result = {"initial_speech": 0, "resumed_speech": 0, "hunger_thresholds": 0}
    for entry in entries:
        kind = entry.get("kind", {})
        if kind.get("type") == "NeedLevel" and kind.get("need") == "Hunger":
            result["hunger_thresholds"] += 1
        effect = kind.get("effect", {})
        if effect.get("type") != "VillageSpeech":
            continue
        # Public speech text is compared in memory, never copied to the report.
        text = effect.get("text", "")
        result["initial_speech"] += text == "Hello Island. I am Herp. Engineering initial."
        result["resumed_speech"] += text == "Herp resumed. Engineering continuation."
    return result


def stop_owned(child):
    """Only terminate handles created by this script; never enumerate other CLIs."""
    if child is None or child.poll() is not None:
        return True
    if os.name == "nt":
        subprocess.run(["taskkill", "/PID", str(child.pid), "/T", "/F"],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                       creationflags=FLAGS, timeout=10, check=False)
    else:
        # SIGINT lets the client's handler clean its separately owned provider group.
        os.killpg(child.pid, signal.SIGINT)
        try:
            child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGKILL)
    if child.poll() is None:
        child.kill()
    child.wait(timeout=10)
    return child.poll() is not None


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--authorize-disposable", action="store_true",
                        help="authorize this one new disposable Codex conversation and one continuation")
    parser.add_argument("--approve-game-tools", action="store_true",
                        help="separately authorize automatic approval of this owned client's validated local NPC game tools")
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/release",
                        help="already-built npcctl/client directory; no build is performed")
    args = parser.parse_args()
    if not args.authorize_disposable:
        parser.error("explicit authorization for one disposable two-turn rehearsal is required")
    suffix = ".exe" if os.name == "nt" else ""
    npcctl = args.bin_dir.resolve() / ("npcctl" + suffix)
    client_program = args.bin_dir.resolve() / ("ai-agent-game-client" + suffix)
    if not npcctl.is_file() or not client_program.is_file():
        parser.error("build npcctl and ai-agent-game-client before this separately authorized rehearsal")

    (ROOT / "target").mkdir(exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix="playtest-client-smoke-", dir=ROOT / "target")).resolve()
    report = {"ok": False, "provider": "codex", "private_history_read": False,
              "game_tools_approval_explicitly_authorized": args.approve_game_tools,
              "turn_limit": 2, "wall_limit_seconds": WALL_SECONDS,
              "turns_completed": 0, "provider_failures": 0,
              "same_session_reused": False, "need_fixture_applied": False,
              "second_turn_after_fixture": False, "owned_resources_stopped": False,
              "evidence_kind": "disposable_engineering_fixture_not_unaided_survival"}
    host = client = None
    url = None
    workspace = run / "agents/herp"
    started = time.monotonic()
    deadline = started + WALL_SECONDS
    first_session = None
    guard_finished = threading.Event()
    guard_failures = queue.Queue(maxsize=1)
    try:
        worlds = run / "worlds"
        worlds.mkdir()
        preset = read_json(ROOT / "content/testbed_content/worlds/island_village.json")
        preset["ai_scheduler"].update({"enabled": True, "events_enabled": True,
            "idle_seconds": 86400, "cooldown_seconds": 5, "poll_seconds": 1,
            "max_nudges_per_hour": 2, "runner_timeout_seconds": 300, "max_concurrent": 1})
        (worlds / "island_village.json").write_text(json.dumps(preset), encoding="utf-8")
        ready = queue.Queue(maxsize=1)
        host = subprocess.Popen([str(npcctl), "serve", "--explorer-home", str(run / "agents"),
            "--actors-dir", str(run / "actors"), "--worlds-dir", str(worlds),
            "--agent", "Herp", "--world", "Island", "--preset", "human",
            "--world-preset", "island_village", "--port", "0", "--paused"],
            cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            text=True, encoding="utf-8", **creation_options())
        threading.Thread(target=readiness, args=(host.stdout, ready), daemon=True).start()
        try:
            url = ready.get(timeout=min(60, max(1, deadline - time.monotonic())))
        except queue.Empty:
            raise SmokeFailure("owned_host_startup_timeout") from None
        current = workspace / "prompts/current.md"
        if not current.is_file() or (workspace / ".ai-client/state.json").exists():
            raise SmokeFailure("fixture_must_be_new_with_current_prompt")
        with current.open("a", encoding="utf-8") as prompt:
            prompt.write(FIXTURE)
        rpc(url, "npc/resume")  # Operator fixture control; never done by the AI client.
        client_args = [str(client_program), "run", "--workspace", str(workspace),
            "--provider", "codex", "--max-turns", "2", "--max-runtime-seconds", "600"]
        if args.approve_game_tools:
            client_args.append("--approve-game-tools")
        client = subprocess.Popen(client_args,
            cwd=ROOT, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL, **creation_options())
        threading.Thread(target=guard_budget,
                         args=(workspace, deadline, guard_finished, guard_failures),
                         daemon=True).start()

        injected = False
        baseline_thresholds = 0
        unit = None
        while time.monotonic() < deadline:
            if not guard_failures.empty():
                raise SmokeFailure(guard_failures.get_nowait())
            public = status(workspace)
            report["turns_completed"] = int(public.get("turns_completed", 0))
            report["provider_failures"] = int(public.get("failures", 0))
            if report["provider_failures"]:
                raise SmokeFailure("first_provider_failure_no_retry_authorized")
            if public.get("state") == "error":
                raise SmokeFailure("client_setup_or_turn_failed_no_retry_authorized")
            if report["turns_completed"] >= 1 and not injected:
                if report["turns_completed"] != 1 or public.get("state") != "waiting":
                    raise SmokeFailure("unexpected_continuation_before_operator_fixture")
                rpc(url, "npc/pause")
                state = private_state(workspace)
                first_session = state["session"]
                unit = state["unit"]
                notes = notebook_counts(workspace)
                pages = physical_counts(url, unit)
                journal = journal_counts(url, unit)
                report.update({"initial_notebook_notes": notes[INITIAL_TITLE],
                               "initial_physical_pages": pages[INITIAL_TITLE],
                               "initial_spoken_lines": journal["initial_speech"]})
                if any(value < 1 for value in (notes[INITIAL_TITLE], pages[INITIAL_TITLE], journal["initial_speech"])):
                    raise SmokeFailure("initial_turn_missing_required_game_artifacts")
                baseline_thresholds = journal["hunger_thresholds"]
                rpc(url, "npc/command", {"cmd": {"type": "SetNeed", "unit": unit,
                    "need": "Hunger", "value": 30000}})
                injected = True
                rpc(url, "npc/resume")
            if injected:
                journal = journal_counts(url, unit)
                report["need_fixture_applied"] = journal["hunger_thresholds"] > baseline_thresholds
                if public.get("state") == "running" or report["turns_completed"] >= 2:
                    report["second_turn_after_fixture"] = True
            if client.poll() is not None:
                if client.returncode != 0:
                    raise SmokeFailure("dedicated_client_exited_unsuccessfully")
                # The final atomic status may have replaced the earlier sample
                # while journal verification was running in this iteration.
                final_public = status(workspace)
                report["turns_completed"] = int(final_public.get("turns_completed", 0))
                report["provider_failures"] = int(final_public.get("failures", 0))
                if report["provider_failures"]:
                    raise SmokeFailure("first_provider_failure_no_retry_authorized")
                if report["turns_completed"] != 2 or not injected:
                    raise SmokeFailure("dedicated_client_did_not_finish_two_turns")
                break
            time.sleep(0.2 if not injected else 0.5)
        else:
            raise SmokeFailure("authorized_wall_budget_exhausted_no_retry")

        final = private_state(workspace)
        report["same_session_reused"] = first_session == final["session"]
        report["session_archives"] = len(list((workspace / ".ai-client").glob("state.previous.*.json")))
        notes = notebook_counts(workspace)
        pages = physical_counts(url, unit)
        journal = journal_counts(url, unit)
        report.update({"resumed_notebook_notes": notes[RESUMED_TITLE],
                       "resumed_physical_pages": pages[RESUMED_TITLE],
                       "resumed_spoken_lines": journal["resumed_speech"]})
        report["ok"] = (report["turns_completed"] == 2 and report["provider_failures"] == 0
            and report["same_session_reused"] and report["session_archives"] == 0
            and report["need_fixture_applied"] and report["second_turn_after_fixture"]
            and all(value >= 1 for value in (notes[RESUMED_TITLE], pages[RESUMED_TITLE], journal["resumed_speech"])))
        if not report["ok"]:
            raise SmokeFailure("continuation_or_game_artifact_acceptance_failed")
    except SmokeFailure as error:
        report["failure_code"] = str(error)
    except (OSError, ValueError, KeyError, TypeError, subprocess.TimeoutExpired):
        report["failure_code"] = "owned_fixture_operation_failed"
    finally:
        guard_finished.set()
        client_stopped = host_stopped = False
        try:
            if client is not None and client.poll() is None:
                subprocess.run([str(client_program), "stop", "--workspace", str(workspace)],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                    timeout=5, **creation_options())
                try:
                    client.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    pass
            client_stopped = stop_owned(client)
        except (OSError, subprocess.TimeoutExpired):
            client_stopped = False
        try:
            if host is not None and host.poll() is None:
                if url:
                    rpc(url, "session/stop")
                try:
                    host.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    pass
            host_stopped = stop_owned(host)
        except (OSError, SmokeFailure, subprocess.TimeoutExpired):
            host_stopped = False
            try:
                host_stopped = stop_owned(host)
            except (OSError, subprocess.TimeoutExpired):
                pass
        report["owned_resources_stopped"] = client_stopped and host_stopped
        # The guard may stop at the first failure before the main loop sampled
        # its increment. Read the final atomic public status after client exit.
        report["final_status_sampled"] = refresh_report_status(report, workspace)
        report["ok"] = report["ok"] and report["owned_resources_stopped"]
        if report["provider_failures"] > 0:
            report["ok"] = False
        report["elapsed_seconds"] = round(time.monotonic() - started, 3)
        report["run"] = str(run.relative_to(ROOT))
        summary = json.dumps(report, indent=2)
        (run / "summary.json").write_text(summary, encoding="utf-8")
        print(summary)
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
