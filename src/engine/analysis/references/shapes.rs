//! Complete-function shapes of compiled lookups.
//!
//! A lookup loop cannot be proved by running it for chosen inputs, so each shape is the whole
//! compiled body of one lookup form. A function is canonicalized, and matches a shape only when
//! every canonical line agrees. A changed register, branch, comparison or call therefore breaks
//! the match instead of being guessed around.
//!
//! Canonical lines keep the argument registers `x0`–`x7`, the frame registers and `sp` as they
//! are, and rename the scratch and callee-saved registers `x8`–`x28` by first use, so one shape
//! covers every register allocation of the same code. Branch targets inside the function become
//! relative line offsets. A branch or call outside the function, and an address formed by `adrp`
//! with `add`, `ldr` or `str`, carry the demangled name at that address as the line's value.
//!
//! A shape line may hold placeholders. `{name}` in the text captures one operand token, such as a
//! field offset; `{name}` as the value captures a name. A placeholder binds once: every later use
//! of the same name must capture the same text.
use crate::engine::analysis::decode::Instruction;
use std::collections::BTreeMap;

/// One canonical instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Mnemonic and canonical operands.
    pub text: String,
    /// The demangled name of the address or call target that the line forms, if any.
    pub value: Option<String>,
}

impl std::fmt::Display for Line {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.value {
            Some(value) => write!(formatter, "{} = {value}", self.text),
            None => formatter.write_str(&self.text),
        }
    }
}

/// Canonicalize a complete function. `names` maps symbol addresses and pointer slots to the
/// demangled name that they hold; an unnamed target gives the value `?`.
pub fn canonical(rows: &[Instruction], names: &BTreeMap<u64, String>) -> Vec<Line> {
    let mut canonicalizer = Canonicalizer {
        names,
        lines: rows
            .iter()
            .enumerate()
            .map(|(index, row)| (row.address, index))
            .collect(),
        registers: Registers::default(),
        pages: BTreeMap::new(),
    };

    rows.iter()
        .enumerate()
        .map(|(index, row)| canonicalizer.line(index, row))
        .collect()
}

/// Canonicalize a function whose `adr` lines form local jump-table bases. Each `adr` keeps its
/// relative instruction position, so a relocated body still matches; `None` when an `adr`
/// target is outside the body.
pub(crate) fn canonical_local(
    rows: &[Instruction],
    names: &BTreeMap<u64, String>,
) -> Option<Vec<Line>> {
    let mut lines = canonical(rows, names);
    for (index, (row, line)) in rows.iter().zip(&mut lines).enumerate() {
        if row.operation != "adr" {
            continue;
        }

        let target = row
            .operands
            .split_once(",#0x")
            .and_then(|(_, address)| u64::from_str_radix(address, 16).ok())
            .and_then(|address| rows.iter().position(|row| row.address == address))?;
        let (destination, _) = line.text.split_once(',')?;
        line.text = format!("{destination},@{:+}", target as i64 - index as i64);
    }

    Some(lines)
}

/// The state that one function's canonical lines share.
struct Canonicalizer<'a> {
    names: &'a BTreeMap<u64, String>,
    /// Line index of each instruction address in the function.
    lines: BTreeMap<u64, usize>,
    registers: Registers,
    /// The page that an `adrp` left in a register, until the register is written again.
    pages: BTreeMap<String, u64>,
}

