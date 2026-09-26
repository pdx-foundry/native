//! Hook names shared by the worker and the reducers. Prefixes include their separator.
macro_rules! hooks {
    ($($name:ident = $wire:literal),+ $(,)?) => {
        $(pub(crate) const $name: &str = $wire;)+

        pub(super) fn python_names() -> serde_json::Value {
            serde_json::Value::Object([$(
                (stringify!($name).to_ascii_lowercase(), serde_json::Value::from($name)),
            )+].into_iter().collect())
        }
    };
}

hooks! {
    REGISTRY = "registry:",
    REGISTRY_RETURN = "registry-return:",
    FIXTURE = "fixture:",
    FIXTURE_LOAD = "fixture:load",
    FIXTURE_REGISTRATION = "fixture:registration",
    FIXTURE_FIELD = "fixture:field",
    FIXTURE_CONSTRUCTOR = "fixture:constructor",
    FIXTURE_READER = "fixture:reader",
    FIXTURE_MEMBER = "fixture:member",
    FIXTURE_MALFORMED = "fixture:malformed",
    FIXTURE_UNEXPECTED = "fixture:unexpected",
    FIXTURE_RETURN = "fixture:return",
    FIXTURE_CONSTRUCTOR_RETURN = "fixture:constructor-return:",
    FIXTURE_MEMBER_RETURN = "fixture:member-return:",
    MODIFIERS = "modifiers:",
    MODIFIERS_DOCUMENTATION = "modifiers:documentation",
    MODIFIERS_RETURN = "modifiers:return",
}
