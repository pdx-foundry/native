"""LLDB-hosted bootstrap; breakpoint meaning remains in the retained callback."""
import json
import os
from pathlib import Path
import sys

from protocol import VERSION, sha, write


def run(debugger):
    import lldb
    import debugger_attempt

    root = Path(__file__).resolve().parent
    out = Path(os.environ["EARLY_RUN"])
    expected = json.loads((root / "expected.json").read_text())
    hello = {
        "protocol": VERSION,
        "run": out.name,
        "gamePid": int(os.environ["EARLY_GAME_PID"]),
        "workerPid": os.getpid(),
        "artifacts": {name: sha(root / name) for name in expected["artifacts"]},
        "target": sha(debugger_attempt.BIN),
        "python": sys.version,
        "lldb": lldb.SBDebugger.GetVersionString(),
        "module": lldb.__file__,
    }
    write(out / "hello.pending", hello)
    (out / "hello.pending").replace(out / "hello.json")
    debugger_attempt.run(debugger)

