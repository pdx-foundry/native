use super::{binary::hash, targets::BindingGroupId};

// These declarations come from the verified final SDK-483 source/debugger_attempt.py.
// They are read-entry roles only; they do not authorize calls or establish stored values.
#[cfg(test)]
const SYNTHETIC_REGISTRATION: &[(&str, u64)] = &[("registration-entry", 0x1234)];
const REGISTRATION: &[(&str, u64)] = &[("registration-entry", 0x1004559bc)];
const CATEGORY_READER: &[(&str, u64)] = &[
    ("category-load-entry", 0x100cd8258),
    ("category-field-read-entry", 0x100cd5f2c),
    ("reader-lexer-offset", 0x30),
    ("lexer-file-offset", 8),
    ("file-name-offset", 0x20),
    ("string-storage-tag-offset", 23),
    ("file-line-offset", 8),
    ("tree-template-token", 16793),
    ("traditions-token", 14263),
];

fn declaration(group: BindingGroupId) -> (&'static str, &'static [(&'static str, u64)]) {
    match group {
        #[cfg(test)]
        BindingGroupId::SyntheticRegistration => ("synthetic-registration", SYNTHETIC_REGISTRATION),
        BindingGroupId::Registration => ("m45-registration/read-entry-v1", REGISTRATION),
        BindingGroupId::CategoryReader => {
            ("m45-category/owner-source-read-entry-v1", CATEGORY_READER)
        }
    }
}

pub(super) fn resolve(group: BindingGroupId) -> String {
    let (revision, declarations) = declaration(group);
    let mut bytes = revision.as_bytes().to_vec();
    for (role, value) in declarations {
        bytes.extend((role.len() as u64).to_le_bytes());
        bytes.extend(role.as_bytes());
        bytes.extend(value.to_le_bytes());
    }
    hash(&bytes)
}

pub(super) fn observation(groups: &[BindingGroupId]) -> std::collections::BTreeMap<String, u64> {
    groups
        .iter()
        .flat_map(|group| declaration(*group).1)
        .map(|(key, value)| ((*key).into(), *value))
        .collect()
}
