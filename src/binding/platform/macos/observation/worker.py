"""The debugger worker of one game session. It runs inside LLDB.

It sets registry and requested fixture hooks before the game runs. When a loader returns, it reads
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
control = protocol.CONTROL['normal']
registry = None
registry_owner = None
registry_owners = {}
returned_registries = []
session_active = set()
safe_pause = False
callback_active = False


class UnsupportedKeyLayout(Exception):
    pass


class TraceStorageBound(Exception):
    pass


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
    if request['fixture_fault'] and request['control'] == protocol.CONTROL['dropped_record'] and kind == 'fixture':
        dropped = ('field-read', 1) if request['fixture']['field_reads'] else ('registration-entry', 2)
        event = fields['event']
        if (event['kind'], event.get('ordinal')) == dropped:
            return
    if control == protocol.CONTROL['dropped_record'] and kind == 'registry-entry' and fields['index'] == 0:
        return
    path = ROOT / 'raw-trace.jsonl'
    limit = protocol.MAX_TRACE - 256 * 1024 if kind == 'registry-entry' else protocol.MAX_TRACE
    if path.stat().st_size + len(encoded) > limit:
        if kind == 'registry-entry':
            raise TraceStorageBound('trace storage bound exceeded')
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
    if registry['key_offset'] is None:
        raise UnsupportedKeyLayout(registry['key_unavailable'] or 'item key storage was not established')
    if control == protocol.CONTROL['access_failure']:
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
        if not key or key in keys or any(character.isspace() or ord(character) < 32 for character in key):
            raise UnsupportedKeyLayout('item key layout not established: empty, duplicate, or invalid item key')
        keys.add(key)
        objects.add(obj)
        emit('registry-entry', name=registry['name'], owner=hex(owner), index=index, object=hex(obj), key=key, thread=thread)
        if control == protocol.CONTROL['worker_loss'] and index == 0:
            emit('worker-loss-ready')
            (ROOT / 'worker-loss-ready').touch(exist_ok=False)
            return True
    if count != uint(process, owner + registry['count_offset'], 4) or data != uint(process, owner + registry['data_offset']):
        raise RuntimeError('registry changed during snapshot')
    if control != protocol.CONTROL['missing_terminal']:
        emit('registry-end', name=registry['name'], owner=hex(owner), count=count, producerLastSequence=sequence + 1, thread=thread)
    finished = True
    return True


def registry_callback(frame, name):
    global registry, registry_owner, control, finished, safe_pause
    is_return = name.startswith('registry-return:')
    selected = name.split(':', 1)[1]
    registry = request['registries'][selected]
    registry_owner = registry_owners.get(selected)
    control = request['control'] if selected == request['control_registry'] else protocol.CONTROL['normal']
    try:
        if not is_return:
            result = registry_begin(frame)
            registry_owners[selected] = registry_owner
            breakpoints[name].SetEnabled(False)
            return result
        breakpoints[name].SetEnabled(False)
        registry_snapshot(frame)
    except (UnsupportedKeyLayout, TraceStorageBound) as error:
        emit('registry-unsupported', name=selected, reason=str(error), thread=frame.GetThread().GetThreadID())
    except Exception:
        emit('registry-unavailable', name=selected, reason=traceback.format_exc(), thread=frame.GetThread().GetThreadID())
        # Continue only after this callback proved its actual loader-return boundary.
        if selected not in returned_registries:
            finished = True
            safe_pause = False
            return True
    if control == protocol.CONTROL['worker_loss']:
        return True
    finished = session_active.issubset(set(returned_registries))
    safe_pause = finished
    return finished


class FixtureObserver:
    """One file's parser window; registry callbacks own the eventual session pause."""
    def __init__(self, config):
        self.config = config
        self.bindings = config['bindings']
        self.fields = {field['token']: field['name'] for field in self.bindings['fields']}
        registry = config['file'].rsplit('/', 1)[0]
        self.outcome_binding = next((item for item in self.bindings['outcome_registries'] if item['registry'] == registry), None)
        self.questions = {item['index']: item for item in config['questions']}
        self.requested_definitions = {item['definition'] for item in config['questions']}
        self.question_by_token = {(item['definition'], item['token']): item for item in config['questions'] if item['token'] is not None}
        self.diagnostics_requested = any(item['diagnostics'] for item in config['questions'])
        self.control = request['control'] if request['fixture_fault'] else protocol.CONTROL['normal']
        self.registrations = 0
        self.field_count = 0
        self.loading = False
        self.returned = False
        self.owner = None
        self.definitions = {}
        self.constructor_count = 0
        self.occurrences = {index: 0 for index in self.questions}
        self.pending_constructors = {}
        self.pending_fields = {}
        self.diagnostics = 0
        self.active_reader = None

    def hooks(self):
        load_entry = self.outcome_binding['load_entry'] if self.questions and self.outcome_binding else self.bindings['load_entry']
        hooks = [('fixture:load', load_entry)]
        if self.config['registration_entries']:
            hooks.append(('fixture:registration', self.bindings['registration_entry']))
        if self.config['field_reads']:
            hooks.append(('fixture:field', self.bindings['field_entry']))
        if self.questions and self.outcome_binding:
            hooks.extend([
                ('fixture:constructor', self.outcome_binding['constructor_entry']),
                ('fixture:reader', self.outcome_binding['reader_entry']),
                ('fixture:member', self.outcome_binding['member_entry']),
            ])
            if self.diagnostics_requested:
                hooks.extend([
                    ('fixture:malformed', self.outcome_binding['malformed_entry']),
                    ('fixture:unexpected', self.outcome_binding['unexpected_entry']),
                ])
        return hooks

    def emit(self, kind, thread, **fields):
        emit('fixture', event=dict(kind=kind, **fields), thread=thread)

    def location(self, process, reader):
        lexer = uint(process, reader + self.bindings['reader_lexer_offset'])
        source = uint(process, lexer + self.bindings['lexer_file_offset'])
        storage = source + self.bindings['file_name_offset']
        address = uint(process, storage) if uint(process, storage + self.bindings['string_tag_offset'], 1) & 128 else storage
        return string(process, address), uint(process, source + self.bindings['file_line_offset'], 4)

    def stored_string(self, process, storage):
        address = uint(process, storage) if uint(process, storage + self.bindings['string_tag_offset'], 1) & 128 else storage
        return string(process, address)

    def return_hook(self, frame, name):
        process = frame.GetThread().GetProcess()
        hook = process.GetTarget().BreakpointCreateByAddress(register(frame, request['machine']['registers']['return']))
        hook.SetThreadID(frame.GetThread().GetThreadID())
        hook.SetOneShot(True)
        hook.SetScriptCallbackFunction('worker.callback')
        if hook.GetNumResolvedLocations() != 1:
            raise RuntimeError('fixture dynamic return hook unresolved')
        breakpoints[name] = hook

    def finish_questions(self, process, thread):
        for index, question in self.questions.items():
            unavailable = question['unavailable']
            owner = self.definitions.get(question['definition'])
            if unavailable:
                self.emit('field-terminal', thread, question=index, owner=None, definition_line=None,
                    reader_id=question['reader_id'], reader_kind=question['reader_kind'],
                    final_value=None, unavailable=unavailable)
            elif owner is None:
                self.emit('field-terminal', thread, question=index, owner=None, definition_line=None,
                    reader_id=question['reader_id'], reader_kind=question['reader_kind'],
                    final_value=None, unavailable='Requested definition constructor was not observed')
            else:
                value = self.stored_string(process, owner['owner'] + question['storage_offset'])
                self.emit('field-terminal', thread, question=index, owner=hex(owner['owner']),
                    definition_line=owner['line'], reader_id=question['reader_id'],
                    reader_kind=question['reader_kind'], final_value=value, unavailable=None)
        if self.diagnostics_requested:
            if self.outcome_binding:
                self.emit('diagnostics-terminal', thread, count=self.diagnostics)
            else:
                self.emit('diagnostics-unavailable', thread,
                    reason='Parser diagnostics are outside this registry binding')

    def on_registration(self, thread, name):
        self.registrations += 1
        self.emit('registration-entry', thread, ordinal=self.registrations)
        if self.registrations != 3:
            return False
        breakpoints[name].SetEnabled(False)
        self.emit('registration-end', thread, count=3)
        if self.control == protocol.CONTROL['worker_loss']:
            emit('worker-loss-ready')
            (ROOT / 'worker-loss-ready').touch(exist_ok=False)
            return True
        return False

    def on_load(self, frame, process, thread, registers):
        file = string(process, register(frame, registers['file']))
        if file != self.config['file']:
            return False
        if self.loading:
            raise RuntimeError('fixture loader entered more than once')
        self.loading = True
        self.emit('load-start', thread, file=file)
        for index, question in self.questions.items():
            supported = (question['unavailable'] is None and question['reader_kind'] == 'String'
                and question['reader_id'] is not None and question['token'] is not None
                and question['storage_offset'] is not None)
            self.emit('field-authority', thread, question=index,
                reader_id=question['reader_id'], reader_kind=question['reader_kind'],
                storage_supported=supported, unavailable=question['unavailable'])
        if self.questions and self.outcome_binding:
            return_address = process.GetTarget().ResolveFileAddress(self.outcome_binding['reader_return'])
            hook = process.GetTarget().BreakpointCreateBySBAddress(return_address)
        else:
            hook = process.GetTarget().BreakpointCreateByAddress(register(frame, registers['return']))
        hook.SetThreadID(thread)
        hook.SetOneShot(True)
        hook.SetScriptCallbackFunction('worker.callback')
        if hook.GetNumResolvedLocations() != 1:
            raise RuntimeError('fixture return hook unresolved')
        breakpoints['fixture:return'] = hook
        if self.control == protocol.CONTROL['access_failure']:
            uint(process, 0)
            raise RuntimeError('access failure unexpectedly read zero')
        return False

    def on_constructor(self, frame, process, registers):
        if not self.loading or self.returned:
            return False
        key = self.stored_string(process, register(frame, 'x2'))
        if key not in self.requested_definitions:
            return False
        if self.constructor_count >= 256:
            raise RuntimeError('fixture definition bound exceeded')
        owner = register(frame, registers['owner'])
        if self.active_reader is None:
            raise RuntimeError('fixture constructor has no active file reader')
        file, line = self.location(process, self.active_reader)
        if file != self.config['file']:
            return False
        self.constructor_count += 1
        dynamic = 'fixture:constructor-return:' + str(self.constructor_count)
        self.pending_constructors[dynamic] = dict(owner=owner, definition=key, line=line)
        self.return_hook(frame, dynamic)
        return False

    def on_constructor_return(self, thread, name):
        breakpoints[name].SetEnabled(False)
        definition = self.pending_constructors.pop(name, None)
        if definition is None:
            return False
        if definition['definition'] in self.definitions:
            raise RuntimeError('fixture definition key constructed more than once')
        self.definitions[definition['definition']] = definition
        self.emit('definition', thread, file=self.config['file'], line=definition['line'],
            definition=definition['definition'], owner=hex(definition['owner']))
        return False

    def on_member(self, frame, process, registers):
        if not self.loading or self.returned:
            return False
        owner = register(frame, registers['owner'])
        definition = next((key for key, value in self.definitions.items() if value['owner'] == owner), None)
        token = register(frame, registers['field-token'])
        question = self.question_by_token.get((definition, token))
        if question is None:
            return False
        index = question['index']
        self.occurrences[index] += 1
        if self.occurrences[index] > 128:
            raise RuntimeError('fixture field occurrence bound exceeded')
        reader = register(frame, registers['reader'])
        file, line = self.location(process, reader)
        if file != self.config['file']:
            raise RuntimeError('fixture member source differs from selected file')
        dynamic = 'fixture:member-return:' + str(index) + ':' + str(self.occurrences[index])
        self.pending_fields[dynamic] = dict(question=index, owner=owner, reader=reader,
            line=line, occurrence=self.occurrences[index], field=question['field'], definition=definition)
        self.return_hook(frame, dynamic)
        return False

    def on_member_return(self, process, thread, name):
        breakpoints[name].SetEnabled(False)
        pending = self.pending_fields.pop(name, None)
        if pending is None:
            return False
        question = self.questions[pending['question']]
        value = self.stored_string(process, pending['owner'] + question['storage_offset'])
        self.emit('field-storage', thread, question=pending['question'], file=self.config['file'],
            line=pending['line'], definition=pending['definition'], field=pending['field'],
            owner=hex(pending['owner']), occurrence=pending['occurrence'], value=value)
        return False

    def on_diagnostic(self, frame, process, thread, registers, stage):
        if not self.loading or self.returned:
            return False
        if self.diagnostics >= 128:
            raise RuntimeError('fixture diagnostic bound exceeded')
        reader = register(frame, registers['owner'])
        text = self.stored_string(process, register(frame, registers['reader']))
        file, line = None, None
        try:
            observed_file, observed_line = self.location(process, reader)
            if observed_file != self.config['file']:
                return False
            file, line = observed_file, observed_line
        except Exception:
            pass
        pending = next((value for value in self.pending_fields.values()
            if value['reader'] == reader), None)
        self.diagnostics += 1
        self.emit('diagnostic', thread, text=text, stage=stage, file=file, line=line,
            definition=pending.get('definition') if pending else None,
            field=pending.get('field') if pending else None,
            occurrence=pending.get('occurrence') if pending else None)
        return False

    def on_field(self, frame, process, thread, registers, name):
        file, line = self.location(process, register(frame, registers['reader']))
        if file != self.config['file']:
            return False
        owner = register(frame, registers['owner'])
        token = register(frame, registers['field-token'])
        if not self.loading or self.returned or not owner or self.owner not in (None, owner):
            raise RuntimeError('fixture field has no matching loader or owner')
        if token not in self.fields or self.field_count >= 2:
            breakpoints[name].SetEnabled(False)
            raise RuntimeError('field outside the bounded category window')
        self.owner = owner
        self.field_count += 1
        self.emit('field-read', thread, file=file, line=line, field=self.fields[token],
            owner=hex(owner), ordinal=self.field_count)
        if self.control == protocol.CONTROL['worker_loss'] and not self.config['registration_entries']:
            emit('worker-loss-ready')
            (ROOT / 'worker-loss-ready').touch(exist_ok=False)
            return True
        return False

    def on_return(self, process, thread):
        if not self.loading or self.returned:
            raise RuntimeError('fixture return has no matching loader entry')
        self.returned = True
        self.finish_questions(process, thread)
        for key, hook in breakpoints.items():
            if key.startswith('fixture:'):
                hook.SetEnabled(False)
        self.emit('load-returned', thread, file=self.config['file'], field_count=self.field_count)
        if self.control != protocol.CONTROL['missing_terminal']:
            self.emit('end', thread, registrations=self.registrations, field_reads=self.field_count,
                field_outcomes=len(self.questions), diagnostics=self.diagnostics,
                producer_last_sequence=sequence + 1)
        return False

    def callback(self, frame, name):
        process = frame.GetThread().GetProcess()
        thread = frame.GetThread().GetThreadID()
        registers = request['machine']['registers']
        if thread != entry_thread:
            raise RuntimeError('fixture callback differs from the launch thread')
        if name == 'fixture:registration':
            return self.on_registration(thread, name)
        if name == 'fixture:load':
            return self.on_load(frame, process, thread, registers)
        if name == 'fixture:constructor':
            return self.on_constructor(frame, process, registers)
        if name == 'fixture:reader':
            if self.loading and not self.returned:
                self.active_reader = register(frame, registers['reader'])
            return False
        if name.startswith('fixture:constructor-return:'):
            return self.on_constructor_return(thread, name)
        if name == 'fixture:member':
            return self.on_member(frame, process, registers)
        if name.startswith('fixture:member-return:'):
            return self.on_member_return(process, thread, name)
        if name == 'fixture:malformed':
            return self.on_diagnostic(frame, process, thread, registers, 'reader-malformed-report')
        if name == 'fixture:unexpected':
            return self.on_diagnostic(frame, process, thread, registers, 'reader-unexpected-report')
        if name == 'fixture:field':
            return self.on_field(frame, process, thread, registers, name)
        if name == 'fixture:return':
            return self.on_return(process, thread)
        return False


