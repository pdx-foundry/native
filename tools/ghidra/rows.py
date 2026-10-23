"""Classify the events of a reader walk (Export.java) into field rows and dispatch facts.

Pure functions over the walk's event lines; export.py runs them during the walk rounds, because a
row's nested reader decides which reader the next round walks. An event line is one of:

    CALL <name> (<prototype>) [a<i>=this+0x<offset>] [a<i>=#0x<constant>] [scope=#0x<mask>]
    CALLIND slot+0x<offset>
    STORE this+0x<offset> size=<bytes> val=<source>
    REF <address> <symbol> slot0=<name> slot5=<entry>=<name>
    LOOP <calls of the loop body, separated by ;>
    KEYTEXT | VALUETEST | SWITCHDEFAULT | SWITCHMISS
"""
import re

# Engine families whose readers the export does not walk: the command grammars, the persistent
# base, script values, the log, the standard library.
NOT_FOLLOWED = ('CEffect::', 'CTrigger::', 'CPersistent::', 'CRootTrigger::', 'CRootEffect::',
                'CScriptableValue', 'CPdxLogFileAndLine', 'std::')
PERSISTENT_BASE = 'CPersistent::ReadMember'
# Calls that only build or log a message, or guard the stack, and change no script state.
AUXILIARY_CALLS = ('CPdxLogFileAndLine::', 'CReader::ReportUnexpected', 'CReader::GetFileLocationDescription',
             'CLogger::', 'CLogStream::', 'CString::CString', 'CString::~CString',
             'CPdxCommonStringAllocator::deallocate', 'operator+', '___stack_chk_fail')
SKIP_CALLS = ('CReader::Skip',)
# Calls that report or log a key, so that a path that makes one rejects the key.
REJECT_CALLS = ('CReader::ReportUnexpected', 'CPdxLogFileAndLine::operator()', 'CLogger::Log')
TEXT_COMPARES = ('_memcmp', '_strcmp', '_strncmp', 'strcmp', 'CString::operator==')
# A loop body that calls one of these reads a list or the reader's statements: the walk followed it.
LOOP_HELPERS = re.compile(r'ReadUniform|InsertAtEmplace|^CReader::Read|^CReader::ParseAdvancedStatement|on=reader$')
MARKERS = ('SWITCHDEFAULT', 'LOOP', 'SWITCHMISS', 'VALUETEST', 'KEYTEXT')

SCALAR_TYPES = {
    'bool': 'Boolean', 'CFixedPoint': 'FixedPoint', 'float': 'Float', 'double': 'Float', 'CString': 'String',
    'int': 'Integer', 'uint': 'Integer', 'short': 'Integer', 'ushort': 'Integer', 'long': 'Integer',
    'ulong': 'Integer', 'uchar': 'Integer', 'char': 'Integer', 'CDate': None,
}
TRIGGER = re.compile(r'ReadTrigger|^CTrigger::Read$|^CAndTrigger::Read')
EFFECT = re.compile(r'ReadEffect|^CEffect::Read$')
SKIP_CONSTRUCTOR = ('CString::', 'CPdxLogFileAndLine::', 'CToken::', 'CEventTarget::', 'std::')


def is_marker(event):
    return event.startswith(MARKERS)


def callee(event):
    """The name, prototype and arguments of a CALL event, or None."""
    match = re.match(r'CALL (.+?) (\(.*\))((?: (?:a\d+|scope)=\S+)*)$', event)
    if not match:
        return None
    arguments = dict(re.findall(r'(a\d+|scope)=(\S+)', match.group(3)))
    return {'name': match.group(1).replace('<EXTERNAL>::', ''), 'proto': match.group(2), 'args': arguments}


def is_constructor(name):
    parts = re.sub(r'<[^<>]*(?:<[^<>]*>[^<>]*)*>', '', name).split('::')
    return len(parts) >= 2 and parts[-1] == parts[-2]


def owner_class(function_name):
    return function_name.rsplit('::', 1)[0] if '::' in function_name else ''


