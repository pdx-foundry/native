"""Read and validate one command snippet with the bound engine recipe.

The supervisor owns the absolute check deadline and disposal. Calls use LLDB's synchronous
continue path so that the returned stop has settled before registers are changed again.
"""
import re
import time

import stored_values


_active_capture = None


def capture_callback(frame, location, _):
    if _active_capture is not None:
        _active_capture.capture(frame, location)
    return False


def attribute_message(text, sources):
    """Only a source-bearing engine message can join a retained check; tokens never suffice."""
    matches = list(re.finditer(r'(?:in file: "([^"]+)"|file: ([^\s]+))(?: (?:near )?line: (\d+))?', text))
    if len(matches) != 1:
        return None, None, text
    match = matches[0]
    source = match.group(1) or match.group(2)
    check = sources.get(source)
    if check is None:
        return None, None, text
    line_text = match.group(3)
    line = int(line_text) if line_text else None
    if line is not None and line < 1:
        line = None
    return check, line, text.replace(source, '<script>')


class DiagnosticCapture:
    def __init__(self, target, process, binding, sources, check, limit, text_limit, thread_id=None):
        self.target = target
        self.process = process
        self.binding = binding
        self.sources = sources
        self.check = check
        self.limit = limit
        self.text_limit = text_limit
        self.thread_id = thread_id
        self.stage = 'read'
        self.messages = []
        self.hooks_active = True
        self.bound_reached = False
        self.failure = None
        address = target.ResolveFileAddress(binding['logger_entry']).GetLoadAddress(target)
        self.hook = target.BreakpointCreateByAddress(address)
        if thread_id is not None:
            self.hook.SetThreadID(thread_id)
        self.hook.SetScriptCallbackBody('import script_checks\nreturn script_checks.capture_callback(frame, bp_loc, internal_dict)')
        self.verify_hook()

    def verify_hook(self):
        self.hooks_active &= (self.hook.IsEnabled() and self.hook.GetNumLocations() == 1
                              and self.hook.GetNumResolvedLocations() == 1)

    def report_hook(self, diagnostics, name):
        if diagnostics is None:
            return
        try:
            diagnostics.update(hooks=[dict(name=name, enabled=self.hook.IsEnabled(),
                locations=self.hook.GetNumLocations(), resolved=self.hook.GetNumResolvedLocations())])
        except Exception:
            pass

    def capture(self, frame, location):
        try:
            if self.thread_id is not None and frame.GetThread().GetThreadID() != self.thread_id:
                return
            if location.GetBreakpoint().GetID() != self.hook.GetID():
                self.hooks_active = False
                return
            if len(self.messages) >= self.limit:
                self.bound_reached = True
                return
            address = frame.FindRegister(self.binding['logger_text_register']).GetValueAsUnsigned()
            text, truncated = read_string(self.process, address, self.binding['string_tag_offset'], self.text_limit)
            self.bound_reached |= truncated
            level = frame.FindRegister(self.binding['logger_level_register']).GetValueAsUnsigned() & 0xffffffff
            if level >= 1 << 31:
                level -= 1 << 32
            self.messages.append((self.stage, level, text))
            self.bound_reached |= len(self.messages) == self.limit
        except Exception as error:
            self.hooks_active = False
            self.failure = str(error)[:256]

    def finish(self, children):
        self.verify_hook()
        self.target.BreakpointDelete(self.hook.GetID())
        result = dict(check=self.check, read_returned=True, children=children,
                      diagnostics=[], foreign=[], unjoined=[], hooks_active=self.hooks_active,
                      bound_reached=self.bound_reached)
        for stage, level, text in self.messages:
            check, line, normalized = attribute_message(text, self.sources)
            message = dict(text=normalized, stage=stage, line=line, level=level)
            if check == self.check:
                result['diagnostics'].append(message)
            elif check is not None:
                result['foreign'].append(dict(check=check, diagnostic=message))
            else:
                result['unjoined'].append(message)
        return result


def read_memory(process, address, size):
    import lldb
    error = lldb.SBError()
    data = process.ReadMemory(address, size, error)
    if error.Fail() or data is None or len(data) != size:
        raise RuntimeError('script check memory read failed: ' + str(error))
    return data


def read_unsigned(process, address, size=8):
    return int.from_bytes(read_memory(process, address, size), 'little')


def read_string(process, address, tag_offset, limit):
    import lldb
    if read_unsigned(process, address + tag_offset, 1) & 128:
        address = read_unsigned(process, address)
    error = lldb.SBError()
    text = process.ReadCStringFromMemory(address, limit + 1, error)
    if error.Fail() or text is None:
        raise RuntimeError('script check string read failed: ' + str(error))
    return text, len(text.encode('utf-8')) >= limit


