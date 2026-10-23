"""Classify the events of a reader walk (Export.java) into field rows and dispatch facts.

Pure functions over the walk's event lines; export.py runs them during the walk rounds, because a
row's nested reader decides which reader the next round walks. An event line is one of:

    CALL <name> (<prototype>) [a<i>=this+0x<offset>] [a<i>=#0x<constant>] [scope=#0x<mask>]
    CALLIND slot+0x<offset>
    STORE this+0x<offset> size=<bytes> val=<source>
    REF <address> <symbol> slot0=<name> slot5=<entry>=<name>
    LOOP <calls of the loop body, separated by ;>
    KEYTEXT | VALUETEST | VALUEBRANCH | SWITCHDEFAULT | SWITCHMISS

VALUETEST is a test of the value's token that the walk does not decide; VALUEBRANCH, in a walk of
one value branch, is the test that the branch's value token decides.
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
# The value form that each skip call reads past.
SKIP_FORMS = {'CReader::SkipToken': 'Any', 'CReader::SkipBody': 'Block'}
# Calls that report or log a key, so that a path that makes one rejects the key.
REJECT_CALLS = ('CReader::ReportUnexpected', 'CPdxLogFileAndLine::operator()', 'CLogger::Log')
TEXT_COMPARES = ('_memcmp', '_strcmp', '_strncmp', 'strcmp', 'CString::operator==')
# A loop body that calls one of these reads a list or the reader's statements: the walk followed it.
LOOP_HELPERS = re.compile(r'ReadUniform|InsertAtEmplace|^CReader::Read|^CReader::ParseAdvancedStatement|on=reader$')
VALUE_BRANCH = 'VALUEBRANCH'
MARKERS = ('SWITCHDEFAULT', 'LOOP', 'SWITCHMISS', 'VALUETEST', VALUE_BRANCH, 'KEYTEXT')

SCALAR_TYPES = {
    'bool': 'Boolean', 'CFixedPoint': 'FixedPoint', 'float': 'Float', 'double': 'Float', 'CString': 'String',
    'int': 'Integer', 'uint': 'Integer', 'short': 'Integer', 'ushort': 'Integer', 'long': 'Integer',
    'ulong': 'Integer', 'uchar': 'Integer', 'char': 'Integer', 'CDate': None,
}
TRIGGER = re.compile(r'ReadTrigger|^CTrigger::Read$|^CAndTrigger::Read')
EFFECT = re.compile(r'ReadEffect|^CEffect::Read$')
# `NParserUtil::ReadTrigger<T>` and `ReadEffect<T>` delete the block's previous contents before they
# read (registry-fields.md, repeat behavior); `CTrigger::Read` and `CEffect::Read` merge.
CLEARING_READER = re.compile(r'::Read(Trigger|Effect)<')
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

    # An inline Boolean (below) is not a presence marker.
    inline_boolean = any(stores_yes(text, yes_token) for _, text in events if text.startswith('STORE '))
    presence = None if inline_boolean else presence_row(events)
    if presence:
        return presence

    always = {text for is_always, text in events if is_always}
    calls = [(text in always, callee(text), text) for text in texts if text.startswith('CALL ')]
    calls = [(is_always, call, text) for is_always, call, text in calls if call]
    names = [call['name'] for _, call, _ in calls]
    references = [text for text in texts if text.startswith('REF ')]
    stores = [(text in always, text) for text in texts if text.startswith('STORE ')]
    virtual_read = any(text.startswith(('CALLIND slot+0x20', 'CALLIND slot+0x28')) for text in texts) or \
        'CPersistent::Read' in names

    def constructed_class():
        """The class that the paths construct, with whether its construction is on every path and
        the constructing call's event."""
        for is_always, call, text in calls:
            if is_constructor(call['name']) and not call['name'].startswith(SKIP_CONSTRUCTOR):
                return call['name'].rsplit('::', 1)[0], is_always, text
        for is_always, call, text in calls:
            match = re.match(r'(?:std::make_shared\S*?<|PdxMakeScopedPtr<)([\w:]+(?:<[^<>]*>)?)', call['name'])
            if match:
                return match.group(1), is_always, text
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
            fresh = name.endswith('::CRootTrigger') or CLEARING_READER.search(name)
            return row('trigger', 'Block', 'Block', repeat='Replace' if fresh else 'Unknown', always=is_always,
                       family='Trigger', call=text)
        if EFFECT.search(name):
            fresh = name.endswith('::CRootEffect') or CLEARING_READER.search(name)
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
            # An empty element that the path then reads one scalar into is one appended value: the
            # scalar read below classifies it.
            if element in ('CString', 'int', 'uint') and not scalar_read:
                return row('list', 'Block', 'Block', repeat='Accumulate', always=is_always,
                           items='String' if element == 'CString' else 'Integer', call=text)
            if element and (virtual_read or constructed_class() is not None):
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
        if stores_yes(store, yes_token):
            return row('scalar', 'Scalar', 'Boolean', repeat='Replace', always=is_always)

    constructed = constructed_class()
    if constructed and virtual_read:
        made, is_always, made_call = constructed
        cls = unwrap(made)
        if cls == 'CRootTrigger':
            return row('trigger', 'Block', 'Block', repeat='Replace', always=is_always, family='Trigger')
        if cls == 'CRootEffect':
            return row('effect', 'Block', 'Block', repeat='Replace', always=is_always, family='Effect')
        stored = any('val=call:' in store and ('operator.new' in store or made in store) for _, store in stores)
        return row('object', 'Block', 'Block', repeat='Replace' if stored else 'Unknown', always=is_always,
                   nested='class:' + cls, call=made_call)
    delegated = next(((is_always, call, text) for is_always, call, text in calls
                      if call['name'].endswith('::ReadMember')), None)
    if delegated:
        return row('delegates', always=delegated[0], nested=delegated[1]['name'], call=delegated[2])
    helper = next(((is_always, text) for is_always, call, text in calls if re.search(r'::Read\w*$', call['name']) and
                   not call['name'].startswith(('CReader::', 'CPersistent::'))), None)
    if helper:
        return row('helper', always=helper[0], call=helper[1])
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


