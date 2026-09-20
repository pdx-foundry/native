"""The debugger worker of one game session. It runs inside LLDB.

It sets one hook at each registry's loader before the game runs. When a loader returns, it reads
the registry's collection and writes what it sees to raw-trace.jsonl. When every observed
registry has returned, it holds the game at that point until the supervisor releases it. Engine
locations arrive in the request, from Native's binding groups."""
import hashlib
import json
import os
from pathlib import Path
import time
import traceback
import protocol

ROOT = Path(__file__).resolve().parent.parent
request = protocol.decode('request', (ROOT / 'worker-request.json').read_bytes())
sequence = 0
finished = False
entry_thread = None
breakpoints = {}
control = 'normal'
registry = None
registry_owner = None
registry_owners = {}
returned_registries = []
session_active = set()
safe_pause = False


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def atomic(kind, name, value):
    temporary = ROOT / (name + '.pending')
    with temporary.open('xb') as output:
        output.write(protocol.encode(kind, value))
        output.flush()
        os.fsync(output.fileno())
    temporary.rename(ROOT / name)


def emit(kind, **fields):
    global sequence
    sequence += 1
    record = dict(seq=sequence, run=request['attempt'], kind=kind, **fields)
    encoded = protocol.encode('record', record)
    if control == 'dropped-record' and kind == 'registry-entry' and fields['index'] == 0:
        return
    path = ROOT / 'raw-trace.jsonl'
    if path.stat().st_size + len(encoded) > protocol.MAX_TRACE:
        raise RuntimeError('trace storage bound exceeded')
    with path.open('ab') as output:
        output.write(encoded)
        output.flush()
        os.fsync(output.fileno())


def register(frame, name):
    value = frame.FindRegister(name)
    if not value.IsValid() or value.GetError().Fail():
        raise RuntimeError('native argument register unavailable: ' + name)
    return value.GetValueAsUnsigned()


def uint(process, address, size=8):
    import lldb
    error = lldb.SBError()
    value = process.ReadMemory(address, size, error)
    if error.Fail() or len(value) != size:
        raise RuntimeError('native memory access failed: ' + str(error))
    return int.from_bytes(value, 'little')


def string(process, address):
    import lldb
    error = lldb.SBError()
    value = process.ReadCStringFromMemory(address, 4096, error)
    if error.Fail() or value is None or len(value) >= 4095:
        raise RuntimeError('native string access failed: ' + str(error))
    return value


def hook_state():
    return {name: dict(enabled=bp.IsEnabled(), locations=bp.GetNumLocations(),
                       resolved=bp.GetNumResolvedLocations(), hits=bp.GetHitCount())
            for name, bp in breakpoints.items()}


def registry_begin(frame):
    global registry_owner
    process = frame.GetThread().GetProcess()
    thread = frame.GetThread().GetThreadID()
    if registry_owner is not None or thread != entry_thread:
        raise RuntimeError('ambiguous registry loader entry')
    registry_owner = register(frame, request['machine']['registers']['owner'])
    if not registry_owner:
        raise RuntimeError('registry loader receiver is null')
    storage = registry_owner + registry['directory_offset']
    address = uint(process, storage) if uint(process, storage + registry['string_tag_offset'], 1) & 128 else storage
    directory = string(process, address)
    if directory != registry['directory']:
        raise RuntimeError('registry loader directory mismatch')
    hook = process.GetTarget().BreakpointCreateByAddress(register(frame, request['machine']['registers']['return']))
    hook.SetThreadID(thread)
    hook.SetOneShot(True)
    hook.SetScriptCallbackFunction('worker.callback')
    if hook.GetNumResolvedLocations() != 1:
        raise RuntimeError('registry return hook unresolved')
    breakpoints['registry-return:' + registry['name']] = hook
    emit('registry-load-start', name=registry['name'], owner=hex(registry_owner), directory=directory, thread=thread)
    return False


