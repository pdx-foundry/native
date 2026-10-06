"""The debugger worker of one game session. It runs inside LLDB.

It sets registry and requested fixture hooks before the game runs. When a loader returns, it reads
the registry's collection and writes what it sees to raw-trace.jsonl. When every observed
registry has returned, it holds the game at that point until the supervisor releases it. A
session that reads the loaded modifier table holds the game where the engine's modifier
documentation returns instead, after all content has loaded. Engine locations arrive in the
request, from Native's binding groups."""
from collections import namedtuple
from functools import partial
import hashlib
import json
import os
from pathlib import Path
import re
import time
import traceback
from threading import Event, Thread
import protocol
import stored_values

ROOT = Path(__file__).resolve().parent.parent
request = None
sequence = 0
entry_thread = None
breakpoints = {}
registry_owners = {}
fixture = None
modifiers = None
diagnostics = None


class SessionProgress:
    """Observed boundaries and failures; observers never choose the session's pause."""
    def __init__(self, active_registries=(), modifier_active=False, fixture_pending=False):
        self.active_registries = set(active_registries)
        if modifier_active:
            self.pause_owner = 'modifiers'
        elif self.active_registries:
            self.pause_owner = 'registries'
        else:
            self.pause_owner = None
        self.returned_registries = []
        self.modifier_returned = False
        self.fixture_pending = fixture_pending
        self.callback_active = False
        self.callback_failed = False
        self.worker_loss_ready = False
        self.deadline_stopped = False


PauseDecision = namedtuple('PauseDecision', ['stop', 'cause'])
progress = SessionProgress()


def decide_pause(state):
    """Choose continuation, a witnessed pause, or a stop without a safe pause."""
    if state.callback_active:
        return PauseDecision(False, None)
    if state.callback_failed or state.worker_loss_ready:
        return PauseDecision(True, None)
    if state.pause_owner == 'modifiers' and state.modifier_returned and not state.fixture_pending:
        return PauseDecision(True, 'content-loaded')
    if state.pause_owner == 'registries' and state.active_registries.issubset(state.returned_registries) and not state.fixture_pending:
        return PauseDecision(True, 'loaders-returned')
    if state.deadline_stopped:
        return PauseDecision(True, 'deadline')
    return PauseDecision(state.pause_owner is None, None)


def fault_control(session_request, target):
    fault = session_request['fault']
    return fault['control'] if fault and fault['target'] == target else protocol.CONTROL['normal']


def requested_hooks(session_request, modifier_observer, fixture_observer):
    """Every hook the session requires, as (name, address)."""
    hooks = [(protocol.HOOK['registry'] + name, value['load_entry']) for name, value in session_request['registries'].items()]
    if modifier_observer:
        hooks.extend(modifier_observer.hooks())
    if fixture_observer:
        hooks.extend(fixture_observer.hooks())
    return hooks


def controlled_hook(session_request):
    """The requested hook that the session's fault control targets, if any."""
    fault = session_request['fault']
    target = fault['target'] if fault else None
    if isinstance(target, dict):
        return protocol.HOOK['registry'] + target['registry']
    if target != 'fixture':
        return None
    return protocol.HOOK['fixture_member']


class UnsupportedKeyLayout(Exception):
    pass


class TraceStorageBound(Exception):
    pass


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def atomic(kind, name, value, limit=protocol.MAX_RECORD):
    encoded = protocol.encode(kind, value, limit)
    temporary = ROOT / (name + '.pending')
    with temporary.open('xb') as output:
        output.write(encoded)
        output.flush()
        os.fsync(output.fileno())
    temporary.rename(ROOT / name)
    return encoded


def publish_diagnostics(value):
    # Unlike control messages, this developer checkpoint replaces its predecessor. Closing
    # before rename is enough for worker-loss retention; each call needs no durability flush.
    encoded = protocol.encode('worker_diagnostics', value, protocol.MAX_WORKER_DIAGNOSTICS)
    pending = ROOT / 'worker-diagnostics.json.pending'
    pending.write_bytes(encoded)
    pending.replace(ROOT / 'worker-diagnostics.json')


def dropped_by_fault(kind, fields, session_request):
    """Whether a requested dropped-record fault leaves this record out of the trace. The record
    still takes its sequence number, so the trace shows the gap."""
    if fault_control(session_request, 'fixture') == protocol.CONTROL['dropped_record'] and kind == 'fixture':
        event = fields['event']
        if event.get('question') == 0 and event.get('occurrence') == 1 and (
                event['kind'] == 'field-storage' or
                (event['kind'] == 'field-parse' and not event['returned'])):
            return True
    return (kind == 'registry-entry' and fields['index'] == 0
            and fault_control(session_request, {'registry': fields['name']}) == protocol.CONTROL['dropped_record'])


def emit(kind, **fields):
    global sequence
    if diagnostics is not None:
        if kind in ('capability-unavailable', 'early-activation-unavailable', 'native-exception', 'callback-error'):
            diagnostics.failure(kind, fields.get('reason', fields.get('error', 'cause unavailable')))
        elif kind == 'session-paused':
            diagnostics.update(kind, thread=fields.get('thread', entry_thread),
                               phase='held', context=None, deadline=None)
        elif kind in ('hooks-requested', 'launch-stopped', 'resume', 'worker-loss-ready'):
            diagnostics.update(kind, thread=fields.get('thread', entry_thread))
    sequence += 1
    record = dict(seq=sequence, run=request['attempt'], kind=kind, **fields)
    encoded = protocol.encode('record', record)
    if dropped_by_fault(kind, fields, request):
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


def memory(process, address, size):
    import lldb
    error = lldb.SBError()
    value = process.ReadMemory(address, size, error) if size else b''
    if error.Fail() or len(value) != size:
        raise RuntimeError('native memory access failed: ' + str(error))
    return value


def uint(process, address, size=8):
    return int.from_bytes(memory(process, address, size), 'little')


def string(process, address):
    import lldb
    error = lldb.SBError()
    value = process.ReadCStringFromMemory(address, 4096, error)
    if error.Fail() or value is None or len(value) >= 4095:
        raise RuntimeError('native string access failed: ' + str(error))
    return value


def cstring(process, storage, tag_offset):
    """The text of the CString at `storage`: in place, or behind its pointer."""
    storage_bytes = memory(process, storage, tag_offset + 1)
    return stored_values.cstring(storage_bytes, tag_offset, partial(memory, process))


def hook_state():
    return {name: dict(enabled=bp.IsEnabled(), locations=bp.GetNumLocations(),
                       resolved=bp.GetNumResolvedLocations(), hits=bp.GetHitCount())
            for name, bp in breakpoints.items()}


