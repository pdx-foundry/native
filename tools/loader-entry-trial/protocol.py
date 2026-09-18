"""Private SDK-515 trial handshake; shared by its Python owner and LLDB worker."""
import hashlib
import json
from pathlib import Path
import time

VERSION = "sdk-515-trial/1"


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def validate_hello(hello, expected, run, game_pid, worker_pid):
    identity = dict(expected, protocol=VERSION, run=run,
                    gamePid=game_pid, workerPid=worker_pid)
    if hello != identity:
        raise ValueError("Worker handshake does not match pinned trial identity")


def await_hello(out, worker, game_pid):
    deadline = time.monotonic() + 15
    path = out / "hello.json"
    while not path.exists():
        if worker.poll() is not None or time.monotonic() >= deadline:
            raise RuntimeError("Worker handshake unavailable; resume refused")
        time.sleep(0.05)
    expected = json.loads((out.parent.parent / "expected.json").read_text())
    validate_hello(json.loads(path.read_text()), expected, out.name,
                   game_pid, worker.pid)

