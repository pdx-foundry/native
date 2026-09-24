
def validate(value, schema, root=None):
    """Validate the generated schema subset; unsupported constraints fail closed."""
    root = schema if root is None else root
    if isinstance(schema, bool):
        if not schema:
            raise ValueError('disallowed value')
        return
    known = {'$schema', '$defs', '$ref', 'title', 'description', 'type', 'properties',
             'required', 'additionalProperties', 'items', 'anyOf', 'oneOf', 'allOf',
             'const', 'enum', 'minimum', 'maximum', 'format', 'default'}
    if set(schema) - known:
        raise ValueError('unsupported schema constraint')
    if '$ref' in schema:
        prefix = '#/$defs/'
        if not schema['$ref'].startswith(prefix):
            raise ValueError('external schema reference')
        validate(value, root['$defs'][schema['$ref'][len(prefix):]], root)
    for key in ('anyOf', 'oneOf', 'allOf'):
        if key in schema:
            matches = 0
            for alternative in schema[key]:
                try:
                    validate(value, alternative, root)
                    matches += 1
                except ValueError:
                    pass
            expected = len(schema[key]) if key == 'allOf' else 1
            if (key == 'anyOf' and matches == 0) or (key != 'anyOf' and matches != expected):
                raise ValueError('schema alternatives do not match')
    if 'const' in schema and value != schema['const']:
        raise ValueError('constant mismatch')
    if 'enum' in schema and value not in schema['enum']:
        raise ValueError('unknown variant')
    if 'type' in schema:
        types = schema['type'] if isinstance(schema['type'], list) else [schema['type']]
        kinds = {'object': dict, 'array': list, 'string': str, 'integer': int,
                 'boolean': bool, 'null': type(None)}
        if not any(type(value) is kinds.get(kind) for kind in types):
            raise ValueError('wrong field type')
    if isinstance(value, dict):
        if set(schema.get('required', [])) - set(value):
            raise ValueError('missing required field')
        properties = schema.get('properties', {})
        for key, field in value.items():
            validate(field, properties.get(key, schema.get('additionalProperties', True)), root)
    if isinstance(value, list) and 'items' in schema:
        for item in value:
            validate(item, schema['items'], root)
    if type(value) is int:
        if value < schema.get('minimum', value) or value > schema.get('maximum', value):
            raise ValueError('integer outside bounds')
        limits = {'uint64': (0, 2**64-1), 'uint32': (0, 2**32-1), 'int64': (-2**63, 2**63-1)}
        if schema.get('format') in limits:
            low, high = limits[schema['format']]
            if not low <= value <= high:
                raise ValueError('integer outside wire range')


def encode(kind, value, limit=MAX_RECORD):
    validate(value, SCHEMAS[kind])
    encoded = (json.dumps(value, separators=(',', ':')) + '\n').encode('utf-8')
    if len(encoded) > limit:
        raise ValueError('wire record too large')
    return encoded


def decode(kind, encoded):
    if len(encoded) > MAX_RECORD:
        raise ValueError('wire record too large')
    value = json.loads(encoded)
    validate(value, SCHEMAS[kind])
    return value