fixture = FixtureObserver(request['fixture']) if request['fixture'] else None


def callback(frame, loc, _):
    global finished, callback_active
    callback_active = True
    try:
        name = next(key for key, bp in breakpoints.items() if bp.GetID() == loc.GetBreakpoint().GetID())
        if name.startswith('fixture:'):
            try:
                return fixture.callback(frame, name)
            except Exception:
                fixture.emit('unavailable', frame.GetThread().GetThreadID(), reason=traceback.format_exc())
                return False
        return registry_callback(frame, name)
    except Exception:
        emit('callback-error', error=traceback.format_exc())
        finished = True
        return True
    finally:
        callback_active = False


def run(debugger):
    global entry_thread, session_active, safe_pause
    import lldb
    import sys
    source_hashes = {name: sha(ROOT / 'source' / name) for name in request['source_hashes']}
    target_hash = sha(request['executable'])
    if request['version'] != protocol.VERSION or source_hashes != request['source_hashes'] or target_hash != request['target']:
        raise RuntimeError('worker package or target mismatch')
    atomic('hello', 'hello.json', dict(version=protocol.VERSION, attempt=request['attempt'],
        game=request['game'], worker=os.getpid(), target=target_hash, source_hashes=source_hashes,
        python=sys.version, lldb=lldb.SBDebugger.GetVersionString(), module=lldb.__file__))
    if request['control'] == protocol.CONTROL['worker_loss_before_activation']:
        emit('worker-loss-ready')
        (ROOT / 'worker-loss-ready').touch(exist_ok=False)
        while True:
            time.sleep(.1)
    debugger.SetAsync(True)
    target = debugger.CreateTargetWithFileAndArch(request['executable'], request['machine']['architecture'])
    hooks = [('registry:' + name, value['load_entry']) for name, value in request['registries'].items()]
    controlled_hook = 'registry:' + request['control_registry'] if request['control_registry'] else None
    if fixture:
        hooks.extend(fixture.hooks())
        if request['fixture_fault']:
            controlled_hook = 'fixture:field' if request['fixture']['field_reads'] else 'fixture:registration'
    for name, address in hooks:
        if name == controlled_hook and request['control'] == protocol.CONTROL['missing_hook']:
            continue
        hook = target.BreakpointCreateBySBAddress(target.ResolveFileAddress(address))
        hook.SetScriptCallbackFunction('worker.callback')
        if name == controlled_hook and request['control'] == protocol.CONTROL['late_hook']:
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
            if name.startswith('registry:'):
                session_active.add(name.split(':', 1)[1])
        else:
            if name.startswith('fixture:'):
                fixture.emit('unavailable', entry_thread, reason='required fixture hook missing or late before resume')
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
    if not finished and process.IsValid() and process.GetState() == lldb.eStateRunning:
        stop_error = process.Stop()
        stop_deadline = time.monotonic() + 2
        while process.GetState() != lldb.eStateStopped and time.monotonic() < stop_deadline:
            time.sleep(.02)
        safe_pause = stop_error.Success() and process.GetState() == lldb.eStateStopped and not callback_active
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