def registry_snapshot(frame):
    global finished
    process = frame.GetThread().GetProcess()
    thread = frame.GetThread().GetThreadID()
    if thread != entry_thread:
        raise RuntimeError('registry thread differs from activation witness')
    owner = registry_owner
    if not owner:
        raise RuntimeError('registry receiver is null')
    def cstring(address):
        storage = uint(process, address) if uint(process, address + registry['string_tag_offset'], 1) & 128 else address
        return string(process, storage)
    directory = cstring(owner + registry['directory_offset'])
    if directory != registry['directory']:
        raise RuntimeError('registry receiver directory mismatch: ' + directory)
    emit('registry-load-returned', name=registry['name'], owner=hex(owner), thread=thread)
    returned_registries.append(registry['name'])
    if control == 'access-failure':
        uint(process, 0)
        raise RuntimeError('access failure unexpectedly read zero')
    count = uint(process, owner + registry['count_offset'], 4)
    data = uint(process, owner + registry['data_offset'])
    if count > 100000 or (count and (not data or data % registry['pointer_size'])):
        raise RuntimeError('registry collection bounds invalid')
    emit('registry-snapshot', name=registry['name'], owner=hex(owner), directory=directory, count=count, thread=thread)
    keys, objects = set(), set()
    for index in range(count):
        obj = uint(process, data + registry['pointer_size'] * index)
        if not obj or obj % registry['pointer_size'] or obj in objects:
            raise RuntimeError('invalid or duplicate registry object')
        key = cstring(obj + registry['key_offset'])
        if not key or key in keys:
            raise RuntimeError('empty or duplicate registry key')
        keys.add(key)
        objects.add(obj)
        emit('registry-entry', name=registry['name'], owner=hex(owner), index=index, object=hex(obj), key=key, thread=thread)
        if control == 'worker-loss' and index == 0:
            emit('worker-loss-ready')
            (ROOT / 'worker-loss-ready').touch(exist_ok=False)
            return True
    if count != uint(process, owner + registry['count_offset'], 4) or data != uint(process, owner + registry['data_offset']):
        raise RuntimeError('registry changed during snapshot')
    if control != 'missing-terminal':
        emit('registry-end', name=registry['name'], owner=hex(owner), count=count, producerLastSequence=sequence + 1, thread=thread)
    finished = True
    return True


def registry_callback(frame, name):
    global registry, registry_owner, control, finished, safe_pause
    is_return = name.startswith('registry-return:')
    selected = name.split(':', 1)[1]
    registry = request['registries'][selected]
    registry_owner = registry_owners.get(selected)
    control = request['control'] if selected == request['control_registry'] else 'normal'
    try:
        if not is_return:
            result = registry_begin(frame)
            registry_owners[selected] = registry_owner
            breakpoints[name].SetEnabled(False)
            return result
        breakpoints[name].SetEnabled(False)
        registry_snapshot(frame)
    except Exception:
        emit('registry-unavailable', name=selected, reason=traceback.format_exc(), thread=frame.GetThread().GetThreadID())
        # Continue only after this callback proved its actual loader-return boundary.
        if selected not in returned_registries:
            finished = True
            safe_pause = False
            return True
    if control == 'worker-loss':
        return True
    finished = session_active.issubset(set(returned_registries))
    safe_pause = finished
    return finished


def callback(frame, loc, _):
    global finished
    try:
        name = next(key for key, bp in breakpoints.items() if bp.GetID() == loc.GetBreakpoint().GetID())
        return registry_callback(frame, name)
    except Exception:
        emit('callback-error', error=traceback.format_exc())
        finished = True
        return True


