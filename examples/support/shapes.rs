//! Draft shape research; normalization here never changes the engine's strict shape matcher.
use std::collections::BTreeMap;

pub fn groups(bodies: Vec<(String, Vec<String>)>, normalize_offsets: bool) -> Vec<Vec<String>> {
    let mut groups: BTreeMap<Vec<String>, Vec<String>> = BTreeMap::new();
    for (name, lines) in bodies {
        let shape = lines
            .iter()
            .map(|line| census_line(line, normalize_offsets))
            .collect();
        groups.entry(shape).or_default().push(name);
    }
    let mut groups: Vec<_> = groups.into_values().collect();
    for members in &mut groups {
        members.sort();
    }
    groups.sort_by(|left, right| right.len().cmp(&left.len()).then(left.cmp(right)));
    groups
}

fn census_line(line: &str, normalize_offsets: bool) -> String {
    let (text, value) = line
        .split_once(" = ")
        .map_or((line, None), |(text, value)| (text, Some(value)));
    let text = if normalize_offsets {
        field_offsets(text)
    } else {
        text.to_owned()
    };
    match value {
        Some(value) => format!("{text} = {}", templates(value)),
        None => text,
    }
}

fn templates(name: &str) -> String {
    let mut arguments = template_arguments(name);
    arguments.sort_by_key(|argument| std::cmp::Reverse(argument.len()));
    let mut replaced = name.to_owned();
    for argument in arguments {
        replaced = replace_argument(&replaced, argument);
    }

    let mut depth = 0;
    let mut normalized = String::new();
    for character in replaced.chars() {
        match character {
            '<' => {
                if depth == 0 {
                    normalized.push_str("<T>");
                }
                depth += 1;
            }
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => normalized.push(character),
            _ => {}
        }
    }
    normalized
}

/// Keep complete arguments at every nesting level, including their template suffixes.
fn template_arguments(name: &str) -> Vec<&str> {
    let mut starts = Vec::new();
    let mut arguments = std::collections::BTreeSet::new();
    for (index, character) in name.char_indices() {
        match character {
            '<' => starts.push(index + 1),
            ',' | '>' if !starts.is_empty() => {
                let start = starts.pop().expect("nonempty template stack");
                let argument = name[start..index].trim();
                if !argument.is_empty() {
                    arguments.insert(argument);
                }
                if character == ',' {
                    starts.push(index + 1);
                }
            }
            _ => {}
        }
    }
    arguments.into_iter().collect()
}

/// Replace complete argument occurrences in instantiated return and parameter types.
fn replace_argument(name: &str, argument: &str) -> String {
    let identifier = |character: char| character.is_alphanumeric() || character == '_';
    let mut replaced = String::new();
    let mut copied = 0;
    for (start, _) in name.match_indices(argument) {
        let end = start + argument.len();
        if name[..start].chars().next_back().is_some_and(identifier)
            || name[end..].chars().next().is_some_and(identifier)
        {
            continue;
        }
        replaced.push_str(&name[copied..start]);
        replaced.push('T');
        copied = end;
    }
    replaced.push_str(&name[copied..]);
    replaced
}

/// Ignore immediate displacements in memory operands based on an object register.
/// Frame/stack offsets, shifts, branch distances and standalone constants remain exact.
fn field_offsets(text: &str) -> String {
    let Some((prefix, memory)) = text.split_once('[') else {
        return text.to_owned();
    };
    let Some((base, offset)) = memory.split_once(",#") else {
        return text.to_owned();
    };
    if base == "sp" || base == "x29" {
        return text.to_owned();
    }
    let end = offset.find([']', ',']).unwrap_or(offset.len());
    format!("{prefix}[{base},#OFFSET{}", &offset[end..])
}

/// Derive only positionally aligned bodies. Different lengths cannot form a complete-function
/// shape; callers get a diagnostic instead of a truncated or invented instruction sequence.
pub fn derive(bodies: &[Vec<String>]) -> Result<String, String> {
    let Some(first) = bodies.first() else {
        return Err("no bodies supplied".into());
    };
    let lengths: Vec<_> = bodies.iter().map(Vec::len).collect();
    if lengths.iter().any(|length| *length != first.len()) {
        return Err(format!("lines do not align: body lengths {lengths:?}"));
    }
    let mut bindings: BTreeMap<Vec<String>, usize> = BTreeMap::new();
    let mut result = vec![format!(
        "# Draft: {} bodies align by line position; review placeholders and semantics.",
        bodies.len()
    )];
    for index in 0..first.len() {
        let lines: Vec<_> = bodies.iter().map(|body| body[index].as_str()).collect();
        let parts: Vec<_> = lines
            .iter()
            .map(|line| line.split_once(" = ").unwrap_or((line, "")))
            .collect();
        let texts: Vec<_> = parts.iter().map(|(text, _)| *text).collect();
        let values: Vec<_> = parts.iter().map(|(_, value)| *value).collect();
        if values.iter().any(|value| value.is_empty())
            && values.iter().any(|value| !value.is_empty())
        {
            result.push(placeholder(&lines, &mut bindings));
            continue;
        }
        let text = derive_text(&texts, &mut bindings);
        if values[0].is_empty() {
            result.push(text);
        } else {
            result.push(format!(
                "{text} = {}",
                common_or_placeholder(&values, &mut bindings)
            ));
        }
    }
    Ok(format!("{}\n", result.join("\n")))
}