def registry_begin(frame, registry, registry_owner):
    process = frame.GetThread().GetProcess()
    thread = frame.GetThread().GetThreadID()
    if registry_owner is not None or thread != entry_thread:
        raise RuntimeError('ambiguous registry loader entry')
    registry_owner = register(frame, request['machine']['registers']['owner'])
    if not registry_owner:
        raise RuntimeError('registry loader receiver is null')
    directory = cstring(process, registry_owner + registry['directory_offset'], registry['string_tag_offset'])
    if directory != registry['directory']:
        raise RuntimeError('registry loader directory mismatch')
    hook = process.GetTarget().BreakpointCreateByAddress(register(frame, request['machine']['registers']['return']))
    hook.SetThreadID(thread)
    hook.SetOneShot(True)
    hook.SetScriptCallbackFunction('worker.callback')
    if hook.GetNumResolvedLocations() != 1:
        raise RuntimeError('registry return hook unresolved')
    breakpoints[protocol.HOOK['registry_return'] + registry['name']] = hook
    emit('registry-load-start', name=registry['name'], owner=hex(registry_owner), directory=directory, thread=thread)
    return registry_owner


def registry_snapshot(frame, registry, registry_owner, control):
    process = frame.GetThread().GetProcess()
    thread = frame.GetThread().GetThreadID()
    if thread != entry_thread:
        raise RuntimeError('registry thread differs from activation witness')
    owner = registry_owner
    if not owner:
        raise RuntimeError('registry receiver is null')
    directory = cstring(process, owner + registry['directory_offset'], registry['string_tag_offset'])
    if directory != registry['directory']:
        raise RuntimeError('registry receiver directory mismatch: ' + directory)
    emit('registry-load-returned', name=registry['name'], owner=hex(owner), thread=thread)
    progress.returned_registries.append(registry['name'])
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
        key = cstring(process, obj + registry['key_offset'], registry['string_tag_offset'])
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
    return False


def registry_callback(frame, name):
    is_return = name.startswith(protocol.HOOK['registry_return'])
    prefix = protocol.HOOK['registry_return'] if is_return else protocol.HOOK['registry']
    selected = name.removeprefix(prefix)
    registry = request['registries'][selected]
    registry_owner = registry_owners.get(selected)
    control = fault_control(request, {'registry': selected})
    try:
        if not is_return:
            registry_owners[selected] = registry_begin(frame, registry, registry_owner)
            breakpoints[name].SetEnabled(False)
            return False
        breakpoints[name].SetEnabled(False)
        return registry_snapshot(frame, registry, registry_owner, control)
    except (UnsupportedKeyLayout, TraceStorageBound) as error:
        emit('registry-unsupported', name=selected, reason=str(error), thread=frame.GetThread().GetThreadID())
    except Exception:
        emit('registry-unavailable', name=selected, reason=traceback.format_exc(), thread=frame.GetThread().GetThreadID())
        # A failed read can continue only after the loader-return boundary was witnessed.
        if selected not in progress.returned_registries:
            progress.callback_failed = True
    return False


def interpret_fixture_log(text, file, file_prefix, line_prefix, returned):
    """Keep a matching file's diagnostic, with a line only when its source is unambiguous. An
    inline script that the file calls names the call's line in its own source."""
    if file not in text:
        return None
    prefix = re.escape(file_prefix + file + line_prefix)
    inline = re.escape(file) + r':([0-9]+)\(inline_script\) '
    lines = {int(match) for match in re.findall(prefix + r'([0-9]+)', text) + re.findall(inline, text)}
    line = next(iter(lines)) if len(lines) == 1 else None
    stage = protocol.DIAGNOSTIC_STAGE['engine_validation'] if returned else protocol.DIAGNOSTIC_STAGE['engine_parser']
    return dict(text=text, stage=stage, file=file, line=line)


