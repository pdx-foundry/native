"""Decode a proven owner storage slot into the wire shape of its public stored value, and the
engine's CString layout for every reader of engine text.

The callers own memory access: `read_unsigned(address, size)` reads a little-endian integer and
`read_string(address)` reads the text of the CString at `address`. Both raise on a failed read.
"""


def cstring_prefix(storage, tag_offset, read, limit):
    """The text of an engine CString cut to at most `limit` bytes, and whether it was cut.
    `storage` holds the object's bytes, at least `tag_offset + 1` long. Bit 7 of the tag byte at
    `tag_offset` marks a long string: the object holds a pointer and a byte length, and
    `read(address, size)` gives its bytes. Otherwise the tag byte is the length of the characters
    that precede it. A cut can split a character; that partial character is dropped, and any
    other malformed UTF-8 raises."""
    tag = storage[tag_offset]
    if tag & 128:
        pointer = int.from_bytes(storage[:8], 'little')
        length = int.from_bytes(storage[8:16], 'little')
        size = min(length, limit)
        value = read(pointer, size) if size else b''
    else:
        if tag > tag_offset:
            raise RuntimeError('short string length outside bound')
        length = tag
        value = bytes(storage[:min(length, limit)])
    cut = length > limit
    text = decode_cut(value) if cut else value.decode('utf-8')
    return text, cut


def decode_cut(value):
    """UTF-8 `value` that was cut at an arbitrary byte, without the character that the cut split
    at its end. Malformed bytes anywhere else raise `UnicodeDecodeError`."""
    try:
        return value.decode('utf-8')
    except UnicodeDecodeError as error:
        split_at_end = (error.reason == 'unexpected end of data'
                        and error.end == len(value)
                        and error.start >= len(value) - 3)
        if not split_at_end:
            raise
        return value[:error.start].decode('utf-8')


def cstring(storage, tag_offset, read):
    """The whole text of an engine CString, as for `cstring_prefix`. Text above 4096 bytes is
    rejected."""
    text, cut = cstring_prefix(storage, tag_offset, read, 4096)
    if cut:
        raise RuntimeError('string length outside bound')
    return text


def signed_integer(raw, bits):
    return raw - (1 << bits) if raw >= (1 << (bits - 1)) else raw


def decode(read_unsigned, read_string, address, decoder):
    if decoder == 'String':
        return {'String': read_string(address)}
    if decoder == 'Integer':
        return {'Integer': signed_integer(read_unsigned(address, 4), 32)}
    if decoder == 'Float':
        return {'Float': dict(bits=read_unsigned(address, 4))}
    if decoder == 'Integer16':
        return {'Integer16': dict(bits=read_unsigned(address, 2))}
    if isinstance(decoder, dict) and 'FixedPoint' in decoder:
        raw = signed_integer(read_unsigned(address, 8), 64)
        return {'FixedPoint': dict(raw=raw, scale=decoder['FixedPoint']['scale'])}
    if isinstance(decoder, dict) and 'ScopedNumeric' in decoder:
        return {'ScopedNumeric': scoped_numeric(read_unsigned, read_string, address, decoder['ScopedNumeric'])}
    raise RuntimeError('fixture storage decoder is unavailable')


def scoped_numeric(read_unsigned, read_string, address, scoped):
    """The literal and every reference slot of a scoped operand, independently of selection."""
    layout = scoped['layout']
    literal_address = address + layout['literal']
    if scoped['literal'] == 'Integer':
        literal = {'Integer': signed_integer(read_unsigned(literal_address, 4), 32)}
    else:
        scale = scoped['literal']['FixedPoint']['scale']
        literal = {'FixedPoint': dict(raw=signed_integer(read_unsigned(literal_address, 8), 64), scale=scale)}
    return dict(
        literal=literal,
        has_source_location=bool(read_string(address + layout['location'])),
        has_trigger=bool(read_unsigned(address + layout['trigger'], 8)),
        has_script_value=bool(read_unsigned(address + layout['script_value'], 8)),
        has_modifier=read_unsigned(address + layout['modifier'], 4) != layout['modifier_unset'],
        variable=read_string(address + layout['variable']))
