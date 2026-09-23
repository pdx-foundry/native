//! Modifier families: the modifier names that a registry's database generator registers for each
//! item of the registry.
//!
//! A database's `GenerateModifiers` function loops over its items and composes one name per
//! registration call from literals and the item's key, with string functions or a formatter.
//! The method runs that function for one item whose memory is unknown except its key, and follows
//! each composed text with the string model (`strings`). At each registration call it reads the
//! composed name and the category mask argument.
//!
//! The item key's place in the item is where the item's constructor stores its copy of the key
//! argument. The method runs the constructor with a labelled long key and finds the labelled
//! buffer pointer in the item. The place is the same for a short key, whose text the object holds
//! in place.
//!
//! The generator runs twice: with a long key and with a short key, since the engine branches on
//! the key's form. Both runs must give the same name and mask at each call. A family is `Always`
//! generated only when no path fails, some path returns, and every returned path reaches the
//! call. A branch on an unknown item field makes a path that skips the call, so such a family is
//! never `Always`. Paths that end in a function that never returns, or in a trap, are ignored.
//!
//! Outside the method: database state other than the item array reads as zero, so a gate on
//! database state is not detected; generation outside the database's own generator; where a
//! modifier takes effect; the engine's handling of a name that is registered again.
use std::collections::{BTreeMap, BTreeSet};

use super::evaluate::{Call, Code, Exit, Machine, ReadOnlyData, Unresolved};

mod strings;

use strings::{Arena, Effect, ITEM_KEY, Model, Node};
pub use strings::{Part, StringFunctions, StringLayout};

/// Name and revision of the modifier-family method.
pub const METHOD: &str = "modifier-families/v1";

/// The long key: longer than the longest short string, so the engine stores it in a buffer.
const LONG_KEY: &str = "generatedmodifierfamilyitemkey01";

/// The short key: the engine stores its text in the string object.
const SHORT_KEY: &str = "itemkey1";

/// Bytes of item memory that the method gives the constructor and the generator.
const ITEM_SPAN: u64 = 0x4000;

/// Bytes of zeroed database memory that the generator reads.
const DATABASE_SPAN: u64 = 0x400;

/// Labels at or above this value are the method's own facts about a path, not text labels. Every
/// scratch and stack address is below it.
const METHOD_LABELS: u64 = 1 << 62;

/// The label of the key's offset that the constructor run found.
const FOUND_KEY: u64 = u64::MAX;

/// Executable-derived input for one registry.
pub struct FamilyInput {
    /// The registry's database generator, when it has one.
    pub generator: Option<Generator>,
    /// Calls that register a generated modifier outside every named registry's database
    /// generator. The method does not follow them; they may generate this registry's modifiers.
    pub unjoined_sites: usize,
    /// The function that registers a generated modifier: the name in `x1`, the category mask on
    /// the stack.
    pub registration: u64,
    /// Offset of the category mask from the stack pointer at the registration call.
    pub category_offset: u64,
    pub database: DatabaseLayout,
    pub strings: StringFunctions,
    pub layout: StringLayout,
    /// The generator and every body of the item constructor.
    pub code: Code,
    pub data: ReadOnlyData,
}

/// A database's `GenerateModifiers` function.
pub struct Generator {
    pub function: u64,
    /// Each registration call in the function.
    pub sites: Vec<u64>,
    /// The entry of the item constructor that takes the item key, when one exists.
    pub constructor: Option<u64>,
}

/// Where a database holds its items: a pointer array and its count.
#[derive(Debug, Clone, Copy)]
pub struct DatabaseLayout {
    pub items_offset: u64,
    pub count_offset: u64,
}

/// What the method established for one generator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyResult {
    /// The key's offset in the item, or why the constructor run did not establish it. Without it
    /// no call is followed.
    pub key_offset: Result<u64, Unresolved>,
    /// Each registration call, in address order.
    pub sites: Vec<(u64, Result<Family, Unresolved>)>,
}