def unwrap(cls):
    match = re.fullmatch(r'(?:CPdxScopedPtrImpl|std::shared_ptr)<(.+?)(?:,(?:false|true))?>', cls)
    return match.group(1) if match else cls


def followed(spec):
    """Whether the export walks the reader that a spec names."""
    return not spec.split(':', 1)[-1].startswith(NOT_FOLLOWED)


def row(kind, value=None, reader_kind=None, repeat='Unknown', always=True, nested=None, family=None, items=None,
        call=None):
    return {'kind': kind, 'value': value, 'reader_kind': reader_kind, 'repeat': repeat, 'conditional': not always,
            'nested': nested, 'family': family, 'items': items, 'call': call}


def classify(events, default_events, constructors, yes_token):
    """The field row of one token's walk. A path that tests the value's token reads a block and a
    bare value differently: its value form is not one form, so the row keeps existence and repeat
    and its branches keep the forms."""
    result = classify_paths(events, default_events, constructors, yes_token)
    texts = [text for _, text in events]
    if 'VALUETEST' in texts and result['value'] in ('Block', 'Scalar'):
        result.update(value='Any', reader_kind=None)
    if result['kind'] in ('persistent', 'serializer', 'object') and result['nested'] is None:
        result.update(value=None, reader_kind=None)
    if result['nested'] and result['nested'].startswith('class:CScriptableValue'):
        result.update(kind='script-value', value=None, reader_kind=None, nested=None, family='Weight')
    return result


