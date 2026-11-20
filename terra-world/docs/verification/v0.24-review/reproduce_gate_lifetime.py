"""Compare exact delivered 0.23 runner bytes with the current owned-child boundary."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise ValueError("Use a new evidence filename")
    root = Path(__file__).resolve().parents[3]
    sys.path.insert(0, str(root / "scripts"))
    import verify
    with args.baseline.open("rb") as source:
        baseline_hash = hashlib.file_digest(source, "sha256").hexdigest()
    with zipfile.ZipFile(args.baseline) as bundle:
        old = bundle.read("terra-world/scripts/verify.py")
    baseline = {"__name__": "delivered_terra023_verify", "__file__": str(root / "scripts/verify.py")}
    exec(compile(old, "delivered-0.23:verify.py", "exec"), baseline)
    current_hash = hashlib.sha256((root / "scripts/verify.py").read_bytes()).hexdigest()
    child = (
        "from pathlib import Path;import sys,time;"
        "release,marker=Path(sys.argv[1]),Path(sys.argv[2]);end=time.monotonic()+3\n"
        "while not release.exists() and time.monotonic()<end: time.sleep(.01)\n"
        "marker.write_text('child completed');print('child output',flush=True)"
    )
    leader = (
        "import subprocess,sys;"
        "subprocess.Popen([sys.executable,'-u','-c',sys.argv[1],sys.argv[2],sys.argv[3]]);"
        "print('leader done',flush=True)"
    )
    cases = {}
    with tempfile.TemporaryDirectory(prefix="terra024-lifetime-proof-") as temporary:
        for label, run in [("delivered_023", baseline["run_logged"]), ("current", verify.run_logged)]:
            directory = Path(temporary) / label
            directory.mkdir()
            release, marker = directory / "release", directory / "marker"
            started = time.monotonic()
            result, captured, timed_out = run(
                [sys.executable, "-u", "-c", leader, child, str(release), str(marker)],
                os.environ.copy(), directory / "gate.log", 0.5,
            )
            before_release = marker.exists()
            release.write_text("release after gate returned")
            time.sleep(0.25)
            cases[label] = {
                "exit_code": result.returncode, "timed_out": timed_out,
                "seconds_through_return_and_observation": time.monotonic() - started,
                "child_completed_at_return": before_release,
                "child_published_after_return": marker.exists() and not before_release,
                "captured_log": captured.decode("utf-8"),
                "published_log_exact": (directory / "gate.log").read_bytes() == captured,
                "log_sha256": hashlib.sha256(captured).hexdigest(),
                "execution": getattr(result, "execution", None),
            }
    success = (cases["delivered_023"]["exit_code"] == 0
               and cases["delivered_023"]["child_published_after_return"]
               and cases["current"]["exit_code"] != 0
               and not cases["current"]["child_published_after_return"]
               and hashlib.sha256((root / "scripts/verify.py").read_bytes()).hexdigest() == current_hash)
    report = {
        "status": "passed" if success else "failed", "checked_utc": datetime.now(timezone.utc).isoformat(),
        "baseline_archive_sha256": baseline_hash,
        "baseline_verify_sha256": hashlib.sha256(old).hexdigest(),
        "current_verify_sha256": current_hash, "cases": cases,
        "scope": "Bounded real child-process regression on this host, comparing exact delivered source; not a release acceptance or Windows proof.",
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as output:
        json.dump(report, output, indent=2)
        output.write("\n")
    print(json.dumps(report, indent=2))
    return 0 if success else 1


if __name__ == "__main__":
    raise SystemExit(main())
