"""One prepared country observation from a witnessed normal world update stack."""
import script_checks
from script_checks import EngineCalls, DiagnosticCapture, read_unsigned, read_string

def ready_boundary(frame, binding):
    """Startup callbacks and loader stacks cannot establish world readiness."""
    process = frame.GetThread().GetProcess()
    target = process.GetTarget()
    address = lambda value: target.ResolveFileAddress(value).GetLoadAddress(target)
    state = read_unsigned(process, address(binding['game_state']))
    idler = read_unsigned(process, address(binding['idler']))
    stack = [f.GetFunctionName() for f in frame.GetThread()]
    return (state != 0 and idler != 0
            and read_unsigned(process, state + binding['ready_offset'], 1) == 1
            and read_unsigned(process, idler + binding['paused_offset'], 1) == 1
            and frame.FindRegister('x0').GetValueAsUnsigned() == idler
            and stack[1:1 + len(binding['normal_stack'])] == binding['normal_stack'])


def selected_flags(names, stored):
    return [dict(name=name, remaining=stored.get(name)) for name in names]


class WorldObserver:
    def __init__(self, process, thread_id, setup, parser, attempt, deadline, limits):
        self.process = process
        self.thread_id = thread_id
        self.binding = setup['binding']
        self.input = setup['input']
        self.parser = parser
        self.attempt = attempt
        self.deadline = deadline
        self.limits = limits
        self.initial_country = None
        self.flag_names = {}

    def country(self, calls):
        binding = self.binding
        state = read_unsigned(self.process, calls.address(binding['game_state']))
        idler = read_unsigned(self.process, calls.address(binding['idler']))
        if (not state or not idler or read_unsigned(self.process, state + binding['ready_offset'], 1) != 1
                or read_unsigned(self.process, idler + binding['paused_offset'], 1) != 1):
            raise RuntimeError('loaded world is no longer ready and paused')
        human = calls.call(binding['local_human'], state)
        if not human:
            raise RuntimeError('world has no local human')
        country = calls.call(binding['human_country'], human)
        country_id = read_unsigned(self.process, human + binding['human_country_offset'], 4)
        if (not country or country_id == 0xffffffff
                or read_unsigned(self.process, country + binding['country_id_offset'], 4) != country_id):
            raise RuntimeError('local human country reference is unresolved')
        identity = (country_id, country)
        if self.initial_country is not None and self.initial_country != identity:
            raise RuntimeError('local human country changed during the observation')
        self.initial_country = identity
        return state, country

    def string_result(self, calls, binding, argument):
        output = calls.allocate(self.parser['string_size'])
        calls.call(binding, argument, result_address=output)
        text, truncated = read_string(self.process, output, self.parser['string_tag_offset'], 256)
        if truncated:
            raise RuntimeError('world string exceeds its bound')
        return text

    def date(self, calls, state):
        address = state + self.binding['date_offset']
        raw = read_unsigned(self.process, address, 4)
        return raw, self.string_result(calls, self.binding['date_string'], address)

    def flags(self, calls, scope):
        binding = self.binding
        store = calls.call(binding['scope_flags'], scope)
        if not store:
            raise RuntimeError('country flag store is unavailable')
        count = read_unsigned(self.process, store + binding['flags_count_offset'], 4)
        if count > 4096 or count != read_unsigned(self.process, store + binding['counts_count_offset'], 4):
            raise RuntimeError('country flag arrays disagree or exceed their bound')
        flags = read_unsigned(self.process, store + binding['flags_data_offset'])
        counts = read_unsigned(self.process, store + binding['counts_data_offset'])
        stored = {}
        for index in range(count):
            token = read_unsigned(self.process, flags + index * binding['flag_width'], binding['flag_width'])
            if token not in self.flag_names:
                name_address = calls.call(binding['flag_name'], token)
                name, truncated = read_string(self.process, name_address, self.parser['string_tag_offset'], 256)
                if truncated or not name:
                    raise RuntimeError('country flag name is unavailable')
                self.flag_names[token] = name
            name = self.flag_names[token]
            if name in stored:
                raise RuntimeError('country flag name is unavailable or duplicated')
            value = read_unsigned(self.process, counts + index * 4, 4)
            stored[name] = value if value < 1 << 31 else value - (1 << 32)
        return selected_flags(self.input['flags'], stored)

    def country_scope(self, calls, country):
        binding = self.binding
        scope = calls.allocate(binding['scope_size'])
        calls.call(binding['scope_constructor'], scope, 0)
        calls.call(binding['scope_country'], scope, country)
        if (read_unsigned(self.process, scope + binding['scope_type_offset']) != binding['scope_type']
                or read_unsigned(self.process, scope + binding['scope_id_offset'], 4) != self.initial_country[0]):
            raise RuntimeError('constructed scope does not name the local human country')
        return scope

    def execute_effect(self, calls, scope, source, capture):
        if not self.input['effect']:
            return False
        checks = script_checks.ScriptChecks(self.process, self.thread_id, self.parser,
                                            self.attempt, self.limits)
        reader = checks.reader(calls, self.input['effect'], source)
        owner, children = checks.read_command(calls, self.parser['effect'], reader,
                                              self.binding['scope_type'])
        capture.stage = 'validation'
        checks.validate_command(calls, self.parser['effect'])
        capture.verify_hook()
        if not capture.hooks_active or capture.bound_reached or capture.messages or not children:
            return False
        capture.stage = 'execution'
        calls.call(self.binding['effect_execute'], owner, scope)
        return True

    def sample(self, calls, scope, day, expected_date):
        state, _ = self.country(calls)
        raw, date = self.date(calls, state)
        if raw != expected_date:
            raise RuntimeError('prepared effect changed the engine date' if day == 0
                               else 'engine did not advance exactly one day')
        return dict(day=day, date=date, flags=self.flags(calls, scope))

    def advance_days(self, calls, scope, initial_date):
        samples = []
        for day in range(1, self.input['days'] + 1):
            calls.call(self.binding['fast_forward'], 1, 0)
            samples.append(self.sample(calls, scope, day, initial_date + 24 * day))
        return samples

    def close_effect_capture(self, calls, capture):
        capture.verify_hook()
        if not capture.hooks_active or capture.bound_reached:
            raise RuntimeError(f'world diagnostic capture incomplete: stage={capture.stage}, '
                               f'hook={capture.hooks_active}, bound={capture.bound_reached}, '
                               f'failure={capture.failure!r}')
        # This capture belongs to the prepared effect; simulation logs belong to the day updates.
        script_checks._active_capture = None
        calls.target.BreakpointDelete(capture.hook.GetID())

    def observe(self):
        calls = EngineCalls(self.process, self.thread_id, self.deadline, suspend_others=False)
        source = f"native_world_{self.attempt}.txt"
        capture = DiagnosticCapture(calls.target, self.process, self.parser, {source: 1}, 1,
                                    self.limits['diagnostics'], self.limits['text'], self.thread_id)
        script_checks._active_capture = capture
        try:
            state, country = self.country(calls)
            name = self.string_result(calls, self.binding['country_name'], country)
            if name != self.input['country']:
                raise RuntimeError(f"local human country differs: {name!r}")
            initial_raw, initial_date = self.date(calls, state)
            scope = self.country_scope(calls, country)
            executed = self.execute_effect(calls, scope, source, capture)
            samples = [self.sample(calls, scope, 0, initial_raw)]
            self.close_effect_capture(calls, capture)
            if executed or not self.input['effect']:
                samples.extend(self.advance_days(calls, scope, initial_raw))
            calls.finish()
            diagnostics = [stage + ': ' + text.replace(source, '<world>') for stage, _, text in capture.messages]
            return dict(country=name, initial_date=initial_date, executed=executed,
                        diagnostics=diagnostics, samples=samples)
        finally:
            script_checks._active_capture = None
            calls.target.BreakpointDelete(capture.hook.GetID())
