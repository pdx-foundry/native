use crate::binding::targets::PersistentRecipe;
use crate::engine::analysis::{
    decode::{call_or_jump_target, call_target},
    discovery::Symbol,
    evaluate::ReadOnlyData,
    fields::{
        FieldInput, Function, KeyReaders, LoaderRoot, ObjectReader, PersistentInput, PointReader,
        has_owner_receiver,
    },
    readers,
};
use crate::{AnalysisError, BlockFamily};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    declarations::{Text, read_only_sections},
    families::Names,
    receivers::SharedPersistent,
    references::Image,
    symbols::IndexedSymbols,
};

/// Largest body of a reader function that the field reader collects.
const FUNCTION_LIMIT: u64 = 1024 * 1024;

/// Largest body of an array insertion specialization that the field reader inspects.
const INSERTION_LIMIT: u64 = 65_536;

/// The parts of a registry's field input that every registry shares, read once per executable.
pub(in crate::binding) struct SharedFieldInput {
    key_readers: KeyReaders,
    persistent: SharedPersistent,
}

impl SharedFieldInput {
    /// Read them from `image` with the recipe of its build; a field input then reads the rest
    /// from the same image.
    pub fn read(
        image: &Image<'_>,
        persistent_recipe: &PersistentRecipe,
    ) -> Result<Self, AnalysisError> {
        let text = Text::read(image.bytes, image.symbols)?;

        Ok(Self {
            key_readers: key_readers(image.symbols, persistent_recipe)?,
            persistent: SharedPersistent::read(image, &text, persistent_recipe)?,
        })
    }
}

/// The function that reads each root statement of a custom loader's files.
pub(super) const STATEMENT_READER: &str = "CReader::ReadSimpleStatement()";

/// The custom loader whose root switch the field input may read: its database class and the
/// reader offset of a statement's token word.
pub(in crate::binding) struct LoaderSearch<'a> {
    pub database: &'a str,
    pub reader_token_offset: u64,
}

/// Read the root of `owner`, the shared engine token constructor and the read-only data that holds
/// jump tables from the same verified buffer. `shared` is the shared part read from `image`.
/// `loader` names the custom loader that builds `owner`, if any.
pub(in crate::binding) fn read(
    image: &Image<'_>,
    shared: &SharedFieldInput,
    owner: &str,
    loader: Option<LoaderSearch<'_>>,
    persistent_recipe: &PersistentRecipe,
) -> Result<FieldInput, AnalysisError> {
    let Image {
        bytes,
        symbols,
        strings,
        pointers,
        bound_slots,
        ..
    } = *image;
    let indexed = image.index.over(symbols);
    let text = Text::read(bytes, symbols)?;
    let data = super::language::constant_data(bytes, pointers, bound_slots)?;
    let mut functions = Vec::new();
    let mut gaps = Vec::new();
    let root = format!("{owner}::ReadMember(CReader&, int)");
    for name in [
        root.as_str(),
        "CPersistent::ReadMember(CReader&, int)",
        "GetTokenArray()",
    ] {
        let starts: BTreeSet<_> = indexed.named(name).map(|s| s.address).collect();
        if starts.len() != 1 {
            gaps.push(format!("missing or ambiguous function {name}"));
            continue;
        }
        let start = *starts.first().unwrap();
        let Some(code) = text.function_within(start, FUNCTION_LIMIT) else {
            gaps.push(format!("unsupported function extent {name}"));
            continue;
        };
        functions.push(Function {
            name: name.into(),
            address: start,
            code: code.to_vec(),
        });
    }
    let objects = collect_objects(
        image,
        &text,
        &data,
        Some(&shared.persistent),
        persistent_recipe,
        &root,
        &mut functions,
        &mut gaps,
    )?;
    if !objects.is_empty() {
        collect_owner_methods(&text, symbols, owner, &mut functions, &mut gaps);
    }
    let loader_root = match loader {
        Some(search) => loader_root(
            &text,
            indexed,
            &data,
            pointers,
            owner,
            &search,
            persistent_recipe,
            &mut functions,
            &mut gaps,
        )?,
        None => None,
    };
    let persistent = super::receivers::persistent(
        image,
        &text,
        &data,
        &shared.persistent,
        owner,
        persistent_recipe,
    )?;
    push_embedded_readers(&text, indexed, &persistent, &mut functions);
    let stubs = push_inherited_readers(image, &text, &root, &mut functions)?;
    push_converters(&text, indexed, &mut functions);
    Ok(FieldInput {
        key_readers: shared.key_readers.clone(),
        persistent: Some(persistent),
        loader_root,
        objects,
        owner: owner.into(),
        symbols: symbols.to_vec(),
        functions,
        stubs,
        strings: strings.clone(),
        read_only_data: read_only_sections(bytes)?,
        gaps,
    })
}

