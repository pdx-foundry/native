#!/usr/bin/env python3
"""SDK-515 private live trial. Builds a guard and launches four bounded games."""
import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time

from protocol import sha, write

REPO = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
RETAINED_RUN = "20260917-002215-none-d8a910"
TARGET = "3d4c8a7046d87175ce7e3b513b1a2ce589050d654d332744518a49d13ac82216"
SLICE = "1e0c9aec45650272fcaecba2eb47f8dce8f17bc08ef2b992be18c99ae098c623"


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def replace_once(source, old, new):
    if source.count(old) != 1:
        raise ValueError("Retained source patch anchor changed: " + old)
    return source.replace(old, new, 1)


def prepare_sources(retained, output, lldb):
    manifest = json.loads((retained / "manifest.json").read_text())
    for name in ("pdx_native.py", "debugger_attempt.py", "atlas_demo.py", "guard.m"):
        source = retained / "source" / name
        if sha(source) != manifest["probeHashes"][name]:
            raise ValueError("Retained source hash mismatch: " + name)
        shutil.copyfile(source, output / name)
    for name in ("protocol.py", "worker.py"):
        shutil.copyfile(HERE / name, output / name)

    owner = (output / "pdx_native.py").read_text()
    owner = replace_once(owner, "HERE=", "from protocol import await_hello\nHERE=")
    old = "cmd=['lldb','-b','-o','command script import '+str(HERE/'debugger_attempt.py'),'-o','script debugger_attempt.run(lldb.debugger)']"
    new = ("cmd=[" + repr(lldb) + ",'-b','-x','-o',"
           "'command script import '+json.dumps(str(HERE/'debugger_attempt.py')),"
           "'-o','command script import '+json.dumps(str(HERE/'worker.py')),"
           "'-o','script worker.run(lldb.debugger)']")
    owner = replace_once(owner, old, new)
    owner = replace_once(owner, "with (out/'worker.log').open('w') as log:",
                         "with (out/'worker.stdout').open('w') as log, (out/'worker.stderr').open('w') as errors:")
    owner = replace_once(owner, "stderr=subprocess.STDOUT", "stderr=errors")
    owner = replace_once(owner, "            (out/'resume-granted').touch()",
                         "            await_hello(out, worker, game_pid)\n"
                         "            record('handshake-accepted', pid=worker.pid)\n"
                         "            (out/'resume-granted').touch()")
    # SIGTERM and keyboard cancellation must reach the retained owner's finally block.
    owner = replace_once(owner, "    game_pid=spawn_suspended(out,game_env)",
                         "    def cancelled(signum, frame):\n"
                         "        raise KeyboardInterrupt('trial cancelled')\n"
                         "    signal.signal(signal.SIGTERM, cancelled)\n"
                         "    game_pid=spawn_suspended(out,game_env)")
    (output / "pdx_native.py").write_text(owner)

    worker = (output / "debugger_attempt.py").read_text()
    worker = replace_once(worker, "seq = 0", "callback_thread = None\nseq = 0")
    worker = replace_once(worker, "run=OUT.name, kind=kind,", "run=OUT.name, kind=kind, thread=callback_thread,")
    worker = replace_once(worker, "global finished, active_file, registration_count, field_count",
                          "global finished, active_file, registration_count, field_count, callback_thread")
    worker = replace_once(worker, "        p=frame.GetThread().GetProcess(); target=p.GetTarget()",
                          "        callback_thread=frame.GetThread().GetThreadID()\n"
                          "        p=frame.GetThread().GetProcess(); target=p.GetTarget()")
    # The retained missing-hook control resumed without the field hook. Fail closed here.
    worker = replace_once(worker, "    installed=all(",
                          "    if set(state) != {'registration','load-file','field'}:\n"
                          "        emit('capability-unavailable', capability='tradition-category-field-reads', reason='required hook missing before resume')\n"
                          "        return\n"
                          "    installed=all(")
    (output / "debugger_attempt.py").write_text(worker)