/// The names that one registration call generates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    /// The parts of each name. At least one is the item key; none is unresolved.
    pub parts: Vec<Part>,
    /// The longest name in bytes that the engine's formatter keeps.
    pub limit: Option<u64>,
    pub mask: u64,
    pub condition: Condition,
}

/// Whether every item generates the family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    Always,
    Unresolved,
}

/// Follow every registration call of the registry's generator. `None` when it has no generator.
pub fn analyze(input: &FamilyInput) -> Option<FamilyResult> {
    let generator = input.generator.as_ref()?;
    let key_offset = match generator.constructor {
        Some(constructor) => key_offset(input, constructor),
        None => Err(Unresolved("constructor")),
    };
    let Ok(offset) = key_offset else {
        return Some(FamilyResult {
            key_offset,
            sites: Vec::new(),
        });
    };

    let paths: Vec<Vec<PathSummary>> = [KeyForm::Long, KeyForm::Short]
        .into_iter()
        .map(|form| generator_paths(input, generator, offset, form))
        .collect();
    let sites = generator
        .sites
        .iter()
        .map(|&site| {
            let records: BTreeSet<_> = paths
                .iter()
                .flatten()
                .filter_map(|path| path.reached.get(&site))
                .collect();
            (site, family(records, condition(&paths, site)))
        })
        .collect();

    Some(FamilyResult { key_offset, sites })
}

#[derive(Debug, Clone, Copy)]
enum KeyForm {
    Long,
    Short,
}

impl KeyForm {
    fn text(self) -> &'static str {
        match self {
            Self::Long => LONG_KEY,
            Self::Short => SHORT_KEY,
        }
    }
}

/// Write the item key into the string object at `object` and label its text.
fn write_key(machine: &mut Machine, layout: StringLayout, object: u64, form: KeyForm) {
    let text = form.text();
    let length = text.len() as u64;

    let (address, flag) = match form {
        KeyForm::Long => {
            let buffer = machine.allocate(length + 1);
            let capacity = (length + 1).next_multiple_of(16);
            machine.write(object, 8, buffer);
            machine.write(object + 8, 8, length);
            machine.write(object + layout.capacity_offset(), 8, capacity | 1 << 63);
            (buffer, None)
        }
        KeyForm::Short => (object, Some(length)),
    };

    for (offset, byte) in text.bytes().enumerate() {
        machine.write(address + offset as u64, 1, u64::from(byte));
    }
    machine.write(address + length, 1, 0);
    if let Some(length) = flag {
        machine.write(object + layout.flag_byte, 1, length);
    }
    machine.label(address, ITEM_KEY);
}

/// Run the item constructor with a long key, and find where it stores the key's buffer pointer.
fn key_offset(input: &FamilyInput, constructor: u64) -> Result<u64, Unresolved> {
    let mut machine = Machine::new(&input.code, &input.data);
    let item = machine.reserve(ITEM_SPAN);
    let key = machine.allocate(input.layout.flag_byte + 1);
    write_key(&mut machine, input.layout, key, KeyForm::Long);
    machine.set_register(0, item);
    machine.set_register(1, 0);
    machine.set_register(2, key);

    let model = model(input, KeyForm::Long);
    let mut arena = Arena::default();
    let paths = machine.run_paths(constructor, &mut |target, machine| {
        if let Some(offset) = stored_key(machine, item) {
            machine.label(FOUND_KEY, offset);
            return Ok(Call::Stop);
        }
        follow(&model, target, machine, &mut arena)
    });

    let mut offsets = BTreeSet::new();
    for path in paths {
        let found = path
            .machine
            .labelled(FOUND_KEY)
            .or_else(|| stored_key(&path.machine, item));
        match (found, ending(input, &path.end)) {
            (Some(offset), _) => {
                offsets.insert(offset);
            }
            (None, Ending::Ignored) => {}
            (None, Ending::Returned | Ending::Failed) => return Err(Unresolved("key-storage")),
        }
    }

    match offsets.len() {
        1 => Ok(offsets.pop_first().expect("one offset")),
        0 => Err(Unresolved("key-storage")),
        _ => Err(Unresolved("key-storage-ambiguous")),
    }
}