def run(debugger):
    global entry_thread, session_active
    import lldb
    import sys
    artifacts = {name: sha(ROOT / 'source' / name) for name in request['artifacts']}
    target_hash = sha(request['executable'])
    if request['version'] != protocol.VERSION or artifacts != request['artifacts'] or target_hash != request['target']:
        raise RuntimeError('worker package or target mismatch')
    atomic('hello', 'hello.json', dict(version=protocol.VERSION, attempt=request['attempt'],
        game=request['game'], worker=os.getpid(), target=target_hash, artifacts=artifacts,
        python=sys.version, lldb=lldb.SBDebugger.GetVersionString(), module=lldb.__file__))
    debugger.SetAsync(True)
    target = debugger.CreateTargetWithFileAndArch(request['executable'], request['machine']['architecture'])
    hooks = [('registry:' + name, value['load_entry']) for name, value in request['registries'].items()]
    controlled_hook = 'registry:' + request['control_registry'] if request['control_registry'] else None
    for name, address in hooks:
        if name == controlled_hook and request['control'] == 'missing-hook':
            continue
        hook = target.BreakpointCreateBySBAddress(target.ResolveFileAddress(address))
        hook.SetScriptCallbackFunction('worker.callback')
        if name == controlled_hook and request['control'] == 'late-hook':
            hook.SetEnabled(False)
        breakpoints[name] = hook
    emit('hooks-requested')
    error = lldb.SBError()
    process = target.Attach(lldb.SBAttachInfo(request['game']), error)
    threads = list(process)
    frames = [dict(function=t.GetFrameAtIndex(0).GetFunctionName() or '') for t in threads]
    entry_thread = next((t.GetThreadID() for t in threads if t.GetFrameAtIndex(0).GetFunctionName() == '_dyld_start'), None)
    emit('launch-stopped', error=str(error), pid=process.GetProcessID(), triple=target.GetTriple() or '', frames=frames, thread=entry_thread)
    state = hook_state()
    if error.Fail():
        emit('capability-unavailable', reason='debugger attach failed: ' + str(error))
        return
    if process.GetState() != lldb.eStateStopped or entry_thread is None or not target.GetTriple().startswith(request['machine']['architecture'] + '-'):
        emit('early-activation-unavailable', reason='ARM64 loader entry not established')
        return
    for name, _ in hooks:
        hook = state.get(name)
        if hook and hook['enabled'] and hook['locations'] == 1 and hook['resolved'] == 1 and hook['hits'] == 0:
            session_active.add(name.split(':', 1)[1])
        else:
            emit('registry-unavailable', name=name.split(':', 1)[1], reason='required registry hook missing or late before resume')
    if not session_active:
        return
    emit('hooks-active-before-resume', hooks=state)
    deadline = time.monotonic() + 15
    while not (ROOT / 'resume-granted.json').exists():
        if time.monotonic() >= deadline:
            emit('capability-unavailable', reason='owner resume acknowledgement missing')
            return
        time.sleep(.02)
    grant = protocol.decode('grant', (ROOT / 'resume-granted.json').read_bytes())
    if grant != dict(version=protocol.VERSION, attempt=request['attempt'], game=request['game'], worker=os.getpid()):
        raise RuntimeError('foreign resume acknowledgement')
    emit('resume', error=str(process.Continue()))
    deadline = time.monotonic() + request['deadline_seconds']
    while time.monotonic() < deadline and process.IsValid() and not finished:
        if process.GetState() in (lldb.eStateExited, lldb.eStateCrashed, lldb.eStateDetached):
            break
        if process.GetState() == lldb.eStateStopped and any(t.GetStopReason() == lldb.eStopReasonException for t in process):
            emit('native-exception', reason='native exception stopped the bounded observation')
            break
        time.sleep(.02)
    if safe_pause and process.GetState() == lldb.eStateStopped:
        emit('session-paused', returned=returned_registries, thread=entry_thread)
        paused_thread = process.GetThreadByID(entry_thread)
        paused_pc = paused_thread.GetFrameAtIndex(0).GetPC()
        witness = dict(attempt=request['attempt'], game=request['game'], worker=os.getpid(),
            thread=entry_thread, returned=returned_registries, generation=0)
        atomic('pause', 'session-paused.json', witness)
        while not (ROOT / 'session-release').exists():
            if not process.IsValid() or process.GetState() != lldb.eStateStopped or paused_thread.GetFrameAtIndex(0).GetPC() != paused_pc:
                raise RuntimeError('session no longer held at the witnessed paused frame')
            check_path = ROOT / 'pause-check.json'
            if check_path.exists():
                check = protocol.decode('pause_check', check_path.read_bytes())
                if check['attempt'] != request['attempt'] or check['game'] != request['game']:
                    raise RuntimeError('foreign pause confirmation request')
                if check['generation'] > witness['generation']:
                    witness['generation'] = check['generation']
                    atomic('pause', 'session-paused.json', witness)
            time.sleep(.02)
        # Complete pending exit handling before the debugger goes away. The independent
        # owner still must waitpid its original child; this response proves no disposal.
        error = process.Kill()
        if error.Fail():
            raise RuntimeError('debugger target termination failed: ' + str(error))
    emit('worker-finished')
    # The owner alone proves disposal. Leave the stopped game for its independent cleanup.
