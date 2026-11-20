#!/usr/bin/env python3
"""Run the repository's release gates and retain reproducible local evidence.

Requires Python 3.11+, the pinned Rust toolchain, native build dependencies,
and permission to bind localhost for the real TCP/TLS tests. No window is
opened. Native interaction and hardware performance need separate review.
"""

from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import io
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
import tomllib

from release_contract import validate_gate_names, validate_gate_results, validate_rust_test_gate_names
from verify_desktop import validate_receipt_execution


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_GATE_TIMEOUT_SECONDS = 3600.0
DEFAULT_GATE_OUTPUT_LIMIT_BYTES = 64 * 1024 * 1024
MAX_GATE_OUTPUT_LIMIT_BYTES = 512 * 1024 * 1024
MAX_CAPTURE_TRAILER_BYTES = 2048


def utc() -> str:
    return datetime.now(timezone.utc).isoformat()


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def source_inputs() -> dict[str, str]:
    """Include Rust, embedded fixtures, source data, assets and verification code."""
    inputs = [ROOT / name for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml")]
    for directory in ("crates", "configs", "scripts", ".github/workflows", ".cargo"):
        inputs.extend(
            path
            for path in (ROOT / directory).rglob("*")
            if path.is_file()
            and not {"target", "__pycache__", ".git"}.intersection(path.relative_to(ROOT).parts)
            and path.suffix not in (".md", ".pyc")
        )
    return {str(path.relative_to(ROOT)).replace(os.sep, "/"): digest(path) for path in sorted(inputs)}


def write_json(path: Path, value: object) -> None:
    with tempfile.TemporaryFile() as content:
        content.write((json.dumps(value, indent=2) + "\n").encode("utf-8"))
        publish_file(content, path)



def publish_completion(path: Path, report: dict) -> None:
    """Publish a terminal receipt once and require exact immediate readback.

    This does not authenticate a publisher or guarantee future file retention.
    Failed publication is retained for diagnosis, never silently overwritten.
    """
    if report.get("status") not in {"passed", "failed"}:
        raise ValueError("Completion receipt requires a terminal status")
    expected = (json.dumps(report, indent=2) + "\n").encode("utf-8")
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=f".{path.name}.", delete=False) as staged:
            temporary = Path(staged.name)
            staged.write(expected)
            staged.flush()
            os.fsync(staged.fileno())
        # Exclusive even if a competing path appeared after preparation.
        os.link(temporary, path)
        if path.read_bytes() != expected:
            raise ValueError("Published completion receipt differs from final captured bytes")
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def publish_terminal_report(directory: Path, report: dict) -> None:
    """Keep the legacy progress alias and a separately named terminal receipt."""
    report["completion_record"] = "completed-gates.json"
    expected = (json.dumps(report, indent=2) + "\n").encode("utf-8")
    alias = directory / "gates.json"
    write_json(alias, report)
    if alias.read_bytes() != expected:
        raise ValueError("Final gates.json readback differs from captured terminal report")
    publish_completion(directory / report["completion_record"], report)


def publish_file(content, path: Path) -> None:
    """Publish a complete snapshot; never expose a child process's live output fd."""
    content.seek(0)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=f".{path.name}.", delete=False) as output:
            temporary = Path(output.name)
            shutil.copyfileobj(content, output)
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def positive_timeout(value: str) -> float:
    """Argparse's float conversion alone admits infinity and NaN."""
    try:
        seconds = float(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("timeout must be a positive finite number") from error
    if not math.isfinite(seconds) or seconds <= 0:
        raise argparse.ArgumentTypeError("timeout must be a positive finite number")
    return seconds


def positive_output_limit(value: str) -> int:
    try:
        limit = int(value)
    except (ValueError, TypeError) as error:
        raise argparse.ArgumentTypeError("output limit must be an integer byte count") from error
    if not 1 <= limit <= MAX_GATE_OUTPUT_LIMIT_BYTES:
        raise argparse.ArgumentTypeError(f"output limit must be 1..{MAX_GATE_OUTPUT_LIMIT_BYTES} bytes")
    return limit


def process_group_completion(process: subprocess.Popen) -> dict:
    """Observe the owned POSIX group; deliberately detached work is outside it."""
    if os.name == "posix":
        try:
            os.killpg(process.pid, 0)
        except ProcessLookupError:
            return {"status": "owned_process_group_absent"}
        except OSError as error:
            return {"status": "unconfirmed", "detail": str(error)}
        return {"status": "owned_process_group_present"}
    return {
        "status": "unconfirmed",
        "detail": "Leader exit and output closure do not enumerate redirected descendants on this platform.",
    }


def stop_process_tree(process: subprocess.Popen) -> dict:
    """Request owned-tree termination, then observe with finite cleanup waits."""
    request = "leader termination requested; descendant cleanup unconfirmed"
    if os.name == "posix":
        try:
            os.killpg(process.pid, signal.SIGKILL)
            request = "owned process group termination requested"
        except ProcessLookupError:
            request = "owned process group already absent"
        except OSError as error:
            request = f"owned process group termination unconfirmed: {error}"
    elif os.name == "nt":
        try:
            subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                timeout=10, check=True,
            )
            request = "owned process tree termination requested"
        except (OSError, subprocess.SubprocessError):
            pass
    if process.poll() is None:
        try:
            process.kill()
        except ProcessLookupError:
            pass
        except OSError as error:
            request += f"; leader termination unconfirmed: {error}"
    try:
        process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        pass
    return {"request": request, "leader_exit_observed": process.poll() is not None,
            "descendant_check": process_group_completion(process)}


class LoggedProcess(subprocess.CompletedProcess):
    """Completed command plus the runner's separately observed lifetime evidence."""
    def __init__(self, command: list[str], returncode: int, execution: dict):
        super().__init__(command, returncode)
        self.execution = execution