class FixtureObserver:
    """One file's parser window; registry callbacks own the eventual session pause."""
    def __init__(self, config):
        self.config = config
        self.bindings = config['bindings']
        registry = config['file'].rsplit('/', 1)[0]
        self.outcome_binding = next((item for item in self.bindings['outcome_registries'] if item['registry'] == registry), None)
        self.questions = {item['index']: item for item in config['questions']}
        self.requested_definitions = {item['definition'] for item in config['questions']}
        self.question_by_token = {(item['definition'], item['token']): item for item in config['questions'] if item['token'] is not None}
        self.diagnostics_requested = any(item['diagnostics'] for item in config['questions'])
        self.validation = config['validation']
        self.validation_finished = False
        self.control = fault_control(request, 'fixture')
        self.loading = False
        self.returned = False
        self.definitions = {}
        self.constructor_count = 0
        self.occurrences = {index: 0 for index in self.questions}
        self.pending_constructors = {}
        self.pending_fields = {}
        self.diagnostics = 0
        self.active_reader = None

    def diagnostic_hooks(self):
        binding = self.bindings['validation']
        if not self.diagnostics_requested or binding is None:
            return []
        return [
            (protocol.HOOK['fixture_log'], binding['log_entry']),
            (protocol.HOOK['fixture_unformatted_log'], binding['unformatted_log_entry']),
            (protocol.HOOK['fixture_stream_log'], binding['stream_log_entry']),
            (protocol.HOOK['fixture_sourced_log'], binding['sourced_log_entry']),
        ]

    def validation_hooks(self):
        if not self.validation:
            return []
        return [(protocol.HOOK['fixture_validated'], self.bindings['validation']['complete_entry'])]

    def hooks(self):
        load_entry = self.outcome_binding['load_entry']
        hooks = [(protocol.HOOK['fixture_load'], load_entry)]
        hooks.extend(self.diagnostic_hooks())
        hooks.extend(self.validation_hooks())
        if self.questions and self.outcome_binding:
            hooks.extend([
                (protocol.HOOK['fixture_constructor'], self.outcome_binding['constructor_entry']),
                (protocol.HOOK['fixture_reader'], self.outcome_binding['reader_entry']),
                (protocol.HOOK['fixture_member'], self.outcome_binding['member_entry']),
            ])
            if self.diagnostics_requested:
                hooks.extend([
                    (protocol.HOOK['fixture_malformed'], self.outcome_binding['malformed_entry']),
                    (protocol.HOOK['fixture_unexpected'], self.outcome_binding['unexpected_entry']),
                ])
        return hooks

    def emit(self, kind, thread, **fields):
        emit('fixture', event=dict(kind=kind, **fields), thread=thread)

    def location(self, process, reader):
        lexer = uint(process, reader + self.bindings['reader_lexer_offset'])
        source = uint(process, lexer + self.bindings['lexer_file_offset'])
        file = self.stored_string(process, source + self.bindings['file_name_offset'])
        return file, uint(process, source + self.bindings['file_line_offset'], 4)

    def stored_string(self, process, storage):
        return cstring(process, storage, self.bindings['string_tag_offset'])

    def fixture_line(self, file, line):
        """The fixture line of a reader location: its own line in the fixture, or the line of the
        fixture's call when the reader reads expanded text, which names the call in its source:
        `<file>:<line>(inline_script) <script>` or `scripted effect <name> at file: <file> line:
        <line>`. None for any other source."""
        if file == self.config['file']:
            return line
        binding = self.bindings['validation'] or {}
        calls = [re.escape(self.config['file']) + r':([0-9]+)\(inline_script\) ']
        if binding:
            calls.append(re.escape(binding['source_file_prefix'] + self.config['file']
                + binding['source_line_prefix']) + r'([0-9]+)')
        for call in calls:
            match = re.search(call, file)
            if match:
                return int(match.group(1))
        return None

    def stored_value(self, process, owner, binding):
        return stored_values.decode(lambda address, size: uint(process, address, size),
                                    lambda address: self.stored_string(process, address),
                                    owner + binding['offset'], binding['decoder'])

    def return_hook(self, frame, name):
        process = frame.GetThread().GetProcess()
        hook = process.GetTarget().BreakpointCreateByAddress(register(frame, request['machine']['registers']['return']))
        hook.SetThreadID(frame.GetThread().GetThreadID())
        # Recursive CPersistent reads share a return address but have different caller stacks.
        hook.SetCondition('$sp == ' + hex(register(frame, 'sp')))
        hook.SetOneShot(True)
        hook.SetScriptCallbackFunction('worker.callback')
        if hook.GetNumResolvedLocations() != 1:
            raise RuntimeError('fixture dynamic return hook unresolved')
        breakpoints[name] = hook

    def finish_questions(self, process, thread):
        for index, question in self.questions.items():
            unavailable = question['storage_unavailable']
            owner = self.definitions.get(question['definition'])
            if unavailable:
                self.emit('field-terminal', thread, question=index, owner=None, definition_line=None,
                    reader_id=question['reader_id'], reader_kind=question['reader_kind'], reader_family=question['reader_family'],
                    final_value=None, unavailable=unavailable)
            elif owner is None:
                self.emit('field-terminal', thread, question=index, owner=None, definition_line=None,
                    reader_id=question['reader_id'], reader_kind=question['reader_kind'], reader_family=question['reader_family'],
                    final_value=None, unavailable='Requested definition constructor was not observed')
            else:
                value = self.stored_value(process, owner['owner'], question['storage'])
                self.emit('field-terminal', thread, question=index, owner=hex(owner['owner']),
                    definition_line=owner['line'], reader_id=question['reader_id'],
                    reader_kind=question['reader_kind'], reader_family=question['reader_family'], final_value=value, unavailable=None)
            if question['parsing']:
                parsing_unavailable = None
                if question['token'] is None:
                    parsing_unavailable = 'No proven field token for parser observation'
                elif question.get('nested') and unavailable:
                    parsing_unavailable = unavailable
                elif owner is None:
                    parsing_unavailable = 'Requested definition constructor was not observed'
                self.emit('parsing-terminal', thread, question=index,
                    count=self.occurrences[index], unavailable=parsing_unavailable)

    def finish_diagnostics(self, thread):
        if self.diagnostics_requested:
            if self.outcome_binding:
                self.emit('diagnostics-terminal', thread, count=self.diagnostics)
            else:
                self.emit('diagnostics-unavailable', thread,
                    reason='Parser diagnostics are outside this registry binding')

    def on_load(self, frame, process, thread, registers):
        file = string(process, register(frame, registers['file']))
        if file != self.config['file']:
            return False
        if self.loading:
            raise RuntimeError('fixture loader entered more than once')
        self.loading = True
        self.emit('load-start', thread, file=file)
        for index, question in self.questions.items():
            storage = question['storage']
            decoder = storage['decoder'] if storage and question['storage_unavailable'] is None else None
            self.emit('field-authority', thread, question=index,
                reader_id=question['reader_id'], reader_kind=question['reader_kind'], reader_family=question['reader_family'],
                storage_decoder=decoder, unavailable=question['storage_unavailable'])
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
        breakpoints[protocol.HOOK['fixture_return']] = hook
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
        dynamic = protocol.HOOK['fixture_constructor_return'] + str(self.constructor_count)
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
        if not question['parsing'] and question['storage_unavailable'] is not None:
            return False
        index = question['index']
        self.occurrences[index] += 1
        if self.occurrences[index] > 128:
            raise RuntimeError('fixture field occurrence bound exceeded')
        reader = register(frame, registers['reader'])
        file, line = self.location(process, reader)
        line = self.fixture_line(file, line)
        if line is None:
            raise RuntimeError('fixture member source differs from selected file')
        file = self.config['file']
        dynamic = protocol.HOOK['fixture_member_return'] + str(index) + ':' + str(self.occurrences[index])
        self.pending_fields[dynamic] = dict(question=index, owner=owner, reader=reader,
            line=line, occurrence=self.occurrences[index], field=question['field'], definition=definition)
        if question['parsing']:
            self.emit('field-parse', frame.GetThread().GetThreadID(), question=index,
                file=file, line=line, definition=definition, field=question['field'],
                owner=hex(owner), occurrence=self.occurrences[index], returned=False)
        if self.control == protocol.CONTROL['worker_loss']:
            emit('worker-loss-ready')
            (ROOT / 'worker-loss-ready').touch(exist_ok=False)
            return True
        self.return_hook(frame, dynamic)
        return False

    def on_member_return(self, process, thread, name):
        breakpoints[name].SetEnabled(False)
        pending = self.pending_fields.pop(name, None)
        if pending is None:
            return False
        question = self.questions[pending['question']]
        if question['parsing']:
            file, line = self.location(process, pending['reader'])
            line = self.fixture_line(file, line)
            self.emit('field-parse', thread, question=pending['question'], file=self.config['file'], line=line,
                definition=pending['definition'], field=pending['field'], owner=hex(pending['owner']),
                occurrence=pending['occurrence'], returned=True)
        if question['storage_unavailable'] is not None:
            return False
        value = self.stored_value(process, pending['owner'], question['storage'])
        self.emit('field-storage', thread, question=pending['question'], file=self.config['file'],
            line=pending['line'], definition=pending['definition'], field=pending['field'],
            owner=hex(pending['owner']), occurrence=pending['occurrence'], value=value)
        return False

    def on_diagnostic(self, frame, process, thread, registers, stage):
        if not self.loading or self.validation_finished or (self.returned and not self.validation):
            return False
        if self.diagnostics >= 128:
            raise RuntimeError('fixture diagnostic bound exceeded')
        reader = register(frame, registers['owner'])
        text = self.stored_string(process, register(frame, registers['reader']))
        file, line = None, None
        try:
            observed_file, observed_line = self.location(process, reader)
            line = self.fixture_line(observed_file, observed_line)
            if line is None:
                return False
            file = self.config['file']
            if observed_file != file:
                text += ' in ' + observed_file + ' near line ' + str(observed_line)
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

    def on_log(self, frame, process, thread, stream=False):
        if not self.loading or self.validation_finished:
            return False
        binding = self.bindings['validation']
        if stream:
            text = string(process, register(frame, binding['stream_log_text_register']))
        else:
            text = self.stored_string(process, register(frame, binding['log_text_register']))
        return self.emit_log(text, thread)

    def on_sourced_log(self, frame, process, thread):
        """An engine error with its owner's source and the instance source that it generated."""
        if not self.loading or self.validation_finished:
            return False
        binding = self.bindings['validation']
        owner = register(frame, binding['sourced_log_owner_register'])
        source = self.stored_string(process, owner + binding['sourced_log_source_offset'])
        if self.config['file'] not in source:
            return False
        text = self.stored_string(process, register(frame, binding['sourced_log_text_register']))
        generated = register(frame, 'x29') - binding['sourced_log_generated_frame_offset']
        generated = self.stored_string(process, generated)
        return self.emit_log(text + ' at ' + source + '\nresulting in source:\n' + generated, thread)

    def emit_log(self, text, thread):
        binding = self.bindings['validation']
        diagnostic = interpret_fixture_log(text, self.config['file'],
            binding['source_file_prefix'], binding['source_line_prefix'], self.returned)
        if diagnostic is None:
            return False
        if self.diagnostics >= 128:
            raise RuntimeError('fixture diagnostic bound exceeded')
        self.diagnostics += 1
        self.emit('diagnostic', thread, **diagnostic,
            definition=None, field=None, occurrence=None)
        return False

    def on_validated(self, thread):
        if not self.validation or not self.returned or self.validation_finished:
            raise RuntimeError('fixture validation has no completed file load')
        self.validation_finished = True
        self.emit('validation-complete', thread, file=self.config['file'])
        self.finish_diagnostics(thread)
        self.finish(thread)
        progress.fixture_pending = False
        return False

    def on_return(self, process, thread):
        if not self.loading or self.returned:
            raise RuntimeError('fixture return has no matching loader entry')
        self.returned = True
        self.finish_questions(process, thread)
        if not self.validation:
            self.finish_diagnostics(thread)
        self.emit('load-returned', thread, file=self.config['file'])
        if self.validation:
            retained = {name for name, _ in self.diagnostic_hooks() + self.validation_hooks()}
            retained.update([protocol.HOOK['fixture_malformed'], protocol.HOOK['fixture_unexpected']])
            for key, hook in breakpoints.items():
                if key.startswith(protocol.HOOK['fixture']) and key not in retained:
                    hook.SetEnabled(False)
        else:
            self.finish(thread)
        return False

    def finish(self, thread):
        for key, hook in breakpoints.items():
            if key.startswith(protocol.HOOK['fixture']):
                hook.SetEnabled(False)
        if self.control != protocol.CONTROL['missing_terminal']:
            self.emit('end', thread,
                field_outcomes=len(self.questions), diagnostics=self.diagnostics,
                producer_last_sequence=sequence + 1)

    def callback(self, frame, name):
        process = frame.GetThread().GetProcess()
        thread = frame.GetThread().GetThreadID()
        registers = request['machine']['registers']
        if name == protocol.HOOK['fixture_log']:
            return self.on_log(frame, process, thread)
        if name == protocol.HOOK['fixture_stream_log']:
            return self.on_log(frame, process, thread, stream=True)
        if name == protocol.HOOK['fixture_sourced_log']:
            return self.on_sourced_log(frame, process, thread)
        if name == protocol.HOOK['fixture_unformatted_log']:
            if self.loading and not self.validation_finished:
                raise RuntimeError('engine diagnostic formatting failed inside the observation window')
            return False
        if thread != entry_thread:
            raise RuntimeError('fixture callback differs from the launch thread')
        if name == protocol.HOOK['fixture_load']:
            return self.on_load(frame, process, thread, registers)
        if name == protocol.HOOK['fixture_constructor']:
            return self.on_constructor(frame, process, registers)
        if name == protocol.HOOK['fixture_reader']:
            if self.loading and not self.returned:
                self.active_reader = register(frame, registers['reader'])
            return False
        if name.startswith(protocol.HOOK['fixture_constructor_return']):
            return self.on_constructor_return(thread, name)
        if name == protocol.HOOK['fixture_member']:
            return self.on_member(frame, process, registers)
        if name.startswith(protocol.HOOK['fixture_member_return']):
            return self.on_member_return(process, thread, name)
        if name == protocol.HOOK['fixture_malformed']:
            return self.on_diagnostic(frame, process, thread, registers, protocol.DIAGNOSTIC_STAGE['reader_malformed'])
        if name == protocol.HOOK['fixture_unexpected']:
            return self.on_diagnostic(frame, process, thread, registers, protocol.DIAGNOSTIC_STAGE['reader_unexpected'])
        if name == protocol.HOOK['fixture_return']:
            return self.on_return(process, thread)
        if name == protocol.HOOK['fixture_validated']:
            return self.on_validated(thread)
        return False


