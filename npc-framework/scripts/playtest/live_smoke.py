"""Explicitly authorized disposable Codex engineering smoke, never private history.

python scripts/playtest/live_smoke.py --authorize-disposable
Hard caps: 600 seconds total, one initial turn and one scheduled continuation,
ChatGPT included login only. No model/permission/trust/global-config overrides.
Requires release npcctl, signed-in Codex and a trusted generated MCP workspace.
"""
import argparse
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import threading
import time
import uuid
from collections import Counter

from capture_native import ROOT, rpc

FLAGS = getattr(subprocess, "CREATE_NO_WINDOW", 0)


def lines(stream, output):
    for line in stream:
        output.put(line)


def stop_owned(child):
    """Stop only the process tree created by this smoke, including its MCP child."""
    if child.poll() is not None:
        return
    if os.name == "nt":
        subprocess.run(["taskkill", "/PID", str(child.pid), "/T", "/F"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            creationflags=FLAGS, timeout=10)
    if child.poll() is None:
        child.kill()
    child.wait(timeout=10)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--authorize-disposable", action="store_true")
    parser.add_argument("--codex", type=Path, default=shutil.which("codex.exe") or shutil.which("codex"))
    parser.add_argument("--provider", choices=["codex", "claude"], default="codex")
    args = parser.parse_args()
    if not args.authorize_disposable:
        parser.error("explicit new-disposable-conversation authorization is required")
    program = args.codex if args.provider == "codex" else Path(shutil.which("claude.exe") or shutil.which("claude") or "")
    if not program or not program.is_file():
        parser.error("selected installed CLI executable unavailable")
    program = program.resolve()
    auth_args = ["login", "status"] if args.provider == "codex" else ["auth", "status"]
    auth = subprocess.run([str(program), *auth_args], capture_output=True, text=True, creationflags=FLAGS, timeout=15)
    if args.provider == "codex":
        authenticated = auth.returncode == 0 and "using ChatGPT" in auth.stdout + auth.stderr
    else:
        status = json.loads(auth.stdout) if auth.returncode == 0 else {}
        authenticated = status.get("loggedIn") and status.get("authMethod") == "claude.ai"
    if not authenticated:
        parser.error("requires existing subscription login; no API-key billing or login is attempted")
    run = ROOT / "target" / ("playtest-live-" + uuid.uuid4().hex[:10])
    run.mkdir()
    worlds = run / "worlds"
    worlds.mkdir()
    preset = json.loads((ROOT / "content/testbed_content/worlds/island_village.json").read_text())
    preset["ai_scheduler"].update({"idle_seconds": 600, "cooldown_seconds": 86400,
        "max_nudges_per_hour": 1, "runner_timeout_seconds": 120})
    (worlds / "island_village.json").write_text(json.dumps(preset), encoding="utf-8")
    npcctl = str(ROOT / "target/release/npcctl.exe")
    report = {"ok": False, "hero": "Herp", "provider": args.provider, "private_history_read": False,
        "caps": {"wall_seconds": 600, "turns": 2, "usage": "existing subscription login; no API key"},
        "fixture_policy": "600s idle (beyond smoke); one nudge/hour, 120s runner timeout; recorded need-threshold event in disposable fixture",
        "original_turn_finished": False, "same_conversation_linked": False,
        "resumed_valid_game_request": False}
    host = client = None
    url = None
    started = time.monotonic()
    deadline = started + 600
    try:
        host_lines = queue.Queue()
        host = subprocess.Popen([npcctl, "serve", "--explorer-home", str(run / "agents"),
            "--actors-dir", str(run / "actors"), "--worlds-dir", str(worlds),
            "--agent", "Herp", "--world", "Island", "--preset", "human",
            "--world-preset", "island_village", "--port", "0", "--paused"],
            cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
            creationflags=FLAGS)
        threading.Thread(target=lines, args=(host.stdout, host_lines), daemon=True).start()
        ready = json.loads(host_lines.get(timeout=60))
        url = ready["listening"]
        workspace = run / "agents/herp"
        mcp_args = []
        if args.provider == "codex":
            mcp = json.loads((workspace / ".mcp.json").read_text())["mcpServers"]["npc"]
            mcp_args = ["-c", "mcp_servers.npc.command=" + json.dumps(mcp["command"]),
                "-c", "mcp_servers.npc.args=" + json.dumps(mcp["args"])]
        check = subprocess.run([str(program), "mcp", "get", "npc", *mcp_args], cwd=workspace,
            capture_output=True, text=True, timeout=15, creationflags=FLAGS)
        if check.returncode:
            report["blocked"] = "generated workspace npc MCP configuration is not active; operator must trust/verify this new folder before a separate authorized smoke"
            return
        report["mcp_preflight"] = True
        rpc(url, "npc/resume")  # Operator resumes; AI never advances the clock.
        prompt = (workspace / "prompts/start.md").read_text(encoding="utf-8")
        prompt += "\nThis is game-client play, not repository coding. Read only this folder's AGENTS.md, "
        prompt += "not parent repository guides. Engineering smoke only, two turns maximum. Use only the registered npc game tools. "
        prompt += "Read AGENTS.md privately, register, enter, take control and observe. "
        prompt += "Use talk once to say the public line 'Hello Island. I am Herp.' "
        prompt += "Make the normal act write_diary with title Smoke initial and a short body; also "
        prompt += "write one short Diary notebook note titled Smoke initial using notes_add, then stop this turn. "
        prompt += "When this SAME conversation is resumed once by the supervisor, observe and write "
        prompt += "one physical page using act write_diary and one notebook note titled Smoke continuation "
        prompt += "with notes_add, then stop. Do not run more turns, "
        prompt += "launch other agents, change configuration, use shell for game actions, or reveal credentials."
        events = queue.Queue()
        if args.provider == "codex":
            # A diagnostic connection configuration only, identically supplied
            # on both turns. It changes no model, permission, trust or global setting.
            initial_args = ["exec", *mcp_args, "--skip-git-repo-check", "--json", "-"]
            resume_args = ["exec", "resume", *mcp_args, "--skip-git-repo-check", "{session_id}", "-"]
            conversation = None
            report["connection"] = "explicit npc MCP argv on both turns; custom runner; default adapter remains separate"
        else:
            conversation = str(uuid.uuid4())  # New requested UUID; never looked up.
            initial_args = ["--print", "--output-format",
                "stream-json", "--verbose", "--session-id", conversation]
            resume_args = ["--print", "--resume", "{session_id}"]
            report["connection"] = "Claude project MCP; custom runner preserves configured model/effort"
        client = subprocess.Popen([str(program), *initial_args],
            cwd=workspace, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, text=True, encoding="utf-8", creationflags=FLAGS)
        threading.Thread(target=lines, args=(client.stdout, events), daemon=True).start()
        client.stdin.write(prompt)
        client.stdin.close()
        event_types = Counter()
        tool_names = Counter()
        tool_outcomes = Counter()

        def record_event(event):
            nonlocal conversation
            event_types[str(event.get("type", "unknown"))[:60]] += 1
            if event.get("type") == "thread.started":
                conversation = event.get("thread_id")
            item = event.get("item", {})
            if item.get("type") == "mcp_tool_call":
                tool_names[str(item.get("tool", "unknown"))[:80]] += 1
                # Closed classifications only; raw errors can contain credentials,
                # arguments or private provider text and must never enter evidence.
                outcome = item.get("status")
                if outcome in ("completed", "failed"):
                    tool_outcomes[outcome] += 1
                if item.get("error"):
                    tool_outcomes["protocol_error"] += 1
                if isinstance(item.get("result"), dict) and item["result"].get("isError"):
                    tool_outcomes["tool_error"] += 1
            for part in event.get("message", {}).get("content", []):
                if part.get("type") == "tool_use":
                    tool_names[str(part.get("name", "unknown"))[:80]] += 1

        while client.poll() is None and time.monotonic() < deadline - 150:
            try:
                event = json.loads(events.get(timeout=1))
                record_event(event)
            except (queue.Empty, json.JSONDecodeError):
                pass
        if client.poll() is None:
            report["blocked"] = "original turn exceeded its bounded budget; no continuation launched"
            return
        # Drain final events without retaining any transcript or tool arguments.
        while not events.empty():
            try:
                event = json.loads(events.get_nowait())
                record_event(event)
            except json.JSONDecodeError:
                pass
        report["original_turn_finished"] = True
        report["original_exit_code"] = client.returncode
        report["event_types"] = dict(event_types.most_common(20))
        report["tool_names"] = dict(tool_names.most_common(32))
        report["tool_outcomes"] = dict(tool_outcomes)
        if client.returncode or not conversation:
            report["blocked"] = "original provider turn failed or returned no new conversation identity"
            return
        # Register a separate observer session only if necessary would steal the
        # actor; use existing read-only operator status and notebook file count.
        diary = workspace / "diary"
        initial_notes = set(diary.glob("*.md"))
        if not initial_notes:
            report["blocked"] = "initial provider turn did not write a valid game Diary note"
            return
        status = rpc(url, "scheduler/status")
        if len(status.get("agents", [])) != 1:
            report["blocked"] = "original provider did not establish one controlled actor"
            return
        unit = status["agents"][0]["unit"]

        def physical_pages():
            state = rpc(url, "world/state")
            actor = next(e for e in state["entities"] if e["sim_id"] == unit)
            return len(actor.get("components", {}).get("island_diary", {}).get("pages", []))

        def spoken_lines():
            entries = rpc(url, "npc/journal", {"unit": unit, "kinds": ["Effect"],
                "grep": "VillageSpeech", "limit": 100})
            return sum(1 for entry in entries if entry.get("kind", {}).get("effect", {}).get("type") == "VillageSpeech")

        initial_pages = physical_pages()
        report["initial_spoken_lines"] = spoken_lines()
        if initial_pages < 1 or report["initial_spoken_lines"] < 1:
            report["blocked"] = "initial provider did not complete its required recorded diary and talk actions"
            return
        binding_args = [npcctl, "scheduler", "bind", "--home", str(run / "agents"),
            "--agent", "Herp", "--provider", "custom", "--program", str(program),
            "--session-id", conversation, "--turn-finished"]
        for argument in resume_args:
            binding_args += ["--runner-arg", argument]
        binding = subprocess.run(binding_args, capture_output=True,
            text=True, creationflags=FLAGS, timeout=15)
        if binding.returncode:
            report["blocked"] = "exact new conversation could not be linked"
            return
        report["same_conversation_linked"] = True
        # Establish the new-link baseline before injecting a labeled engineering
        # event. This tests event dispatch, without waiting for the idle fallback.
        time.sleep(2)
        rpc(url, "npc/command", {"cmd": {"type": "SetNeed", "unit": unit,
            "need": "Hunger", "value": 30000}})
        report["event_fixture"] = "operator recorded SetNeed(Hunger,30000); not unaided survival evidence"
        states = []
        while time.monotonic() < deadline:
            status = rpc(url, "scheduler/status")
            actor = status["agents"][0]
            state = actor["state"]
            if actor.get("trigger") == "events" and actor.get("last_delivery_seq") is not None:
                report["event_triggered"] = True
            if not states or states[-1] != state:
                states.append(state)
            if len(set(diary.glob("*.md")) - initial_notes) >= 1 and physical_pages() > initial_pages:
                report["resumed_valid_game_request"] = True
            if actor.get("nudges_last_hour", 0) >= 1 and state != "running":
                report["continuation_result"] = actor.get("last_result")
                break
            time.sleep(0.5)
        report["states"] = states
        report["ok"] = (report.get("event_triggered", False) and report["resumed_valid_game_request"]
            and report.get("continuation_result") == "turn finished")
        if not report["ok"]:
            report["blocked"] = "authenticated continuation did not finish and make its required game request within the cap"
    except (OSError, RuntimeError, subprocess.TimeoutExpired, queue.Empty, ValueError) as error:
        # Keep provider output/UUID/token/private notes out of the public summary.
        report["blocked"] = "owned smoke failed: " + type(error).__name__
    finally:
        if client is not None and client.poll() is None:
            stop_owned(client)
        if host is not None:
            try:
                if url:
                    rpc(url, "session/stop")
                host.wait(timeout=20)
            except (OSError, RuntimeError, subprocess.TimeoutExpired):
                host.kill()
                host.wait(timeout=10)
        report["owned_resources_stopped"] = True
        report["elapsed_seconds"] = round(time.monotonic() - started, 3)
        report["run"] = str(run.relative_to(ROOT))
        (run / "summary.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps(report, indent=2))
        if not report["ok"]:
            raise SystemExit(1)


if __name__ == "__main__":
    main()