def preflight(retained, output):
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise RuntimeError("Trial requires native ARM64 macOS")
    subprocess.run([sys.executable, str(REPO / "tools/evidence.py"), "typed-extraction"], check=True)
    bundle = json.loads((REPO / ".local/evidence/bundles/typed-extraction.manifest.json").read_text())
    restored = REPO / ".local/evidence/restored/typed-extraction"
    pins = {row["path"]: row for row in bundle["files"]}
    for name in ("manifest.json", "producer-content.json", "source/pdx_native.py",
                 "source/debugger_attempt.py", "source/atlas_demo.py", "source/guard.m"):
        path = retained / name
        if sha(path) != pins[str(path.relative_to(restored))]["sha256"]:
            raise ValueError("Restored input differs from verified archive: " + name)
    manifest = json.loads((retained / "manifest.json").read_text())
    if sha(retained / "producer-content.json") != manifest["producerContentManifestSha256"]:
        raise ValueError("Retained content manifest changed")
    binary = Path(manifest["target"]["executable"])
    if sha(binary) != TARGET:
        raise ValueError("Installed executable differs from M45-observe")
    if subprocess.run(["pgrep", "-x", "stellaris"], capture_output=True).returncode == 0:
        raise RuntimeError("Existing Stellaris process; refusing trial")
    game = binary.parents[3]
    content = json.loads((retained / "producer-content.json").read_text())
    current_paths = {"launcher-settings.json"}
    for directory in ("common/tradition_categories", "common/traditions"):
        current_paths.update(str(path.relative_to(game)) for path in (game / directory).rglob("*.txt"))
    if current_paths != set(content) or any(sha(game / name) != digest for name, digest in content.items()):
        raise ValueError("Installed content differs from retained 68-file boundary")
    output.mkdir(parents=True, exist_ok=False)
    command("xcrun", "lipo", str(binary), "-thin", "arm64", "-output", str(output / "target.arm64"))
    if sha(output / "target.arm64") != SLICE:
        raise ValueError("ARM64 executable slice differs")
    (output / "target.arm64").unlink()
    lldb = command("xcrun", "--find", "lldb")
    probe = command(lldb, "-b", "-x", "-o",
                    "script import json,sys,lldb; print('SDK515='+json.dumps(dict(python=sys.version,lldb=lldb.SBDebugger.GetVersionString(),module=lldb.__file__)))", "-o", "quit")
    tools = json.loads(next(line.removeprefix("SDK515=") for line in probe.splitlines() if line.startswith("SDK515=")))
    prepare_sources(retained, output, lldb)
    command("xcrun", "clang", "-arch", "arm64", "-dynamiclib", "-fobjc-arc", "-framework", "AppKit",
            "-o", str(output / "guard.dylib"), str(output / "guard.m"))
    artifacts = {path.name: sha(path) for path in sorted(output.iterdir())}
    expected = dict(tools, artifacts=artifacts, target=TARGET)
    write(output / "expected.json", expected)
    preflight_record = {
        "qualification": "candidate; no production support", "retainedRun": RETAINED_RUN,
        "target": TARGET, "slice": SLICE, "content": content, "tools": tools,
        "lldbPath": lldb, "lldbSha256": sha(lldb),
        "compiler": command("xcrun", "clang", "--version"),
        "os": command("sw_vers"), "ownerPython": sys.version,
        "trialSources": {p.name: sha(p) for p in sorted(HERE.glob("*.py"))},
        "artifacts": artifacts, "expectedSha256": sha(output / "expected.json"),
    }
    write(output / "preflight.json", preflight_record)
    source_copy = output / "trial-source"
    source_copy.mkdir()
    for path in HERE.glob("*.py"):
        shutil.copyfile(path, source_copy / path.name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="New private output directory")
    args = parser.parse_args()
    output = args.output.resolve()
    retained = REPO / ".local/evidence/restored/typed-extraction/typed-extraction/early-observation-prototype/runs" / RETAINED_RUN
    started = time.monotonic()
    preflight(retained, output)
    environment = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
    for name in ("PYTHONHOME", "PYTHONPATH", "DYLD_INSERT_LIBRARIES"):
        environment.pop(name, None)
    with (output / "owner.stdout").open("w") as stdout, (output / "owner.stderr").open("w") as stderr:
        subprocess.run([sys.executable, str(output / "atlas_demo.py"), "--scenario", "all"],
                       env=environment, stdout=stdout, stderr=stderr, check=True)
    write(output / "timing.json", {"wallSeconds": time.monotonic() - started})
    subprocess.run([sys.executable, str(HERE / "verify.py"), str(output)], check=True)


if __name__ == "__main__":
    main()