/// The one function of the loader's database that reads root statements and calls an
/// `(EScopeType)` constructor of `owner`, and the reader register at its statement read. Adds its
/// body to `functions`. None when no function of the database does both, as for a loader that
/// passes no scope to its items; several, or a statement read that the method cannot follow, is a
/// gap.
#[allow(clippy::too_many_arguments)] // The input parts that `read` holds, and its two outputs.
fn loader_root(
    text: &Text,
    symbols: IndexedSymbols<'_>,
    data: &ReadOnlyData,
    pointers: &BTreeMap<u64, u64>,
    owner: &str,
    search: &LoaderSearch<'_>,
    recipe: &PersistentRecipe,
    functions: &mut Vec<Function>,
    gaps: &mut Vec<String>,
) -> Result<Option<LoaderRoot>, AnalysisError> {
    let scoped_constructor = |symbol: &&Symbol| symbol.name.ends_with("(EScopeType)");
    let constructors: BTreeSet<u64> = symbols
        .constructors_of(owner)
        .into_iter()
        .filter(scoped_constructor)
        .map(|symbol| symbol.address)
        .collect();
    let statement_reads: BTreeSet<u64> = addresses(symbols, STATEMENT_READER).into_iter().collect();

    if constructors.is_empty() || statement_reads.is_empty() {
        return Ok(None);
    }

    let prefix = format!("{}::", search.database);
    let mut roots = Vec::new();

    for symbol in symbols
        .symbols
        .iter()
        .filter(|s| s.name.starts_with(&prefix))
    {
        let Some(code) = text.function_within(symbol.address, FUNCTION_LIMIT) else {
            continue;
        };
        let rows = crate::engine::analysis::decode::decode_arm64(code, symbol.address)
            .map_err(|_| AnalysisError::InvalidRange)?;
        let calls = |targets: &BTreeSet<u64>| -> Vec<usize> {
            rows.iter()
                .enumerate()
                .filter(|(_, row)| row.operation == "bl")
                .filter(|(_, row)| {
                    u64::from_str_radix(row.operands.trim_start_matches("#0x"), 16)
                        .is_ok_and(|target| targets.contains(&target))
                })
                .map(|(index, _)| index)
                .collect()
        };

        if calls(&constructors).is_empty() {
            continue;
        }

        let reads = calls(&statement_reads);
        if !reads.is_empty() {
            roots.push((symbol, statement_reader(&rows, &reads)));
        }
    }

    let [(symbol, read)] = roots.as_slice() else {
        if roots.len() > 1 {
            gaps.push(format!("several loader roots of {}", search.database));
        }

        return Ok(None);
    };
    let Some((start, reader)) = read.clone() else {
        gaps.push(format!("unsupported statement read in {}", symbol.name));
        return Ok(None);
    };

    push_function(text, functions, symbol);

    let names = Names::new(symbols.symbols);
    let unique_name = |address: u64| names.unique(address).map(str::to_owned);
    let item_reader = symbols
        .vtable_group(data, owner)
        .and_then(|vtable| vtable.address_points.get(&0).copied())
        .and_then(|point| super::receivers::concrete_reader(recipe, pointers, point, &unique_name));

    Ok(Some(LoaderRoot {
        function: symbol.name.clone(),
        start,
        reader,
        token_offset: search.reader_token_offset,
        constructors,
        item_reader,
    }))
}

/// The address after the one statement read at `reads` and the callee-saved register that the
/// instruction before it copies into the reader argument.
fn statement_reader(
    rows: &[crate::engine::analysis::decode::Instruction],
    reads: &[usize],
) -> Option<(u64, String)> {
    let [read] = reads else {
        return None;
    };
    let copy = rows.get(read.checked_sub(1)?)?;
    let (destination, source) = copy.operands.split_once(',')?;
    let callee_saved = source
        .strip_prefix('x')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (19..=28).contains(&number));

    (copy.operation == "mov" && destination == "x0" && callee_saved)
        .then(|| (rows[*read].address + 4, source.to_owned()))
}

/// The member readers that the method may walk for the keys of an embedded object.
fn push_embedded_readers(
    text: &Text,
    symbols: IndexedSymbols<'_>,
    persistent: &PersistentInput,
    functions: &mut Vec<Function>,
) {
    for reader in persistent
        .readers
        .values()
        .filter(|reader| reader.family == BlockFamily::Unknown)
    {
        if let [symbol] = symbols.named(&reader.member).collect::<Vec<_>>()[..] {
            push_function(text, functions, symbol);
        }
    }
}

/// The member readers that `root` and the readers it reaches call directly, which dispatch
/// follows as inherited readers, and the import stubs among those calls with the body that each
/// reaches. A stub counts only when the image's fixups resolve its pointer slot to a text body of
/// the stub's name, as same-image weak coalescing does.
fn push_inherited_readers(
    image: &Image<'_>,
    text: &Text,
    root: &str,
    functions: &mut Vec<Function>,
) -> Result<BTreeMap<u64, u64>, AnalysisError> {
    let symbols = image.index.over(image.symbols);
    let slice = super::selected_slice(image.bytes).map_err(|_| AnalysisError::InvalidRange)?;
    let file = object::File::parse(slice).map_err(|_| AnalysisError::InvalidRange)?;
    let mut stubs = BTreeMap::new();
    let mut callers = vec![root.to_owned()];

    while let Some(caller) = callers.pop() {
        let Some(function) = functions.iter().find(|function| function.name == caller) else {
            continue;
        };
        let targets: BTreeSet<u64> = function
            .code
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter_map(|(index, word)| {
                call_or_jump_target(
                    u32::from_le_bytes(*word),
                    function.address + 4 * index as u64,
                )
            })
            .collect();
        let calls: Vec<Symbol> = symbols
            .at(&targets)
            .filter(|symbol| readers::is_inheritable_member(&symbol.name))
            .cloned()
            .collect();

        for call in calls {
            let body = if text.starts.contains(&call.address) {
                Some(call.address)
            } else {
                stub_slot(&file, call.address).and_then(|slot| image.pointers.get(&slot).copied())
            };
            let Some(definition) = body.and_then(|body| {
                symbols
                    .named(&call.name)
                    .find(|symbol| symbol.address == body && text.starts.contains(&body))
            }) else {
                continue;
            };
            let body = definition.address;

            if body != call.address {
                stubs.insert(call.address, body);
            }
            let known = functions.iter().any(|function| function.address == body);

            if !known && push_function(text, functions, definition) {
                callers.push(call.name);
            }
        }
    }

    Ok(stubs)
}