fn common_or_placeholder(parts: &[&str], bindings: &mut BTreeMap<Vec<String>, usize>) -> String {
    if parts.iter().all(|part| *part == parts[0]) {
        parts[0].to_owned()
    } else {
        placeholder(parts, bindings)
    }
}

fn placeholder(parts: &[&str], bindings: &mut BTreeMap<Vec<String>, usize>) -> String {
    let next = bindings.len() + 1;
    let key = parts.iter().map(|part| (*part).to_owned()).collect();
    let number = bindings.entry(key).or_insert(next);
    format!("{{p{number}}}")
}

fn derive_text(texts: &[&str], bindings: &mut BTreeMap<Vec<String>, usize>) -> String {
    let tokens: Vec<Vec<&str>> = texts
        .iter()
        .map(|text| text.split_inclusive([' ', ',', '[', ']', '#']).collect())
        .collect();
    if tokens.iter().any(|parts| parts.len() != tokens[0].len()) {
        return common_or_placeholder(texts, bindings);
    }
    (0..tokens[0].len())
        .map(|index| {
            let parts: Vec<_> = tokens.iter().map(|tokens| tokens[index]).collect();
            if parts.iter().all(|part| *part == parts[0]) {
                return parts[0].to_owned();
            }
            let delimiter = parts[0]
                .chars()
                .last()
                .filter(|character| [' ', ',', '[', ']', '#'].contains(character));
            if let Some(delimiter) = delimiter
                && parts.iter().all(|part| part.ends_with(delimiter))
            {
                let values: Vec<_> = parts.iter().map(|part| &part[..part.len() - 1]).collect();
                return format!("{}{delimiter}", placeholder(&values, bindings));
            }
            placeholder(&parts, bindings)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn body(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| (*line).into()).collect()
    }

    #[test]
    fn grouping_normalizes_nested_templates_but_preserves_control_flow_and_offsets() {
        let bodies = vec![
            (
                "B".into(),
                body(&[
                    "ldr x0,[xr0,#0x18]",
                    "bl CALL = DB<Nested<B>>::Get()",
                    "b @+2",
                ]),
            ),
            (
                "A".into(),
                body(&["ldr x0,[xr0,#0x10]", "bl CALL = DB<A>::Get()", "b @+2"]),
            ),
            (
                "C".into(),
                body(&["ldr x0,[xr0,#0x10]", "bl CALL = DB<A>::Get()", "b @+3"]),
            ),
        ];
        assert_eq!(groups(bodies.clone(), false).len(), 3);
        assert_eq!(groups(bodies, true), vec![vec!["A", "B"], vec!["C"]]);
        assert_eq!(field_offsets("ldr x0,[sp,#0x18]"), "ldr x0,[sp,#0x18]");
        assert_eq!(
            templates("D::ValueType Read<D>(D const&)"),
            "T::ValueType Read<T>(T const&)"
        );
    }

    #[test]
    fn nested_arguments_normalize_their_complete_repeated_types() {
        let first = "DB<Nested<A>>::Get(Nested<A> const&)";
        let second = "DB<Other<B>>::Get(Other<B> const&)";
        assert_eq!(templates(first), "DB<T>::Get(T const&)");
        assert_eq!(templates(first), templates(second));
        assert_eq!(
            templates("DB<ns::Pair<A, B>>::Get(ns::Pair<A, B> const&, AAA*)"),
            "DB<T>::Get(T const&, AAA*)"
        );
        assert_eq!(
            groups(
                vec![
                    ("first".into(), body(&[&format!("bl CALL = {first}")])),
                    ("second".into(), body(&[&format!("bl CALL = {second}")])),
                ],
                false
            ),
            vec![vec!["first", "second"]]
        );
    }

    #[test]
    fn derivation_keeps_literals_and_reuses_only_identical_binding_columns() {
        let draft = derive(&[
            body(&[
                "ldr x0,[xr0,#0x10]",
                "str x1,[xr0,#0x10]",
                "bl CALL = A<T>::Get()",
            ]),
            body(&[
                "ldr x0,[xr0,#0x18]",
                "str x1,[xr0,#0x18]",
                "bl CALL = B<T>::Get()",
            ]),
        ])
        .unwrap();
        assert!(
            draft.ends_with("ldr x0,[xr0,#{p1}]\nstr x1,[xr0,#{p1}]\nbl CALL = {p2}\n"),
            "{draft}"
        );
        assert!(
            derive(&[body(&["ret"]), body(&["nop", "ret"])])
                .unwrap_err()
                .contains("do not align")
        );
        assert!(derive(&[]).is_err());
    }
}