impl Canonicalizer<'_> {
    fn line(&mut self, index: usize, row: &Instruction) -> Line {
        let operands = split_operands(&row.operands);
        let destination = operands.first().copied().unwrap_or_default();
        let line = if row.operation == "adrp" {
            self.page(destination, &operands)
        } else if let Some(target) = branch_target(&row.operation, &operands) {
            self.branch(index, &row.operation, &operands, target)
        } else if let Some((base, offset)) = page_offset(&row.operation, &operands) {
            self.page_access(&row.operation, &operands, base, offset)
        } else {
            self.plain(&row.operation, &operands)
        };
        if row.operation != "adrp" && writes_register(&row.operation) {
            self.pages.remove(&self.registers.canonical(destination));
        }

        line
    }

    fn page(&mut self, destination: &str, operands: &[&str]) -> Line {
        if let Some(page) = operands.get(1).and_then(|page| number(page)) {
            self.pages
                .insert(self.registers.canonical(destination), page as u64);
        }

        Line {
            text: format!("adrp {},PAGE", self.registers.rename(destination)),
            value: None,
        }
    }

    /// A local branch keeps its relative line offset; any other target is a named call.
    fn branch(&mut self, index: usize, operation: &str, operands: &[&str], target: u64) -> Line {
        let prefix: Vec<String> = operands[..operands.len() - 1]
            .iter()
            .map(|operand| self.registers.rename(operand))
            .collect();
        let (label, value) = match self.lines.get(&target) {
            Some(line) => (format!("@{:+}", *line as i64 - index as i64), None),
            None => ("CALL".to_owned(), Some(self.name(target))),
        };

        Line {
            text: join(operation, prefix.into_iter().chain([label])),
            value,
        }
    }

    /// An `add`, `ldr` or `str` that completes an `adrp` address names that address.
    fn page_access(&mut self, operation: &str, operands: &[&str], base: &str, offset: i64) -> Line {
        let page = self.pages.get(&self.registers.canonical(base)).copied();
        let renamed = operands_text(operands, &mut self.registers);
        let Some(page) = page else {
            return Line {
                text: format!("{operation} {renamed}"),
                value: None,
            };
        };

        Line {
            text: format!(
                "{operation} {}",
                renamed.replacen(&format!("#{offset:#x}"), "G", 1)
            ),
            value: Some(self.name(page.wrapping_add_signed(offset))),
        }
    }

    fn plain(&mut self, operation: &str, operands: &[&str]) -> Line {
        let renamed = operands_text(operands, &mut self.registers);
        let text = if renamed.is_empty() {
            operation.to_owned()
        } else {
            format!("{operation} {renamed}")
        };

        Line { text, value: None }
    }

    fn name(&self, address: u64) -> String {
        self.names
            .get(&address)
            .cloned()
            .unwrap_or_else(|| "?".into())
    }
}

/// A complete-function lookup shape.
#[derive(Debug, Clone)]
pub struct Shape {
    lines: Vec<(String, Option<String>)>,
}

/// Placeholder captures of one successful match.
pub type Bindings = BTreeMap<String, String>;

impl Shape {
    /// Parse shape text: one canonical line per text line, `text = value` when the line carries
    /// a name. Blank lines and lines starting with `#` are ignored.
    pub fn parse(text: &str) -> Self {
        let lines = text
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| match line.split_once(" = ") {
                Some((text, value)) => (text.to_owned(), Some(value.to_owned())),
                None => (line.to_owned(), None),
            })
            .collect();

        Self { lines }
    }

    /// The captures when every line of `lines` matches this shape, in order and in full.
    pub fn matches(&self, lines: &[Line]) -> Option<Bindings> {
        if lines.len() != self.lines.len() {
            return None;
        }

        let mut bindings = Bindings::new();
        for (line, (text, value)) in lines.iter().zip(&self.lines) {
            if !capture(&line.text, text, &mut bindings) {
                return None;
            }

            let agrees = match (&line.value, value) {
                (None, None) => true,
                (Some(actual), Some(pattern)) => capture_whole(actual, pattern, &mut bindings),
                _ => false,
            };
            if !agrees {
                return None;
            }
        }

        Some(bindings)
    }
}

/// Scratch and callee-saved registers renamed by first use; the others keep their names.
#[derive(Default)]
struct Registers {
    renamed: BTreeMap<u8, usize>,
}

impl Registers {
    fn number(operand: &str) -> Option<(char, u8)> {
        let mut characters = operand.chars();
        let width = characters
            .next()
            .filter(|width| matches!(width, 'x' | 'w'))?;
        let number: u8 = characters.as_str().parse().ok()?;

        (8..=28).contains(&number).then_some((width, number))
    }

    /// The canonical spelling of one register operand, allocating a name on first use.
    fn rename(&mut self, operand: &str) -> String {
        let Some((width, number)) = Self::number(operand) else {
            return operand.to_owned();
        };
        let next = self.renamed.len();
        let index = *self.renamed.entry(number).or_insert(next);

        format!("{width}r{index}")
    }

    /// The width-free key of one register, used to track `adrp` pages.
    fn canonical(&mut self, operand: &str) -> String {
        let renamed = self.rename(operand);

        renamed.trim_start_matches(['x', 'w']).to_owned()
    }
}