/// The pointer slot that the import stub at `stub` jumps through: `adrp x16`, `ldr x16, [x16, #n]`
/// and `br x16`.
fn stub_slot(file: &object::File<'_>, stub: u64) -> Option<u64> {
    use object::{Object, ObjectSection};

    let bytes = file
        .sections()
        .find_map(|section| section.data_range(stub, 12).ok().flatten())?;
    let rows = crate::engine::analysis::decode::decode_arm64(bytes, stub).ok()?;
    let [page, load, jump] = rows.as_slice() else {
        return None;
    };
    let page_address = page
        .operands
        .strip_prefix("x16,#0x")
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        .filter(|_| page.operation == "adrp")?;
    let offset = load
        .operands
        .strip_prefix("x16,[x16,#0x")
        .and_then(|rest| rest.strip_suffix(']'))
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        .filter(|_| load.operation == "ldr")?;

    (jump.operation == "br" && jump.operands == "x16").then_some(page_address + offset)
}

/// The token converters that the collected member readers call directly, which the keyword
/// domain method runs.
fn push_converters(text: &Text, symbols: IndexedSymbols<'_>, functions: &mut Vec<Function>) {
    let targets: BTreeSet<u64> = functions
        .iter()
        .filter(|function| readers::is_member(&function.name))
        .flat_map(|function| {
            function
                .code
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .filter_map(|(index, word)| {
                    let word = u32::from_le_bytes(*word);
                    call_target(word, function.address + 4 * index as u64)
                })
        })
        .collect();
    let converters: Vec<_> = symbols
        .at(&targets)
        .filter(|symbol| readers::converter(&symbol.name).is_some())
        .cloned()
        .collect();
    for symbol in &converters {
        push_function(text, functions, symbol);
    }
}

/// The objects that the member reader `root` constructs and inserts. Follow direct constructor
/// and factory calls only; a symbol alone does not attach a nested class to a field. With
/// `nested`, an object whose reader has no block family is a nested owner: it gets its own
/// constructor evidence and inserted objects, one level deep.
#[allow(clippy::too_many_arguments)] // The image parts that `read` holds, and its two outputs.
fn collect_objects(
    image: &Image<'_>,
    text: &Text,
    data: &ReadOnlyData,
    nested: Option<&SharedPersistent>,
    recipe: &PersistentRecipe,
    root: &str,
    functions: &mut Vec<Function>,
    gaps: &mut Vec<String>,
) -> Result<Vec<ObjectReader>, AnalysisError> {
    let symbols = image.index.over(image.symbols);
    let pointers = image.pointers;
    let Some(root) = functions.iter().find(|function| function.name == root) else {
        return Ok(Vec::new());
    };
    let rows = crate::engine::analysis::decode::decode_arm64(&root.code, root.address)
        .map_err(|_| AnalysisError::InvalidRange)?;
    let calls: BTreeSet<_> = rows
        .iter()
        .filter(|row| row.operation == "bl")
        .filter_map(|row| u64::from_str_radix(row.operands.trim_start_matches("#0x"), 16).ok())
        .collect();
    let mut classes = BTreeMap::<String, Vec<&Symbol>>::new();

    for symbol in symbols.at(&calls) {
        if let Some(class) = super::receivers::constructor_class(&symbol.name) {
            classes.entry(class.to_owned()).or_default();
        } else if let Some(class) = factory_class(&symbol.name) {
            classes.entry(class.to_owned()).or_default().push(symbol);
        }
    }

    if classes.is_empty() {
        return Ok(Vec::new());
    }

    let names = Names::new(symbols.symbols);
    let unique_name = |address: u64| names.unique(address).map(str::to_owned);
    let mut objects = Vec::new();

    for (class, factories) in classes {
        let Some(vtable) = symbols.vtable_group(data, &class) else {
            continue;
        };
        let reads: BTreeSet<_> = symbols
            .named("CPersistent::Read(CReader&)")
            .filter(|symbol| vtable.slots.contains_key(&symbol.address))
            .map(|symbol| symbol.address)
            .collect();

        if reads.len() != 1 {
            continue;
        }

        let reader = vtable.address_points.get(&0).and_then(|&point| {
            Some(PointReader {
                point,
                reader: super::receivers::concrete_reader(recipe, pointers, point, &unique_name)?,
            })
        });
        let member_name = format!("{class}::ReadMember(CReader&, int)");
        let mut bodies_available = match symbols.named(&member_name).next() {
            Some(symbol) => push_function(text, functions, symbol),
            None => reader.is_some(),
        };

        for factory in &factories {
            bodies_available &= push_function(text, functions, factory);
        }

        if !bodies_available {
            continue;
        }

        let mut slots = BTreeMap::new();
        for point in vtable.address_points.values() {
            let end = symbols.next_address(*point).unwrap_or(*point);
            for at in (*point..end).step_by(8) {
                if let Some(value) = data.read(at, 8) {
                    slots.insert(at, value);
                }
            }
        }
        let insert = addresses(
            symbols,
            &format!(
                "{class}*& CPdxArray<{class}*, int>::InsertAtEmplace<{class}* const&>(int, {class}* const&)"
            ),
        );
        let scoped = format!("CPdxScopedPtrImpl<{class}, false>");
        let moving_insert = addresses(
            symbols,
            &format!(
                "{scoped}& CPdxArray<{scoped}, int>::InsertAtEmplace<{scoped} >(int, {scoped}&&)"
            ),
        )
        .into_iter()
        .filter(|&start| clears_moved_source_at(text, start))
        .collect();
        let data_offset = array_member_offset(text, &insert, array_pointer_offsets)?;
        let count_offset = array_member_offset(text, &insert, array_count_offsets)?;
        let owner_of_keys = reader
            .as_ref()
            .is_none_or(|bound| bound.reader.family == BlockFamily::Unknown);
        let (persistent, nested_objects) = match nested.filter(|_| owner_of_keys) {
            Some(shared) => nested_owner(
                image,
                text,
                data,
                shared,
                recipe,
                &class,
                &member_name,
                functions,
                gaps,
            )?,
            None => (None, Vec::new()),
        };
        objects.push(ObjectReader {
            data_offset,
            count_offset,
            constructors: symbols
                .constructors_of(&class)
                .iter()
                .map(|symbol| symbol.address)
                .collect(),
            factories: factories.iter().map(|symbol| symbol.address).collect(),
            insert,
            moving_insert,
            class,
            vtables: vtable.address_points,
            pointers: slots,
            read: *reads.first().unwrap(),
            reader,
            persistent,
            objects: nested_objects,
        });
    }
    Ok(objects)
}

