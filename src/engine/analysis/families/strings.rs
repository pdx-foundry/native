//! A model of the engine's string functions: it builds the real text of each string, and follows
//! where each part of that text came from.
//!
//! The model keeps, for each path, a label from a text address to a node of the shared arena. A
//! text address is a long string's buffer, a short string's object (its text is in place), or a
//! formatter's buffer. A node is the ordered parts of the text: literals from read-only data, the
//! item key, or an unresolved part. Moves and copies of a string object carry its buffer pointer,
//! so the model reads a string by its labels and never needs the object's flag byte: vector copies
//! of partly unwritten temporaries leave that byte unknown.
//!
//! The model writes real bytes for every string that it builds, so the code under analysis takes
//! the same branches that it takes for that text. Strings that it builds are in the long form.
use std::collections::{BTreeMap, BTreeSet};

use super::super::evaluate::{Call, Machine, ReadOnlyData, Unresolved};

/// The longest text that the model reads or writes, in bytes.
const TEXT_LIMIT: u64 = 4096;

/// Addresses of the engine functions that the model follows.
#[derive(Debug, Clone, Default)]
pub struct StringFunctions {
    /// `CString::CString(char const*)`: the object in `x0`, the text in `x1`.
    pub from_text: BTreeSet<u64>,
    /// `CString::operator+=(CString const&)`: the object in `x0`, the other object in `x1`.
    pub append_string: BTreeSet<u64>,
    /// `CString::operator+=(char const*)`: the object in `x0`, the text in `x1`.
    pub append_text: BTreeSet<u64>,
    /// `PdxStrFmt<N>::PdxStrFmt(char const*, ...)`, with the buffer capacity `N`: the buffer in
    /// `x0`, the format in `x1`, and the arguments on the stack.
    pub formatters: BTreeMap<u64, u64>,
    /// String allocators: the size in `x1`.
    pub allocators: BTreeSet<u64>,
    /// `operator new[]`: the size in `x0`.
    pub array_allocators: BTreeSet<u64>,
    /// Functions that release memory and have no other effect.
    pub releases: BTreeSet<u64>,
    /// `strlen`.
    pub lengths: BTreeSet<u64>,
    /// `memmove` and `memcpy`: the destination in `x0`, the source in `x1`, the length in `x2`.
    pub copies: BTreeSet<u64>,
    /// Functions that never return, such as `__stack_chk_fail` and `_Unwind_Resume`.
    pub never_return: BTreeSet<u64>,
}

/// Layout of the engine's string object. A long string holds its buffer pointer at 0, its length
/// at 8 and its capacity at 16; bit 7 of the flag byte marks the long form. A short string holds
/// its text in place and its length in the flag byte.
#[derive(Debug, Clone, Copy)]
pub struct StringLayout {
    pub flag_byte: u64,
}

impl StringLayout {
    /// The capacity word of a long string; its top byte is the flag byte.
    pub fn capacity_offset(self) -> u64 {
        self.flag_byte - 7
    }
}

/// One part of a composed name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Part {
    Literal(String),
    ItemKey,
    Unresolved,
}

/// The parts of one text, and the longest text in bytes that its buffer keeps, when a formatter
/// bounds it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Node {
    pub parts: Vec<Part>,
    pub limit: Option<u64>,
}

impl Node {
    fn unresolved() -> Self {
        Self {
            parts: vec![Part::Unresolved],
            limit: None,
        }
    }

    fn literal(text: String) -> Self {
        Self {
            parts: vec![Part::Literal(text)],
            limit: None,
        }
    }

    /// This text followed by `other`. A bounded text followed by more text is unresolved: the
    /// model does not follow a formatter's cut into a later composition.
    fn followed_by(&self, other: &Node) -> Self {
        if self.limit.is_some() || other.limit.is_some() {
            return Self::unresolved();
        }

        let mut parts = self.parts.clone();
        for part in &other.parts {
            match (parts.last_mut(), part) {
                (_, Part::Literal(more)) if more.is_empty() => {}
                (Some(Part::Literal(text)), Part::Literal(more)) => text.push_str(more),
                _ => parts.push(part.clone()),
            }
        }
        Self { parts, limit: None }
    }

    pub fn is_resolved(&self) -> bool {
        !self.parts.contains(&Part::Unresolved)
    }

    /// The real text, with `key` for the item key. `None` when a part is unresolved.
    fn text(&self, key: &str) -> Option<String> {
        self.parts
            .iter()
            .map(|part| match part {
                Part::Literal(text) => Some(text.as_str()),
                Part::ItemKey => Some(key),
                Part::Unresolved => None,
            })
            .collect()
    }
}