def stored_text(process, address, tag_offset):
    text, truncated = read_string(process, address, tag_offset, 4096)
    if truncated:
        raise RuntimeError('script check stored text exceeds its bound')
    return text


def loaded_receivers(calls, durations):
    """Receivers by loaded vtable address point. One that does not resolve classifies no child."""
    loaded = {}
    for receiver in durations:
        try:
            loaded[calls.address(receiver['vtable'])] = receiver
        except RuntimeError:
            pass
    return loaded


def group_duration(read_unsigned, read_text, child_address, child, group):
    count = group['count']
    factor = group['factor_offset']
    return dict(
        child=child,
        units=group['units'],
        count=stored_values.decode(read_unsigned, read_text, child_address + count['offset'], count['decoder']),
        factor=None if factor is None else stored_values.signed_integer(read_unsigned(child_address + factor, 4), 32))


def stored_durations(read_unsigned, read_text, owner, command, children, receivers):
    """Each top-level child's duration counts, and whether every child was classified and read.

    A child whose vtable matches no receiver, or whose slots cannot be read, adds no entry and
    leaves the result incomplete. A receiver with incomplete groups does the same for its child.
    """
    stored = []
    complete = True
    try:
        array = read_unsigned(owner + command['children_array_offset'], 8) if children else 0
    except RuntimeError:
        return dict(complete=False, stored=stored)
    for child in range(children):
        try:
            address = read_unsigned(array + 8 * child, 8)
            receiver = receivers.get(read_unsigned(address, 8))
            if receiver is None:
                complete = False
                continue
            stored.extend([group_duration(read_unsigned, read_text, address, child, group)
                           for group in receiver['groups']])
            complete &= receiver['groups_complete']
        except RuntimeError:
            complete = False
    return dict(complete=complete, stored=stored)


class WorkerDiagnostics:
    """One bounded checkpoint; failed reporting cannot replace an engine failure."""
    def __init__(self, attempt, game, publish):
        from threading import Lock
        self.started = time.monotonic()
        self.publish = publish
        self.lock = Lock()
        self.ordinal = 0
        self.deadline = None
        self.state = dict(attempt=attempt, game=game, phase='startup', context=None, thread=None,
                          operation='worker-start', details={}, elapsed_milliseconds=0, deadline_milliseconds=None,
                          last_attempted_call=None, last_completed_call=None, hooks=[], failure=None)

    def update(self, operation=None, **fields):
        try:
            with self.lock:
                if self.state['failure'] is not None:
                    self.write()
                    return
                if operation is not None:
                    self.state['operation'] = operation[:240]
                    self.state['details'] = {}
                if 'deadline' in fields:
                    self.deadline = fields.pop('deadline')
                if 'details' in fields:
                    fields['details'] = {key[:240]: str(value)[:240] for key, value in list(fields['details'].items())[:16]}
                self.state.update(fields)
                self.write()
        except Exception:
            # Reporting has no authority over calls, deadlines or cleanup.
            pass

    def attempt_call(self, operation):
        self.ordinal += 1
        self.update(operation, last_attempted_call=dict(ordinal=self.ordinal, operation=operation[:240]))

    def complete_call(self):
        self.update(last_completed_call=self.state['last_attempted_call'])

    def failure(self, kind, reason, **details):
        try:
            with self.lock:
                if self.state['failure'] is not None:
                    self.write()
                    return
                self.state['failure'] = dict(kind=kind[:240], reason=str(reason)[:240],
                    details={key[:240]: str(value)[:240] for key, value in list(details.items())[:16]})
                self.write()
        except Exception:
            pass

    def write(self):
        self.state['elapsed_milliseconds'] = max(0, int((time.monotonic() - self.started) * 1000))
        self.state['deadline_milliseconds'] = (None if self.deadline is None else
            max(0, int((self.deadline - self.started) * 1000)))
        self.publish(self.state)