/// The constructor evidence and inserted objects of the nested owner `class`, whose member reader
/// is `member`. A constructor that the binding cannot read leaves the evidence out, with a gap.
#[allow(clippy::too_many_arguments)] // The image parts that `read` holds, and its two outputs.
fn nested_owner(
    image: &Image<'_>,
    text: &Text,
    data: &ReadOnlyData,
    shared: &SharedPersistent,
    recipe: &PersistentRecipe,
    class: &str,
    member: &str,
    functions: &mut Vec<Function>,
    gaps: &mut Vec<String>,
) -> Result<(Option<PersistentInput>, Vec<ObjectReader>), AnalysisError> {
    let persistent = match super::receivers::persistent(image, text, data, shared, class, recipe) {
        Ok(persistent) => {
            push_embedded_readers(
                text,
                image.index.over(image.symbols),
                &persistent,
                functions,
            );
            Some(persistent)
        }
        Err(_) => {
            gaps.push(format!("unreadable constructors of nested owner {class}"));
            None
        }
    };
    let objects = collect_objects(image, text, data, None, recipe, member, functions, gaps)?;

    Ok((persistent, objects))
}

/// The class that a `PdxMakeScopedPtr<Class, …>` factory allocates and constructs.
fn factory_class(name: &str) -> Option<&str> {
    let (_, arguments) = name.split_once(" PdxMakeScopedPtr<")?;
    let mut depth = 0usize;

    for (at, character) in arguments.char_indices() {
        match character {
            '<' => depth += 1,
            '>' if depth == 0 => return Some(&arguments[..at]),
            '>' => depth -= 1,
            ',' if depth == 0 => return Some(&arguments[..at]),
            _ => {}
        }
    }

    None
}

fn addresses(symbols: IndexedSymbols<'_>, name: &str) -> Vec<u64> {
    symbols.named(name).map(|symbol| symbol.address).collect()
}

/// Add a symbol's bounded body once; whether the body is available.
fn push_function(text: &Text, functions: &mut Vec<Function>, symbol: &Symbol) -> bool {
    if functions
        .iter()
        .any(|function| function.address == symbol.address)
    {
        return true;
    }

    let Some(code) = text.function_within(symbol.address, FUNCTION_LIMIT) else {
        return false;
    };

    functions.push(Function {
        name: symbol.name.clone(),
        address: symbol.address,
        code: code.to_vec(),
    });
    true
}

/// Whether the moving insertion at `start` clears its source pointer on every return.
fn clears_moved_source_at(text: &Text, start: u64) -> bool {
    let Some(code) = text.function_within(start, INSERTION_LIMIT) else {
        return false;
    };

    crate::engine::analysis::decode::decode_arm64(code, start)
        .is_ok_and(|rows| clears_moved_source(&rows))
}