/// The nodes that every path of one analysis shares. Nodes never change, so paths can share them.
#[derive(Debug, Clone)]
pub struct Arena {
    nodes: Vec<Node>,
}

/// The node of the item key; every arena starts with it.
pub const ITEM_KEY: u64 = 0;

impl Default for Arena {
    fn default() -> Self {
        Self {
            nodes: vec![Node {
                parts: vec![Part::ItemKey],
                limit: None,
            }],
        }
    }
}

impl Arena {
    pub fn node(&self, index: u64) -> &Node {
        &self.nodes[index as usize]
    }

    fn add(&mut self, node: Node) -> u64 {
        self.nodes.push(node);
        self.nodes.len() as u64 - 1
    }
}

/// The model for one run: the functions it follows, the string layout, and the item key's text.
pub struct Model<'a> {
    pub functions: &'a StringFunctions,
    pub layout: StringLayout,
    pub data: &'a ReadOnlyData,
    /// The text of the item key in this run.
    pub key: &'a str,
}

/// What the model did at one call.
pub enum Effect {
    /// The model followed the call.
    Followed(Call),
    /// The call is not a string function. Any string that it receives is now unresolved.
    Other,
}

impl Model<'_> {
    /// Follow the call to `target`, when it is a string function.
    pub fn call(
        &self,
        target: Option<u64>,
        machine: &mut Machine,
        arena: &mut Arena,
    ) -> Result<Effect, Unresolved> {
        let functions = self.functions;
        let Some(target) = target else {
            self.forget_arguments(machine, arena);
            return Ok(Effect::Other);
        };

        let call = if functions.never_return.contains(&target) {
            Call::Stop
        } else if functions.from_text.contains(&target) {
            let node = self.text_node(machine, machine.register(1), arena);
            self.build(machine, machine.register(0), node, arena)?
        } else if functions.append_string.contains(&target) {
            let other = self.object_node(machine, machine.register(1), arena);
            self.append(machine, other, arena)?
        } else if functions.append_text.contains(&target) {
            let other = self.text_node(machine, machine.register(1), arena);
            self.append(machine, other, arena)?
        } else if let Some(&capacity) = functions.formatters.get(&target) {
            self.format(machine, capacity, arena)?
        } else if functions.allocators.contains(&target) {
            Call::Return(allocation(machine, machine.register(1)))
        } else if functions.array_allocators.contains(&target) {
            Call::Return(allocation(machine, machine.register(0)))
        } else if functions.releases.contains(&target) {
            Call::Return(None)
        } else if functions.lengths.contains(&target) {
            let length = machine
                .register(0)
                .and_then(|text| text_length(machine, text));
            Call::Return(length)
        } else if functions.copies.contains(&target) {
            if !copy(machine) {
                self.forget_arguments(machine, arena);
            }
            Call::Return(machine.register(0))
        } else {
            self.forget_arguments(machine, arena);
            return Ok(Effect::Other);
        };
        Ok(Effect::Followed(call))
    }

    /// The node of the text at `address`: its label, else a literal from read-only data. Only
    /// read-only data is a literal: text that code writes elsewhere has no known origin.
    fn text_node(&self, machine: &Machine, address: Option<u64>, arena: &mut Arena) -> u64 {
        let Some(address) = address else {
            return arena.add(Node::unresolved());
        };
        if let Some(node) = machine.labelled(address) {
            return node;
        }

        match self.data.string(address) {
            Some(text) => arena.add(Node::literal(text)),
            None => arena.add(Node::unresolved()),
        }
    }

    /// The node of the string object at `address`: a short string is labelled at the object, a
    /// long string at its buffer.
    pub fn object_node(&self, machine: &Machine, address: Option<u64>, arena: &mut Arena) -> u64 {
        let label = address.and_then(|address| {
            machine.labelled(address).or_else(|| {
                let buffer = machine.read(address, 8)?;
                machine.labelled(buffer)
            })
        });
        label.unwrap_or_else(|| arena.add(Node::unresolved()))
    }

    /// Make the object at `object` a long string that holds the text of `node`.
    fn build(
        &self,
        machine: &mut Machine,
        object: Option<u64>,
        node: u64,
        arena: &Arena,
    ) -> Result<Call, Unresolved> {
        let object = object.ok_or(Unresolved("string-object"))?;
        let text = arena.node(node).text(self.key).unwrap_or_default();
        let buffer = machine.allocate(text.len() as u64 + 1);
        for (offset, byte) in text.bytes().enumerate() {
            machine.write(buffer + offset as u64, 1, u64::from(byte));
        }

        let capacity = (text.len() as u64 + 1).next_multiple_of(16);
        machine.write(object, 8, buffer);
        machine.write(object + 8, 8, text.len() as u64);
        machine.write(
            object + self.layout.capacity_offset(),
            8,
            capacity | 1 << 63,
        );
        machine.label(buffer, node);
        // The object now holds a long string, so a short string's label on it is stale.
        machine.unlabel(object);
        Ok(Call::Return(Some(object)))
    }

    fn append(
        &self,
        machine: &mut Machine,
        other: u64,
        arena: &mut Arena,
    ) -> Result<Call, Unresolved> {
        let object = machine.register(0);
        let node = self.object_node(machine, object, arena);
        let appended = arena.node(node).followed_by(arena.node(other));
        let appended = arena.add(appended);
        self.build(machine, object, appended, arena)
    }

    /// `PdxStrFmt<N>`: `%s` takes the next eight-byte stack argument; `%%` is a percent sign.
    /// Any other directive leaves the text unresolved.
    fn format(
        &self,
        machine: &mut Machine,
        capacity: u64,
        arena: &mut Arena,
    ) -> Result<Call, Unresolved> {
        let buffer = machine.register(0).ok_or(Unresolved("format-buffer"))?;
        let format = machine
            .register(1)
            .and_then(|format| self.data.string(format));
        let node = match format {
            Some(format) => self.formatted(machine, &format, arena),
            None => Node::unresolved(),
        };
        let node = Node {
            limit: Some(capacity - 1),
            ..node
        };

        match node.text(self.key) {
            Some(text) if (text.len() as u64) < capacity => {
                for (offset, byte) in text.bytes().enumerate() {
                    machine.write(buffer + offset as u64, 1, u64::from(byte));
                }
                machine.write(buffer + text.len() as u64, 1, 0);
            }
            _ => machine.forget(buffer, capacity),
        }
        let node = arena.add(node);
        machine.label(buffer, node);
        Ok(Call::Return(Some(buffer)))
    }

    fn formatted(&self, machine: &Machine, format: &str, arena: &mut Arena) -> Node {
        let mut node = Node {
            parts: Vec::new(),
            limit: None,
        };
        let mut arguments = (0..).map(|index| machine.stack_pointer() + index * 8);
        let mut rest = format;

        while let Some(start) = rest.find('%') {
            node = node.followed_by(&Node::literal(rest[..start].to_owned()));
            let directive = rest[start + 1..].chars().next();
            let argument = match directive {
                Some('%') => Node::literal("%".into()),
                Some('s') => {
                    let text = arguments.next().and_then(|slot| machine.read(slot, 8));
                    let text = self.text_node(machine, text, arena);
                    arena.node(text).clone()
                }
                _ => return Node::unresolved(),
            };
            node = node.followed_by(&argument);
            rest = &rest[start + 2..];
        }

        node.followed_by(&Node::literal(rest.to_owned()))
    }

    /// A call that the model does not follow may change any string that it receives, so each
    /// argument string becomes unresolved.
    fn forget_arguments(&self, machine: &mut Machine, arena: &mut Arena) {
        for register in 0..=2 {
            let Some(address) = machine.register(register) else {
                continue;
            };
            let buffer = machine.read(address, 8);

            for text in [Some(address), buffer].into_iter().flatten() {
                if machine.labelled(text).is_some() {
                    let unresolved = arena.add(Node::unresolved());
                    machine.label(text, unresolved);
                }
            }
        }
    }
}