/// The offset in the item of a word that points at the labelled key text. Only words below
/// `METHOD_LABELS` can be text addresses.
fn stored_key(machine: &Machine, item: u64) -> Option<u64> {
    machine
        .known_words()
        .into_iter()
        .filter(|(address, value)| {
            (item..item + ITEM_SPAN).contains(address)
                && *value < METHOD_LABELS
                && machine.labelled(*value) == Some(ITEM_KEY)
        })
        .map(|(address, _)| address - item)
        .next()
}

fn model(input: &FamilyInput, form: KeyForm) -> Model<'_> {
    Model {
        functions: &input.strings,
        layout: input.layout,
        data: &input.data,
        key: form.text(),
    }
}

/// Let the string model follow a call. A call that it does not follow returns an unknown value.
fn follow(
    model: &Model,
    target: Option<u64>,
    machine: &mut Machine,
    arena: &mut Arena,
) -> Result<Call, Unresolved> {
    Ok(match model.call(target, machine, arena)? {
        Effect::Followed(call) => call,
        Effect::Other => Call::Return(None),
    })
}

/// How one path ended, for the generation condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    Returned,
    /// A trap or a function that never returns: the path does not finish the loop.
    Ignored,
    Failed,
}

fn ending(input: &FamilyInput, end: &Result<Exit, Unresolved>) -> Ending {
    match end {
        Ok(Exit::Returned) => Ending::Returned,
        Ok(Exit::Trapped) => Ending::Ignored,
        Ok(Exit::Stopped(target)) if input.strings.never_return.contains(target) => Ending::Ignored,
        Ok(Exit::Stopped(_) | Exit::Reached) | Err(_) => Ending::Failed,
    }
}

/// One path of a generator run: how it ended, and the name and mask at each call it reached.
struct PathSummary {
    ending: Ending,
    reached: BTreeMap<u64, (Node, Option<u64>)>,
}

/// Run the generator over one item whose key has `form`, and summarize each path.
fn generator_paths(
    input: &FamilyInput,
    generator: &Generator,
    key_offset: u64,
    form: KeyForm,
) -> Vec<PathSummary> {
    let mut machine = Machine::new(&input.code, &input.data);
    let database = machine.allocate(DATABASE_SPAN);
    let items = machine.allocate(8);
    let item = machine.reserve(ITEM_SPAN);
    machine.write(database + input.database.items_offset, 8, items);
    machine.write(database + input.database.count_offset, 4, 1);
    machine.write(items, 8, item);
    write_key(&mut machine, input.layout, item + key_offset, form);
    machine.set_register(0, database);

    let model = model(input, form);
    let mut arena = Arena::default();
    let mut records = Vec::new();
    let paths = machine.run_paths(generator.function, &mut |target, machine| {
        let site = machine.register(30).map(|link| link - 4);
        match site {
            Some(site) if target == Some(input.registration) && generator.sites.contains(&site) => {
                let name = model.object_node(machine, machine.register(1), &mut arena);
                let mask = machine.read(machine.stack_pointer() + input.category_offset, 4);
                records.push((name, mask));
                machine.label(METHOD_LABELS | site, records.len() as u64 - 1);
                Ok(Call::Return(Some(1)))
            }
            _ => follow(&model, target, machine, &mut arena),
        }
    });

    paths
        .into_iter()
        .map(|path| PathSummary {
            ending: ending(input, &path.end),
            reached: generator
                .sites
                .iter()
                .filter_map(|&site| {
                    let record = path.machine.labelled(METHOD_LABELS | site)?;
                    let (name, mask) = records[record as usize];
                    Some((site, (arena.node(name).clone(), mask)))
                })
                .collect(),
        })
        .collect()
}