/// Whether every return leaves the 8-byte slot that `x2` names on entry holding zero, as the
/// moving insertion of a scoped pointer must. The slot is cleared by `str xzr` through a register
/// that still holds the entry `x2`; any other store through such a register, or a call that may
/// receive it, makes the slot unknown again. Stores through other registers are taken not to
/// alias the caller's source slot, because the insertion writes only its array and frame.
fn clears_moved_source(rows: &[crate::engine::analysis::decode::Instruction]) -> bool {
    #[derive(Clone, PartialEq)]
    struct Source {
        registers: BTreeSet<String>,
        cleared: bool,
    }

    if rows.is_empty() {
        return false;
    }

    let indexes: BTreeMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.address, index))
        .collect();
    let mut states: Vec<Option<Source>> = vec![None; rows.len()];
    states[0] = Some(Source {
        registers: BTreeSet::from(["x2".to_owned()]),
        cleared: false,
    });
    let mut pending = std::collections::VecDeque::from([0]);
    let mut returned = false;
    let mut steps = 0;

    while let Some(pc) = pending.pop_front() {
        steps += 1;
        if steps > rows.len() * 32 {
            return false;
        }

        let row = &rows[pc];
        let Some(mut source) = states[pc].clone() else {
            return false;
        };
        let parts: Vec<_> = row.operands.split(',').collect();

        if row.operation == "ret" {
            if !source.cleared {
                return false;
            }
            returned = true;
            continue;
        }

        if row.operation == "br" {
            return false;
        }

        if row.operation.starts_with("st")
            && memory_base(&row.operands).is_some_and(|base| source.registers.contains(base))
        {
            source.cleared = row.operation == "str" && parts.len() == 2 && parts[0] == "xzr";
        }

        if (row.operation == "bl" || row.operation == "blr")
            && (0..8).any(|index| source.registers.contains(&format!("x{index}")))
        {
            source.cleared = false;
        }

        let copied = row.operation == "mov"
            && parts.len() == 2
            && parts[0].starts_with('x')
            && source.registers.contains(parts[1]);

        for register in
            crate::engine::analysis::decode::written_registers(&row.operation, &row.operands)
        {
            source.registers.remove(&format!("x{register}"));
        }

        if copied {
            source.registers.insert(parts[0].into());
        }

        let mut next = Vec::new();

        if !matches!(row.operation.as_str(), "b" | "brk") && pc + 1 < rows.len() {
            next.push(pc + 1);
        }

        if row.operation == "b"
            || row.operation.starts_with("b.")
            || matches!(row.operation.as_str(), "cbz" | "cbnz" | "tbz" | "tbnz")
        {
            let Some(target) = branch_target(&row.operands) else {
                return false;
            };
            let Some(&index) = indexes.get(&target) else {
                return false;
            };
            next.push(index);
        }

        for next in next {
            let joined = match &states[next] {
                Some(previous) => Source {
                    registers: previous
                        .registers
                        .intersection(&source.registers)
                        .cloned()
                        .collect(),
                    cleared: previous.cleared && source.cleared,
                },
                None => source.clone(),
            };

            if states[next].as_ref() != Some(&joined) {
                states[next] = Some(joined);
                pending.push_back(next);
            }
        }
    }

    returned
}

/// The base register of an instruction's memory operand.
fn memory_base(operands: &str) -> Option<&str> {
    let (_, memory) = operands.split_once('[')?;

    memory.split([',', ']']).next()
}

fn branch_target(operands: &str) -> Option<u64> {
    let operand = operands.rsplit(',').next()?.trim_start_matches('#');

    match operand.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => operand.parse().ok(),
    }
}

fn collect_owner_methods(
    text: &Text,
    symbols: &[Symbol],
    owner: &str,
    functions: &mut Vec<Function>,
    gaps: &mut Vec<String>,
) {
    for symbol in symbols
        .iter()
        .filter(|symbol| has_owner_receiver(&symbol.name, owner))
    {
        if functions
            .iter()
            .any(|function| function.address == symbol.address)
        {
            continue;
        }
        if functions.len() >= 128 {
            gaps.push("owner method collection reached 128 functions".into());
            break;
        }
        let Ok((address, code)) = text.function(symbol.address) else {
            continue;
        };
        if code.len() as u64 > FUNCTION_LIMIT {
            gaps.push(format!("owner method extent: {}", symbol.name));
            continue;
        }
        functions.push(Function {
            name: symbol.name.clone(),
            address,
            code: code.to_vec(),
        });
    }
}

/// The one offset of an array member that `find` reads from every insertion specialization, such
/// as the pointer member that the specialization loads through its preserved receiver. Refuse
/// ambiguous offsets, unfamiliar receiver copies, or an absent specialization.
fn array_member_offset(
    text: &Text,
    inserts: &[u64],
    find: fn(&[crate::engine::analysis::decode::Instruction]) -> Option<BTreeSet<u64>>,
) -> Result<Option<u64>, AnalysisError> {
    let mut offsets = BTreeSet::new();
    for &start in inserts {
        let Some(code) = text.function_within(start, INSERTION_LIMIT) else {
            return Ok(None);
        };
        let rows = crate::engine::analysis::decode::decode_arm64(code, start)
            .map_err(|_| AnalysisError::InvalidRange)?;
        let Some(found) = find(&rows) else {
            return Ok(None);
        };
        offsets.extend(found);
    }
    Ok((offsets.len() == 1).then(|| *offsets.first().unwrap()))
}