/// Split operands at commas outside brackets.
pub(crate) fn split_operands(operands: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (index, character) in operands.char_indices() {
        match character {
            '[' | '{' => depth += 1,
            ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&operands[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if start < operands.len() {
        parts.push(&operands[start..]);
    }

    parts
}

/// Rename every register and normalize every immediate of an operand list.
fn operands_text(operands: &[&str], registers: &mut Registers) -> String {
    let renamed: Vec<String> = operands
        .iter()
        .map(|operand| rename_within(operand, registers))
        .collect();

    renamed.join(",")
}

/// Rename the registers inside one operand, such as `[x19,#0x10]`, and normalize its
/// immediates to lowercase hexadecimal.
fn rename_within(operand: &str, registers: &mut Registers) -> String {
    let mut result = String::with_capacity(operand.len());
    let mut token = String::new();
    let flush = |token: &mut String, result: &mut String, registers: &mut Registers| {
        if token.is_empty() {
            return;
        }
        result.push_str(&normalize_token(token, registers));
        token.clear();
    };
    for character in operand.chars() {
        if character.is_ascii_alphanumeric() || character == '#' || character == '-' {
            token.push(character);
        } else {
            flush(&mut token, &mut result, registers);
            result.push(character);
        }
    }
    flush(&mut token, &mut result, registers);

    result
}

fn normalize_token(token: &str, registers: &mut Registers) -> String {
    if let Some(immediate) = token.strip_prefix('#')
        && let Some(value) = number(immediate)
    {
        return if value < 0 {
            format!("#-{:#x}", value.unsigned_abs())
        } else {
            format!("#{value:#x}")
        };
    }

    registers.rename(token)
}

fn join(operation: &str, operands: impl Iterator<Item = String>) -> String {
    let operands: Vec<String> = operands.collect();

    format!("{operation} {}", operands.join(","))
}

/// The absolute target of a direct branch or call.
fn branch_target(operation: &str, operands: &[&str]) -> Option<u64> {
    let is_branch = matches!(operation, "b" | "bl" | "cbz" | "cbnz" | "tbz" | "tbnz")
        || operation.starts_with("b.");
    if !is_branch {
        return None;
    }

    operands
        .last()
        .and_then(|target| number(target))
        .map(|target| target as u64)
}

/// The base register and offset of `add xD,xN,#offset`, `ldr xD,[xN,#offset]` or
/// `str xS,[xN,#offset]`, which may complete an `adrp` address.
fn page_offset<'a>(operation: &str, operands: &[&'a str]) -> Option<(&'a str, i64)> {
    match (operation, operands) {
        ("add", [_, base, offset]) => Some((base, number(offset)?)),
        ("ldr" | "str", [_, memory]) => {
            let inside = memory.strip_prefix('[')?.strip_suffix(']')?;
            let (base, offset) = inside.split_once(',')?;
            Some((base, number(offset)?))
        }
        _ => None,
    }
}

/// Whether the first operand of `operation` is a destination register.
fn writes_register(operation: &str) -> bool {
    !matches!(
        operation,
        "str"
            | "stp"
            | "stur"
            | "strb"
            | "strh"
            | "sturb"
            | "sturh"
            | "cmp"
            | "cmn"
            | "tst"
            | "ccmp"
            | "ccmn"
            | "b"
            | "bl"
            | "br"
            | "blr"
            | "ret"
            | "cbz"
            | "cbnz"
            | "tbz"
            | "tbnz"
    ) && !operation.starts_with("b.")
}

fn number(text: &str) -> Option<i64> {
    let text = text.strip_prefix('#').unwrap_or(text);
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, text),
    };
    let magnitude = match digits.strip_prefix("0x") {
        Some(hex) => i64::from_str_radix(hex, 16).ok()?,
        None => digits.parse().ok()?,
    };

    Some(if negative { -magnitude } else { magnitude })
}