def stores_yes(store, yes_token):
    """Whether a STORE event writes the `yes` token into one byte, which an inline Boolean does."""
    return f'val=#{yes_token:#x}' in store and 'size=1' in store


def presence_row(events):
    """The row of a key whose presence stores a constant, or None. Every path reads no part of the
    value (it calls nothing but one known skip, and tests neither the value's token nor the key's
    text), and stores only constants. A skip gives the form that it reads past; with no skip the
    value is the one scalar token that the reader already holds."""
    texts = [text for _, text in events]
    calls = [callee(text) for text in texts if text.startswith('CALL ')]
    stores = [(is_always, text) for is_always, text in events if text.startswith('STORE ')]
    reads_value = any(text.startswith(('CALLIND', 'LOOP', 'VALUETEST', VALUE_BRANCH, 'KEYTEXT')) for text in texts)
    constant_stores = stores and all(re.search(r' val=#0x[0-9a-f]+$', text) for _, text in stores)
    forms = {SKIP_FORMS.get(call['name']) if call else None for call in calls} or {'Scalar'}
    if reads_value or not constant_stores or len(forms) != 1 or None in forms:
        return None
    value, = forms
    return row('presence', value, 'Presence', repeat='Replace', always=all(is_always for is_always, _ in stores))


def value_branch(value_token, walk, outcome):
    """One branch of a row whose path tests the value's token, from `outcome`, the classification
    of the branch's walk, with `proved`: whether the walk proves what the branch reads.

    A branch is proved when its walk has one path with no cut loop, that path reads and stores
    nothing before the value test, and after it reads the branch's form with one call: a block for
    the block's value token and one scalar for any other. The only other call after the test may
    be an array insertion that makes the element that the call reads into; an insertion alone reads
    nothing. A path that makes one call twice (`repeated_calls`, which the walk records because its
    events keep one copy of each) is no proof. A block branch that only skips the block, and a scalar
    branch that calls and stores nothing, keep nothing (`Skipped`): the reader holds a scalar
    value's token already."""
    texts = [text for _, text in walk['events']]
    tested = texts.index(VALUE_BRANCH) if VALUE_BRANCH in texts else None
    before, after = (texts[:tested], texts[tested + 1:]) if tested is not None else (texts, [])
    effects = lambda part: [text for text in part if text.startswith(('CALL ', 'CALLIND', 'STORE'))]
    names = [call['name'] for call in map(callee, effects(after)) if call]
    form = 'Block' if value_token == 'block' else 'Scalar'

    if value_token == 'other' and not effects(after):
        outcome = dict(outcome, kind='skipped', value=form, reader_kind='Skipped', repeat='Unknown', nested=None,
                       family=None, items=None)
    elif outcome['kind'] == 'skipped' and value_token == 'block' and \
            all(SKIP_FORMS.get(name) == 'Block' for name in names) and len(names) == len(effects(after)):
        outcome = dict(outcome, value=form)

    one_path = walk['leaves'] == 1 and not walk['unknown'] and not walk['truncated'] and \
        not walk.get('repeated_calls') and not any(text.startswith('LOOP') for text in texts)
    tested_first = tested is not None and not effects(before)
    calls = [text for text in effects(after) if not text.startswith('STORE')]
    others = [text for text in calls if text != outcome.get('call')]
    inserts = lambda text: 'InsertAtEmplace<' in (callee(text) or {}).get('name', '')
    reading = outcome.get('call')
    reads_once = (reading in calls and not inserts(reading)) if calls else outcome['kind'] == 'skipped'
    only_the_reading = reads_once and calls.count(reading) <= 1 and all(map(inserts, others))
    reads_form = outcome['value'] == form and outcome['reader_kind'] not in (None, 'Unknown')
    return {'value_token': value_token, 'kind': outcome['kind'], 'value': outcome['value'],
            'reader_kind': outcome['reader_kind'], 'repeat': outcome['repeat'], 'family': outcome['family'],
            'items': outcome['items'], 'nested': outcome['nested'],
            'proved': one_path and tested_first and only_the_reading and reads_form,
            'paths': {'leaves': walk['leaves'], 'unknown': walk['unknown'], 'truncated': walk['truncated']}}


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