/// The offsets of the pointer members that the specialization loads through its receiver.
fn array_pointer_offsets(
    rows: &[crate::engine::analysis::decode::Instruction],
) -> Option<BTreeSet<u64>> {
    let states = receiver_states(rows)?;
    Some(
        rows.iter()
            .zip(states)
            .filter_map(|(row, state)| pointer_member_offset(row, &state?))
            .collect(),
    )
}

/// The offsets of the 32-bit members that the specialization increments: it loads the word
/// through its receiver, adds one, and stores the sum back at the same offset. The array's count
/// is such a member. The scan reads the instructions in address order.
fn array_count_offsets(
    rows: &[crate::engine::analysis::decode::Instruction],
) -> Option<BTreeSet<u64>> {
    let states = receiver_states(rows)?;
    let mut loaded: BTreeMap<String, u64> = BTreeMap::new();
    let mut incremented: BTreeMap<String, u64> = BTreeMap::new();
    let mut counts = BTreeSet::new();
    for (row, receivers) in rows.iter().zip(states) {
        let receivers = receivers.unwrap_or_default();
        let parts: Vec<&str> = row.operands.split(',').collect();
        let member = |base: &str, offset: Option<&&str>| {
            let base = base.strip_prefix('[')?;
            let offset =
                offset.map_or(Some(0), |offset| parse_offset(offset.trim_end_matches(']')))?;
            receivers
                .contains(base.trim_end_matches(']'))
                .then_some(offset)
        };
        let mut words: Vec<(String, u64)> = Vec::new();
        let mut sum = None;
        match (row.operation.as_str(), parts.as_slice()) {
            ("ldr", [destination, base, offset @ ..]) if destination.starts_with('w') => {
                words.extend(member(base, offset.first()).map(|at| (destination.to_string(), at)));
            }
            ("ldp", [first, second, base, offset @ ..]) if first.starts_with('w') => {
                if let Some(at) = member(base, offset.first()) {
                    words.push((first.to_string(), at));
                    words.push((second.to_string(), at + 4));
                }
            }
            ("sxtw" | "mov", [destination, source]) => {
                let source = format!("w{}", &source[1..]);
                if let Some(&at) = loaded.get(&source) {
                    words.push((format!("w{}", &destination[1..]), at));
                }
            }
            ("add", [destination, source, "#1"]) if destination.starts_with('w') => {
                sum = loaded.get(*source).map(|&at| (destination.to_string(), at));
            }
            ("str", [value, base, offset @ ..]) => {
                let at = member(base, offset.first());
                if at.is_some() && incremented.get(*value) == at.as_ref() {
                    counts.extend(at);
                }
            }
            ("stp", [first, second, base, offset @ ..]) => {
                if let Some(at) = member(base, offset.first()) {
                    if incremented.get(*first) == Some(&at) {
                        counts.insert(at);
                    }
                    if incremented.get(*second) == Some(&(at + 4)) {
                        counts.insert(at + 4);
                    }
                }
            }
            _ => {}
        }
        for register in
            crate::engine::analysis::decode::written_registers(&row.operation, &row.operands)
        {
            loaded.remove(&format!("w{register}"));
            incremented.remove(&format!("w{register}"));
        }
        loaded.extend(words);
        incremented.extend(sum);
    }
    Some(counts)
}

/// An immediate offset such as `#0x14`.
fn parse_offset(operand: &str) -> Option<u64> {
    let digits = operand.strip_prefix('#')?;
    match digits.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => digits.parse().ok(),
    }
}

/// For each instruction, the registers that hold the specialization's receiver before it runs:
/// `x0` at entry and its copies, joined across branches. `None` when the flow leaves through a
/// register or exceeds its bound.
fn receiver_states(
    rows: &[crate::engine::analysis::decode::Instruction],
) -> Option<Vec<Option<BTreeSet<String>>>> {
    if rows.is_empty() {
        return None;
    }
    let indexes: BTreeMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.address, index))
        .collect();
    let mut states = vec![None; rows.len()];
    states[0] = Some(BTreeSet::from(["x0".to_owned()]));
    let mut pending = std::collections::VecDeque::from([0]);
    let mut steps = 0;
    while let Some(pc) = pending.pop_front() {
        steps += 1;
        if steps > rows.len() * 32 {
            return None;
        }
        let row = &rows[pc];
        if row.operation == "br" {
            return None;
        }
        let mut receiver = states[pc].clone()?;
        let parts: Vec<_> = row.operands.split(',').collect();
        let copied = row.operation == "mov"
            && parts.len() == 2
            && parts[0].starts_with('x')
            && receiver.contains(parts[1]);
        for register in
            crate::engine::analysis::decode::written_registers(&row.operation, &row.operands)
        {
            receiver.remove(&format!("x{register}"));
        }
        if copied {
            receiver.insert(parts[0].into());
        }
        let mut next = Vec::new();
        if !matches!(row.operation.as_str(), "ret" | "brk" | "b") && pc + 1 < rows.len() {
            next.push(pc + 1);
        }
        if row.operation == "b"
            || row.operation.starts_with("b.")
            || matches!(row.operation.as_str(), "cbz" | "cbnz" | "tbz" | "tbnz")
        {
            let operand = row.operands.rsplit(',').next()?.trim_start_matches('#');
            let target = if let Some(hex) = operand.strip_prefix("0x") {
                u64::from_str_radix(hex, 16).ok()?
            } else {
                operand.parse().ok()?
            };
            next.push(*indexes.get(&target)?);
        }
        for next in next {
            match &mut states[next] {
                Some(previous) => {
                    let joined = previous.intersection(&receiver).cloned().collect();
                    if *previous != joined {
                        *previous = joined;
                        pending.push_back(next);
                    }
                }
                empty @ None => {
                    *empty = Some(receiver.clone());
                    pending.push_back(next);
                }
            }
        }
    }
    Some(states)
}