def run_logged(
    command: list[str], env: dict[str, str], log: Path,
    timeout_seconds: float = DEFAULT_GATE_TIMEOUT_SECONDS,
    output_limit_bytes: int = DEFAULT_GATE_OUTPUT_LIMIT_BYTES,
) -> tuple[LoggedProcess, bytes, bool]:
    if not math.isfinite(timeout_seconds) or timeout_seconds <= 0:
        raise ValueError("timeout must be a positive finite number")
    if type(output_limit_bytes) is not int or not 1 <= output_limit_bytes <= MAX_GATE_OUTPUT_LIMIT_BYTES:
        raise ValueError(f"output limit must be 1..{MAX_GATE_OUTPUT_LIMIT_BYTES} integer bytes")
    data = bytearray()
    finished, overflow = threading.Event(), threading.Event()
    read_errors: list[Exception] = []
    process = None
    reader = None
    failure = None
    detail = ""
    timed_out = False
    returncode = 125
    cleanup = {"status": "not_needed"}
    descendant_check = {"status": "not_started"}
    started = time.monotonic()

    def capture() -> None:
        try:
            while True:
                chunk = process.stdout.read(min(65536, output_limit_bytes + 1 - len(data)))
                if not chunk:
                    break
                data.extend(chunk)
                if len(data) > output_limit_bytes:
                    overflow.set()
                    break
        except (OSError, ValueError) as error:
            read_errors.append(error)
        finally:
            process.stdout.close()
            finished.set()

    try:
        try:
            # A pipe applies backpressure; no unlimited disk spool exists. The
            # unbuffered reader retains at most the allowance + overflow byte.
            process = subprocess.Popen(
                command, cwd=ROOT, env=env, stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0,
                start_new_session=os.name == "posix",
                creationflags=getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0) if os.name == "nt" else 0,
            )
        except OSError as error:
            failure, detail, returncode = "spawn_failed", f"command could not start: {error}", 127
        if process is not None:
            reader = threading.Thread(target=capture, name="terra-gate-output", daemon=True)
            reader.start()
            while process.poll() is None or not finished.is_set():
                if overflow.is_set():
                    failure, detail = "output_limit", f"output exceeded its {output_limit_bytes}-byte limit"
                    break
                remaining = timeout_seconds - (time.monotonic() - started)
                if remaining <= 0:
                    timed_out = True
                    failure, detail, returncode = "timeout", f"timed out after {timeout_seconds:g} seconds, including output completion", 124
                    break
                time.sleep(min(0.01, remaining))
            if failure is None and overflow.is_set():
                failure, detail = "output_limit", f"output exceeded its {output_limit_bytes}-byte limit"
            if failure is None and read_errors:
                failure, detail = "output_read_error", f"output could not be read: {read_errors[0]}"
            if failure is None:
                returncode = process.returncode
                descendant_check = process_group_completion(process)
                if returncode != 0:
                    failure = "nonzero_exit"
                elif os.name == "posix" and descendant_check["status"] != "owned_process_group_absent":
                    failure, detail, returncode = "owned_descendants", "owned process-group completion was not confirmed after leader exit and output closure", 125
            if failure is not None:
                cleanup = stop_process_tree(process)
                reader.join(timeout=1)
                cleanup["output_closed"] = finished.is_set()
                if descendant_check["status"] == "not_started":
                    descendant_check = cleanup["descendant_check"]
    except BaseException:
        failure, detail = "interrupted", "execution interrupted; retained output is partial"
        if process is not None:
            cleanup = stop_process_tree(process)
            if reader is not None and reader.ident is not None:
                reader.join(timeout=1)
        raise
    finally:
        if process is not None and process.stdout is not None:
            # FileIO has no buffered-reader lock to wait for. An unclosed pipe
            # cannot make the daemon reader hold this command open indefinitely.
            process.stdout.close()
        captured = bytes(data[:output_limit_bytes])
        if detail:
            trailer = f"\nTerra verification {detail}; cleanup={cleanup}\n".encode("utf-8")
            captured += trailer[:MAX_CAPTURE_TRAILER_BYTES]
        # Publish privately captured bytes after the observation, also on
        # failure. A synchronized workspace never sees a child's live fd.
        publish_file(io.BytesIO(captured), log)
    execution = {
        "status": "passed" if returncode == 0 and failure is None else "failed",
        "process_platform": os.name,
        "failure_reason": failure, "output_limit_bytes": output_limit_bytes,
        "raw_output_bytes_observed": len(data), "retained_log_bytes": len(captured),
        "output_overflow": overflow.is_set(),
        "output_complete": finished.is_set() and not overflow.is_set() and not read_errors,
        "leader_exit_observed": process is not None and process.poll() is not None,
        "descendant_check": descendant_check, "cleanup": cleanup,
        "scope": "Owned POSIX group; deliberately detached descendants are outside this check. Other platforms establish leader exit and output closure only.",
    }
    return LoggedProcess(command, returncode, execution), captured, timed_out


def location(path: Path) -> str:
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return str(path)


def command_report(body: str) -> dict:
    start = re.search(r"^\{", body, re.MULTILINE)
    if start is None:
        raise ValueError("Command did not emit its JSON acceptance report")
    value = json.loads(body[start.start():])
    if not isinstance(value, dict):
        raise ValueError("Acceptance report must be a JSON object")
    return value


