"""The debugger worker of one game session. It runs inside LLDB.

It sets registry and requested fixture hooks before the game runs. When a loader returns, it reads
the registry's collection and writes what it sees to raw-trace.jsonl. When every observed
registry has returned, it holds the game at that point until the supervisor releases it. A
session that reads the loaded modifier table holds the game where the engine's modifier
documentation returns instead, after all content has loaded. Engine locations arrive in the
request, from Native's binding groups."""
from collections import namedtuple
import hashlib
import json
import os
from pathlib import Path
import re
import time
import traceback
import protocol

ROOT = Path(__file__).resolve().parent.parent
request = None
sequence = 0
entry_thread = None
breakpoints = {}
registry_owners = {}
fixture = None
modifiers = None


class SessionProgress:
    """Observed boundaries and failures; observers never choose the session's pause."""
    def __init__(self, active_registries=(), modifier_active=False, fixture_validation=False):
        self.active_registries = set(active_registries)
        if modifier_active:
            self.pause_owner = 'modifiers'
        elif self.active_registries:
            self.pause_owner = 'registries'
        else:
            self.pause_owner = None
        self.returned_registries = []
        self.modifier_returned = False
        self.fixture_validation_pending = fixture_validation
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
    if state.pause_owner == 'modifiers' and state.modifier_returned and not state.fixture_validation_pending:
        return PauseDecision(True, 'content-loaded')
    if state.pause_owner == 'registries' and state.active_registries.issubset(state.returned_registries) and not state.fixture_validation_pending:
        return PauseDecision(True, 'loaders-returned')
    if state.deadline_stopped:
        return PauseDecision(True, 'deadline')
    return PauseDecision(state.pause_owner is None, None)


def fault_control(session_request, target):
    fault = session_request['fault']
    return fault['control'] if fault and fault['target'] == target else protocol.CONTROL['normal']


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


def dropped_by_fault(kind, fields, session_request):
    """Whether a requested dropped-record fault leaves this record out of the trace. The record
    still takes its sequence number, so the trace shows the gap."""
    if fault_control(session_request, 'fixture') == protocol.CONTROL['dropped_record'] and kind == 'fixture':
        dropped = ('field-read', 1) if session_request['fixture']['field_reads'] else ('registration-entry', 2)
        event = fields['event']
        if (event['kind'], event.get('ordinal')) == dropped:
            return True
    return (kind == 'registry-entry' and fields['index'] == 0
            and fault_control(session_request, {'registry': fields['name']}) == protocol.CONTROL['dropped_record'])


def emit(kind, **fields):
    global sequence
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


def long_cstring(tag):
    """Bit 7 of a CString's tag byte says that it holds a pointer to its text."""
    return tag & 128


