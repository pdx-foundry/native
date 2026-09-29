"""Read and validate one command snippet with the bound engine recipe.

The supervisor owns the absolute check deadline and disposal. Calls use LLDB's synchronous
continue path so that the returned stop has settled before registers are changed again.
"""
import re
import time


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
    def __init__(self, target, process, binding, sources, check, limit, text_limit):
        self.target = target
        self.process = process
        self.binding = binding
        self.sources = sources
        self.check = check
        self.limit = limit
        self.text_limit = text_limit
        self.stage = 'read'
        self.messages = []
        self.hooks_active = True
        self.bound_reached = False
        address = target.ResolveFileAddress(binding['logger_entry']).GetLoadAddress(target)
        self.hook = target.BreakpointCreateByAddress(address)
        self.hook.SetScriptCallbackBody('import script_checks\nreturn script_checks.capture_callback(frame, bp_loc, internal_dict)')
        self.verify_hook()

    def verify_hook(self):
        self.hooks_active &= (self.hook.IsEnabled() and self.hook.GetNumLocations() == 1
                              and self.hook.GetNumResolvedLocations() == 1)

    def capture(self, frame, location):
        try:
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
        except Exception:
            self.hooks_active = False

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


class EngineCalls:
    def __init__(self, process, thread_id, deadline):
        import lldb
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
            if thread.GetThreadID() != thread_id and not thread.IsSuspended():
                if not thread.Suspend():
                    raise RuntimeError('script check cannot hold another thread')
                self.suspended.append(thread.GetThreadID())
        self.target.GetDebugger().SetAsync(False)

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
        error = lldb.SBError()
        address = self.process.AllocateMemory(size, lldb.ePermissionsReadable | lldb.ePermissionsWritable, error)
        if error.Fail() or len(data) > size:
            raise RuntimeError('script check allocation failed: ' + str(error))
        self.write(address, data + bytes(size - len(data)))
        return address

    def write(self, address, data):
        import lldb
        error = lldb.SBError()
        count = self.process.WriteMemory(address, data, error)
        if error.Fail() or count != len(data):
            raise RuntimeError('script check memory write failed: ' + str(error))

    def call(self, binding, *arguments):
        import lldb
        if time.monotonic() >= self.deadline:
            raise RuntimeError('script check deadline elapsed')
        if len(arguments) != len(binding['widths']) or len(arguments) > 8:
            raise RuntimeError('script check call signature mismatch')
        if any(value < 0 or value >= 1 << width for value, width in zip(arguments, binding['widths'])):
            raise RuntimeError('script check call argument exceeds its bound width')
        changed = {f'x{i}': value for i, value in enumerate(arguments)}
        changed.update(sp=self.stack, lr=self.return_address, pc=self.address(binding['address']))
        for name, value in changed.items():
            if not self.frame().FindRegister(name).SetValueFromCString(hex(value)):
                raise RuntimeError('script check call register write failed')
        if any(self.frame().FindRegister(name).GetValueAsUnsigned() != value for name, value in changed.items()):
            raise RuntimeError('script check call registers differ before resume')
        returned = self.target.BreakpointCreateByAddress(self.return_address)
        returned.SetThreadID(self.thread_id)
        returned.SetOneShot(True)
        if returned.GetNumResolvedLocations() != 1:
            raise RuntimeError('script check return hook unresolved')
        previous_stop = self.process.GetStopID()
        error = self.process.Continue()
        if error.Fail():
            raise RuntimeError('script check could not resume: ' + str(error))
        while True:
            thread = self.process.GetThreadByID(self.thread_id)
            if (self.process.GetState() == lldb.eStateStopped
                    and self.process.GetStopID() > previous_stop
                    and thread.GetStopReason() == lldb.eStopReasonBreakpoint
                    and thread.GetStopReasonDataAtIndex(0) == returned.GetID()
                    and self.frame().FindRegister('pc').GetValueAsUnsigned() == self.return_address):
                break
            if time.monotonic() >= self.deadline:
                raise RuntimeError('script check did not stop at its return hook: ' + str(thread))
            time.sleep(.001)
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
                raise RuntimeError('script check register restoration failed: ' + str(error))
        self.verify_registers()

    def verify_registers(self):
        import lldb
        if self.process.GetState() != lldb.eStateStopped:
            raise RuntimeError('script check did not return to a stopped process')
        for name, saved in self.registers.items():
            error = lldb.SBError()
            actual = self.frame().FindRegister(name).GetData().ReadRawData(error, 0, saved.GetByteSize())
            expected = saved.ReadRawData(error, 0, saved.GetByteSize())
            if error.Fail() or actual != expected:
                raise RuntimeError(f'script check changed a saved register: {name}: {expected!r} != {actual!r}')

    def finish(self):
        self.verify_registers()
        for thread_id in self.suspended:
            if not self.process.GetThreadByID(thread_id).Resume():
                raise RuntimeError('script check cannot restore thread suspension')
        self.target.GetDebugger().SetAsync(True)