NestedSelector = namedtuple('NestedSelector', 'parent_token owner_offset member_entry leaf_token')


class InlineFixtureObserver(FixtureObserver):
    """One inline file loop; keys become known only when each root read returns."""
    def __init__(self, config):
        super().__init__(config)
        self.inline = self.outcome_binding['inline']
        self.root = None
        self.parent = None
        self.leaf = None
        self.invocations = 0
        self.root_count = 0
        self.thread = None
        self.buffer = []
        self.root_occurrences = {}
        self.root_unavailable = {}
        self.selectors = {}
        for question in self.questions.values():
            nested = question.get('nested')
            if nested:
                selector = NestedSelector(nested['parent_token'], nested['owner_offset'], nested['member_entry'], question['token'])
                self.selectors.setdefault(selector, []).append(question)

    def hooks(self):
        binding = self.outcome_binding
        hooks = [
            (protocol.HOOK['fixture_load'], binding['load_entry']),
            (protocol.HOOK['fixture_reader'], binding['reader_entry']),
            (protocol.HOOK['fixture_constructor'], self.inline['root_return']),
            (protocol.HOOK['fixture_member'], binding['member_entry']),
            (protocol.HOOK['fixture_return'], binding['reader_return']),
        ]
        hooks.extend((protocol.HOOK['fixture_nested'] + str(entry), entry)
                     for entry in sorted({selector.member_entry for selector in self.selectors}))
        if self.diagnostics_requested:
            hooks.extend([
                (protocol.HOOK['fixture_malformed'], binding['malformed_entry']),
                (protocol.HOOK['fixture_unexpected'], binding['unexpected_entry']),
            ])
        hooks.extend(self.diagnostic_hooks())
        return hooks

    def begin_file(self, frame, process, thread):
        lexer = register(frame, 'x1')
        source = uint(process, lexer + self.bindings['lexer_file_offset'])
        file = self.stored_string(process, source + self.bindings['file_name_offset'])
        if file != self.config['file']:
            return False
        if self.loading:
            raise RuntimeError('fixture loader entered more than once')
        self.loading = True
        self.thread = thread
        self.active_reader = register(frame, 'x0')
        self.emit('load-start', thread, file=file)
        for index, question in self.questions.items():
            storage = question['storage']
            self.emit('field-authority', thread, question=index,
                reader_id=question['reader_id'], reader_kind=question['reader_kind'],
                reader_family=question['reader_family'], storage_decoder=storage['decoder'] if storage else None,
                unavailable=question['storage_unavailable'])
        if self.control == protocol.CONTROL['access_failure']:
            uint(process, 0)
            raise RuntimeError('access failure unexpectedly read zero')
        return False

    def begin_root(self, frame, process):
        if not self.loading or self.returned:
            return False
        if register(frame, 'x1') != self.active_reader:
            raise RuntimeError('inline definition has a different file reader')
        if self.root is not None or self.parent is not None or self.leaf is not None:
            raise RuntimeError('inline definition overlaps an unfinished read')
        file, line = self.location(process, self.active_reader)
        if file != self.config['file']:
            raise RuntimeError('inline definition source differs from selected file')
        self.root_count += 1
        if self.root_count > 256:
            raise RuntimeError('fixture definition bound exceeded')
        self.root = dict(owner=register(frame, 'x0'), line=line)
        self.buffer = []
        self.root_occurrences = {}
        self.root_unavailable = {}
        return False

    def begin_parent(self, frame, process):
        if self.root is None or register(frame, 'x0') != self.root['owner']:
            return False
        token = register(frame, 'w2')
        selectors = [selector for selector in self.selectors if selector.parent_token == token]
        if not selectors:
            return False
        if self.parent is not None or self.leaf is not None:
            raise RuntimeError('nested parent overlaps an unfinished read')
        reader = register(frame, 'x1')
        if reader != self.active_reader or self.location(process, reader)[0] != self.config['file']:
            for selector in selectors:
                self.root_unavailable[selector] = 'Nested parent source or reader does not match the file reader'
            return False
        if self.control == protocol.CONTROL['worker_loss']:
            emit('worker-loss-ready')
            (ROOT / 'worker-loss-ready').touch(exist_ok=False)
            return True
        self.invocations += 1
        if self.invocations > 4096:
            raise RuntimeError('nested fixture invocation bound exceeded')
        name = protocol.HOOK['fixture_parent_return'] + str(self.invocations)
        self.parent = dict(token=token, name=name)
        self.return_hook(frame, name)
        return False

    def begin_leaf(self, frame, process, entry):
        if self.root is None or self.parent is None:
            return False
        token = register(frame, 'w2')
        selectors = [selector for selector in self.selectors
                     if selector.parent_token == self.parent['token'] and selector.member_entry == entry and selector.leaf_token == token]
        if not selectors:
            return False
        if self.leaf is not None:
            raise RuntimeError('nested member overlaps an unfinished read')
        reader = register(frame, 'x1')
        for selector in selectors:
            if register(frame, 'x0') != self.root['owner'] + selector.owner_offset:
                self.root_unavailable[selector] = 'Nested member owner does not match the embedded receiver'
                continue
            if reader != self.active_reader:
                self.root_unavailable[selector] = 'Nested member uses a different source reader'
                continue
            file, line = self.location(process, reader)
            if file != self.config['file']:
                self.root_unavailable[selector] = 'Nested member source differs from selected file'
                continue
            occurrence = self.root_occurrences.get(selector, 0) + 1
            if occurrence > 128:
                raise RuntimeError('fixture field occurrence bound exceeded')
            self.root_occurrences[selector] = occurrence
            self.invocations += 1
            if self.invocations > 4096:
                raise RuntimeError('nested fixture invocation bound exceeded')
            name = protocol.HOOK['fixture_member_return'] + str(self.invocations)
            self.leaf = dict(selector=selector, reader=reader, line=line, occurrence=occurrence, name=name)
            self.buffer.append(('parse', dict(self.leaf, returned=False)))
            self.return_hook(frame, name)
        return False

    def finish_leaf(self, process, name):
        breakpoints[name].SetEnabled(False)
        if self.leaf is None or self.leaf['name'] != name or self.root is None:
            raise RuntimeError('nested member return has no matching entry')
        file, line = self.location(process, self.leaf['reader'])
        if file != self.config['file']:
            self.root_unavailable[self.leaf['selector']] = 'Nested member return source differs from selected file'
        else:
            question = self.selectors[self.leaf['selector']][0]
            value = self.stored_value(process, self.root['owner'], question['storage'])
            self.buffer.append(('parse', dict(self.leaf, line=line, returned=True)))
            self.buffer.append(('storage', dict(self.leaf, value=value)))
        self.leaf = None
        return False

    def finish_root(self, process, thread):
        if not self.loading or self.returned:
            return False
        if self.root is None or self.parent is not None or self.leaf is not None:
            raise RuntimeError('inline definition returned with an unfinished owner join')
        key = self.stored_value(process, self.root['owner'], self.inline['key_storage'])['String']
        requested = key in self.requested_definitions
        if requested:
            if key in self.definitions:
                raise RuntimeError('fixture definition key read more than once')
            self.definitions[key] = dict(self.root, definition=key)
            self.emit('definition', thread, file=self.config['file'], line=self.root['line'],
                definition=key, owner=hex(self.root['owner']))
        for selector, reason in self.root_unavailable.items():
            for question in self.selectors[selector]:
                if question['definition'] == key:
                    question['storage_unavailable'] = reason
        for kind, event in self.buffer:
            selector = event.get('selector')
            question = next((question for question in self.selectors.get(selector, []) if question['definition'] == key), None)
            if kind == 'diagnostic':
                self.emit('diagnostic', thread, text=event['text'], stage=event['stage'],
                    file=event['file'], line=event['line'],
                    definition=key if question else None, field=question['field'] if question else None,
                    parent_field=question['parent_field'] if question else None,
                    occurrence=event['occurrence'] if question else None)
                continue
            if question is None:
                continue
            index = question['index']
            self.occurrences[index] = self.root_occurrences[selector]
            fields = dict(question=index, file=self.config['file'], line=event['line'],
                definition=key, field=question['field'], owner=hex(self.root['owner']), occurrence=event['occurrence'])
            if kind == 'parse' and question['parsing']:
                self.emit('field-parse', thread, **fields, returned=event['returned'])
            elif kind == 'storage':
                self.emit('field-storage', thread, **fields, value=event['value'])
        self.root = None
        self.buffer = []
        return False

    def on_diagnostic(self, frame, process, thread, registers, stage):
        if not self.loading or self.returned:
            return False
        reader = register(frame, registers['owner'])
        file, line = self.location(process, reader)
        if file != self.config['file']:
            return False
        if self.diagnostics >= 128:
            raise RuntimeError('fixture diagnostic bound exceeded')
        text = self.stored_string(process, register(frame, registers['reader']))
        self.diagnostics += 1
        pending = self.leaf if self.leaf and self.leaf['reader'] == reader else None
        event = dict(text=text, stage=stage, file=file, line=line,
            selector=pending['selector'] if pending else None,
            occurrence=pending['occurrence'] if pending else None)
        if self.root is not None:
            self.buffer.append(('diagnostic', event))
        else:
            self.emit('diagnostic', thread, text=text, stage=stage, file=file, line=line,
                definition=None, field=None, occurrence=None)
        return False

    def callback(self, frame, name):
        process = frame.GetThread().GetProcess()
        thread = frame.GetThread().GetThreadID()
        if name in {hook for hook, _ in self.diagnostic_hooks()}:
            return super().callback(frame, name)
        if self.loading and not self.returned and thread != self.thread:
            raise RuntimeError('nested fixture callback moved to another thread')
        if name == protocol.HOOK['fixture_load']:
            return self.begin_file(frame, process, thread)
        if name == protocol.HOOK['fixture_reader']:
            return self.begin_root(frame, process)
        if name == protocol.HOOK['fixture_constructor']:
            return self.finish_root(process, thread)
        if name == protocol.HOOK['fixture_member']:
            return self.begin_parent(frame, process)
        if name.startswith(protocol.HOOK['fixture_nested']):
            return self.begin_leaf(frame, process, int(name[len(protocol.HOOK['fixture_nested']):]))
        if name.startswith(protocol.HOOK['fixture_member_return']):
            return self.finish_leaf(process, name)
        if name.startswith(protocol.HOOK['fixture_parent_return']):
            breakpoints[name].SetEnabled(False)
            if self.parent is None or self.parent['name'] != name or self.leaf is not None:
                raise RuntimeError('nested parent return has no completed member join')
            self.parent = None
            return False
        if name == protocol.HOOK['fixture_return']:
            if not self.loading or self.returned or register(frame, 'x0') != self.active_reader:
                return False
            if self.root is not None or self.parent is not None or self.leaf is not None:
                raise RuntimeError('fixture file ended with an unfinished owner join')
            progress.fixture_pending = False
            return self.on_return(process, thread)
        return super().callback(frame, name)


