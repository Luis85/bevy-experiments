#!/usr/bin/env python3
"""Exercise built server/client processes using loopback only; no data downloads.

Build first: cargo build -p terra-net --bins
Run: python3 crates/terra-net/tests/process_smoke.py
Alternate binaries: --server /path/terra-server --client /path/terra-client
"""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time


def main():
    root = Path(__file__).resolve().parents[3]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", type=Path, default=root / "target/debug/terra-server")
    parser.add_argument("--client", type=Path, default=root / "target/debug/terra-client")
    args = parser.parse_args()
    server_bin, client_bin = str(args.server.resolve()), str(args.client.resolve())
    children = []
    logs = []
    results = {}
    with tempfile.TemporaryDirectory(prefix="terra-process-smoke-") as temp:
        work = Path(temp)
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            address = f"127.0.0.1:{probe.getsockname()[1]}"

        def start_server(label, arguments, ticks=60):
            log_path = work / f"{label}.log"
            log = log_path.open("w")
            logs.append(log)
            child = subprocess.Popen(
                [server_bin, "--bind", address, "--ticks", str(ticks), *map(str, arguments)],
                cwd=work, stdout=log, stderr=subprocess.STDOUT,
            )
            children.append(child)
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if "listening on" in log_path.read_text():
                    return child
                if child.poll() is not None:
                    raise AssertionError(f"{label} failed before listening: {log_path.read_text()}")
                time.sleep(0.01)
            raise AssertionError(f"{label} did not become ready")

        def start_client(arguments):
            child = subprocess.Popen(
                [client_bin, "--connect", address, "--seconds", "0.5", *map(str, arguments)],
                cwd=work, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            )
            children.append(child)
            return child

        def finish_client(child, expected_ok=True):
            out, err = child.communicate(timeout=10)
            if expected_ok:
                assert child.returncode == 0, f"client failed: {out} {err}"
                assert "Snapshot tick=" in out and "authoritative snapshots" in out
            else:
                assert child.returncode != 0 and "does not match" in err, err
            return out

        def finish_server(child):
            assert child.wait(timeout=12) == 0, "server failed to finish cleanly"

        def reject_server(arguments, message):
            run = subprocess.run(
                [server_bin, "--bind", address, "--ticks", "0", *map(str, arguments)],
                cwd=work, capture_output=True, text=True, timeout=10,
            )
            assert run.returncode != 0 and message in run.stderr, run.stderr

        try:
            world = work / "server.json"
            craft = work / "craft.json"
            craft.write_text(json.dumps({"type": "craft", "recipe": "planks", "count": 2}))
            initial = start_server("procedural", ["--save", world])
            clients = [start_client(["--session", work / f"player-{name}.json", "--command", craft]) for name in ("a", "b")]
            for child in clients:
                finish_client(child)
            finish_server(initial)
            saved = json.loads(world.read_text())
            players = saved["sandbox"]["players"]
            assert len(players) == 2
            assert all(player["inventory"].get("planks") == 8 for player in players.values())
            player_ids = set(players)
            # The server ingests neighbours, not just its initial spawn tile.
            assert len(saved["sandbox"]["world_features"]) > 64
            results["procedural_two_clients"] = True

            craft.write_text(json.dumps({"type": "craft", "recipe": "planks", "count": 1}))
            resumed = start_server("resume", ["--load", world, "--save", world])
            clients = [start_client(["--session", work / f"player-{name}.json", "--command", craft]) for name in ("a", "b")]
            for child in clients:
                finish_client(child)
            finish_server(resumed)
            saved = json.loads(world.read_text())
            players = saved["sandbox"]["players"]
            assert set(players) == player_ids
            assert all(player["inventory"].get("planks") == 12 for player in players.values())
            assert set(saved["sandbox_sequences"].values()) == {2}
            results["save_restart_two_identities"] = True

            data = root / "crates/terra-history/data"
            pack = data / "synthetic-era-b/manifest.json"
            politics = data / "synthetic-politics-eras.geojson"
            pack_id = json.loads(pack.read_text())["id"]
            politics_id = json.loads(politics.read_text())["terra"]["id"]
            historical_save = work / "historical.json"
            history_args = ["--world-pack", pack, "--epoch", "2000-01-01", "--politics", politics]
            historical = start_server("historical", [*history_args, "--save", historical_save], ticks=80)
            manifest = {
                "world_id": "terra-reference-world", "seed": 42,
                "base_data_revision": pack_id, "terrain_provider": "historical-pack",
                "chunk_level": 15, "terrain_resolution": 17,
                "data_epoch_unix": 946684800, "political_dataset_revision": politics_id,
            }
            expected = work / "manifest.json"
            expected.write_text(json.dumps(manifest))
            finish_client(start_client(["--expect-manifest", expected]))
            results["historical_pack_politics_client"] = True
            for field, wrong in [("data_epoch_unix", 978307200), ("base_data_revision", "wrong-pack"), ("political_dataset_revision", "wrong-politics")]:
                mismatched = dict(manifest, **{field: wrong})
                expected.write_text(json.dumps(mismatched))
                finish_client(start_client(["--expect-manifest", expected]), expected_ok=False)
            finish_server(historical)
            historical_state = json.loads(historical_save.read_text())
            assert len(historical_state["sandbox"]["players"]) == 1
            assert historical_state["manifest"]["data_epoch_unix"] == 946684800
            assert historical_state["clock"]["unix_seconds"] > 946684800
            results["mismatched_epoch_pack_politics_rejected"] = True
            reject_server(["--world-pack", pack], "requires an explicit --epoch")
            reject_server(["--epoch", "2000-01-01"], "requires --world-pack")
            reject_server(["--world-pack", pack, "--epoch", "1950-01-01"], "outside the historical pack")
            reject_server([*history_args, "--lat", "0", "--lon", "0"], "outside historical terrain coverage")
            reject_server([*history_args, "--level", "16"], "differs from the historical pack")
            reject_server(["--world-pack", pack, "--epoch", "2001-01-01", "--politics", politics, "--load", historical_save], "manifest mismatch")
            results["invalid_history_startup_and_resume_rejected"] = True
            if hasattr(os, "mkfifo"):
                fifo = work / "not-a-file"
                os.mkfifo(fifo)
                reject_server(["--load", fifo], "regular file")
                for flag in ["--command", "--session", "--expect-manifest"]:
                    run = subprocess.run([client_bin, flag, str(fifo)], cwd=work, capture_output=True, text=True, timeout=3)
                    assert run.returncode != 0 and "regular file" in run.stderr, run.stderr
                results["special_file_inputs_rejected_without_blocking"] = True
            print(json.dumps(results, indent=2))
        finally:
            for child in children:
                if child.poll() is None:
                    child.terminate()
                    try:
                        child.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait(timeout=3)
            for log in logs:
                log.close()


if __name__ == "__main__":
    main()