class EngineCalls:
    def __init__(self, process, thread_id, deadline, suspend_others=True, diagnostics=None):
        import lldb
        self.diagnostics = diagnostics
        self.note('save-registers', thread=thread_id, deadline=deadline)
        self.process = process
        self.target = process.GetTarget()
        self.thread_id = thread_id
        self.deadline = deadline
        self.register_names = ([f'x{i}' for i in range(29)] + ['fp', 'lr', 'sp', 'cpsr', 'fpsr', 'fpcr']
                               + [f'v{i}' for i in range(32)] + ['pc'])
        frame = self.frame()
        self.registers = {name: frame.FindRegister(name).GetData() for name in self.register_names}
        if any(not data.GetByteSize() for data in self.registers.values()):
            raise RuntimeError('script check cannot save registers')
        self.return_address = frame.FindRegister('pc').GetValueAsUnsigned()
        # Keep the paused frame's 128-byte ARM64 red zone untouched. The native thread
        # stack has its OS guard; a small debugger allocation does not.
        self.stack = (frame.FindRegister('sp').GetValueAsUnsigned() - 256) & ~15
        self.suspended = []
        for thread in process:
            if suspend_others and thread.GetThreadID() != thread_id and not thread.IsSuspended():
                if not thread.Suspend():
                    raise RuntimeError('script check cannot hold another thread')
                self.suspended.append(thread.GetThreadID())
        self.target.GetDebugger().SetAsync(False)

    def note(self, operation=None, **fields):
        diagnostics = getattr(self, 'diagnostics', None)
        if diagnostics is not None:
            diagnostics.update(operation, **fields)

    def fail(self, kind, reason, **details):
        diagnostics = getattr(self, 'diagnostics', None)
        if diagnostics is not None:
            diagnostics.failure(kind, reason, **details)
        raise RuntimeError(reason)

    def stop_details(self, returned):
        """Debugger inspection is best effort and cannot obscure the original failure."""
        try:
            thread = self.process.GetThreadByID(self.thread_id)
            frame = self.frame()
            details = dict(process_state=self.process.GetState(), stop_id=self.process.GetStopID(),
                        stop_reason=thread.GetStopReason(), breakpoint=thread.GetStopReasonDataAtIndex(0),
                        expected_breakpoint=returned.GetID(), thread=thread.GetThreadID(),
                        expected_thread=self.thread_id, pc=hex(frame.FindRegister('pc').GetValueAsUnsigned()),
                        expected_pc=hex(self.return_address), sp=hex(frame.FindRegister('sp').GetValueAsUnsigned()),
                        expected_sp=hex(self.stack))
            try:
                details['stopped_threads'] = '; '.join(
                    f'{item.GetThreadID()}:{item.GetStopReason()}' for item in list(self.process)[:8]
                    if item.GetStopReason())
                import lldb
                if thread.GetStopReason() == lldb.eStopReasonException:
                    details['exception_stack'] = '; '.join(
                        f'0x{frame.GetPC():x} {frame.GetFunctionName() or "unknown"}' for frame in list(thread)[:8])
            except Exception:
                pass
            return details
        except Exception as error:
            return dict(stop_details_unavailable=str(error))

    def frame(self):
        return self.process.GetThreadByID(self.thread_id).GetFrameAtIndex(0)

    def address(self, file_address):
        import lldb
        address = self.target.ResolveFileAddress(file_address).GetLoadAddress(self.target)
        if address == lldb.LLDB_INVALID_ADDRESS:
            raise RuntimeError('script check function is unresolved')
        return address

    def allocate(self, size, data=b''):
        import lldb
        self.note('allocate-memory', details=dict(size=size))
        error = lldb.SBError()
        permissions = lldb.ePermissionsReadable | lldb.ePermissionsWritable
        address = self.process.AllocateMemory(size, permissions, error)
        if error.Fail() or address == lldb.LLDB_INVALID_ADDRESS or len(data) > size:
            self.fail('allocation', 'script check allocation failed: ' + str(error), size=size, address=hex(address), debugger_error=error, data_size=len(data))
        self.note(details=dict(size=size, address=hex(address), debugger_error=str(error)))
        self.write(address, data + bytes(size - len(data)))
        return address

    def write(self, address, data):
        import lldb
        self.note('write-memory')
        error = lldb.SBError()
        count = self.process.WriteMemory(address, data, error)
        if error.Fail() or count != len(data):
            self.fail('memory-write', 'script check memory write failed: ' + str(error), address=hex(address), expected_count=len(data), actual_count=count, debugger_error=error)

    def call(self, bindings, operation, *arguments, result_address=None):
        import lldb
        diagnostics = getattr(self, 'diagnostics', None)
        if diagnostics is not None:
            diagnostics.attempt_call(operation)
        binding = bindings[operation]
        if time.monotonic() >= self.deadline:
            self.fail('call-timeout', 'script check deadline elapsed')
        if len(arguments) != len(binding['widths']) or len(arguments) > 8:
            raise RuntimeError('script check call signature mismatch')
        if any(value < 0 or value >= 1 << width for value, width in zip(arguments, binding['widths'])):
            raise RuntimeError('script check call argument exceeds its bound width')
        changed = {f'x{i}': value for i, value in enumerate(arguments)}
        if result_address is not None:
            changed['x8'] = result_address
        changed.update(sp=self.stack, lr=self.return_address, pc=self.address(binding['address']))
        for name, value in changed.items():
            if not self.frame().FindRegister(name).SetValueFromCString(hex(value)):
                self.fail('register-write', 'script check call register write failed', register=name, expected=hex(value))
        for name, value in changed.items():
            actual = self.frame().FindRegister(name).GetValueAsUnsigned()
            if actual != value:
                self.fail('register-mismatch', 'script check call registers differ before resume', register=name, expected=hex(value), actual=hex(actual))
        returned = self.target.BreakpointCreateByAddress(self.return_address)
        returned.SetThreadID(self.thread_id)
        # Nested world updates can reach the paused instruction on a deeper stack.
        returned.SetCondition('$sp == ' + hex(self.stack))
        returned.SetOneShot(True)
        if returned.GetNumResolvedLocations() != 1:
            self.fail('missing-return-hook', 'script check return hook unresolved', expected_pc=hex(self.return_address), expected_sp=hex(self.stack), resolved=returned.GetNumResolvedLocations(), breakpoint=returned.GetID())
        previous_stop = self.process.GetStopID()
        self.note(details=dict(expected_pc=hex(self.return_address), expected_sp=hex(self.stack), expected_thread=self.thread_id, expected_breakpoint=returned.GetID()))
        error = self.process.Continue()
        self.note(details=self.stop_details(returned))
        if error.Fail():
            self.fail('resume', 'script check could not resume: ' + str(error), debugger_error=error, **self.stop_details(returned))
        while True:
            thread = self.process.GetThreadByID(self.thread_id)
            if (self.process.GetState() == lldb.eStateStopped
                    and self.process.GetStopID() > previous_stop
                    and thread.GetStopReason() == lldb.eStopReasonBreakpoint
                    and thread.GetStopReasonDataAtIndex(0) == returned.GetID()
                    and self.frame().FindRegister('pc').GetValueAsUnsigned() == self.return_address
                    and self.frame().FindRegister('sp').GetValueAsUnsigned() == self.stack):
                break
            if time.monotonic() >= self.deadline:
                kind = 'native-exception' if thread.GetStopReason() == lldb.eStopReasonException else 'return-stop'
                self.fail(kind, 'script check did not stop at its return hook', deadline_elapsed=True, **self.stop_details(returned))
            time.sleep(.001)
        result = self.frame().FindRegister("x0").GetValueAsUnsigned()
        self.target.BreakpointDelete(returned.GetID())
        for name, data in self.registers.items():
            register = self.frame().FindRegister(name)
            if name.startswith('v'):
                # LLDB SetData reports success for vector registers without writing them.
                raw = data.ReadRawData(error, 0, data.GetByteSize())
                value = '{' + ' '.join(f'0x{byte:02x}' for byte in raw) + '}'
                restored = error.Success() and register.SetValueFromCString(value)
            else:
                restored = register.SetData(data, error)
            if not restored:
                self.fail('register-restoration', 'script check register restoration failed: ' + str(error), register=name, debugger_error=error, **self.register_details(name, data))
        self.verify_registers()
        if diagnostics is not None:
            diagnostics.complete_call()
        return result

    def register_details(self, name, saved):
        try:
            import lldb
            error = lldb.SBError()
            expected = saved.ReadRawData(error, 0, saved.GetByteSize())
            actual = self.frame().FindRegister(name).GetData().ReadRawData(error, 0, saved.GetByteSize())
            return dict(expected=expected.hex(), actual=actual.hex())
        except Exception as error:
            return dict(register_details_unavailable=str(error))

    def verify_registers(self):
        import lldb
        if self.process.GetState() != lldb.eStateStopped:
            self.fail('register-restoration', 'script check did not return to a stopped process', actual_state=self.process.GetState())
        for name, saved in self.registers.items():
            error = lldb.SBError()
            actual = self.frame().FindRegister(name).GetData().ReadRawData(error, 0, saved.GetByteSize())
            expected = saved.ReadRawData(error, 0, saved.GetByteSize())
            if error.Fail() or actual != expected:
                self.fail('register-mismatch', f'script check changed a saved register: {name}: {expected!r} != {actual!r}', register=name, expected=expected.hex() if isinstance(expected, bytes) else 'unavailable',
                          actual=actual.hex() if isinstance(actual, bytes) else 'unavailable', debugger_error=error)

    def finish(self):
        self.note('verify-held-pause')
        self.verify_registers()
        for thread_id in self.suspended:
            if not self.process.GetThreadByID(thread_id).Resume():
                raise RuntimeError('script check cannot restore thread suspension')
        self.target.GetDebugger().SetAsync(True)