/// `Always` when, in every run, no path failed, some path returned, and every returned path
/// reached the call.
fn condition(runs: &[Vec<PathSummary>], site: u64) -> Condition {
    let always = runs.iter().all(|paths| {
        let returned: Vec<_> = paths
            .iter()
            .filter(|path| path.ending == Ending::Returned)
            .collect();
        !paths.iter().any(|path| path.ending == Ending::Failed)
            && !returned.is_empty()
            && returned.iter().all(|path| path.reached.contains_key(&site))
    });

    if always {
        Condition::Always
    } else {
        Condition::Unresolved
    }
}

/// The family of one call from every name and mask that the paths recorded there.
fn family(
    records: BTreeSet<&(Node, Option<u64>)>,
    condition: Condition,
) -> Result<Family, Unresolved> {
    let mut records = records.into_iter();
    let Some((name, mask)) = records.next() else {
        return Err(Unresolved("unreached"));
    };
    if records.next().is_some() {
        return Err(Unresolved("paths-disagree"));
    }

    let mask = mask.ok_or(Unresolved("category-mask"))?;
    if !name.is_resolved() {
        return Err(Unresolved("name"));
    }
    if !name.parts.contains(&Part::ItemKey) {
        return Err(Unresolved("no-item-key"));
    }

    Ok(Family {
        parts: name.parts.clone(),
        limit: name.limit,
        mask,
        condition,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::decode::Instruction;

    const FROM_TEXT: u64 = 0x900;
    const APPEND_STRING: u64 = 0x904;
    const APPEND_TEXT: u64 = 0x908;
    const FORMAT: u64 = 0x90c;
    const ALLOCATE: u64 = 0x910;
    const LENGTH: u64 = 0x91c;
    const COPY: u64 = 0x920;
    const NEVER_RETURNS: u64 = 0x924;
    const REGISTER: u64 = 0x930;
    const OTHER: u64 = 0x940;

    const CONSTRUCTOR: u64 = 0x100;
    const THUNK: u64 = 0x300;
    const GENERATOR: u64 = 0x400;

    type Rows = Vec<(u64, String, String)>;

    fn rows(start: u64, lines: &[(&str, &str)]) -> Rows {
        lines
            .iter()
            .enumerate()
            .map(|(index, (operation, operands))| {
                (
                    start + index as u64 * 4,
                    (*operation).into(),
                    (*operands).into(),
                )
            })
            .collect()
    }

    /// Replace the instruction at `address`.
    fn patch(rows: &mut Rows, address: u64, operation: &str, operands: &str) {
        let row = rows.iter_mut().find(|row| row.0 == address).unwrap();
        *row = (address, operation.into(), operands.into());
    }

    /// Copies the key argument inline into a stack temporary, then into the item at `offset`
    /// through vector registers. The temporary's last eight bytes are never written, so the copy
    /// of its capacity and flag byte is unknown.
    fn constructor(offset: u64) -> Rows {
        rows(
            CONSTRUCTOR,
            &[
                ("sub", "sp,sp,#0x40"),
                ("mov", "x20,x0"),
                ("ldrsb", "w8,[x2,#0x17]"),
                ("ldr", "x9,[x2]"),
                ("cmp", "w8,#0"),
                ("csel", "x22,x9,x2,lt"),
                ("mov", "x0,x22"),
                ("bl", &format!("#{LENGTH:#x}")),
                ("mov", "x23,x0"),
                ("add", "x1,x23,#1"),
                ("bl", &format!("#{ALLOCATE:#x}")),
                ("mov", "x24,x0"),
                ("str", "x24,[sp,#8]"),
                ("str", "x23,[sp,#0x10]"),
                ("str", "x23,[sp,#0x18]"),
                ("mov", "x0,x24"),
                ("mov", "x1,x22"),
                ("mov", "x2,x23"),
                ("bl", &format!("#{COPY:#x}")),
                ("ldur", "q0,[sp,#0x18]"),
                ("ldur", "q1,[sp,#8]"),
                ("stur", &format!("q0,[x20,#{:#x}]", offset + 0x10)),
                ("stur", &format!("q1,[x20,#{offset:#x}]")),
                ("bl", &format!("#{OTHER:#x}")),
                ("add", "sp,sp,#0x40"),
                ("ret", ""),
            ],
        )
    }

    /// `planet_{key}_build_speed_mult`, with the key object at item `+0x10`: build, append the
    /// key, move the object through vector registers, append the suffix, register.
    fn concatenating_generator() -> Rows {
        rows(
            GENERATOR,
            &[
                ("sub", "sp,sp,#0x80"),
                ("ldrsw", "x8,[x0,#0x54]"),
                ("cbz", "w8,#0x47c"),
                ("mov", "x21,x0"),
                ("ldr", "x27,[x0,#0x48]"),
                ("add", "x28,x27,x8,lsl#3"),
                // 0x418: the loop.
                ("ldr", "x8,[x27]"),
                ("add", "x26,x8,#0x10"),
                ("add", "x0,sp,#0x28"),
                ("adrp", "x1,#0x5000"),
                ("add", "x1,x1,#0"),
                ("bl", &format!("#{FROM_TEXT:#x}")),
                ("add", "x0,sp,#0x28"),
                ("mov", "x1,x26"),
                ("bl", &format!("#{APPEND_STRING:#x}")),
                ("ldur", "q0,[sp,#0x28]"),
                ("ldur", "q1,[sp,#0x38]"),
                ("stp", "q0,q1,[sp,#0x50]"),
                ("add", "x0,sp,#0x50"),
                ("adrp", "x1,#0x5000"),
                ("add", "x1,x1,#0x10"),
                ("bl", &format!("#{APPEND_TEXT:#x}")),
                ("mov", "w8,#0x40000000"),
                ("str", "w8,[sp]"),
                ("add", "x1,sp,#0x50"),
                // 0x464: the registration.
                ("bl", &format!("#{REGISTER:#x}")),
                ("add", "x27,x27,#8"),
                ("cmp", "x27,x28"),
                ("b.ne", "#0x418"),
                ("nop", ""),
                // 0x47c
                ("add", "sp,sp,#0x80"),
                ("ret", ""),
            ],
        )
    }

    /// `%s_empire_windup_mult` through the formatter, with the key object at item `+0x18`. The
    /// formatter takes the key's text: the object for a short key, its buffer for a long one.
    fn formatting_generator(format: u64) -> Rows {
        rows(
            GENERATOR,
            &[
                ("sub", "sp,sp,#0xc0"),
                ("ldr", "x24,[x0,#0x48]"),
                ("ldr", "x28,[x24]"),
                ("add", "x27,x28,#0x18"),
                ("ldrsb", "w9,[x28,#0x2f]"),
                ("mov", "x8,x27"),
                ("tbz", "w9,#0x1f,#0x420"),
                ("ldr", "x8,[x27]"),
                // 0x420
                ("str", "x8,[sp]"),
                ("add", "x0,sp,#0x40"),
                ("adrp", "x1,#0x5000"),
                ("add", "x1,x1,#{format}"),
                ("bl", &format!("#{FORMAT:#x}")),
                ("add", "x0,sp,#0x18"),
                ("add", "x1,sp,#0x40"),
                ("bl", &format!("#{FROM_TEXT:#x}")),
                ("mov", "w8,#0x100"),
                ("str", "w8,[sp]"),
                ("add", "x1,sp,#0x18"),
                // 0x44c: the registration.
                ("bl", &format!("#{REGISTER:#x}")),
                ("add", "sp,sp,#0xc0"),
                ("ret", ""),
            ],
        )
        .into_iter()
        .map(|(address, operation, operands)| {
            let operands = operands.replace("{format}", &format!("{format:#x}"));
            (address, operation, operands)
        })
        .collect()
    }

    /// `{key}_build_speed_mult`, with the key object at item `+0x10` copied inline into a stack
    /// string first: a short key into the object itself, a long key into a new buffer.
    fn copying_generator() -> Rows {
        rows(
            GENERATOR,
            &[
                ("sub", "sp,sp,#0x80"),
                ("ldr", "x8,[x0,#0x48]"),
                ("ldr", "x19,[x8]"),
                ("ldr", "x8,[x19,#0x10]!"),
                ("ldrsb", "w9,[x19,#0x17]"),
                ("cmp", "w9,#0"),
                ("csel", "x28,x8,x19,lt"),
                ("mov", "x0,x28"),
                ("bl", &format!("#{LENGTH:#x}")),
                ("mov", "x21,x0"),
                ("cmp", "x0,#0x16"),
                ("b.hi", "#0x43c"),
                ("strb", "w21,[sp,#0x57]"),
                ("add", "x24,sp,#0x40"),
                ("b", "#0x454"),
                // 0x43c: a long key gets a buffer.
                ("add", "x1,x21,#1"),
                ("bl", &format!("#{ALLOCATE:#x}")),
                ("mov", "x24,x0"),
                ("str", "x0,[sp,#0x40]"),
                ("str", "x21,[sp,#0x48]"),
                ("nop", ""),
                // 0x454
                ("mov", "x0,x24"),
                ("mov", "x1,x28"),
                ("mov", "x2,x21"),
                ("bl", &format!("#{COPY:#x}")),
                ("strb", "wzr,[x24,x21]"),
                ("add", "x0,sp,#0x40"),
                ("adrp", "x1,#0x5000"),
                ("add", "x1,x1,#0x10"),
                ("bl", &format!("#{APPEND_TEXT:#x}")),
                ("mov", "w8,#0x100"),
                ("str", "w8,[sp]"),
                ("add", "x1,sp,#0x40"),
                // 0x484: the registration.
                ("bl", &format!("#{REGISTER:#x}")),
                ("add", "sp,sp,#0x80"),
                ("ret", ""),
            ],
        )
    }

    fn family_input(parts: &[Rows], sites: &[u64], constructor: Option<u64>) -> FamilyInput {
        let code = Code::from_rows(
            parts
                .iter()
                .flatten()
                .map(|(address, operation, operands)| Instruction {
                    address: *address,
                    bytes: [0; 4],
                    operation: operation.clone(),
                    operands: operands.clone(),
                }),
        );
        let data = ReadOnlyData::new(vec![(
            0x5000,
            b"planet_\0\0\0\0\0\0\0\0\0_build_speed_mult\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0%s_empire_windup_mult\0\0\0\0\0\0\0\0\0\0\0%d_bad\0"
                .to_vec(),
        )]);

        FamilyInput {
            unjoined_sites: 0,
            generator: Some(Generator {
                function: GENERATOR,
                sites: sites.to_vec(),
                constructor,
            }),
            registration: REGISTER,
            category_offset: 0,
            database: DatabaseLayout {
                items_offset: 0x48,
                count_offset: 0x54,
            },
            strings: StringFunctions {
                from_text: [FROM_TEXT].into(),
                append_string: [APPEND_STRING].into(),
                append_text: [APPEND_TEXT].into(),
                formatters: [(FORMAT, 128)].into(),
                allocators: [ALLOCATE].into(),
                lengths: [LENGTH].into(),
                copies: [COPY].into(),
                never_return: [NEVER_RETURNS].into(),
                ..StringFunctions::default()
            },
            layout: StringLayout { flag_byte: 0x17 },
            code,
            data,
        }
    }

    fn literal(text: &str) -> Part {
        Part::Literal(text.into())
    }

    fn only_site(input: &FamilyInput) -> Result<Family, Unresolved> {
        let result = analyze(input).unwrap();
        assert_eq!(result.sites.len(), 1);
        result.sites[0].1.clone()
    }

    #[test]
    fn a_concatenated_name_is_a_template_for_every_item() {
        let input = family_input(
            &[constructor(0x10), concatenating_generator()],
            &[0x464],
            Some(CONSTRUCTOR),
        );
        let result = analyze(&input).unwrap();
        assert_eq!(result.key_offset, Ok(0x10));
        assert_eq!(
            result.sites,
            [(
                0x464,
                Ok(Family {
                    parts: vec![
                        literal("planet_"),
                        Part::ItemKey,
                        literal("_build_speed_mult")
                    ],
                    limit: None,
                    mask: 0x4000_0000,
                    condition: Condition::Always,
                })
            )]
        );
    }

    #[test]
    fn a_formatted_name_keeps_the_formatter_bound() {
        let input = family_input(
            &[constructor(0x18), formatting_generator(0x30)],
            &[0x44c],
            Some(CONSTRUCTOR),
        );
        assert_eq!(
            only_site(&input),
            Ok(Family {
                parts: vec![Part::ItemKey, literal("_empire_windup_mult")],
                limit: Some(127),
                mask: 0x100,
                condition: Condition::Always,
            })
        );

        let other_directive = family_input(
            &[constructor(0x18), formatting_generator(0x50)],
            &[0x44c],
            Some(CONSTRUCTOR),
        );
        assert_eq!(only_site(&other_directive), Err(Unresolved("name")));
    }

    /// Post-freeze revision 1: a short key copied into a stack string and then appended to must
    /// not keep the short string's label, or the two key forms disagree.
    #[test]
    fn an_inline_copy_of_either_key_form_is_the_item_key() {
        let input = family_input(
            &[constructor(0x10), copying_generator()],
            &[0x484],
            Some(CONSTRUCTOR),
        );
        assert_eq!(
            only_site(&input),
            Ok(Family {
                parts: vec![Part::ItemKey, literal("_build_speed_mult")],
                limit: None,
                mask: 0x100,
                condition: Condition::Always,
            })
        );
    }

    #[test]
    fn a_branch_on_an_item_field_leaves_the_condition_unresolved() {
        let mut generator = concatenating_generator();
        patch(&mut generator, 0x41c, "b", "#0x4a0");
        generator.extend(rows(
            0x4a0,
            &[
                ("add", "x26,x8,#0x10"),
                ("ldrb", "w9,[x8,#0x508]"),
                ("cbz", "w9,#0x468"),
                ("b", "#0x420"),
            ],
        ));
        let input = family_input(&[constructor(0x10), generator], &[0x464], Some(CONSTRUCTOR));
        let family = only_site(&input).unwrap();
        assert_eq!(family.condition, Condition::Unresolved);
        assert_eq!(family.mask, 0x4000_0000);
    }

    #[test]
    fn a_path_into_a_function_that_never_returns_is_ignored() {
        let mut generator = concatenating_generator();
        patch(&mut generator, 0x478, "b", "#0x4a0");
        generator.extend(rows(
            0x4a0,
            &[
                ("ldr", "x9,[x21,#0x48]"),
                ("ldr", "x9,[x9]"),
                ("ldr", "x9,[x9,#0x200]"),
                ("cbz", "x9,#0x4b4"),
                ("bl", &format!("#{NEVER_RETURNS:#x}")),
                // 0x4b4
                ("add", "sp,sp,#0x80"),
                ("ret", ""),
            ],
        ));
        patch(&mut generator, 0x47c, "b", "#0x4a0");
        let input = family_input(&[constructor(0x10), generator], &[0x464], Some(CONSTRUCTOR));
        assert_eq!(only_site(&input).unwrap().condition, Condition::Always);
    }

    #[test]
    fn paths_that_disagree_on_the_mask_leave_the_call_unresolved() {
        let mut generator = concatenating_generator();
        patch(&mut generator, 0x458, "b", "#0x4a0");
        generator.extend(rows(
            0x4a0,
            &[
                ("ldr", "x9,[x27]"),
                ("ldrb", "w9,[x9,#0x100]"),
                ("mov", "w8,#1"),
                ("cbz", "w9,#0x4b4"),
                ("mov", "w8,#2"),
                // 0x4b4
                ("b", "#0x45c"),
            ],
        ));
        let input = family_input(&[constructor(0x10), generator], &[0x464], Some(CONSTRUCTOR));
        assert_eq!(only_site(&input), Err(Unresolved("paths-disagree")));
    }

    #[test]
    fn a_call_outside_the_model_leaves_the_name_unresolved() {
        let mut generator = concatenating_generator();
        patch(&mut generator, 0x458, "b", "#0x4a0");
        generator.extend(rows(
            0x4a0,
            &[
                ("add", "x0,sp,#0x50"),
                ("bl", &format!("#{OTHER:#x}")),
                ("mov", "w8,#0x40000000"),
                ("b", "#0x45c"),
            ],
        ));
        let input = family_input(&[constructor(0x10), generator], &[0x464], Some(CONSTRUCTOR));
        assert_eq!(only_site(&input), Err(Unresolved("name")));
    }

    /// Post-freeze revision 2: a copy of unknown length into a string may change it.
    #[test]
    fn a_copy_of_unknown_length_leaves_the_name_unresolved() {
        let mut generator = concatenating_generator();
        patch(&mut generator, 0x458, "b", "#0x4a0");
        generator.extend(rows(
            0x4a0,
            &[
                ("ldr", "x0,[sp,#0x50]"),
                ("ldr", "x2,[x27]"),
                ("ldr", "x2,[x2,#0x300]"),
                ("bl", &format!("#{COPY:#x}")),
                ("mov", "w8,#0x40000000"),
                ("b", "#0x45c"),
            ],
        ));
        let input = family_input(&[constructor(0x10), generator], &[0x464], Some(CONSTRUCTOR));
        assert_eq!(only_site(&input), Err(Unresolved("name")));
    }

    #[test]
    fn a_name_without_the_item_key_is_not_a_family() {
        let mut generator = concatenating_generator();
        patch(&mut generator, 0x434, "mov", "x1,x0");
        patch(&mut generator, 0x438, "nop", "");
        let input = family_input(&[constructor(0x10), generator], &[0x464], Some(CONSTRUCTOR));
        assert_eq!(only_site(&input), Err(Unresolved("no-item-key")));

        let unreached = family_input(
            &[constructor(0x10), concatenating_generator()],
            &[0x464, 0x468],
            Some(CONSTRUCTOR),
        );
        assert_eq!(
            analyze(&unreached).unwrap().sites[1].1,
            Err(Unresolved("unreached"))
        );
    }

    #[test]
    fn the_key_storage_is_found_through_thunks_and_failed_paths() {
        let thunk = rows(THUNK, &[("b", &format!("#{CONSTRUCTOR:#x}"))]);
        let through_thunk = family_input(
            &[constructor(0x18), thunk, concatenating_generator()],
            &[0x464],
            Some(THUNK),
        );
        assert_eq!(analyze(&through_thunk).unwrap().key_offset, Ok(0x18));

        let mut failing = constructor(0x18);
        patch(&mut failing, 0x15c, "fmla", "s0,s1,s2");
        let stored_then_failed = family_input(
            &[failing, concatenating_generator()],
            &[0x464],
            Some(CONSTRUCTOR),
        );
        assert_eq!(analyze(&stored_then_failed).unwrap().key_offset, Ok(0x18));

        let no_store = rows(CONSTRUCTOR, &[("ret", "")]);
        let missing = family_input(
            &[no_store, concatenating_generator()],
            &[0x464],
            Some(CONSTRUCTOR),
        );
        let result = analyze(&missing).unwrap();
        assert_eq!(result.key_offset, Err(Unresolved("key-storage")));
        assert!(result.sites.is_empty());

        let no_constructor = family_input(&[concatenating_generator()], &[0x464], None);
        assert_eq!(
            analyze(&no_constructor).unwrap().key_offset,
            Err(Unresolved("constructor"))
        );
    }
}