fn pointer_member_offset(
    row: &crate::engine::analysis::decode::Instruction,
    receivers: &BTreeSet<String>,
) -> Option<u64> {
    if row.operation != "ldr" {
        return None;
    }
    let (destination, memory) = row.operands.split_once(',')?;
    if !destination.starts_with('x') {
        return None;
    }
    let memory = memory.strip_prefix('[')?.strip_suffix(']')?;
    let (base, offset) = memory.split_once(',')?;
    if !receivers.contains(base) {
        return None;
    }
    let digits = offset.strip_prefix('#')?;
    if let Some(hex) = digits.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).ok()
    } else {
        digits.parse().ok()
    }
}

/// Bind the compound reader boundaries used by both field and command dispatch.
pub(super) fn key_readers(
    symbols: &[Symbol],
    recipe: &super::super::targets::PersistentRecipe,
) -> Result<crate::engine::analysis::fields::KeyReaders, AnalysisError> {
    use super::declarations::{addresses, unique};
    let token_copy = addresses(symbols, "CToken::CToken(CToken const&)");
    let target_construct = addresses(
        symbols,
        "CEventTarget::CEventTarget(CToken, EScopeType, CString const&)",
    );
    if token_copy.is_empty() || target_construct.is_empty() {
        return Err(AnalysisError::InvalidRange);
    }
    Ok(crate::engine::analysis::fields::KeyReaders {
        compound_sizes: recipe.compound_sizes,
        array_data: recipe.string_array[0],
        array_count: recipe.string_array[1],
        string_stride: recipe.string_array[2],
        value_token: recipe.value_token as i64,
        token_text: recipe.token_text as i64,
        token_copy: token_copy.into_iter().collect(),
        target_construct: target_construct.into_iter().collect(),
        target_move: Some(unique(symbols, "CEventTarget::operator=(CEventTarget&&)")?),
        string_emplace: Some(unique(
            symbols,
            "void CPdxArray<CString, int>::SetSizeAndEmplace<>(int, const&)",
        )?),
        optional_string: Some(unique(
            symbols,
            "void CPdxOptional<CString>::SetEmplace<char const*>(char const*&&)",
        )?),
        string_read: Some(unique(symbols, "CReader::Read(CString&, bool)")?),
        persistent_read: Some(unique(symbols, "CReader::Read(CPersistent&)")?),
        string_from_text: addresses(symbols, "CString::CString(char const*)")
            .into_iter()
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::analysis_support::{IMAGE_JUMP_TABLE, macho_with_fixups};

    #[test]
    fn the_read_only_sections_hold_the_jump_tables_and_the_string_literals() {
        let sections = read_only_sections(&macho_with_fixups(6)).unwrap();

        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].address, 0x1_0000_2000);
        assert_eq!(sections[0].bytes, b"other\0\0\0entity_offset\0");
        assert_eq!(sections[1].address, 0x1_0000_3000);
        assert_eq!(sections[1].bytes, IMAGE_JUMP_TABLE);
    }
    /// The shape of the pointer insertion specialization: the count at `+0x14` is incremented
    /// on the path that has room and on the path that grows the buffer.
    #[test]
    fn the_count_is_the_word_that_an_insertion_loads_increments_and_stores_back() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;
        let counts = |code: Vec<u8>| array_count_offsets(&decode_arm64(&code, 0x1000).unwrap());
        let insertion = arm64!(at 0x1000;
            mov x19, x0;
            ldp w8, w25, [x0, #0x10];
            sxtw x25, w25;
            cmp w25, w8;
            b.ne extern 0x1020;
            add w26, w25, #1;
            stp w8, w26, [x19, #0x10];
            ret;
            ldr x8, [x19, #8]; // 0x1020: room for one more
            add w10, w25, #1;
            str w10, [x19, #0x14];
            ret
        );
        let other_word = arm64!(at 0x1000;
            ldr w8, [x0, #0x10];
            add w9, w8, #1;
            str w9, [x0, #0x14];
            ret
        );

        assert_eq!(counts(insertion), Some(BTreeSet::from([0x14])));
        assert_eq!(counts(other_word), Some(BTreeSet::new()));
    }

    #[test]
    fn a_moving_insertion_must_clear_its_source_on_every_return() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;
        let clears = |code: Vec<u8>| clears_moved_source(&decode_arm64(&code, 0x1000).unwrap());
        let moved = arm64!(at 0x1000;
            mov x22, x2;
            bl extern 0x2000; // allocation
            ldr x8, [x22];
            str xzr, [x22];
            str x8, [x0];
            ret
        );
        let bypassed = arm64!(at 0x1000;
            mov x22, x2;
            cbz w1, extern 0x1010;
            ldr x8, [x22];
            str xzr, [x22];
            ret
        );
        let other_slot = arm64!(at 0x1000;
            ldr x8, [x2];
            str xzr, [x3];
            ret
        );
        let kept = arm64!(at 0x1000;
            ldr x8, [x2];
            str x8, [x0];
            ret
        );
        let passed_on = arm64!(at 0x1000;
            str xzr, [x2];
            mov x0, x2;
            bl extern 0x2000; // may store through the source
            ret
        );

        assert!(clears(moved));
        for code in [bypassed, other_slot, kept, passed_on] {
            assert!(!clears(code));
        }
    }

    #[test]
    fn an_object_whose_member_is_the_root_does_not_add_the_root_twice() {
        // A second body with the root's name would make the root ambiguous to the dispatch walk.
        let root = Symbol {
            name: "CExample::ReadMember(CReader&, int)".into(),
            address: 0x1000,
        };
        let mut functions = vec![Function {
            name: root.name.clone(),
            address: root.address,
            code: vec![0; 4],
        }];

        let text = Text {
            address: 0x1000,
            code: &[],
            starts: BTreeSet::new(),
        };

        assert!(push_function(&text, &mut functions, &root));
        assert_eq!(functions.len(), 1);
    }

    #[test]
    fn a_statement_read_names_the_callee_saved_register_that_holds_the_reader() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;
        let reader = |code: Vec<u8>| {
            let rows = decode_arm64(&code, 0x1000).unwrap();
            let reads: Vec<usize> = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.operation == "bl")
                .map(|(index, _)| index)
                .collect();
            statement_reader(&rows, &reads)
        };
        let saved = arm64!(at 0x1000;
            mov x0, x23;
            bl extern 0x2000; // CReader::ReadSimpleStatement
            ret
        );
        let argument = arm64!(at 0x1000;
            mov x0, x1;
            bl extern 0x2000;
            ret
        );
        let twice = arm64!(at 0x1000;
            mov x0, x23;
            bl extern 0x2000;
            mov x0, x23;
            bl extern 0x2000;
            ret
        );

        assert_eq!(reader(saved), Some((0x1008, "x23".into())));
        assert_eq!(reader(argument), None, "x1 does not survive the call");
        assert_eq!(reader(twice), None);
    }

    #[test]
    fn a_factory_names_its_class_as_the_first_template_argument() {
        assert_eq!(
            factory_class(
                "CPdxScopedPtrImpl<A<B>, false> PdxMakeScopedPtr<A<B>, int, C const&>(int, C const&)"
            ),
            Some("A<B>")
        );
        assert_eq!(
            factory_class("CPdxScopedPtrImpl<A, false> PdxMakeScopedPtr<A>()"),
            Some("A")
        );
        assert_eq!(factory_class("A::A(int)"), None);
    }

    #[test]
    fn collection_buffer_requires_a_receiver_on_every_incoming_path() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;
        let bypass = arm64!(at 0x1000;
            cbz w2, extern 0x1008;
            mov x19, x0;
            ldr x8, [x19, #8];
            ret
        );
        let proven = arm64!(at 0x1000;
            mov x19, x0;
            cbz w2, extern 0x100c;
            mov x19, x0;
            ldr x8, [x19, #8];
            ret
        );
        assert_eq!(
            array_pointer_offsets(&decode_arm64(&bypass, 0x1000).unwrap()),
            Some(BTreeSet::new())
        );
        assert_eq!(
            array_pointer_offsets(&decode_arm64(&proven, 0x1000).unwrap()),
            Some(BTreeSet::from([8]))
        );
    }
    #[test]
    fn collection_buffer_forgets_a_receiver_copy_that_an_instruction_writes() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;
        let inserted = arm64!(at 0x1000;
            mov x19, x0;
            bfi x19, x8, #0, #8;
            ldr x9, [x19, #8];
            ret
        );
        let pre_index = arm64!(at 0x1000;
            mov x19, x0;
            ldr x9, [x19, #16]!;
            ldr x10, [x19, #8];
            ret
        );
        let post_index = arm64!(at 0x1000;
            mov x19, x0;
            ldr x9, [x19], #16;
            ldr x10, [x19, #8];
            ret
        );
        let kept = arm64!(at 0x1000;
            mov x19, x0;
            ldr x9, [x0, #16];
            ldr x10, [x19, #8];
            ret
        );
        let offsets = |code: Vec<u8>| array_pointer_offsets(&decode_arm64(&code, 0x1000).unwrap());

        assert_eq!(offsets(inserted), Some(BTreeSet::new()));
        assert_eq!(offsets(pre_index), Some(BTreeSet::new()));
        assert_eq!(offsets(post_index), Some(BTreeSet::new()));
        assert_eq!(offsets(kept), Some(BTreeSet::from([8, 16])));
    }
    #[test]
    fn collection_buffer_is_unresolved_when_a_branch_leaves_the_function() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;
        let conditional_exit = arm64!(at 0x1000;
            ldr x8, [x0, #8];
            cbz w2, extern 0x2000; // cold block outside the bounded function
            ret
        );
        let tail_exit = arm64!(at 0x1000;
            ldr x8, [x0, #8];
            b extern 0x2000
        );
        for code in [conditional_exit, tail_exit] {
            assert_eq!(
                array_pointer_offsets(&decode_arm64(&code, 0x1000).unwrap()),
                None
            );
        }
    }
}