def cstring(process, storage, tag_offset):
    """The text of the CString at `storage`: in place, or behind its pointer."""
    tag = uint(process, storage + tag_offset, 1)
    address = uint(process, storage) if long_cstring(tag) else storage
    return string(process, address)


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
    """Keep a matching file's diagnostic, with a line only when its source is unambiguous."""
    if file not in text:
        return None
    prefix = re.escape(file_prefix + file + line_prefix)
    lines = {int(match) for match in re.findall(prefix + r'([0-9]+)', text)}
    line = next(iter(lines)) if len(lines) == 1 else None
    stage = protocol.DIAGNOSTIC_STAGE['engine_validation'] if returned else protocol.DIAGNOSTIC_STAGE['engine_parser']
    return dict(text=text, stage=stage, file=file, line=line)


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
        self.validation = config['validation']
        self.validation_finished = False
        self.control = fault_control(request, 'fixture')
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

    def validation_hooks(self):
        if not self.validation:
            return []
        binding = self.bindings['validation']
        return [
            (protocol.HOOK['fixture_log'], binding['log_entry']),
            (protocol.HOOK['fixture_unformatted_log'], binding['unformatted_log_entry']),
            (protocol.HOOK['fixture_stream_log'], binding['stream_log_entry']),
            (protocol.HOOK['fixture_sourced_log'], binding['sourced_log_entry']),
            (protocol.HOOK['fixture_validated'], binding['complete_entry']),
        ]

    def hooks(self):
        load_entry = self.outcome_binding['load_entry'] if self.questions and self.outcome_binding else self.bindings['load_entry']
        hooks = [(protocol.HOOK['fixture_load'], load_entry)]
        hooks.extend(self.validation_hooks())
        if self.config['registration_entries']:
            hooks.append((protocol.HOOK['fixture_registration'], self.bindings['registration_entry']))
        if self.config['field_reads']:
            hooks.append((protocol.HOOK['fixture_field'], self.bindings['field_entry']))
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
                value = self.stored_string(process, owner['owner'] + question['storage_offset'])
                self.emit('field-terminal', thread, question=index, owner=hex(owner['owner']),
                    definition_line=owner['line'], reader_id=question['reader_id'],
                    reader_kind=question['reader_kind'], reader_family=question['reader_family'], final_value=value, unavailable=None)
            if question['parsing']:
                parsing_unavailable = None
                if question['token'] is None:
                    parsing_unavailable = 'No proven field token for parser observation'
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
            supported = (question['storage_unavailable'] is None and question['reader_kind'] == protocol.READER_KIND['string']
                and question['reader_id'] is not None and question['token'] is not None
                and question['storage_offset'] is not None)
            self.emit('field-authority', thread, question=index,
                reader_id=question['reader_id'], reader_kind=question['reader_kind'], reader_family=question['reader_family'],
                storage_supported=supported, unavailable=question['storage_unavailable'])
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
        if file != self.config['file']:
            raise RuntimeError('fixture member source differs from selected file')
        dynamic = protocol.HOOK['fixture_member_return'] + str(index) + ':' + str(self.occurrences[index])
        self.pending_fields[dynamic] = dict(question=index, owner=owner, reader=reader,
            line=line, occurrence=self.occurrences[index], field=question['field'], definition=definition)
        if question['parsing']:
            self.emit('field-parse', frame.GetThread().GetThreadID(), question=index,
                file=file, line=line, definition=definition, field=question['field'],
                owner=hex(owner), occurrence=self.occurrences[index], returned=False)
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
            self.emit('field-parse', thread, question=pending['question'], file=file, line=line,
                definition=pending['definition'], field=pending['field'], owner=hex(pending['owner']),
                occurrence=pending['occurrence'], returned=True)
        if question['storage_unavailable'] is not None:
            return False
        value = self.stored_string(process, pending['owner'] + question['storage_offset'])
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
        if not self.loading or self.validation_finished:
            return False
        binding = self.bindings['validation']
        owner = register(frame, binding['sourced_log_owner_register'])
        source = self.stored_string(process, owner + binding['sourced_log_source_offset'])
        if self.config['file'] not in source:
            return False
        text = self.stored_string(process, register(frame, binding['sourced_log_text_register']))
        return self.emit_log(text + ' at ' + source, thread)

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
        progress.fixture_validation_pending = False
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
        if not self.validation:
            self.finish_diagnostics(thread)
        self.emit('load-returned', thread, file=self.config['file'], field_count=self.field_count)
        if self.validation:
            retained = {name for name, _ in self.validation_hooks()}
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
            self.emit('end', thread, registrations=self.registrations, field_reads=self.field_count,
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
        if name == protocol.HOOK['fixture_registration']:
            return self.on_registration(thread, name)
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
        if name == protocol.HOOK['fixture_field']:
            return self.on_field(frame, process, thread, registers, name)
        if name == protocol.HOOK['fixture_return']:
            return self.on_return(process, thread)
        if name == protocol.HOOK['fixture_validated']:
            return self.on_validated(thread)
        return False


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

    def read(self, process, address, size):
        import lldb
        error = lldb.SBError()
        value = process.ReadMemory(address, size, error) if size else b''
        if error.Fail() or len(value) != size:
            raise RuntimeError('native memory access failed: ' + str(error))
        return value

    def text(self, process, buffer, offset):
        """The engine string object at `offset` in `buffer`: short text in place, or a pointer
        and a length when bit 7 of its tag byte is set."""
        tag = buffer[offset + self.binding['string_tag_offset']]
        if long_cstring(tag):
            pointer = int.from_bytes(buffer[offset:offset + 8], 'little')
            length = int.from_bytes(buffer[offset + 8:offset + 16], 'little')
            if length > 4096:
                raise RuntimeError('string length outside bound')
            value = self.read(process, pointer, length)
        else:
            if tag > self.binding['string_tag_offset']:
                raise RuntimeError('short string length outside bound')
            value = bytes(buffer[offset:offset + tag])
        return value.decode('utf-8')

    def array(self, process, address, stride, bound):
        count = uint(process, address + self.binding['array_count_offset'], 4)
        data = uint(process, address + self.binding['array_data_offset'])
        if count > bound or (count and not data):
            raise RuntimeError('engine array bounds invalid')
        return count, self.read(process, data, count * stride)

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
            header = self.read(process, database + registry['directory_offset'], 24)
            if self.text(process, header, 0) != directory:
                raise RuntimeError('registry database directory mismatch')
            count = uint(process, database + registry['count_offset'], 4)
            data = uint(process, database + registry['data_offset'])
            if count > 100000 or (count and (not data or data % registry['pointer_size'])):
                raise RuntimeError('registry collection bounds invalid')
            size = registry['pointer_size']
            pointers = self.read(process, data, count * size)
            keys = []
            for index in range(count):
                item = int.from_bytes(pointers[index * size:index * size + size], 'little')
                if not item or item % registry['pointer_size']:
                    raise RuntimeError('invalid registry object')
                key = self.text(process, self.read(process, item + registry['key_offset'], 24), 0)
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


def run(debugger):
    global request, entry_thread, progress, fixture, modifiers
    import lldb
    import sys
    request = protocol.decode('request', (ROOT / 'worker-request.json').read_bytes())
    progress = SessionProgress()
    fixture = FixtureObserver(request['fixture']) if request['fixture'] else None
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
    hooks = [(protocol.HOOK['registry'] + name, value['load_entry']) for name, value in request['registries'].items()]
    fault_target = request['fault']['target'] if request['fault'] else None
    controlled_hook = protocol.HOOK['registry'] + fault_target['registry'] if isinstance(fault_target, dict) else None
    if modifiers:
        hooks.extend(modifiers.hooks())
    if fixture:
        hooks.extend(fixture.hooks())
        if fault_target == 'fixture':
            controlled_hook = protocol.HOOK['fixture_field'] if request['fixture']['field_reads'] else protocol.HOOK['fixture_registration']
    for name, address in hooks:
        if name == controlled_hook and control == protocol.CONTROL['missing_hook']:
            continue
        hook = target.BreakpointCreateBySBAddress(target.ResolveFileAddress(address))
        hook.SetScriptCallbackFunction('worker.callback')
        if name == controlled_hook and control == protocol.CONTROL['late_hook']:
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
    progress = SessionProgress(active_registries, modifier_active, bool(fixture and fixture.validation))
    if decide_pause(progress).stop:
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
    while time.monotonic() < deadline and process.IsValid() and not decide_pause(progress).stop:
        if process.GetState() in (lldb.eStateExited, lldb.eStateCrashed, lldb.eStateDetached):
            break
        if process.GetState() == lldb.eStateStopped and any(t.GetStopReason() == lldb.eStopReasonException for t in process):
            emit('native-exception', reason='native exception stopped the bounded observation')
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
    decision = decide_pause(progress)
    # The supervisor must kill the worker for this fault; do not let LLDB quit first.
    while progress.worker_loss_ready and time.monotonic() < deadline:
        time.sleep(.02)
    if decision.cause and process.GetState() == lldb.eStateStopped:
        emit('session-paused', returned=progress.returned_registries, cause=decision.cause, thread=entry_thread)
        paused_thread = process.GetThreadByID(entry_thread)
        paused_pc = paused_thread.GetFrameAtIndex(0).GetPC()
        witness = dict(attempt=request['attempt'], game=request['game'], worker=os.getpid(),
            thread=entry_thread, returned=progress.returned_registries, generation=0)
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