def classify_paths(events, default_events, constructors, yes_token):
    """The field row of one token's walk, with the nested reader that it reaches. `yes_token` is the
    identity of the `yes` token, which an inline Boolean stores."""
    texts = [text for _, text in events]
    plain = [text for text in texts if not is_marker(text)]
    if plain == [text for text in default_events if not is_marker(text)]:
        return row('default-routed')
    base_calls = [text for text in plain if text.startswith('CALL ') and text.split(' (')[0].endswith('::ReadMember')]
    if base_calls and all(text in default_events for text in base_calls) and \
            not any(text.startswith(('CALL ', 'CALLIND', 'STORE')) and text not in default_events for text in plain):
        return row('default-routed')

    always = {text for is_always, text in events if is_always}
    calls = [(text in always, callee(text), text) for text in texts if text.startswith('CALL ')]
    calls = [(is_always, call, text) for is_always, call, text in calls if call]
    names = [call['name'] for _, call, _ in calls]
    references = [text for text in texts if text.startswith('REF ')]
    stores = [(text in always, text) for text in texts if text.startswith('STORE ')]
    virtual_read = any(text.startswith(('CALLIND slot+0x20', 'CALLIND slot+0x28')) for text in texts) or \
        'CPersistent::Read' in names

    def constructed_class():
        for name in names:
            if is_constructor(name) and not name.startswith(SKIP_CONSTRUCTOR):
                return name.rsplit('::', 1)[0]
        for name in names:
            match = re.match(r'(?:std::make_shared\S*?<|PdxMakeScopedPtr<)([\w:]+(?:<[^<>]*>)?)', name)
            if match:
                return match.group(1)
        return None

    scalar_read = any(call['name'] == 'CReader::Read' and 'CPersistent' not in call['proto'] for _, call, _ in calls)
    for is_always, call, text in calls:
        element = re.search(r'InsertAtEmplace<((?:CPdxScopedPtrImpl|std::shared_ptr)<[\w:<>,]+?>)>', call['name'])
        element = unwrap(element.group(1)) if element else None
        if element is None:
            pointer = re.match(r'CPdxArray<([\w:]+(?:<[^<>]*>)?)\*,int>::InsertAtEmplace<', call['name'])
            element = pointer.group(1) if pointer else None
        # An array of `X const*` holds references to database items, not objects that it reads.
        if element and element not in ('CString', 'int', 'uint', 'char') and not element.endswith('_const') \
                and not scalar_read:
            simple = element.split('::')[-1]
            vtable = next((re.search(r'slot5=([0-9a-f]+)=', reference).group(1) for reference in references
                           if re.search(r'slot5=[0-9a-f]+=\S*' + re.escape(simple) + r'::ReadMember', reference)), None)
            return row('object-list', 'Block', 'Block', repeat='Accumulate', always=is_always,
                       nested=('0x' + vtable) if vtable else 'class:' + element, call=text)

    for is_always, call, text in calls:
        name, proto, arguments = call['name'], call['proto'], call['args']
        if name == 'CReader::Read':
            parameter = re.search(r'(?:, |\()(\w[\w:<>]*) \* param_1[,)]', proto)
            parameter_type = parameter.group(1) if parameter else ''
            if parameter_type == 'CPersistent':
                serializer = next((reference for reference in references if 'CSerializer' in reference and
                                   re.search(r'slot5=[0-9a-f]+=\S+::ReadMember', reference)), None)
                if serializer:
                    target = re.search(r'slot5=([0-9a-f]+)=(\S+)', serializer)
                    return row('serializer', 'Block', 'Block', always=is_always, nested='0x' + target.group(1),
                               call=text)
                made = next((n for n in names if n.endswith('::CSerializer::CSerializer')), None)
                if made:
                    return row('serializer', 'Block', 'Block', always=is_always,
                               nested='class:' + made.rsplit('::', 1)[0], call=text)
                destination = arguments.get('a2', '')
                element = inserted_element(names)
                if not destination.startswith('this+') and element:
                    return row('object-list', 'Block', 'Block', repeat='Accumulate', always=is_always,
                               nested='class:' + element, call=text)
                if destination.startswith('this+'):
                    target = constructors.get(int(destination[5:], 16))
                    return row('persistent', 'Block', 'Block', always=is_always, nested=target, call=text)
                return row('persistent', 'Block', 'Block', always=is_always, call=text)
            if SCALAR_TYPES.get(parameter_type):
                appended = any('SetSizeAndEmplace' in n or 'InsertAtEmplace' in n for n in names)
                return row('scalar', 'Scalar', SCALAR_TYPES[parameter_type],
                           repeat='Accumulate' if appended else 'Replace', always=is_always, call=text)
            return row('scalar-other', 'Scalar', None, always=is_always, call=text)
        if name.startswith('TokenToEnum<'):
            return row('enum', 'Scalar', 'Keyword', repeat='Replace', always=is_always, call=text)
        if re.match(r'NParserUtil::ReadKeyReference\w*Uniform|CReader::ReadUniform', name):
            element = re.search(r'<([\w:]+)', name).group(1) if '<' in name else ''
            items = 'Reference' if 'KeyReference' in name else {'CString': 'String', 'int': 'Integer',
                                                                'uint': 'Integer', 'CFixedPoint': 'FixedPoint',
                                                                'float': 'Float'}.get(element)
            return row('list', 'Block', 'Reference' if items == 'Reference' else 'Block', repeat='Accumulate',
                       always=is_always, items=items, call=text)
        if name.startswith('NParserUtil::ReadKeyReference'):
            return row('keyref', 'Scalar', 'Reference', repeat='Replace', always=is_always, call=text)
        if TRIGGER.search(name):
            fresh = name.endswith('::CRootTrigger')
            return row('trigger', 'Block', 'Block', repeat='Replace' if fresh else 'Unknown', always=is_always,
                       family='Trigger', call=text)
        if EFFECT.search(name):
            fresh = name.endswith('::CRootEffect')
            return row('effect', 'Block', 'Block', repeat='Replace' if fresh else 'Unknown', always=is_always,
                       family='Effect', call=text)
        if name.endswith('::ReadEntry'):
            return row('entry', 'Any', None, always=is_always, nested=name, call=text)
        if 'InsertAtEmplace' in name:
            element = re.search(r'InsertAtEmplace<((?:CPdxScopedPtrImpl|std::shared_ptr)<[\w:<>,]+?>)>', name)
            element = unwrap(element.group(1)) if element else None
            if element is None:
                array = re.match(r'CPdxArray<([\w:]+(?:<[^<>]*>)?),int>::InsertAtEmplace<', name)
                element = unwrap(array.group(1)) if array else None
            if element in ('CString', 'int', 'uint'):
                return row('list', 'Block', 'Block', repeat='Accumulate', always=is_always,
                           items='String' if element == 'CString' else 'Integer', call=text)
            if element and (virtual_read or constructed_class()):
                return row('object-list', 'Block', 'Block', repeat='Accumulate', always=is_always,
                           nested='class:' + element, call=text)
        if name.startswith('std::shared_ptr<') and '::operator=' in name:
            element = re.match(r'std::shared_ptr<(.+)>::operator=', name).group(1)
            if element == 'CRootTrigger':
                return row('trigger', 'Block', 'Block', repeat='Replace', always=is_always, family='Trigger', call=text)
            if element == 'CRootEffect':
                return row('effect', 'Block', 'Block', repeat='Replace', always=is_always, family='Effect', call=text)
            return row('object', 'Block', 'Block', repeat='Replace', always=is_always, nested='class:' + element,
                       call=text)
        if ('__assign_external' in name or name == 'CString::operator=') and arguments.get('a1', '').startswith('this+'):
            return row('scalar', 'Scalar', 'String', repeat='Replace', always=is_always, call=text)
        if name.startswith('CReader::Skip'):
            return row('skipped', None, 'Skipped', always=is_always, call=text)
        if re.match(r'CReader::Read(FixedPoint|Float)', name):
            return row('scalar', 'Scalar', 'FixedPoint', repeat='Replace', always=is_always, call=text)

    for is_always, store in stores:
        if re.search(r'val=call:(CRootEffect|CEffect)::C', store):
            return row('effect', 'Block', 'Block', repeat='Replace', always=is_always, family='Effect')
        if re.search(r'val=call:(CRootTrigger|CTrigger|CAndTrigger)::C', store):
            return row('trigger', 'Block', 'Block', repeat='Replace', always=is_always, family='Trigger')
        if f'val=#{yes_token:#x}' in store and 'size=1' in store:
            return row('scalar', 'Scalar', 'Boolean', repeat='Replace', always=is_always)

    made = constructed_class()
    if made and virtual_read:
        cls = unwrap(made)
        if cls == 'CRootTrigger':
            return row('trigger', 'Block', 'Block', repeat='Replace', family='Trigger')
        if cls == 'CRootEffect':
            return row('effect', 'Block', 'Block', repeat='Replace', family='Effect')
        stored = any('val=call:' in store and ('operator.new' in store or made in store) for _, store in stores)
        made_call = next((text for _, call, text in calls if call['name'].startswith(made)), None)
        return row('object', 'Block', 'Block', repeat='Replace' if stored else 'Unknown', nested='class:' + cls,
                   call=made_call)
    delegated = next(((call, text) for _, call, text in calls if call['name'].endswith('::ReadMember')), None)
    if delegated:
        return row('delegates', nested=delegated[0]['name'], call=delegated[1])
    helper = next(((call, text) for _, call, text in calls if re.search(r'::Read\w*$', call['name']) and
                   not call['name'].startswith(('CReader::', 'CPersistent::'))), None)
    if helper:
        return row('helper', call=helper[1])
    for is_always, store in stores:
        if 'val=call:' in store and re.search(r'Database::Get|::Get\w+\b', store):
            return row('lookup', 'Scalar', None, repeat='Replace', always=is_always)
    for is_always, store in stores:
        if ('val=call:' in store or 'val=load' in store) and 'CString::CString' in names:
            return row('lookup', 'Scalar', None, repeat='Replace', always=is_always)
    if any('CPdxLogFileAndLine::' in text for text in texts):
        return row('logged')
    if stores:
        return row('store', always=stores[0][0])
    return row('unclassified')


