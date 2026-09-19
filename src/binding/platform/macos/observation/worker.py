"""Bounded ARM64 read-entry strategy; engine locations arrive from Native binding groups."""
import hashlib
import json
import os
from pathlib import Path
import time
import traceback
import protocol

ROOT = Path(__file__).resolve().parent.parent
request = protocol.decode('request', (ROOT / 'worker-request.json').read_bytes())
bindings = request['bindings']
sequence = 0
field_count = 0
registration_count = 0
finished = False
active_file = None
active_thread = None
active_owner = None
entry_thread = None
breakpoints = {}
control = request['control']
registry = request['registry']
registry_owner = None


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
    if control == 'dropped-record' and ((kind == 'field-observed' and field_count == 1) or (kind == 'registry-entry' and fields['index'] == 0)):
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


def location(process, reader):
    lexer = uint(process, reader + bindings['reader-lexer-offset'])
    file = uint(process, lexer + bindings['lexer-file-offset'])
    storage = file + bindings['file-name-offset']
    address = uint(process, storage) if uint(process, storage + bindings['string-storage-tag-offset'], 1) & 128 else storage
    return dict(file=string(process, address), line=uint(process, file + bindings['file-line-offset'], 4))


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
    breakpoints['registry-return'] = hook
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


def callback(frame, loc, _):
    global finished, active_file, active_thread, active_owner, registration_count, field_count
    try:
        process = frame.GetThread().GetProcess()
        thread = frame.GetThread().GetThreadID()
        name = next((key for key, bp in breakpoints.items() if bp.GetID() == loc.GetBreakpoint().GetID()), 'file-return')
        if name == 'registry':
            return registry_begin(frame)
        if name == 'registry-return':
            return registry_snapshot(frame)
        if name == 'registration':
            if thread != entry_thread:
                raise RuntimeError('registration thread differs from loader-entry witness')
            registration_count += 1
            if registration_count == 1:
                emit('phase-reached', phase='effect-registration', stack=[f.GetFunctionName() or '' for f in frame.GetThread()][:8], thread=thread)
            emit('registration-observed', ordinal=registration_count, engineToken=register(frame, request['machine']['registers']['registration-token']), thread=thread)
            if registration_count == 3:
                breakpoints[name].SetEnabled(False)
                emit('registration-window-complete', observed=3, thread=thread)
                if control == 'worker-loss':
                    emit('worker-loss-ready')
                    (ROOT / 'worker-loss-ready').touch(exist_ok=False)
                    return True
        elif name == 'load-file':
            file = string(process, register(frame, request['machine']['registers']['file']))
            if file != request['fixture']:
                return False
            if active_file is not None or thread != entry_thread:
                raise RuntimeError('ambiguous fixture entry or thread')
            active_file, active_thread = file, thread
            emit('phase-reached', phase='fixture-file-parse', file=file, thread=thread)
            if control == 'access-failure':
                uint(process, 0)
                raise RuntimeError('access failure control unexpectedly read address zero')
            return_hook = process.GetTarget().BreakpointCreateByAddress(register(frame, request['machine']['registers']['return']))
            return_hook.SetThreadID(thread)
            return_hook.SetOneShot(True)
            return_hook.SetScriptCallbackFunction('worker.callback')
        elif name == 'field':
            where = location(process, register(frame, request['machine']['registers']['reader']))
            if where['file'] != request['fixture']:
                return False
            if active_file != where['file'] or thread != active_thread:
                raise RuntimeError('field lacks matching loader/thread witness')
            owner = hex(register(frame, request['machine']['registers']['owner']))
            if owner == '0x0' or active_owner not in (None, owner):
                raise RuntimeError('field owner changed or is null')
            active_owner = owner
            token = register(frame, request['machine']['registers']['field-token'])
            fields = {bindings['tree-template-token']: 'tree_template', bindings['traditions-token']: 'traditions'}
            if token not in fields or field_count >= 2:
                raise RuntimeError('field outside bounded category window')
            field_count += 1
            emit('field-observed', **where, field=fields[token], owner=owner, ordinal=field_count, thread=thread)
        else:
            if thread != active_thread or active_file is None:
                raise RuntimeError('loader return lacks matching entry')
            emit('phase-complete', phase='fixture-file-parse', file=active_file, producerFieldCount=field_count, thread=thread)
            if control != 'missing-terminal':
                emit('stream-end', producerLastSequence=sequence + 1, producerFieldCount=field_count, registrations=registration_count, thread=thread)
            finished = True
            return True
    except Exception:
        emit('callback-error', error=traceback.format_exc())
        finished = True
        return True
    return False


def run(debugger):
    global entry_thread
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
    hooks = [('registry', registry['load_entry'])] if registry else [(name, bindings[role]) for name, role in [('registration', 'registration-entry'), ('load-file', 'category-load-entry'), ('field', 'category-field-read-entry')]]
    controlled_hook = 'registry' if registry else 'field'
    for name, address in hooks:
        if name == controlled_hook and control == 'missing-hook':
            continue
        hook = target.BreakpointCreateBySBAddress(target.ResolveFileAddress(address))
        hook.SetScriptCallbackFunction('worker.callback')
        if name == controlled_hook and control == 'late-hook':
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
    if set(state) != {name for name, _ in hooks} or not all(h['enabled'] and h['locations'] == 1 and h['resolved'] == 1 and h['hits'] == 0 for h in state.values()):
        emit('capability-unavailable', reason='required hook missing or late before resume')
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
    emit('worker-finished')
    # The owner alone proves disposal. Leave the stopped game for its independent cleanup.