# Bounds that no plausible table exceeds; each bounds a read before it is made.
MAX_TOKENS = 400000
MAX_MODIFIERS = 200000


class ModifierObserver:
    """The loaded modifier table, read when the engine's modifier documentation returns. That
    function has just named every entry through the lexer, so the lexer's lookup is current."""
    def __init__(self, binding):
        self.binding = binding
        self.control = fault_control(request, 'modifiers')
        self.entered = False

    def hooks(self):
        return [(protocol.HOOK['modifiers_documentation'], self.binding['documentation_entry'])]

    def load(self, target, address):
        return target.ResolveFileAddress(address).GetLoadAddress(target)

    def text(self, process, buffer, offset):
        """The engine string object at `offset` in `buffer`."""
        tag_offset = self.binding['string_tag_offset']
        storage = buffer[offset:offset + tag_offset + 1]
        return stored_values.cstring(storage, tag_offset, partial(memory, process))

    def array(self, process, address, stride, bound):
        count = uint(process, address + self.binding['array_count_offset'], 4)
        data = uint(process, address + self.binding['array_data_offset'])
        if count > bound or (count and not data):
            raise RuntimeError('engine array bounds invalid')
        return count, memory(process, data, count * stride)

    def entries(self, process, target):
        b = self.binding
        lookup = self.load(target, b['lookup'])
        if uint(process, lookup + b['array_count_offset'], 4) != uint(process, self.load(target, b['lookup_size']), 4):
            raise RuntimeError('lexer token lookup is not current')
        names, lookup_bytes = self.array(process, lookup, b['lookup_stride'], MAX_TOKENS)
        count, table = self.array(process, self.load(target, b['definitions']), b['definition_stride'], MAX_MODIFIERS)
        entries = []
        for index in range(count):
            row = index * b['definition_stride']
            token = int.from_bytes(table[row + b['token_offset']:row + b['token_offset'] + 4], 'little')
            mask = int.from_bytes(table[row + b['mask_offset']:row + b['mask_offset'] + 4], 'little')
            if token >= names:
                raise RuntimeError('modifier token outside the lexer lookup')
            name = self.text(process, lookup_bytes, token * b['lookup_stride'])
            if not name or any(character.isspace() or ord(character) < 32 for character in name):
                raise RuntimeError('invalid modifier name')
            entries.append(dict(name=name, mask=mask))
        return entries

    def registry_keys(self, process, target, directory, registry):
        if registry['key_offset'] is None:
            return {'unavailable': registry['key_unavailable'] or 'item key storage was not established'}
        try:
            database = uint(process, self.load(target, registry['instance']))
            if not database or database % registry['pointer_size']:
                raise RuntimeError('registry database instance is null')
            header = memory(process, database + registry['directory_offset'], 24)
            if self.text(process, header, 0) != directory:
                raise RuntimeError('registry database directory mismatch')
            count = uint(process, database + registry['count_offset'], 4)
            data = uint(process, database + registry['data_offset'])
            if count > 100000 or (count and (not data or data % registry['pointer_size'])):
                raise RuntimeError('registry collection bounds invalid')
            size = registry['pointer_size']
            pointers = memory(process, data, count * size)
            keys = []
            for index in range(count):
                item = int.from_bytes(pointers[index * size:index * size + size], 'little')
                if not item or item % registry['pointer_size']:
                    raise RuntimeError('invalid registry object')
                key = self.text(process, memory(process, item + registry['key_offset'], 24), 0)
                if not key or key in keys or any(character.isspace() or ord(character) < 32 for character in key):
                    raise RuntimeError('item key layout not established: empty, duplicate, or invalid item key')
                keys.append(key)
            return {'keys': keys}
        except Exception as error:
            return {'unavailable': str(error)}

    def on_return(self, frame, name):
        breakpoints[name].SetEnabled(False)
        process = frame.GetThread().GetProcess()
        target = process.GetTarget()
        thread = frame.GetThread().GetThreadID()
        try:
            entries = self.entries(process, target)
            registries = {directory: self.registry_keys(process, target, directory, registry)
                          for directory, registry in self.binding['registries'].items()}
            encoded = atomic('modifier_table', 'loaded-modifiers.json',
                             dict(attempt=request['attempt'], entries=entries, registries=registries),
                             protocol.MAX_MODIFIER_TABLE)
            emit('modifier-table', count=len(entries), bytes=len(encoded),
                 sha256=hashlib.sha256(encoded).hexdigest(), thread=thread)
            if self.control == protocol.CONTROL['worker_loss']:
                emit('worker-loss-ready')
                (ROOT / 'worker-loss-ready').touch(exist_ok=False)
                return True
            emit('modifier-table-end', count=len(entries), producerLastSequence=sequence + 1, thread=thread)
        except Exception:
            emit('modifier-unavailable', reason=traceback.format_exc(), thread=thread)
        # The return of the documentation function is the witnessed boundary of this pause.
        progress.modifier_returned = True
        return False

    def callback(self, frame, name):
        thread = frame.GetThread().GetThreadID()
        if thread != entry_thread:
            raise RuntimeError('modifier documentation runs on another thread than the launch')
        if name == protocol.HOOK['modifiers_return']:
            return self.on_return(frame, name)
        if self.entered:
            raise RuntimeError('modifier documentation entered more than once')
        self.entered = True
        breakpoints[name].SetEnabled(False)
        emit('modifier-documentation-entered', thread=thread)
        hook = frame.GetThread().GetProcess().GetTarget().BreakpointCreateByAddress(
            register(frame, request['machine']['registers']['return']))
        hook.SetThreadID(thread)
        hook.SetOneShot(True)
        hook.SetScriptCallbackFunction('worker.callback')
        if hook.GetNumResolvedLocations() != 1:
            raise RuntimeError('modifier documentation return hook unresolved')
        breakpoints[protocol.HOOK['modifiers_return']] = hook
        return False