def inserted_element(names):
    """The element class of the first insertion into an array of objects that the paths call."""
    for name in names:
        match = re.match(r'CPdxArray<([\w:]+(?:<[^<>]*>)?),int>::InsertAtEmplace<', name)
        if match:
            return match.group(1)
    return None


def default_kind(events):
    """What the default path does with a token that the reader does not enumerate: `persistent`
    (`CPersistent::ReadMember`, which reports the key as unexpected), `base-reader` (one other
    member reader, with the reader's name), `log-and-skip` (a report or log call), `return` (no
    call), `miss` (a switch with no default), or `helper`. The call that decides the kind must be on
    every path; a path that does neither leaves the default a `helper`."""
    texts = [text for _, text in events]
    if 'SWITCHMISS' in texts:
        return 'miss', None
    always = {text for is_always, text in events if is_always}
    calls = [(text in always, callee(text)) for text in texts if text.startswith('CALL ')]
    calls = [(is_always, call['name']) for is_always, call in calls if call]
    names = [name for _, name in calls]
    on_every_path = {name for is_always, name in calls if is_always}
    other = [text for text in texts if text.startswith(('CALLIND', 'STORE', 'LOOP'))]
    members = member_calls(events)
    rest = [name for name in names if not name.endswith('::ReadMember') and not name.startswith(AUXILIARY_CALLS + SKIP_CALLS)]
    if other or rest or len(members) > 1:
        return 'helper', None
    if members:
        return ('base-reader', members[0]) if members[0] in on_every_path else ('helper', None)
    if PERSISTENT_BASE in names:
        return ('persistent', None) if PERSISTENT_BASE in on_every_path else ('helper', None)
    if any(name.startswith(REJECT_CALLS) for name in on_every_path):
        return 'log-and-skip', None
    return ('return', None) if not names else ('helper', None)