class ScriptChecks:
    def __init__(self, process, thread_id, binding, attempt, limits, diagnostics=None, control=None):
        self.process = process
        self.thread_id = thread_id
        self.binding = binding
        self.attempt = attempt
        self.limits = limits
        self.sources = {}
        self.diagnostics = diagnostics
        self.control = control

    def reader(self, calls, text, source):
        binding = self.binding
        raw = text.encode('utf-8') + b'\n\0'
        raw_address = calls.allocate(len(raw), raw)
        string = calls.allocate(binding['string_size'])
        calls.call(binding, 'string_constructor', string, raw_address)
        blob = calls.allocate(binding['blob_size'])
        calls.call(binding, 'blob_constructor', blob)
        calls.call(binding, 'blob_append', blob, string)
        memory_file = calls.allocate(binding['file_size'])
        calls.call(binding, 'file_constructor', memory_file, blob, *binding['file_arguments'])
        source_bytes = source.encode('ascii') + b'\0'
        source_address = calls.allocate(len(source_bytes), source_bytes)
        calls.call(binding, 'string_assign', memory_file + binding['file_name_offset'], source_address)
        observed, truncated = read_string(self.process, memory_file + binding['file_name_offset'], binding['string_tag_offset'], 4096)
        if observed != source or truncated:
            raise RuntimeError('script check memory file source differs')
        lexer = calls.allocate(binding['lexer_size'])
        calls.call(binding, 'lexer_constructor', lexer, memory_file, binding['lexer_argument'])
        reader = calls.allocate(binding['reader_size'])
        calls.call(binding, 'reader_constructor', reader, lexer)
        return reader

    def read_command(self, calls, command, reader, scope):
        owner = calls.allocate(command['size'])
        calls.call(command, 'constructor', owner)
        for write in command['writes']:
            value = calls.address(write['value']) if write['relocate'] else write['value']
            calls.write(owner + write['offset'], value.to_bytes(write['width'], 'little'))
        calls.call(command, 'read', owner, reader, scope)
        children = read_unsigned(self.process, owner + command['children_offset'], 4)
        if children > self.limits['text']:
            raise RuntimeError('script check child count exceeds its text bound')
        return owner, children

    def validate_command(self, calls, command):
        for database in command['validation']:
            instance = read_unsigned(self.process, calls.address(database['instance']))
            if not instance:
                raise RuntimeError('script check database is not initialized')
            calls.call(database, 'post_init', instance)
            calls.call(database, 'post_validate', instance)

    def check(self, request):
        global _active_capture
        if request['attempt'] != self.attempt or request['check'] != len(self.sources) + 1:
            raise RuntimeError('foreign or out-of-order script check')
        if len(request['text'].encode('utf-8')) > self.limits['text'] or '\0' in request['text']:
            raise RuntimeError('script check text bound exceeded')
        if request['check'] > self.limits['checks'] or request['scope'] not in self.binding['scopes'].values():
            raise RuntimeError('script check count or scope is invalid')
        if len(request['durations']) > self.limits['durations']:
            raise RuntimeError('script check duration receiver bound exceeded')
        command = self.binding[request['kind']]
        source = f"native_{self.attempt}_{request['check']}.txt"
        self.sources[source] = request['check']
        if self.diagnostics is not None:
            self.diagnostics.update(phase='script-check', context=f"check {request['check']}")
        calls = EngineCalls(self.process, self.thread_id, time.monotonic() + self.limits['seconds'], diagnostics=self.diagnostics)
        if self.control == 'access-failure':
            import lldb
            calls.write(lldb.LLDB_INVALID_ADDRESS, b'\0')
        capture = DiagnosticCapture(calls.target, self.process, self.binding, self.sources,
                                    request['check'], self.limits['diagnostics'], self.limits['text'])
        capture.report_hook(self.diagnostics, 'script-logger')
        _active_capture = capture
        try:
            reader = self.reader(calls, request['text'], source)
            owner, children = self.read_command(calls, command, reader, request['scope'])
            durations = stored_durations(
                lambda address, size: read_unsigned(self.process, address, size),
                lambda address: stored_text(self.process, address, self.binding['string_tag_offset']),
                owner, command, children, loaded_receivers(calls, request['durations']))
            calls.note(phase='script-validation')
            capture.stage = 'validation'
            self.validate_command(calls, command)
            calls.finish()
            return dict(observation=capture.finish(children), durations=durations)
        finally:
            _active_capture = None
            calls.target.BreakpointDelete(capture.hook.GetID())
