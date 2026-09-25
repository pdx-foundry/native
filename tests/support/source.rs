//! Source-literal shapes shared by the static source gates.

/// Whether a string literal has the shape of a game build identity: a dotted version such as
/// `4.5.0`, or a SHA-256 digest.
pub fn has_build_shape(text: &str) -> bool {
    let parts: Vec<_> = text.split('.').collect();
    let version = parts.len() >= 2
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        });
    let hash = text.len() == 64 && text.chars().all(|character| character.is_ascii_hexdigit());

    version || hash
}