def member_calls(events):
    """The member readers other than `CPersistent::ReadMember` that the paths call, in order."""
    names = [call['name'] for call in (callee(text) for _, text in events if text.startswith('CALL ')) if call]
    return list(dict.fromkeys(name for name in names if name.endswith('::ReadMember') and name != PERSISTENT_BASE))


def loop_bodies(events):
    """The call lists of the loops that the walk cut, as LOOP events give them."""
    return [text[5:].split(';') if len(text) > 5 else [] for _, text in events if text.startswith('LOOP')]


def recognized_loop(body):
    """Whether a cut loop's body is a known helper: a uniform list read, an array insertion, or a
    statement loop whose body the walk followed."""
    return any(LOOP_HELPERS.search(name) for name in body)


def compares_text(events):
    names = [call['name'] for call in (callee(text) for _, text in events if text.startswith('CALL ')) if call]
    return any(name.startswith(TEXT_COMPARES) for name in names)


def looks_up_text(events):
    """Whether a path calls a lookup (`Get*`, `Find*`) other than the reader's own position getters,
    which a key's text may be the argument of."""
    names = [call['name'] for call in (callee(text) for _, text in events if text.startswith('CALL ')) if call]
    return any(re.search(r'::(Get|Find)\w*$|::Find<', name) and not name.startswith('CReader::') for name in names)


def store_of(events, result):
    """The owner offset and size that the row's paths store to, when they name exactly one."""
    offsets = {}
    for _, text in events:
        match = re.match(r'STORE this\+0x([0-9a-f]+) size=(\d+)', text)
        if match:
            offsets.setdefault(int(match.group(1), 16), int(match.group(2)))
    call = callee(result['call']) if result.get('call') else None
    if call:
        for name in ('a1', 'a2'):
            destination = call['args'].get(name, '')
            if destination.startswith('this+') and (name == 'a2' or 'InsertAtEmplace' in call['name']):
                offsets.setdefault(int(destination[5:], 16), None)
    if len(offsets) != 1:
        return None
    (offset, size), = offsets.items()
    return {'offset': offset, 'size': size}


def scope_mask_of(result):
    """The literal scope mask that the row's reading call passes, or None."""
    call = callee(result['call']) if result.get('call') else None
    mask = call['args'].get('scope') if call else None
    return int(mask[1:], 16) if mask else None