/// Match `actual` against `pattern`, binding each `{name}` to the text up to the next literal.
fn capture(actual: &str, pattern: &str, bindings: &mut Bindings) -> bool {
    let Some(open) = pattern.find('{') else {
        return actual == pattern;
    };
    let Some(close) = pattern[open..].find('}').map(|close| open + close) else {
        return false;
    };
    let Some(rest) = actual.strip_prefix(&pattern[..open]) else {
        return false;
    };
    let name = &pattern[open + 1..close];
    let after = &pattern[close + 1..];
    let stop = after
        .find('{')
        .map_or(after, |next| &after[..next])
        .chars()
        .next();
    let end = match stop {
        Some(stop) => match rest.find(stop) {
            Some(end) => end,
            None => return false,
        },
        None => rest.len(),
    };
    let captured = &rest[..end];
    if captured.is_empty() || !bind(name, captured, bindings) {
        return false;
    }

    capture(&rest[end..], after, bindings)
}

/// Match a whole value: a literal, or one placeholder that binds the entire value.
fn capture_whole(actual: &str, pattern: &str, bindings: &mut Bindings) -> bool {
    match pattern
        .strip_prefix('{')
        .and_then(|name| name.strip_suffix('}'))
    {
        Some(name) => bind(name, actual, bindings),
        None => actual == pattern,
    }
}

fn bind(name: &str, captured: &str, bindings: &mut Bindings) -> bool {
    match bindings.get(name) {
        Some(previous) => previous == captured,
        None => {
            bindings.insert(name.to_owned(), captured.to_owned());
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(address: u64, operation: &str, operands: &str) -> Instruction {
        Instruction {
            address,
            bytes: [0; 4],
            operation: operation.into(),
            operands: operands.into(),
        }
    }

    fn names() -> BTreeMap<u64, String> {
        BTreeMap::from([
            (0x5008, "TGameDatabase<CExampleDatabase>::_pInstance".into()),
            (0x9000, "_memcmp".into()),
        ])
    }

    #[test]
    fn scratch_registers_are_renamed_and_globals_named() {
        let rows = [
            row(0x1000, "adrp", "x19,#0x5000"),
            row(0x1004, "ldr", "x19,[x19,#0x8]"),
            row(0x1008, "ldr", "w20,[x19,#84]"),
            row(0x100c, "cbz", "w20,#0x1018"),
            row(0x1010, "mov", "x0,x19"),
            row(0x1014, "bl", "#0x9000"),
            row(0x1018, "ret", ""),
        ];
        let lines: Vec<String> = canonical(&rows, &names())
            .iter()
            .map(ToString::to_string)
            .collect();

        assert_eq!(
            lines,
            [
                "adrp xr0,PAGE",
                "ldr xr0,[xr0,G] = TGameDatabase<CExampleDatabase>::_pInstance",
                "ldr wr1,[xr0,#0x54]",
                "cbz wr1,@+3",
                "mov x0,xr0",
                "bl CALL = _memcmp",
                "ret",
            ]
        );
    }

    #[test]
    fn a_store_through_a_page_names_the_global() {
        let rows = [
            row(0x1000, "adrp", "x20,#0x5000"),
            row(0x1004, "str", "x0,[x20,#0x8]"),
        ];
        let lines: Vec<String> = canonical(&rows, &names())
            .iter()
            .map(ToString::to_string)
            .collect();

        assert_eq!(
            lines,
            [
                "adrp xr0,PAGE",
                "str x0,[xr0,G] = TGameDatabase<CExampleDatabase>::_pInstance",
            ]
        );
    }

    #[test]
    fn a_shape_binds_each_placeholder_once() {
        let shape = Shape::parse(
            "# example\nldr xr0,[x0,#{input}]\nstr xr0,[x0,#{input}]\nbl CALL = {callee}\n",
        );
        let lines = |second: &str| {
            vec![
                Line {
                    text: "ldr xr0,[x0,#0x10]".into(),
                    value: None,
                },
                Line {
                    text: second.into(),
                    value: None,
                },
                Line {
                    text: "bl CALL".into(),
                    value: Some("_memcmp".into()),
                },
            ]
        };

        let bindings = shape.matches(&lines("str xr0,[x0,#0x10]")).unwrap();
        assert_eq!(bindings["input"], "0x10");
        assert_eq!(bindings["callee"], "_memcmp");
        assert!(shape.matches(&lines("str xr0,[x0,#0x18]")).is_none());
    }

    #[test]
    fn argument_registers_keep_their_names() {
        let original = canonical(&[row(0x1000, "mov", "x19,x0")], &names());
        let changed = canonical(&[row(0x1000, "mov", "x19,x1")], &names());

        assert_ne!(original, changed);
    }
}