def callback(frame, loc, _):
    progress.callback_active = True
    worker_loss_ready = False
    try:
        name = next(key for key, bp in breakpoints.items() if bp.GetID() == loc.GetBreakpoint().GetID())
        if name.startswith(protocol.HOOK['fixture']):
            try:
                worker_loss_ready = fixture.callback(frame, name)
            except Exception:
                fixture.emit('unavailable', frame.GetThread().GetThreadID(), reason=traceback.format_exc())
                progress.fixture_pending = False
        elif name.startswith(protocol.HOOK['modifiers']):
            worker_loss_ready = modifiers.callback(frame, name)
        else:
            worker_loss_ready = registry_callback(frame, name)
    except Exception:
        progress.callback_failed = True
        emit('callback-error', error=traceback.format_exc())
    finally:
        if worker_loss_ready:
            progress.worker_loss_ready = True
        progress.callback_active = False
    return decide_pause(progress).stop


def disable_observation_hooks():
    """File-load hooks have no work after admission and must not stop later engine calls."""
    for hook in breakpoints.values():
        hook.SetEnabled(False)


def pause_registers(process, thread_id):
    # Re-read registers: LLDB can retain a stale frame PC after an engine call.
    frame = process.GetThreadByID(thread_id).GetFrameAtIndex(0)
    return {name: register(frame, name) for name in ('pc', 'sp', 'fp', 'lr')}