def test_identities(body: str, packages: list[dict], selected: set[str]) -> set[tuple[str, str, str]]:
    # Cargo prints the actual harness executable before its test output. Resolve
    # its target through metadata and this gate's package selection, so identical
    # integration-test names in different crates cannot be conflated.
    targets: dict[str, set[str]] = {}
    for package in packages:
        if package["name"] in selected:
            for target in package["targets"]:
                targets.setdefault(target["name"].replace("-", "_"), set()).add(package["name"])
    identities: set[tuple[str, str, str]] = set()
    current: tuple[str, str] | None = None
    for line in body.splitlines():
        running = re.search(r"Running .+\((?:.*[/\\])?([^/\\ )]+)-[a-f0-9]+(?:\.exe)?\)", line)
        doctests = re.search(r"Doc-tests (\S+)", line)
        if running:
            harness = running.group(1)
            candidates = targets.get(harness, set())
            if len(candidates) != 1:
                raise ValueError(f"Cannot identify test harness {harness}: {sorted(candidates)}")
            current = (next(iter(candidates)), harness)
        elif doctests:
            crate = doctests.group(1).replace("_", "-")
            if crate not in selected:
                raise ValueError(f"Unexpected doctest package: {crate}")
            current = (crate, "doctest")
        passed = re.match(r"test (.+) \.\.\. ok$", line)
        if passed:
            if current is None:
                raise ValueError(f"Test output without a known harness: {line}")
            identities.add((*current, passed.group(1)))
    return identities


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true", help="Use only cached Cargo dependencies")
    parser.add_argument("--jobs", type=int, default=None, help="Limit Cargo build jobs (positive integer)")
    parser.add_argument(
        "--gate-timeout-seconds", type=positive_timeout, default=DEFAULT_GATE_TIMEOUT_SECONDS,
        help="Deadline for each acceptance command; positive finite seconds (default: 3600)",
    )
    parser.add_argument(
        "--gate-output-limit-bytes", type=positive_output_limit, default=DEFAULT_GATE_OUTPUT_LIMIT_BYTES,
        help="Maximum stdout/stderr bytes per command, 1..536870912 (default: 67108864); overflow fails the gate",
    )
    parser.add_argument("--report-dir", type=Path, help="New evidence directory; default: repository target/verification/<UTC timestamp>")
    args = parser.parse_args()
    if args.jobs is not None and args.jobs < 1:
        parser.error("--jobs must be positive")
    env = os.environ.copy()
    env["CARGO_TERM_COLOR"] = "never"
    if args.jobs is not None:
        env["CARGO_BUILD_JOBS"] = str(args.jobs)
    flags = ["--locked"] + (["--offline"] if args.offline else [])
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    report_dir = (args.report_dir or ROOT / "target" / "verification" / timestamp).resolve()
    if report_dir.exists() and (not report_dir.is_dir() or any(report_dir.iterdir())):
        parser.error(f"Evidence directory must be new or empty: {report_dir}")
    for source_dir in (ROOT / "crates", ROOT / "configs", ROOT / "scripts", ROOT / ".github", ROOT / ".cargo"):
        if report_dir == source_dir or source_dir in report_dir.parents:
            parser.error("Evidence must be outside source/input directories")
    if report_dir == ROOT or report_dir in ROOT.parents:
        parser.error("Evidence directory must not contain the repository")
    report_dir.mkdir(parents=True, exist_ok=True)
    try:
        with (report_dir / "run.claim").open("x", encoding="utf-8") as claim:
            claim.write(f"{utc()} pid={os.getpid()}\n")
    except FileExistsError:
        parser.error("Another invocation already claimed this evidence directory")
    recovery_path = report_dir / "recovery.json"
    recovery_bundle_path = report_dir / "recovery-bundle.json"
    sessions_path = report_dir / "regional-sessions.json"
    source_work = tempfile.TemporaryDirectory(prefix="terra-verified-sources-")
    exported_source = Path(source_work.name) / "exported"
    installed_source = Path(source_work.name) / "installed"
    authored_pack = Path(source_work.name) / "authored-pack"
    vehicle_authored_pack = Path(source_work.name) / "vehicle-authored-pack"
    character_authored_pack = Path(source_work.name) / "character-authored-pack"
    source_fixture = "crates/terra-stream/data/heightmap-connected/manifest.json"
    report = {
        "version": None, "started_utc": utc(), "status": "starting",
        "environment": {"os": platform.system(), "architecture": platform.machine(),
                        "python": platform.python_version(), "cargo_build_jobs": env.get("CARGO_BUILD_JOBS", "default"),
                        "cargo_build_target_environment": env.get("CARGO_BUILD_TARGET"),
                        "offline": args.offline, "gate_timeout_seconds": args.gate_timeout_seconds,
                        "gate_output_limit_bytes": args.gate_output_limit_bytes},
        "scope": "Local TCP/TLS with prepared terrain and 1024 cold overlay regions; bounded autonomous geometry including narrow corridors and restored support, live owner commitments, finite commercial careers, named managed and complete local recovery, portable sources, original animated human/vehicle/wildlife packs, retained diagnostics, native code and build. Native interaction, hardware/platform qualification and uncoached player experience require separate review.",
        "source_inputs": 0, "gates": [], "evidence_artifacts": {},
    }
    def publish_report(path: Path, value: object) -> None:
        expected = (json.dumps(value, indent=2) + "\n").encode("utf-8")
        write_json(path, value)
        report["evidence_artifacts"][path.relative_to(report_dir).as_posix()] = hashlib.sha256(expected).hexdigest()

    def startup_probe(name: str, command: list[str]) -> str:
        """Startup work has the same owned-tree and retained-output contract."""
        log = report_dir / f"startup-{name}.log"
        timeout = min(args.gate_timeout_seconds, 120.0)
        start = time.monotonic()
        result, captured, timed_out = run_logged(command, env, log, timeout, args.gate_output_limit_bytes)
        captured_hash = hashlib.sha256(captured).hexdigest()
        report.setdefault("startup_probes", []).append({
            "name": name, "command": command, "exit_code": result.returncode,
            "elapsed_seconds": round(time.monotonic() - start, 3),
            "log": log.name, "log_sha256": captured_hash,
            "timed_out": timed_out, "timeout_seconds": timeout, "execution": result.execution,
        })
        report["evidence_artifacts"][log.name] = captured_hash
        write_json(report_dir / "gates.json", report)
        if result.returncode:
            reason = f"exceeded its {timeout:g}-second deadline" if timed_out else f"exited {result.returncode}"
            raise ValueError(f"Startup probe {name} {reason}; see {log.name}")
        return captured.decode("utf-8", errors="replace")

    write_json(report_dir / "gates.json", report)
    identities: set[tuple[str, str, str]] = set()
    initial_inputs = None
    gates = []
    code = 0
    try:
        metadata = json.loads(startup_probe(
            "metadata", ["cargo", "metadata", "--no-deps", "--format-version", "1", *flags],
        ))
        packages = metadata["packages"]
        workspace = {p["name"] for p in packages if p["id"] in metadata["workspace_members"]}
        report["version"] = next(p["version"] for p in packages if p["name"] == "terra-demo")
        report["toolchain"] = startup_probe("toolchain", ["rustc", "--version"]).strip()
        pinned = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
        actual = re.fullmatch(r"rustc (\S+) \(.+\)", report["toolchain"])
        if actual is None or actual.group(1) != pinned:
            raise ValueError(f"Actual compiler {report['toolchain']} differs from pinned acceptance toolchain {pinned}")
        initial_inputs = source_inputs()
        report["source_inputs"] = len(initial_inputs)
        report["status"] = "running"
        publish_report(report_dir / "source-hashes.json", initial_inputs)
        write_json(report_dir / "gates.json", report)
        career_name = "six_session_warehouse_career_conserves_finite_goods_cash_and_recovers_scarcity"
        purchased_career_name = "purchased_stock_career_accounts_for_acquisition_setup_and_recovery_without_inventing_profit"
        novice_career_name = "finite_novice_passenger_career_prices_rental_costs_and_resumes_for_one_physical_hour"
        source_inspection = None
        source_package_identity = None
        authored_pack_identity = None
        vehicle_authored_pack_identity = None
        character_authored_pack_identity = None
        cluster_test = "autonomous_vehicle_passes_a_cluster_without_moving_parked_bodies_or_losing_its_load"
        launcher_recovery_test = "launcher::recovery::tests::managed_recovery_journey_preserves_complete_world_after_original_loss"
        world_names_test = "launcher::recovery::tests::named_recovery_copies_and_rename_preserve_progress_and_survive_relisting"
        guidance_continuity_test = "continuing_guidance_journey_preserves_obligations_and_rebuilds_after_resume"
        gates = [
            ("compatibility", [sys.executable, "scripts/check_compatibility.py"], set()),
            ("desktop-packaging-tests", [sys.executable, "-m", "unittest", "discover", "-s", "scripts", "-p", "test_*.py", "-v"], set()),
            ("format", ["cargo", "fmt", "--all", "--", "--check"], set()),
            ("headless", ["cargo", "test", "--workspace", "--exclude", "terra-demo", "--exclude", "terra-bevy", *flags], workspace - {"terra-demo", "terra-bevy"}),
            ("career-report", ["cargo", "test", "-p", "terra-sandbox", "--test", "company_depots", career_name, *flags, "--", "--exact", "--nocapture"], {"terra-sandbox"}),
            ("purchased-career-report", ["cargo", "test", "-p", "terra-sandbox", "--test", "company_depots", purchased_career_name, *flags, "--", "--exact", "--nocapture"], {"terra-sandbox"}),
            ("novice-career-report", ["cargo", "test", "-p", "terra-sandbox", "--test", "company_career", novice_career_name, *flags, "--", "--exact", "--nocapture"], {"terra-sandbox"}),
            ("traffic-recovery", ["cargo", "test", "-p", "terra-sandbox", "--test", "traffic_recovery", *flags, "--", "--nocapture"], {"terra-sandbox"}),
            ("navigation-cluster", ["cargo", "test", "-p", "terra-sandbox", "--test", "navigation_clusters", cluster_test, *flags, "--", "--exact", "--nocapture"], {"terra-sandbox"}),
            ("navigation-geometry", ["cargo", "test", "-p", "terra-sandbox", "--test", "navigation_geometry", *flags, "--", "--nocapture"], {"terra-sandbox"}),
            ("guidance-continuity", ["cargo", "test", "-p", "terra-sandbox", "--test", "guidance_commitments", guidance_continuity_test, *flags, "--", "--exact", "--nocapture"], {"terra-sandbox"}),
            ("bevy-headless", ["cargo", "test", "-p", "terra-bevy", "--no-default-features", *flags], {"terra-bevy"}),
            ("bevy-render", ["cargo", "test", "-p", "terra-bevy", "--lib", *flags], {"terra-bevy"}),
            ("demo", ["cargo", "test", "-p", "terra-demo", *flags], {"terra-demo"}),
            ("launcher-recovery", ["cargo", "test", "-p", "terra-demo", launcher_recovery_test, *flags, "--", "--exact", "--nocapture"], {"terra-demo"}),
            ("launcher-world-names", ["cargo", "test", "-p", "terra-demo", world_names_test, *flags, "--", "--exact", "--nocapture"], {"terra-demo"}),
            ("demo-check", ["cargo", "check", "-p", "terra-demo", *flags], set()),
            ("headless-example", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--example", "headless", *flags], set()),
            ("custom-provider-example", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--example", "custom_provider", *flags], set()),
            ("asset-pack", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--bin", "terra-assets", *flags, "--", "validate", "crates/terra-bevy/assets/wildlife-pack/pack.json"], set()),
            ("asset-inspect", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--bin", "terra-assets", *flags, "--", "inspect", "crates/terra-bevy/assets/wildlife-pack/deer.glb"], set()),
            ("asset-build", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--bin", "terra-assets", *flags, "--", "build", "crates/terra-bevy/assets/wildlife-authoring.recipe.json", str(authored_pack)], set()),
            ("asset-authored-validate", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--bin", "terra-assets", *flags, "--", "validate", str(authored_pack / "pack.json")], set()),
            ("vehicle-generator", [sys.executable, "crates/terra-bevy/assets/transport-pack/generate.py", "--check"], set()),
            ("vehicle-inspect", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--bin", "terra-assets", *flags, "--", "inspect", "crates/terra-bevy/assets/transport-pack/comet.glb"], set()),
            ("vehicle-pack", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--bin", "terra-assets", *flags, "--", "validate", "crates/terra-bevy/assets/transport-pack/pack.json"], set()),
            ("vehicle-demo-pack", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--bin", "terra-assets", *flags, "--", "validate", "crates/terra-bevy/assets/transport-demo.pack.json"], set()),
            ("vehicle-asset-build", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--bin", "terra-assets", *flags, "--", "build", "crates/terra-bevy/assets/transport-authoring.recipe.json", str(vehicle_authored_pack)], set()),
            ("vehicle-authored-validate", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--bin", "terra-assets", *flags, "--", "validate", str(vehicle_authored_pack / "pack.json")], set()),
            ("character-generator", [sys.executable, "crates/terra-bevy/assets/character-pack/generate.py", "--check"], set()),
            ("character-inspect", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--bin", "terra-assets", *flags, "--", "inspect", "crates/terra-bevy/assets/character-pack/fieldhand.glb"], set()),
            ("character-pack", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--bin", "terra-assets", *flags, "--", "validate", "crates/terra-bevy/assets/character-pack/pack.json"], set()),
            ("character-demo-pack", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--bin", "terra-assets", *flags, "--", "validate", "crates/terra-bevy/assets/character-demo.pack.json"], set()),
            ("character-asset-build", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--bin", "terra-assets", *flags, "--", "build", "crates/terra-bevy/assets/character-authoring.recipe.json", str(character_authored_pack)], set()),
            ("character-authored-validate", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--bin", "terra-assets", *flags, "--", "validate", str(character_authored_pack / "pack.json")], set()),
            ("source-inspect", ["cargo", "run", "-p", "terra-stream", "--bin", "terra-sources", *flags, "--", "inspect", source_fixture, "--require-complete"], set()),
            ("source-export", ["cargo", "run", "-p", "terra-stream", "--bin", "terra-sources", *flags, "--", "export", source_fixture, str(exported_source), "--require-complete"], set()),
            ("source-install", ["cargo", "run", "-p", "terra-stream", "--bin", "terra-sources", *flags, "--", "install", str(exported_source), str(installed_source), "--require-complete"], set()),
            ("source-verify", ["cargo", "run", "-p", "terra-stream", "--bin", "terra-sources", *flags, "--", "verify", str(installed_source), "--require-complete"], set()),
            ("recovery-probe", ["cargo", "run", "-p", "terra-persistence", "--example", "recovery_probe", *flags, "--", "--output", location(recovery_path)], set()),
            ("recovery-bundle", ["cargo", "run", "-p", "terra-bevy", "--no-default-features", "--example", "recovery_bundle_probe", *flags, "--", "--output", location(recovery_bundle_path)], set()),
            ("session-probe-checks", ["cargo", "test", "-p", "terra-net", "--example", "regional_session_load", *flags], {"terra-net"}),
            ("regional-sessions", ["cargo", "run", "-p", "terra-net", "--example", "regional_session_load", *flags, "--", "--clients", "16", "--cycles", "8", "--ticks", "400", "--cold-regions", "1024", "--transport", "tls", "--heightmap", source_fixture, "--output", location(sessions_path)], set()),
            ("native-build", ["cargo", "build", "-p", "terra-demo", "--message-format=json", *flags], set()),
        ]
        validate_gate_names([name for name, _, _ in gates])
        validate_rust_test_gate_names([name for name, _, selected in gates if selected])
        for name, command, selected in gates:
            if name in {"source-install", "source-verify"}:
                if not source_package_identity or not source_inspection:
                    raise ValueError("Source installation requires its reviewed package and source identities")
                command += ["--expect-package", source_package_identity, "--expect-source", source_inspection["source_identity"]]
            print(f"[{name}] running", flush=True)
            start = time.monotonic()
            log = report_dir / f"{name}.log"
            report["active_gate"] = {"name": name, "command": command, "started_utc": utc(), "log": log.name, "timeout_seconds": args.gate_timeout_seconds, "output_limit_bytes": args.gate_output_limit_bytes}
            write_json(report_dir / "gates.json", report)
            result, captured, timed_out = run_logged(command, env, log, args.gate_timeout_seconds, args.gate_output_limit_bytes)
            body = captured.decode("utf-8", errors="replace")
            summaries = [tuple(map(int, match)) for match in re.findall(
                r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored", body)]
            entry = {
                "name": name, "command": command, "exit_code": result.returncode,
                "elapsed_seconds": round(time.monotonic() - start, 3), "log": log.name,
                "log_sha256": hashlib.sha256(captured).hexdigest(), "passed_test_executions": sum(x[0] for x in summaries),
                "failed_tests": sum(x[1] for x in summaries), "ignored_tests": sum(x[2] for x in summaries),
                "timed_out": timed_out, "timeout_seconds": args.gate_timeout_seconds,
                "execution": result.execution,
            }
            report["gates"].append(entry)
            report.pop("active_gate", None)
            write_json(report_dir / "gates.json", report)
            print(f"[{name}] exit {result.returncode}; {entry['passed_test_executions']} tests passed; {entry['elapsed_seconds']}s", flush=True)
            if result.returncode or entry["failed_tests"] or entry["ignored_tests"]:
                if timed_out:
                    report["error"] = f"Gate {name} exceeded its {args.gate_timeout_seconds:g}-second deadline"
                print(body[-6000:], flush=True)
                code = result.returncode or 1
                break
            if selected:
                found = test_identities(body, packages, selected)
                if not found or len(found) != entry["passed_test_executions"]:
                    raise ValueError(f"{name}: identified {len(found)} of {entry['passed_test_executions']} test executions")
                identities.update(found)
                if name == "guidance-continuity":
                    if found != {("terra-sandbox", "guidance_commitments", guidance_continuity_test)}:
                        raise ValueError("Guidance gate did not execute its exact journey")
                    records = re.findall(r"^GUIDANCE_CONTINUITY_REPORT (\{[^\r\n]*\})", body, re.MULTILINE)
                    if len(records) != 1:
                        raise ValueError("Guidance gate must emit one continuity report")
                    journey = json.loads(records[0])
                    required = {"earliest_deadline_order", "exact_open_obligations_after_resume",
                                "return_preserves_goods_and_wallet", "duplicate_return_rejected_without_mutation",
                                "expired_request_returns_to_board", "stale_obligation_requires_review",
                                "remaining_obligation_preserved", "physical_delivery_without_position_override",
                                "exact_inventory_and_reserved_payment", "duplicate_payment_rejected_without_mutation",
                                "exact_completed_save_resume", "complete_catalog_validation"}
                    if (journey.get("case") != "three_obligations_return_expiry_physical_delivery_resume"
                            or journey.get("accepted_contracts") != 3
                            or any(journey.get(key) != 1 for key in ("explicit_returns", "missed_deadlines", "paid_deliveries"))
                            or journey.get("remaining_accepted_supply") != 0
                            or not 0 < journey.get("physical_walk_seconds", 0) <= 240
                            or journey.get("walk_distance_m", 0) <= 0
                            or not 0 < journey.get("maximum_walk_step_m", 0) <= 1
                            or journey.get("final_personal_wallet_minor", 0) != journey.get("starting_personal_wallet_minor", 0) + journey.get("gross_reward_minor", 0)
                            or not journey.get("scope") or set(journey.get("verification", {})) != required
                            or not all(value is True for value in journey["verification"].values())):
                        raise ValueError("Guidance did not prove its exact obligations, physical delivery and resumed state")
                    publish_report(report_dir / "guidance-continuity.json", journey)
                if name == "navigation-geometry":
                    records = re.findall(r"^NAVIGATION_GEOMETRY_REPORT (\{[^\r\n]*\})", body, re.MULTILINE)
                    matrix = sorted((json.loads(record) for record in records), key=lambda row: row["case"])
                    cases = {"parked_body_narrow_corridor", "flank_unavailable_then_restored", "unavailable_reconnection", "two_moving_crossings", "blocked_narrow_junction"}
                    required = {"physical_arrival", "bounded_motion", "supported_complete_envelopes",
                                "body_sweeps_clear", "goods_and_money_conserved", "source_work_bounded",
                                "complete_state_valid", "exact_save_resume"}
                    if len(matrix) != len(cases) or {row.get("case") for row in matrix} != cases:
                        raise ValueError("Navigation gate must emit the complete distinct geometry matrix")
                    for row in matrix:
                        metrics, limits = row.get("metrics", {}), row.get("limits", {})
                        if (not row.get("scope") or set(row.get("verification", {})) != required
                                or not all(value is True for value in row["verification"].values())
                                or limits.get("physical_step_seconds") != 0.1
                                or limits.get("extra_corridor_admissions_per_vehicle_update") != 12
                                or not 0 < metrics.get("physical_steps", 0) <= 12000
                                or abs(metrics.get("elapsed_physical_seconds", -1) - metrics["physical_steps"] * 0.1) > 1e-6
                                or not 0 < metrics.get("driver_max_step_m", 0) <= 0.801
                                or not 0 <= metrics.get("driver_max_heading_delta_deg", -1) <= 16.05
                                or not 1 <= metrics.get("max_path_waypoints", 0) <= 129
                                or not 0 < metrics.get("max_source_queries_per_update", 0) <= 32768
                                or metrics.get("exact_resumed_steps", 0) <= 0):
                            raise ValueError(f"Navigation geometry lacks bounded, conserved physical progress: {row.get('case')}")
                    publish_report(report_dir / "navigation-geometry.json", matrix)
                if name == "launcher-world-names":
                    if found != {("terra-demo", "terra_demo", world_names_test)}:
                        raise ValueError("World naming gate did not execute its exact recovery journey")
                    records = re.findall(r"^WORLD_NAMES_REPORT (\{[^\r\n]*\})", body, re.MULTILINE)
                    if len(records) != 1:
                        raise ValueError("World naming gate must emit one preservation report")
                    named = json.loads(records[0])
                    required = {"separate_world_folders", "distinct_chosen_names_after_relisting",
                                "only_selected_profile_changed", "only_title_field_changed",
                                "progress_source_and_backup_bytes_exact", "invalid_recovery_name_rejected_before_output"}
                    if (named.get("case") != "named_recovery_copies_and_metadata_only_rename"
                            or named.get("restored_copies") != 2 or named.get("equal_creation_timestamps") is not True
                            or named.get("files_changed_by_rename") != 1 or not named.get("scope")
                            or set(named.get("verification", {})) != required
                            or not all(value is True for value in named["verification"].values())):
                        raise ValueError("Named recovery did not prove metadata-only preservation and later identification")
                    publish_report(report_dir / "launcher-world-names.json", named)
                if name == "novice-career-report":
                    if found != {("terra-sandbox", "company_career", novice_career_name)}:
                        raise ValueError("Novice career gate did not execute its exact acceptance test")
                    records = re.findall(r"^NOVICE_PASSENGER_CAREER_REPORT (\{[^\r\n]*\})", body, re.MULTILINE)
                    if len(records) != 1:
                        raise ValueError("Novice career gate must produce exactly one measurement")
                    career = json.loads(records[0])
                    accounting = career.get("persisted_accounting", {})
                    checkpoints = career.get("save_resume_checkpoints", [])
                    cost_keys = ("registration_minor_credits", "recruitment_minor_credits",
                                 "payroll_minor_credits", "rental_lease_minor_credits",
                                 "fuel_cash_net_of_return_refund_minor_credits", "rental_damage_minor_credits")
                    if (career.get("service_physical_seconds") != 3600
                            or career.get("total_physical_seconds_including_completion_and_return", 0) < 3600
                            or career.get("ecology_clock_seconds") != 0
                            or career.get("initial_owner_minor_credits") != 25_000
                            or career.get("additional_owner_deposits_minor_credits") != 0
                            or career.get("deliveries", 0) < 12
                            or len(checkpoints) != 6
                            or [c.get("physical_seconds") for c in checkpoints] != [600, 1200, 1800, 2400, 3000, 3600]
                            or career.get("regional_accounting_resume_exact") is not True
                            or accounting.get("reconciled") is not True
                            or accounting.get("coverage") != "since_registration"
                            or accounting.get("cash") != career.get("final_company_minor_credits")
                            or accounting.get("cash") != accounting.get("expected_cash")
                            or career.get("unpriced_starter_vehicle_unused") is not True
                            or len(accounting.get("recent_entries", [])) > 64
                            or accounting.get("monetary_entry_count", -1) < len(accounting.get("recent_entries", []))
                            or accounting.get("totals", {}).get("public_fares") != career.get("gross_public_fares_minor_credits")
                            or accounting.get("totals", {}).get("payroll") != career.get("payroll_minor_credits")
                            or accounting.get("totals", {}).get("owner_deposit") != career.get("owner_deposits_minor_credits")
                            or any(type(career.get(key)) is not int or career[key] < 0 for key in cost_keys)
                            or "road_holds_waited_10_seconds_then_resumed" not in career
                            or not career.get("unpriced") or not career.get("limits")):
                        raise ValueError("Novice career lacks its exact duration, costs, finite funding, recovery or resume evidence")
                    known_net = career["gross_public_fares_minor_credits"] - sum(career[key] for key in cost_keys)
                    cash_net = career["final_owner_minor_credits"] + career["final_company_minor_credits"] - career["initial_owner_minor_credits"]
                    if (known_net <= 0 or career.get("net_after_all_known_cash_costs_minor_credits") != known_net
                            or cash_net != known_net or career.get("cash_and_escrow_conserved_minor_credits", 0) <= 0
                            or career.get("passengers_conserved", 0) <= 0
                            or career.get("bond_returned_minor_credits", 0) > career.get("bond_posted_minor_credits", -1)
                            or career.get("bond_posted_minor_credits", 0) - career.get("bond_returned_minor_credits", 0) != career.get("rental_damage_minor_credits")):
                        raise ValueError("Novice career cash, priced rental return and conservation do not reconcile")
                    publish_report(report_dir / f"{name}.json", career)
                if name in {"career-report", "purchased-career-report"}:
                    expected_name = career_name if name == "career-report" else purchased_career_name
                    if found != {("terra-sandbox", "company_depots", expected_name)}:
                        raise ValueError("Career gate did not execute its exact acceptance test")
                    prefix = "CAREER_REPORT" if name == "career-report" else "PURCHASED_CAREER_REPORT"
                    records = re.findall(rf"^{prefix} (\{{[^\r\n]*\}})", body, re.MULTILINE)
                    if len(records) != 1:
                        raise ValueError("Career gate must produce exactly one measurement")
                    career = json.loads(records[0])
                    if not isinstance(career, dict) or not {"sessions", "cash_conserved_minor_credits", "physical_seconds"} <= career.keys():
                        raise ValueError("Career measurement is missing its workload or conservation results")
                    accounting = career.get("persisted_accounting", {})
                    operating = accounting.get("operating", {})
                    capital = accounting.get("capital", {})
                    financing = accounting.get("financing", {})
                    if (career["sessions"] != 6 or career.get("deliveries") != 24
                            or career.get("save_resume_checkpoints") != 7
                            or accounting.get("reconciled") is not True
                            or accounting.get("coverage") != "since_registration"
                            or accounting.get("cash") != career.get("company_final_minor_credits")
                            or accounting.get("cash") != accounting.get("expected_cash")
                            or len(accounting.get("recent_entries", [])) != 64
                            or accounting.get("monetary_entry_count", 0) <= 64):
                        raise ValueError("Career report lacks its persisted, bounded, reconciled cash history")
                    before_funding = sum(flow["receipts"] - flow["payments"] for flow in (operating, capital))
                    owner_costs = accounting["owner_paid_operating"] + accounting["owner_paid_capital"]
                    if (accounting["cash_result_before_financing"] != before_funding
                            or accounting["cash_result_after_known_owner_costs"] != before_funding - owner_costs
                            or accounting["cash"] != accounting["opening_cash"] + before_funding + financing["receipts"] - financing["payments"]):
                        raise ValueError("Career financial classes do not reconcile to actual cash")
                    acquisition = career.get("source_acquisition_minor_credits")
                    if name == "purchased-career-report":
                        if not isinstance(acquisition, int) or acquisition <= 0 or accounting["totals"].get("inventory_purchases") != acquisition or accounting.get("unpriced_stock_in_units") != 0:
                            raise ValueError("Purchased-stock career does not record its acquisition cost")
                    elif acquisition is not None or accounting.get("unpriced_stock_in_units") != 96:
                        raise ValueError("Contributed-stock career must disclose its unknown acquisition cost")
                    publish_report(report_dir / f"{name}.json", career)
                if name == "launcher-recovery":
                    if found != {("terra-demo", "terra_demo", launcher_recovery_test)}:
                        raise ValueError("Launcher recovery did not run its exact managed-world journey")
                    records = re.findall(r"^LAUNCHER_RECOVERY_REPORT (\{[^\r\n]*\})", body, re.MULTILINE)
                    if len(records) != 1:
                        raise ValueError("Launcher recovery must emit one retained journey report")
                    recovered = json.loads(records[0])
                    required = {"original_world_and_inputs_removed", "complete_archive_exact",
                                "source_identity_retained", "political_input_exact",
                                "character_and_presentation_retained", "restored_profile_discoverable",
                                "checkpoint_present", "runtime_paths_absolute_and_rehomed",
                                "profile_published_after_complete_restore", "original_backup_unchanged",
                                "recovered_world_can_be_backed_up_again", "repeat_backup_uses_new_directory"}
                    if (recovered.get("schema_version") != 1 or recovered.get("source_generation") != 2
                            or recovered.get("players") != 2 or recovered.get("resources", 0) < 16
                            or recovered.get("living_bodies", 0) < 1 or recovered.get("nonempty_regions", 0) < 16
                            or not all(recovered.get(key) is True for key in required)):
                        raise ValueError("Launcher recovery did not retain complete state, profile and dependencies after original loss")
                    publish_report(report_dir / "launcher-recovery.json", recovered)
                if name == "navigation-cluster":
                    if found != {("terra-sandbox", "navigation_clusters", cluster_test)}:
                        raise ValueError("Cluster gate did not run its exact physical-progress case")
                    records = re.findall(r"^NAVIGATION_CLUSTER_REPORT (\{[^\r\n]*\})", body, re.MULTILINE)
                    if len(records) != 1:
                        raise ValueError("Cluster gate must emit one measured journey")
                    cluster = json.loads(records[0])
                    required = {"physical_arrival", "bounded_steps_and_steering", "supported_complete_envelopes",
                                "body_sweeps_clear", "parked_bodies_unchanged", "goods_and_money_conserved",
                                "source_work_bounded", "path_bound_retained", "exact_save_resume", "complete_state_valid"}
                    if (cluster.get("case") != "three_parked_bodies" or cluster.get("obstacle_count") != 3
                            or cluster.get("physical_step_seconds") != 0.1
                            or not 0 < cluster.get("physical_steps", 0) <= 1200
                            or not 0 < cluster.get("elapsed_physical_seconds", 0) <= 120
                            or abs(cluster["elapsed_physical_seconds"] - cluster["physical_steps"] * 0.1) > 1e-6
                            or not 60 <= cluster.get("distance_travelled_m", 0) < 160
                            or not 0 <= cluster.get("remaining_goal_distance_m", -1) < 1.5
                            or cluster.get("farthest_lateral_distance_m", 0) <= 10
                            or not 0 < cluster.get("max_step_distance_m", 0) <= 0.801
                            or not 0 <= cluster.get("max_heading_delta_deg", -1) <= 16.05
                            or not 1 <= cluster.get("max_path_waypoints", 0) <= 129
                            or cluster.get("fixture_source_query_ceiling_per_step") != 16_384
                            or not 0 < cluster.get("max_source_queries_per_step", 0) <= 16_384
                            or cluster.get("exact_resumed_steps", 0) <= 0
                            or set(cluster.get("verification", {})) != required
                            or not all(v is True for v in cluster["verification"].values())):
                        raise ValueError("Cluster journey did not prove its bounded physical arrival and exact continuation")
                    publish_report(report_dir / "navigation-cluster.json", cluster)
                if name == "traffic-recovery":
                    records = re.findall(r"TRAFFIC_RECOVERY_REPORT (\{[^\r\n]*\})", body)
                    if len(records) != 2:
                        raise ValueError("Traffic recovery gate must emit both historical replay measurements")
                    traffic = [json.loads(record) for record in records]
                    if {record.get("fixture") for record in traffic} != {"rear-trap", "occupied-exit"}:
                        raise ValueError("Traffic report did not identify both exact historical fixtures")
                    for record in traffic:
                        if (record.get("replay_window_seconds") != 120
                                or not 0 < record.get("return_physical_seconds", 0) <= 120
                                or record.get("original_delivery", {}).get("amount") != 12
                                or record.get("original_delivery", {}).get("item") != "wood"
                                or record.get("goods_total") != 770
                                or sum(record.get("goods_by_item", {}).values()) != 770
                                or record.get("money_total_minor_credits") != 150000
                                or not all(record.get(key) is True for key in ("every_step_conservation_exact", "source_return_transfer_exact", "save_resume_exact"))):
                            raise ValueError("Historical traffic did not physically return and conserve its exact load")
                    publish_report(report_dir / "traffic-recovery.json", traffic)
            if name == "asset-pack":
                accepted = command_report(body)
                if accepted.get("accepted") is not True or not accepted.get("models"):
                    raise ValueError("Asset gate did not produce a populated acceptance report")
                publish_report(report_dir / "asset-pack.json", accepted)
            if name == "asset-inspect":
                inspected = command_report(body)
                model = inspected.get("model", {})
                if (inspected.get("accepted") is not True
                        or model.get("sha256") != digest(ROOT / "crates/terra-bevy/assets/wildlife-pack/deer.glb")
                        or {clip.get("name") for clip in model.get("clips", [])} != {"idle", "walk", "run", "feed", "drink"}
                        or model.get("budget", {}).get("bytes", 0) <= 0):
                    raise ValueError("GLB inspection did not verify the original model and named animations")
                publish_report(report_dir / "asset-inspect.json", inspected)
            if name == "asset-build":
                built = command_report(body)
                if (built.get("accepted") is not True or len(built.get("sources", [])) != 5
                        or built.get("recipe_sha256") != digest(ROOT / "crates/terra-bevy/assets/wildlife-authoring.recipe.json")
                        or json.loads((authored_pack / "authoring-report.json").read_text()) != built):
                    raise ValueError("Asset authoring report does not match the recipe and published pack")
                authored_pack_identity = built.get("identity")
                if not authored_pack_identity:
                    raise ValueError("Authored pack has no immutable identity")
                publish_report(report_dir / "asset-build.json", built)
            if name == "asset-authored-validate":
                accepted = command_report(body)
                if (accepted.get("accepted") is not True or len(accepted.get("models", [])) != 5
                        or not authored_pack_identity or accepted.get("identity") != authored_pack_identity):
                    raise ValueError("Runtime importer did not accept the exact recipe-built pack")
                publish_report(report_dir / "asset-authored-validate.json", accepted)
            if name == "vehicle-generator":
                generated = command_report(body)
                if (generated.get("status") != "matched" or generated.get("files") != 4
                        or generated.get("glb_sha256") != digest(ROOT / "crates/terra-bevy/assets/transport-pack/comet.glb")
                        or generated.get("glb_bytes") != (ROOT / "crates/terra-bevy/assets/transport-pack/comet.glb").stat().st_size):
                    raise ValueError("Vehicle generator did not reproduce every original example output")
                publish_report(report_dir / "vehicle-generator.json", generated)
            if name == "vehicle-inspect":
                inspected = command_report(body)
                model = inspected.get("model", {})
                if (inspected.get("accepted") is not True
                        or model.get("sha256") != digest(ROOT / "crates/terra-bevy/assets/transport-pack/comet.glb")
                        or {clip.get("name") for clip in model.get("clips", [])} != {"parked", "rolling"}
                        or model.get("budget", {}).get("bytes", 0) <= 0):
                    raise ValueError("Vehicle inspection did not accept its exact model and named clips")
                publish_report(report_dir / "vehicle-inspect.json", inspected)
            if name in {"vehicle-pack", "vehicle-demo-pack"}:
                accepted = command_report(body)
                expected = {"comet"} if name == "vehicle-pack" else {"comet", "deer", "ibex", "boar", "fox", "hare"}
                if (accepted.get("accepted") is not True
                        or {model.get("id") for model in accepted.get("models", [])} != expected
                        or not accepted.get("identity")):
                    raise ValueError("Vehicle demonstration pack did not retain its exact model set")
                publish_report(report_dir / f"{name}.json", accepted)
            if name == "vehicle-asset-build":
                built = command_report(body)
                if (built.get("accepted") is not True or len(built.get("sources", [])) != 6
                        or built.get("recipe_sha256") != digest(ROOT / "crates/terra-bevy/assets/transport-authoring.recipe.json")
                        or json.loads((vehicle_authored_pack / "authoring-report.json").read_text()) != built):
                    raise ValueError("Vehicle authoring receipt differs from the six-model recipe and published pack")
                vehicle_authored_pack_identity = built.get("identity")
                if not vehicle_authored_pack_identity:
                    raise ValueError("Vehicle authored pack has no immutable identity")
                publish_report(report_dir / "vehicle-asset-build.json", built)
            if name == "vehicle-authored-validate":
                accepted = command_report(body)
                if (accepted.get("accepted") is not True or len(accepted.get("models", [])) != 6
                        or not vehicle_authored_pack_identity or accepted.get("identity") != vehicle_authored_pack_identity):
                    raise ValueError("Runtime did not accept the exact authored vehicle/wildlife pack")
                publish_report(report_dir / "vehicle-authored-validate.json", accepted)
            if name == "character-generator":
                generated = command_report(body)
                generated_files = generated.get("files", [])
                expected = {"character-pack/fieldhand.glb", "character-pack/comet-cabin.glb", "character-pack/pack.json", "character-demo.pack.json", "character-authoring.recipe.json"}
                if (generated.get("status") != "matched" or len(generated_files) != 5
                        or {entry.get("file") for entry in generated_files} != expected):
                    raise ValueError("Human generator did not reproduce every original example output")
                for entry in generated_files:
                    path = ROOT / "crates/terra-bevy/assets" / entry["file"]
                    if entry.get("sha256") != digest(path) or entry.get("bytes") != path.stat().st_size:
                        raise ValueError("Human generator receipt differs from its exact generated files")
                publish_report(report_dir / "character-generator.json", generated)
            if name == "character-inspect":
                inspected = command_report(body)
                model = inspected.get("model", {})
                if (inspected.get("accepted") is not True
                        or model.get("sha256") != digest(ROOT / "crates/terra-bevy/assets/character-pack/fieldhand.glb")
                        or {clip.get("name") for clip in model.get("clips", [])} != {"idle", "walk", "run", "seated"}
                        or model.get("budget", {}).get("bytes", 0) <= 0):
                    raise ValueError("Human inspection did not accept its exact model and named clips")
                publish_report(report_dir / "character-inspect.json", inspected)
            if name in {"character-pack", "character-demo-pack"}:
                accepted = command_report(body)
                expected = {"fieldhand"} if name == "character-pack" else {"fieldhand", "comet", "deer", "ibex", "boar", "fox", "hare"}
                if (accepted.get("accepted") is not True
                        or {model.get("id") for model in accepted.get("models", [])} != expected
                        or not accepted.get("identity")):
                    raise ValueError("Human demonstration pack did not retain its exact model set")
                publish_report(report_dir / f"{name}.json", accepted)
            if name == "character-asset-build":
                built = command_report(body)
                if (built.get("accepted") is not True or len(built.get("sources", [])) != 7
                        or built.get("recipe_sha256") != digest(ROOT / "crates/terra-bevy/assets/character-authoring.recipe.json")
                        or json.loads((character_authored_pack / "authoring-report.json").read_text()) != built):
                    raise ValueError("Human authoring receipt differs from the seven-model recipe and published pack")
                character_authored_pack_identity = built.get("identity")
                if not character_authored_pack_identity:
                    raise ValueError("Human authored pack has no immutable identity")
                publish_report(report_dir / "character-asset-build.json", built)
            if name == "character-authored-validate":
                accepted = command_report(body)
                if (accepted.get("accepted") is not True or len(accepted.get("models", [])) != 7
                        or not character_authored_pack_identity or accepted.get("identity") != character_authored_pack_identity):
                    raise ValueError("Runtime did not accept the exact authored human/vehicle/wildlife pack")
                publish_report(report_dir / "character-authored-validate.json", accepted)
            if name == "source-inspect":
                source_inspection = command_report(body)
                coverage = source_inspection.get("coverage", {})
                if (source_inspection.get("schema_version") != 1
                        or not source_inspection.get("files")
                        or coverage.get("complete") is not True
                        or coverage.get("requested_tiles") != 81
                        or coverage.get("fully_covered_tiles") != 81
                        or coverage.get("missing_tiles") != []):
                    raise ValueError("Prepared fixture did not establish its exact 81-tile coverage")
                publish_report(report_dir / "source-inspection.json", source_inspection)
            if name in {"source-export", "source-install", "source-verify"}:
                packaged = command_report(body)
                identity = packaged.get("package_identity")
                if packaged.get("inspection") != source_inspection or not isinstance(identity, str) or not identity.startswith("blake3:"):
                    raise ValueError("Portable source distribution changed the inspected content or coverage")
                if name == "source-export":
                    source_package_identity = identity
                elif identity != source_package_identity:
                    raise ValueError("Installed source package identity differs from exported input")
                publish_report(report_dir / f"{name}.json", packaged)
            if name == "recovery-probe":
                recovered = command_report(body)
                if json.loads(recovery_path.read_text()) != recovered:
                    raise ValueError("Recovery sidecar differs from the captured command report")
                if (recovered.get("version") != report["version"]
                        or recovered.get("scope", {}).get("complete_nonempty_regions", 0) < 1024
                        or not recovered.get("verification")
                        or not all(value is True for value in recovered["verification"].values())):
                    raise ValueError("Storage recovery report did not verify the required current workload")
                publish_report(recovery_path, recovered)
            if name == "recovery-bundle":
                recovered = command_report(body)
                scope = recovered.get("scope", {})
                created = recovered.get("created", {})
                restored = recovered.get("restored", {})
                required = {
                    "original_database_configuration_and_source_removed", "full_state_exact",
                    "local_host_metadata_exact", "world_identity_exact", "catalog_exact",
                    "generation_preserved", "dormant_records_preserved",
                    "source_identity_exact_without_original_tree", "runtime_paths_rebased",
                    "bundle_database_bytes_unchanged", "bundle_receipt_unchanged",
                    "damaged_copy_rejected_without_output", "political_source_bytes_exact",
                    "political_source_identity_exact", "authored_political_state_exact",
                }
                if (json.loads(recovery_bundle_path.read_text()) != recovered
                        or recovered.get("version") != report["version"]
                        or scope.get("resource_groups") != 1024
                        or scope.get("players") != 2 or scope.get("factions") != 1 or scope.get("plots") != 1
                        or scope.get("resources", 0) < 4096
                        or not 0 <= scope.get("active_resources", -1) < scope.get("resources", 0)
                        or scope.get("server_capabilities") is not False or scope.get("renderer") is not False
                        or not source_inspection or scope.get("prepared_source_identity") != source_inspection["source_identity"]
                        or scope.get("political_input_sha256") != digest(ROOT / "crates/terra-history/data/natural-earth-countries-2026-10-26.geojson")
                        or not scope.get("political_dataset_identity")
                        or created.get("source_generation") != 2
                        or created.get("file_count") != len(source_inspection["files"]) + 5
                        or created.get("inspection", {}).get("nonempty_regions", 0) < 1024
                        or not all(created.get(key) is True for key in ("prepared_source", "political_source", "domain_validated", "local_host_metadata_validated", "catalog_exact", "source_dependencies_validated"))
                        or restored.get("committed_generation") != 2
                        or restored.get("bundle_identity") != created.get("bundle_identity")
                        or not all(restored.get(key) is True for key in ("domain_and_local_metadata_exact", "source_and_catalog_exact", "runtime_paths_rebased", "original_bundle_unchanged"))
                        or not recovered.get("verification")
                        or not required.issubset(recovered["verification"])
                        or not all(value is True for value in recovered["verification"].values())
                        or (os.name == "posix" and recovered.get("unix_private_modes_verified") is not True)):
                    raise ValueError("Self-contained recovery did not verify exact state after removing original dependencies")
                publish_report(recovery_bundle_path, recovered)
            if name == "regional-sessions":
                sessions = command_report(body)
                if json.loads(sessions_path.read_text()) != sessions:
                    raise ValueError("Session sidecar differs from the captured command report")
                workload = sessions.get("workload", {})
                checked = sessions.get("verification", {})
                expected_workload = {"real_socket_clients": 16, "churn_cycles": 8,
                                     "active_ticks_per_cycle": 400, "cold_overlay_regions": 1024,
                                     "cold_overlay_resources": 4096, "source_ingestion": True,
                                     "initial_owned_placement_checked": True}
                expected_checks = {"exact_offline_owner_checks": 64, "exact_resume_owner_checks": 64,
                                   "exact_restart_owner_checks": 128, "authenticated_recovered_owner_checks": 16,
                                   "source_unavailable_and_restored_checks": 1, "expected_storage_failures": 1}
                required_checks = {"failure_atomic_and_retry_exact", "cargo_conserved",
                                   "craft_inventory_and_sequences_exact", "all_real_sessions_received_snapshots",
                                   "backup_full_state_and_private_metadata_exact", "backup_generation_exact",
                                   "backup_source_bytes_unchanged", "durable_resume_sequences_exact", "cold_overlay_records_exact"}
                if (sessions.get("release") != report["version"] or sessions.get("fixed_steps") != 4840
                        or any(workload.get(key) != value for key, value in expected_workload.items())
                        or any(checked.get(key) != value for key, value in expected_checks.items())
                        or not all(checked.get(key) is True for key in required_checks)
                        or sessions.get("durable_command_ack_ms", {}).get("samples") != 128
                        or sessions.get("ping_roundtrip_ms", {}).get("samples", 0) == 0
                        or sessions.get("final_store", {}).get("dormant_regions", 0) < 1024
                        or workload.get("prepared_source_revision") != source_inspection["source_identity"]
                        or sessions.get("backup_generation") != sessions.get("recovery_receipt", {}).get("generation")):
                    raise ValueError("Live prepared-world recovery did not verify its exact workload and continuations")
                publish_report(sessions_path, sessions)
            if name == "native-build":
                artifacts = []
                for line in body.splitlines():
                    if not line.startswith("{"):
                        continue
                    artifact = json.loads(line)
                    if artifact.get("reason") == "compiler-artifact" and artifact.get("target", {}).get("name") == "terra-demo" and artifact.get("executable"):
                        artifacts.append(artifact)
                if len(artifacts) != 1:
                    raise ValueError("Native build must identify exactly one executable artifact")
                binary = Path(artifacts[0]["executable"])
                report["native_binary_sha256"] = digest(binary)
                report["native_executable"] = str(binary)
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError, KeyboardInterrupt) as error:
        report["error"] = str(error) or "interrupted"
        if isinstance(error, subprocess.CalledProcessError) and error.output:
            (report_dir / "startup.log").write_text(str(error.output), encoding="utf-8")
        print(f"Verification stopped: {report['error']}", file=sys.stderr, flush=True)
        code = 1
    report["finished_utc"] = utc()
    try:
        source_work.cleanup()
    except OSError as error:
        report["cleanup_error"] = str(error)
        code = 1
    try:
        report["source_inputs_unchanged"] = initial_inputs is not None and source_inputs() == initial_inputs
        if not report["source_inputs_unchanged"]:
            report["source_error"] = "Source inputs unavailable or changed during verification; these gates do not certify the final files"
            code = 1
    except OSError as error:
        report["source_inputs_unchanged"] = None
        report["source_error"] = str(error)
        code = 1
    report["distinct_tests_by_crate"] = dict(sorted(Counter(item[0] for item in identities).items()))
    report["distinct_passing_rust_tests"] = len(identities)
    try:
        publish_report(report_dir / "distinct-tests.json", [
            {"crate": crate, "harness": harness, "name": name} for crate, harness, name in sorted(identities)
        ])
        report["published_reports_unchanged"] = all(
            digest(report_dir / name) == expected for name, expected in report["evidence_artifacts"].items()
        )
        if not report["published_reports_unchanged"]:
            report["report_error"] = "A published report differs from its accepted captured evidence"
            code = 1
    except OSError as error:
        report["published_reports_unchanged"] = None
        report["report_error"] = str(error)
        code = 1
    try:
        report["published_logs_unchanged"] = all(
            digest(report_dir / gate["log"]) == gate["log_sha256"] for gate in report["gates"]
        )
        if not report["published_logs_unchanged"]:
            report["evidence_error"] = "A published log differs from the captured command output"
            code = 1
    except OSError as error:
        report["published_logs_unchanged"] = None
        report["evidence_error"] = str(error)
        code = 1
    report["passing_rust_test_executions"] = sum(gate["passed_test_executions"] for gate in report["gates"])
    if code == 0 and gates and len(report["gates"]) == len(gates):
        try:
            validate_gate_results(report["gates"], require_execution=True,
                                  platform=platform.system(), output_limit_bytes=args.gate_output_limit_bytes)
            validate_receipt_execution(report, {
                path.name: path.stat().st_size for path in report_dir.glob("*.log")
            })
        except ValueError as error:
            report["gate_results_error"] = str(error)
            code = 1
    report["status"] = "passed" if code == 0 and gates and len(report["gates"]) == len(gates) else "failed"
    try:
        publish_terminal_report(report_dir, report)
    except (OSError, ValueError) as error:
        # Never announce success after missing/stale/failed publication.
        print(f"Verification receipt publication failed: {error}", file=sys.stderr, flush=True)
        return 1
    print(f"Verification {report['status']}: {len(identities)} distinct Rust tests. Evidence: {report_dir}", flush=True)
    return code if report["status"] == "passed" else code or 1


if __name__ == "__main__":
    raise SystemExit(main())