/// Fresh memory for an allocation of a known size.
fn allocation(machine: &mut Machine, size: Option<u64>) -> Option<u64> {
    let size = size.filter(|size| *size <= TEXT_LIMIT)?;
    Some(machine.allocate(size))
}

/// The length of the known text at `address`, when every byte up to its end is known.
fn text_length(machine: &Machine, address: u64) -> Option<u64> {
    (0..TEXT_LIMIT).find_map(|offset| match machine.read(address + offset, 1) {
        Some(0) => Some(Some(offset)),
        Some(_) => None,
        None => Some(None),
    })?
}

/// `memmove` and `memcpy` copy each byte, known or not, and the source's label. `false` when the
/// copy's extent is unknown, so any string that it receives may have changed.
fn copy(machine: &mut Machine) -> bool {
    let (Some(destination), Some(source), Some(length)) = (
        machine.register(0),
        machine.register(1),
        machine.register(2),
    ) else {
        return false;
    };
    if length > TEXT_LIMIT {
        return false;
    }

    for offset in 0..length {
        match machine.read(source + offset, 1) {
            Some(byte) => machine.write(destination + offset, 1, byte),
            None => machine.forget(destination + offset, 1),
        }
    }
    match machine.labelled(source) {
        Some(node) => machine.label(destination, node),
        None => machine.unlabel(destination),
    }
    true
}