def run_script_check(checks, check, completion):
    try:
        completion['result'] = {'Ok': checks.check(check)}
        if diagnostics is not None:
            diagnostics.update('script-check-completed', phase='held', context=None, deadline=None)
    except Exception as error:
        if diagnostics is not None:
            diagnostics.failure('script-check', error)
        completion['result'] = {'Err': traceback.format_exc()}


def attach(target, info, error, timeout=15):
    completed = Event()

    def expire():
        if completed.wait(timeout):
            return
        try:
            emit('capability-unavailable', reason=f'debugger attach timed out after {timeout:g} seconds; '
                 'approve one debugger attach in a terminal in the same login session, then retry')
        finally:
            # LLDB shutdown can wait on the blocked attach. The supervisor owns cleanup.
            os._exit(1)

    watchdog = Thread(target=expire, daemon=True)
    watchdog.start()
    try:
        # LLDB's script command owns API locks: Attach must stay on this thread.
        return target.Attach(info, error)
    finally:
        completed.set()
        watchdog.join()


def run(debugger):
    global request, entry_thread, progress, fixture, modifiers, diagnostics
    import lldb
    import sys
    request = protocol.decode('request', (ROOT / 'worker-request.json').read_bytes())
    from script_checks import WorkerDiagnostics
    diagnostics = WorkerDiagnostics(request['attempt'], request['game'], publish_diagnostics)
    diagnostics.update()
    progress = SessionProgress()
    fixture = FixtureObserver(request['fixture']) if request['fixture'] else None
    if fixture and fixture.outcome_binding and fixture.outcome_binding.get('inline'):
        fixture = InlineFixtureObserver(request['fixture'])
    modifiers = ModifierObserver(request['modifiers']) if request['modifiers'] else None
    control = request['fault']['control'] if request['fault'] else protocol.CONTROL['normal']
    modifier_active = False
    active_registries = set()
    source_hashes = {name: sha(ROOT / 'source' / name) for name in request['source_hashes']}
    target_hash = sha(request['executable'])
    if request['version'] != protocol.VERSION or source_hashes != request['source_hashes'] or target_hash != request['target']:
        raise RuntimeError('worker package or target mismatch')
    atomic('hello', 'hello.json', dict(version=protocol.VERSION, attempt=request['attempt'],
        game=request['game'], worker=os.getpid(), target=target_hash, source_hashes=source_hashes,
        python=sys.version, lldb=lldb.SBDebugger.GetVersionString(), module=lldb.__file__))
    if control == protocol.CONTROL['worker_loss_before_activation']:
        emit('worker-loss-ready')
        (ROOT / 'worker-loss-ready').touch(exist_ok=False)
        while True:
            time.sleep(.1)
    debugger.SetAsync(True)
    target = debugger.CreateTargetWithFileAndArch(request['executable'], request['machine']['architecture'])
    hooks = requested_hooks(request, modifiers, fixture)
    controlled = controlled_hook(request)
    for name, address in hooks:
        if name == controlled and control == protocol.CONTROL['missing_hook']:
            continue
        hook = target.BreakpointCreateBySBAddress(target.ResolveFileAddress(address))
        hook.SetScriptCallbackFunction('worker.callback')
        if name == controlled and control == protocol.CONTROL['late_hook']:
            hook.SetEnabled(False)
        breakpoints[name] = hook
    emit('hooks-requested', hooks=[name for name, _ in hooks])
    error = lldb.SBError()
    diagnostics.update('debugger-attach', deadline=time.monotonic() + 15)
    process = attach(target, lldb.SBAttachInfo(request['game']), error)
    threads = list(process)
    frames = [dict(function=t.GetFrameAtIndex(0).GetFunctionName() or '') for t in threads]
    entry_thread = next((t.GetThreadID() for t in threads if t.GetFrameAtIndex(0).GetFunctionName() == '_dyld_start'), None)
    emit('launch-stopped', error=str(error), pid=process.GetProcessID(), triple=target.GetTriple() or '', frames=frames, thread=entry_thread)
    state = hook_state()
    diagnostic_hooks = [(name, state.get(name, dict(enabled=False, locations=0, resolved=0))) for name, _ in hooks]
    diagnostic_hooks.sort(key=lambda item: bool(item[1]['enabled'] and item[1]['resolved'] == 1))
    diagnostics.update(hooks=[dict(name=name[:240], enabled=hook['enabled'], locations=hook['locations'],
        resolved=hook['resolved']) for name, hook in diagnostic_hooks[:8]])
    if error.Fail():
        emit('capability-unavailable', reason='debugger attach failed: ' + str(error))
        return
    if process.GetState() != lldb.eStateStopped or entry_thread is None or not target.GetTriple().startswith(request['machine']['architecture'] + '-'):
        emit('early-activation-unavailable', reason='ARM64 loader entry not established')
        return
    for name, _ in hooks:
        hook = state.get(name)
        if hook and hook['enabled'] and hook['locations'] == 1 and hook['resolved'] == 1 and hook['hits'] == 0:
            if name.startswith(protocol.HOOK['registry']):
                active_registries.add(name.removeprefix(protocol.HOOK['registry']))
            elif name.startswith(protocol.HOOK['modifiers']):
                modifier_active = True
        else:
            if name.startswith(protocol.HOOK['modifiers']):
                emit('modifier-unavailable', reason='required modifier hook missing or late before resume')
            elif name.startswith(protocol.HOOK['fixture']):
                fixture.emit('unavailable', entry_thread, reason='required fixture hook missing or late before resume')
            else:
                emit('registry-unavailable', name=name.removeprefix(protocol.HOOK['registry']), reason='required registry hook missing or late before resume')
    progress = SessionProgress(active_registries, modifier_active, bool(fixture and (fixture.validation or isinstance(fixture, InlineFixtureObserver))))
    if decide_pause(progress).stop:
        return
    emit('hooks-active-before-resume', hooks=state)
    deadline = time.monotonic() + 15
    diagnostics.update('await-resume-grant', deadline=deadline)
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
    diagnostics.update('observe-load', phase='loading', deadline=deadline)
    while time.monotonic() < deadline and process.IsValid() and not decide_pause(progress).stop:
        if process.GetState() in (lldb.eStateExited, lldb.eStateCrashed, lldb.eStateDetached):
            break
        if process.GetState() == lldb.eStateStopped and any(t.GetStopReason() == lldb.eStopReasonException for t in process):
            stopped = next(t for t in process if t.GetStopReason() == lldb.eStopReasonException)
            frames = [f'0x{frame.GetPC():x} {frame.GetFunctionName() or "unknown"}'
                      for frame in list(stopped)[:8]]
            diagnostics.failure('native-exception', 'native exception',
                **{f'frame_{index}': frame for index, frame in enumerate(frames)})
            emit('native-exception', reason='native exception: ' + '; '.join(frames))
            break
        time.sleep(.02)
    if not decide_pause(progress).stop and process.IsValid() and process.GetState() == lldb.eStateRunning:
        stop_error = process.Stop()
        stop_deadline = time.monotonic() + 2
        while process.GetState() != lldb.eStateStopped and time.monotonic() < stop_deadline:
            time.sleep(.02)
        # A callback that completed while the game was being stopped owns the pause and its cause.
        if not decide_pause(progress).stop:
            progress.deadline_stopped = stop_error.Success() and process.GetState() == lldb.eStateStopped and not progress.callback_active
    if (not decide_pause(progress).stop and process.IsValid()
            and process.GetState() == lldb.eStateStopped and not progress.callback_active):
        progress.deadline_stopped = True
    decision = decide_pause(progress)
    # The supervisor must kill the worker for this fault; do not let LLDB quit first.
    while progress.worker_loss_ready and time.monotonic() < deadline:
        time.sleep(.02)
    if decision.cause and process.GetState() == lldb.eStateStopped:
        emit('session-paused', returned=progress.returned_registries, cause=decision.cause, thread=entry_thread)
        disable_observation_hooks()
        held_registers = pause_registers(process, entry_thread)
        witness = dict(attempt=request['attempt'], game=request['game'], worker=os.getpid(),
            thread=entry_thread, returned=progress.returned_registries, generation=0, state='held')
        atomic('pause', 'session-paused.json', witness)
        checks = None
        if request.get('script_checks'):
            import script_checks
            checks = script_checks.ScriptChecks(process, entry_thread, request['script_checks'],
                                                request['attempt'], protocol.SCRIPT_LIMITS, diagnostics=diagnostics,
                                                control=fault_control(request, 'script-checks'))
        check_thread = None
        current_check = None
        completion = {}
        while not (ROOT / 'session-release').exists():
            checking = check_thread is not None and check_thread.is_alive()
            if not checking and 'failed' not in witness['state']:
                try:
                    if current_check is not None and 'Err' in completion['result']:
                        raise RuntimeError(completion['result']['Err'])
                    if not process.IsValid() or process.GetState() != lldb.eStateStopped:
                        raise RuntimeError('session no longer stopped at the witnessed pause')
                    if pause_registers(process, entry_thread) != held_registers:
                        raise RuntimeError('session registers changed from the witnessed pause')
                    if current_check is not None:
                        atomic('script_reply', 'script-check-reply.json', dict(attempt=request['attempt'],
                            check=current_check['check'], result=completion['result']), 256 * 1024)
                        current_check = None
                    witness['state'] = 'held'
                    check_path = ROOT / 'script-check.json'
                    if check_path.exists():
                        if checks is None:
                            raise RuntimeError('script check requested without a binding')
                        current_check = protocol.decode('script_check', check_path.read_bytes())
                        check_path.unlink()
                        completion = {}
                        witness['state'] = {'checking': current_check['check']}
                        check_thread = Thread(target=run_script_check,
                            args=(checks, current_check, completion), daemon=True)
                        check_thread.start()
                except Exception:
                    witness['state'] = {'failed': traceback.format_exc()}
                    emit('capability-unavailable', reason=witness['state']['failed'])
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