class ScriptChecks:
    def __init__(self, process, thread_id, binding, attempt, limits):
        self.process = process
        self.thread_id = thread_id
        self.binding = binding
        self.attempt = attempt
        self.limits = limits
        self.sources = {}

    def reader(self, calls, text, source):
        binding = self.binding
        raw = text.encode('utf-8') + b'\n\0'
        raw_address = calls.allocate(len(raw), raw)
        string = calls.allocate(binding['string_size'])
        calls.call(binding['string_constructor'], string, raw_address)
        blob = calls.allocate(binding['blob_size'])
        calls.call(binding['blob_constructor'], blob)
        calls.call(binding['blob_append'], blob, string)
        memory_file = calls.allocate(binding['file_size'])
        calls.call(binding['file_constructor'], memory_file, blob, *binding['file_arguments'])
        source_bytes = source.encode('ascii') + b'\0'
        source_address = calls.allocate(len(source_bytes), source_bytes)
        calls.call(binding['string_assign'], memory_file + binding['file_name_offset'], source_address)
        observed, truncated = read_string(self.process, memory_file + binding['file_name_offset'], binding['string_tag_offset'], 4096)
        if observed != source or truncated:
            raise RuntimeError('script check memory file source differs')
        lexer = calls.allocate(binding['lexer_size'])
        calls.call(binding['lexer_constructor'], lexer, memory_file, binding['lexer_argument'])
        reader = calls.allocate(binding['reader_size'])
        calls.call(binding['reader_constructor'], reader, lexer)
        return reader

    def check(self, request):
        global _active_capture
        if request['attempt'] != self.attempt or request['check'] != len(self.sources) + 1:
            raise RuntimeError('foreign or out-of-order script check')
        if len(request['text'].encode('utf-8')) > self.limits['text'] or '\0' in request['text']:
            raise RuntimeError('script check text bound exceeded')
        if request['check'] > self.limits['checks'] or request['scope'] not in self.binding['scopes'].values():
            raise RuntimeError('script check count or scope is invalid')
        command = self.binding[request['kind']]
        source = f"native_{self.attempt}_{request['check']}.txt"
        self.sources[source] = request['check']
        calls = EngineCalls(self.process, self.thread_id, time.monotonic() + self.limits['seconds'])
        capture = DiagnosticCapture(calls.target, self.process, self.binding, self.sources,
                                    request['check'], self.limits['diagnostics'], self.limits['text'])
        _active_capture = capture
        try:
            reader = self.reader(calls, request['text'], source)
            owner = calls.allocate(command['size'])
            calls.call(command['constructor'], owner)
            for write in command['writes']:
                value = calls.address(write['value']) if write['relocate'] else write['value']
                calls.write(owner + write['offset'], value.to_bytes(write['width'], 'little'))
            calls.call(command['read'], owner, reader, request['scope'])
            children = read_unsigned(self.process, owner + command['children_offset'], 4)
            capture.stage = 'validation'
            for database in command['validation']:
                instance = read_unsigned(self.process, calls.address(database['instance']))
                if not instance:
                    raise RuntimeError('script check database is not initialized')
                calls.call(database['post_init'], instance)
                calls.call(database['post_validate'], instance)
            calls.finish()
            return capture.finish(children)
        finally:
            _active_capture = None
            calls.target.BreakpointDelete(capture.hook.GetID())
