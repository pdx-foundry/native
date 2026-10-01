"""Decode a proven owner storage slot into the wire shape of its public stored value.

The callers own memory access: `read_unsigned(address, size)` reads a little-endian integer and
`read_string(address)` reads the text of the CString at `address`. Both raise on a failed read.
"""


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
