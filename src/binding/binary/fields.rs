use crate::binding::targets::PersistentRecipe;
use crate::engine::analysis::{
    declarations,
    decode::{Instruction, call_or_jump_target, call_target, is_branch},
    discovery::Symbol,
    evaluate::ReadOnlyData,
    fields::{
        self, FieldInput, Function, KeyReaders, LoaderRoot, ObjectReader, PersistentInput,
        BaseRoot, PointReader, SerializerRoot, StackSerializer, has_owner_receiver,
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

/// The persistent reads that run an object's member reader over each statement of a block.
const PERSISTENT_READ: &str = "CPersistent::Read(CReader&)";
const READER_READ: &str = "CReader::Read(CPersistent&)";

/// Largest body of an array insertion specialization that the field reader inspects.
const INSERTION_LIMIT: u64 = 65_536;

/// The parts of a registry's field input that every registry shares, read once per executable.
pub(in crate::binding) struct SharedFieldInput {
    key_readers: KeyReaders,
    persistent: SharedPersistent,
}

impl SharedFieldInput {
    /// Read them from `image` with the recipe of its build and the reader offset of a statement's
    /// token word; a field input then reads the rest from the same image.
    pub fn read(
        image: &Image<'_>,
        persistent_recipe: &PersistentRecipe,
        reader_token_offset: u64,
    ) -> Result<Self, AnalysisError> {
        let text = Text::read(image.bytes, image.symbols)?;
        // The field method reports a token table that it cannot read as its own gap.
        let tokens = text
            .token_table(image.symbols, image.strings)
            .unwrap_or_default();

        Ok(Self {
            key_readers: key_readers(
                image.symbols,
                &tokens,
                persistent_recipe,
                reader_token_offset,
            )?,
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
    /// Whether the loader names one directory. The part points of a loader that names several
    /// are not inferred, since its read object is not scoped to one directory.
    pub one_directory: bool,
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
    let (root, serializer) = root_reader(&text, image, owner, persistent_recipe)?;
    let base_root = match (&serializer, indexed.named(&root).next()) {
        (None, None) => base_root(image, &data, owner, persistent_recipe),
        _ => None,
    };
    let root = base_root
        .as_ref()
        .map_or(root, |base| base.reader.clone());
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
    let stubs = push_inherited_readers(image, &text, &root, &mut functions)?;
    // The nested-object pass takes `x0` as the owner, so a serializer root collects no objects.
    let objects = match serializer {
        Some(_) => Vec::new(),
        None => collect_objects(
            image,
            &text,
            &data,
            Some(&shared.persistent),
            persistent_recipe,
            &root,
            &mut functions,
            &mut gaps,
        )?,
    };
    if !objects.is_empty() {
        collect_owner_methods(&text, symbols, owner, &mut functions, &mut gaps);
    }
    let loader_root = match &loader {
        Some(search) => loader_root(
            &text,
            indexed,
            &data,
            pointers,
            owner,
            search,
            persistent_recipe,
            &mut functions,
            &mut gaps,
        )?,
        None => None,
    };
    let mut persistent = super::receivers::persistent(
        image,
        &text,
        &data,
        &shared.persistent,
        owner,
        persistent_recipe,
    )?;
    if let Some(search) = &loader {
        let points = loader_part_points(&text, indexed, &data, owner, search)?;
        let names = Names::new(symbols);
        let unique_name = |address: u64| names.unique(address).map(str::to_owned);
        for (&offset, &point) in &points {
            if let Some(reader) =
                super::receivers::concrete_reader(persistent_recipe, pointers, point, &unique_name)
            {
                persistent.readers.insert(point, reader);
            }
            persistent.loader_points.insert(offset as i64, point);
        }
    }
    push_embedded_readers(&text, indexed, &persistent, &mut functions);
    push_converters(&text, indexed, &mut functions);
    let stack_serializers = stack_serializers(image, &text, persistent_recipe, &mut functions);
    Ok(FieldInput {
        base_root,
        stack_serializers,
        key_readers: shared.key_readers.clone(),
        persistent: Some(persistent),
        loader_root,
        serializer,
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

/// The address points that the custom loader `search` installs, by owner offset, in each item
/// that it builds inline. The loader's one read of an `owner` object gives the build. Each part
/// that the build passes in `x0` at an offset to a member function of one class `Part`, where
/// `Part::Part()` exists, takes that constructor's stores shifted by the offset, when they agree
/// with the build's own stores ([`inline_build::part_points`]).
fn loader_part_points(
    text: &Text,
    symbols: IndexedSymbols<'_>,
    data: &ReadOnlyData,
    owner: &str,
    search: &LoaderSearch<'_>,
) -> Result<BTreeMap<u64, u64>, AnalysisError> {
    use crate::engine::analysis::discovery::{inline_build, loader_owner};

    let mut points = BTreeMap::new();
    if !search.one_directory {
        return Ok(points);
    }
    let Some(owner_point) = symbols
        .vtable_group(data, owner)
        .and_then(|vtable| vtable.address_points.get(&0).copied())
    else {
        return Ok(points);
    };
    let functions = super::loader_owners::own_functions(text, symbols.symbols, search.database);
    let statement_reads = addresses(symbols, STATEMENT_READER).into_iter().collect();
    let anchors = super::constructors::anchors(symbols.symbols);
    let reads = loader_owner::LoaderReads {
        enumerations: &anchors.file_enumerations,
        statement_reads: &statement_reads,
        token_offset: search.reader_token_offset,
        token_names: &BTreeMap::new(),
    };
    let builders = BTreeMap::from([(owner_point, owner.to_owned())]);
    let Some((start, read)) = loader_owner::read_object(&functions, &reads, &builders) else {
        return Ok(points);
    };
    let build = inline_build::inline_build(
        &functions[&start],
        read.store,
        read.register,
        Some(read.call),
    );
    let is_address_point =
        |point: u64| super::families::point_class(symbols.symbols, data, point).is_some();
    // Offsets that two parts give different points have no point.
    let mut conflicts = BTreeSet::new();

    for (&offset, targets) in build.member_calls.range(1..) {
        let Some(class) = method_class(symbols, targets) else {
            continue;
        };
        let Some(rows) = default_constructor(text, symbols, class) else {
            continue;
        };
        let part = inline_build::inline_build(&rows, 0, 0, None);
        let Some(part_points) = inline_build::part_points(&build, offset, &part, is_address_point)
        else {
            continue;
        };

        for (at, point) in part_points {
            if *points.entry(at).or_insert(point) != point {
                conflicts.insert(at);
            }
        }
    }

    points.retain(|at, _| !conflicts.contains(at));

    Ok(points)
}

/// The one class whose methods every call target in `targets` is. `None` when a target has no
/// symbol or a symbol that names no class, or when the targets are methods of several classes.
fn method_class<'a>(symbols: IndexedSymbols<'a>, targets: &BTreeSet<u64>) -> Option<&'a str> {
    let mut classes = BTreeSet::new();

    for &target in targets {
        let names: Vec<_> = symbols.at(&BTreeSet::from([target])).collect();

        if names.is_empty() {
            return None;
        }

        for symbol in names {
            classes.insert(super::receivers::method_class(&symbol.name)?);
        }
    }

    let [class] = classes.into_iter().collect::<Vec<_>>()[..] else {
        return None;
    };

    Some(class)
}

/// The decoded body of `class`'s one default constructor `Class::Class()`, after the symbols of
/// that name that only branch to another of them. `None` when there is no such body, several, or
/// one that does not decode.
fn default_constructor(
    text: &Text,
    symbols: IndexedSymbols<'_>,
    class: &str,
) -> Option<Vec<Instruction>> {
    let name = format!("{class}::{}()", super::receivers::unqualified_base(class));
    let entries: BTreeSet<u64> = symbols.named(&name).map(|symbol| symbol.address).collect();
    let mut bodies = Vec::new();

    for &entry in &entries {
        let (address, code) = text.function(entry).ok()?;
        let rows = crate::engine::analysis::decode::decode_arm64(code, address).ok()?;
        let alias = declarations::branch_alias(&rows).is_some_and(|to| entries.contains(&to));

        if !alias {
            bodies.push(rows);
        }
    }

    let [rows] = <[_; 1]>::try_from(bodies).ok()?;

    Some(rows)
}

/// The member reader of `owner`'s root keys: its own `ReadMember(CReader&, int)`, or, for an owner
/// with none, the member reader in the vtable of the stack serializer that its `Read(CReader&)`
/// reads.
fn root_reader(
    text: &Text,
    image: &Image<'_>,
    owner: &str,
    recipe: &PersistentRecipe,
) -> Result<(String, Option<SerializerRoot>), AnalysisError> {
    let own = format!("{owner}::ReadMember(CReader&, int)");

    if image.index.over(image.symbols).named(&own).next().is_some() {
        return Ok((own, None));
    }

    let read = text.unique_body(image.symbols, &format!("{owner}::Read(CReader&)"))?;
    let unique = |name| super::declarations::unique(image.symbols, name).ok();
    let (Some(persistent), Some(reader)) = (unique(PERSISTENT_READ), unique(READER_READ)) else {
        return Ok((own, None));
    };
    let reads = fields::PersistentReads { persistent, reader };
    let names = Names::new(image.symbols);
    let unique_name = |address: u64| names.unique(address).map(str::to_owned);
    let serializer = read
        .and_then(|rows| fields::serializer_read(&rows, &reads))
        .and_then(|(point, owner_slot)| {
            let reader =
                super::receivers::concrete_reader(recipe, image.pointers, point, &unique_name)?;

            Some(SerializerRoot {
                reader: reader.member,
                owner_slot,
            })
        });

    Ok(match serializer {
        Some(serializer) => (serializer.reader.clone(), Some(serializer)),
        None => (own, None),
    })
}

/// The base class's member reader that reads the keys of `owner`, a class with no member reader
/// of its own: the one address point of the owner's vtable whose member slot holds a
/// `ReadMember(CReader&, int)` of another class (not a thunk), with the point's subobject offset.
/// `None` when no point or several hold one.
fn base_root(
    image: &Image<'_>,
    data: &ReadOnlyData,
    owner: &str,
    recipe: &PersistentRecipe,
) -> Option<BaseRoot> {
    let names = Names::new(image.symbols);
    let group = image.index.over(image.symbols).vtable_group(data, owner)?;
    let members: Vec<(u64, &str)> = group
        .address_points
        .iter()
        .filter_map(|(&offset, &point)| {
            let target = image.pointers.get(&point.checked_add(recipe.member_slot)?)?;
            let name = names.unique(*target)?;
            (name.ends_with("::ReadMember(CReader&, int)")
                && !name.starts_with('{')
                && name != "CPersistent::ReadMember(CReader&, int)"
                && !name.starts_with(&format!("{owner}::")))
            .then_some((offset, name))
        })
        .collect();
    let [(offset, reader)] = members[..] else {
        return None;
    };

    Some(BaseRoot {
        reader: reader.to_owned(),
        offset: i64::try_from(offset).ok()?,
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

/// The member readers and value readers that `root` and the readers it reaches call directly,
/// which dispatch follows as inherited readers, and the import stubs among those calls with the body that each
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
    let mut scanned = BTreeSet::new();

    while let Some(caller) = callers.pop() {
        if !scanned.insert(caller.clone()) {
            continue;
        }
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
            .filter(|symbol| {
                readers::is_inheritable_member(&symbol.name)
                    || readers::is_member_value_reader(&symbol.name)
            })
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
            // Another collection step may have added the body already, under another name. Dispatch
            // finds a body by name, so the inherited reader keeps its own copy.
            let alias = functions
                .iter()
                .find(|function| function.address == body && function.name != call.name)
                .map(|function| Function {
                    name: call.name.clone(),
                    ..function.clone()
                });
            let named = functions
                .iter()
                .any(|function| function.address == body && function.name == call.name);
            let collected = match alias {
                _ if named => true,
                Some(alias) => {
                    functions.push(alias);
                    true
                }
                None => push_function(text, functions, definition),
            };

            if collected {
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
    let Some(root) = functions
        .iter()
        .find(|function| function.name == root)
        .cloned()
    else {
        return Ok(Vec::new());
    };
    let rows = crate::engine::analysis::decode::decode_arm64(&root.code, root.address)
        .map_err(|_| AnalysisError::InvalidRange)?;
    // Dispatch goes on in the inherited readers, so the objects that they read are the root's.
    let mut calls = direct_calls(&rows);
    for inherited in functions.iter().filter(|function| {
        readers::is_inheritable_member(&function.name)
            || readers::is_member_value_reader(&function.name)
    }) {
        let inherited_rows =
            crate::engine::analysis::decode::decode_arm64(&inherited.code, inherited.address)
                .map_err(|_| AnalysisError::InvalidRange)?;
        calls.extend(direct_calls(&inherited_rows));
    }
    let mut classes = BTreeMap::<String, Vec<&Symbol>>::new();

    for symbol in symbols.at(&calls) {
        if let Some(class) = super::receivers::constructor_class(&symbol.name) {
            classes.entry(class.to_owned()).or_default();
        } else if let Some(class) = factory_class(&symbol.name) {
            classes.entry(class.to_owned()).or_default().push(symbol);
        } else if let Some(class) = inserted_class(&symbol.name) {
            classes.entry(class.to_owned()).or_default();
        } else if let Some(class) = emplaced_class(&symbol.name) {
            classes.entry(class.to_owned()).or_default();
        }
    }

    if classes.is_empty() {
        return Ok(Vec::new());
    }

    let initializers: Vec<_> = symbols
        .at(&calls)
        .filter(|symbol| is_scalar_constructor(&symbol.name))
        .map(|symbol| symbol.address)
        .collect();

    let names = Names::new(symbols.symbols);
    let unique_name = |address: u64| names.unique(address).map(str::to_owned);
    let operator_new: BTreeSet<_> = addresses(symbols, "operator new(unsigned long)")
        .into_iter()
        .collect();
    let array_new: BTreeSet<_> = addresses(symbols, "operator new[](unsigned long)")
        .into_iter()
        .collect();
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
        let insert: Vec<_> = pointer_insertions(&class)
            .iter()
            .flat_map(|name| addresses(symbols, name))
            .collect();
        let moving_insert: BTreeMap<_, _> = [scoped_insertion(&class), shared_insertion(&class)]
            .iter()
            .flat_map(|name| addresses(symbols, name))
            .filter_map(|start| Some((start, moved_source_width_at(text, start)?)))
            .collect();
        let shared_emplace_point = symbols
            .vtable_group(data, &shared_emplace(&class))
            .and_then(|group| group.address_points.get(&0).copied());
        let release_prefix = format!("std::__1::shared_ptr<{class}>::~shared_ptr");
        let mut shared_releases = Vec::new();
        for symbol in symbols.at(&calls) {
            if symbol.name.starts_with(&release_prefix)
                && symbol.name.ends_with("()")
                && push_function(text, functions, symbol)
            {
                shared_releases.push(symbol.address);
            }
        }
        let emplace = addresses(symbols, &emplacement(&class));
        // A class with an insertion or a moving insertion keeps the insertion's layout: its
        // pointer array is another collection than the array of objects that the resizing grows.
        let emplace_layout = match emplace.as_slice() {
            [start] if insert.is_empty() && moving_insert.is_empty() => {
                emplace_layout(text, *start, &array_new)
            }
            _ => None,
        };
        let (data_offset, count_offset) = match emplace_layout {
            Some(layout) => (Some(layout.data), Some(layout.count)),
            None => (
                array_member_offset(text, &insert, array_pointer_offsets)?,
                array_member_offset(text, &insert, array_count_offsets)?,
            ),
        };
        let owner_of_keys = reader
            .as_ref()
            .is_none_or(|bound| bound.reader.family == BlockFamily::Unknown);
        let constructors: Vec<_> = symbols
            .constructors_of(&class)
            .iter()
            .map(|symbol| symbol.address)
            .collect();
        let inline = if constructors.is_empty() {
            let insertions: Vec<_> = insert.iter().chain(moving_insert.keys()).copied().collect();
            inline_constructions(&root, &rows, &operator_new, &insertions)
        } else {
            Vec::new()
        };
        let (persistent, nested_objects) = match nested.filter(|_| owner_of_keys) {
            Some(shared) => nested_owner(
                image,
                text,
                data,
                shared,
                recipe,
                &class,
                inline,
                &member_name,
                functions,
                gaps,
            )?,
            None => (None, Vec::new()),
        };
        objects.push(ObjectReader {
            data_offset,
            count_offset,
            constructors,
            factories: factories.iter().map(|symbol| symbol.address).collect(),
            initializers: initializers.clone(),
            insert,
            moving_insert,
            shared_emplace_point,
            shared_releases,
            array_emplace: emplace_layout.map_or_else(Vec::new, |_| emplace),
            array_element_size: emplace_layout.map(|layout| layout.element_size),
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
    inline: Vec<declarations::Function>,
    member: &str,
    functions: &mut Vec<Function>,
    gaps: &mut Vec<String>,
) -> Result<(Option<PersistentInput>, Vec<ObjectReader>), AnalysisError> {
    let persistent = if inline.is_empty() {
        super::receivers::persistent(image, text, data, shared, class, recipe)
    } else {
        super::receivers::persistent_from(image, text, data, shared, recipe, inline)
    };
    let persistent = match persistent {
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

/// Parameter types that carry no address: the arithmetic types and the engine's scope enum.
const SCALAR_PARAMETERS: [&str; 15] = [
    "bool",
    "char",
    "signed char",
    "unsigned char",
    "short",
    "unsigned short",
    "int",
    "unsigned int",
    "long",
    "unsigned long",
    "long long",
    "unsigned long long",
    "float",
    "double",
    "EScopeType",
];

/// Whether `name` is a constructor whose only address argument is the object it builds: every
/// parameter is one of [`SCALAR_PARAMETERS`].
fn is_scalar_constructor(name: &str) -> bool {
    let Some(class) = super::receivers::constructor_class(name) else {
        return false;
    };
    let method = &name[class.len() + 2..];
    let Some(parameters) = method
        .split_once('(')
        .and_then(|(_, rest)| rest.strip_suffix(')'))
    else {
        return false;
    };

    parameters.is_empty()
        || parameters
            .split(", ")
            .all(|parameter| SCALAR_PARAMETERS.contains(&parameter))
}

/// The constructor bodies of an object that `root` constructs inline: the construction before
/// each call in `rows` to one of its `insertions`, where there is one. The method uses them only
/// when every joined path allocates the object at the start of one of them.
fn inline_constructions(
    root: &Function,
    rows: &[Instruction],
    operator_new: &BTreeSet<u64>,
    insertions: &[u64],
) -> Vec<declarations::Function> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| calls_one_of(row, insertions))
        .filter_map(|(at, _)| inline_construction_before(root, &rows[..at], operator_new))
        .collect()
}

/// The inline construction that ends just before an insertion, as a constructor body: the code
/// from the return of the last `operator new` call to the object's virtual read, the last indirect
/// call, with a return in place of the read. Nothing between the allocation and the read may branch
/// or call indirectly, and no call or branch may follow the read.
fn inline_construction_before(
    root: &Function,
    rows: &[Instruction],
    operator_new: &BTreeSet<u64>,
) -> Option<declarations::Function> {
    const RET: [u8; 4] = 0xd65f_03c0_u32.to_le_bytes();
    let transfers =
        |row: &Instruction| is_branch(&row.operation) || row.operation.starts_with("bl");
    let read = rows.iter().rposition(|row| row.operation == "blr")?;
    if rows[read + 1..].iter().any(transfers) {
        return None;
    }
    let allocation = rows[..read]
        .iter()
        .rposition(|row| calls_one_of(row, operator_new))?;
    let construction = &rows[allocation + 1..read];
    if construction
        .iter()
        .any(|row| is_branch(&row.operation) || row.operation == "blr")
    {
        return None;
    }
    let start = rows[allocation + 1].address;
    let offset = |address: u64| (address - root.address) as usize;
    let mut code = root.code[offset(start)..offset(rows[read].address)].to_vec();

    code.extend(RET);
    Some(declarations::Function {
        address: start,
        code,
    })
}

/// Whether `row` is a direct call to one of `targets`.
fn calls_one_of<'a>(row: &Instruction, targets: impl IntoIterator<Item = &'a u64>) -> bool {
    row.operation == "bl"
        && call_target(u32::from_le_bytes(row.bytes), row.address)
            .is_some_and(|target| targets.into_iter().any(|&known| known == target))
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

/// The class whose pointer an insertion into a `CPdxArray` of `Class*` or of
/// `CPdxScopedPtrImpl<Class, false>` takes. A `Class const*` insertion names no class: such an
/// array also holds items that the root looks up, such as the `CTechnology const*` prerequisites
/// of `CBuildingType::ReadMember` on M452.
fn inserted_class(name: &str) -> Option<&str> {
    let class = match name.strip_prefix("CPdxScopedPtrImpl<") {
        Some(scoped) => scoped.split_once(", false>& CPdxArray<")?.0,
        None => name.split_once("*& CPdxArray<")?.0,
    };
    let [pointer, _] = pointer_insertions(class);
    let names_class = name == pointer || name == scoped_insertion(class);

    (names_class && !class.ends_with(" const")).then_some(class)
}

/// The class whose objects `void CPdxArray<Class, int>::SetSizeAndEmplace<>(int, const&)`
/// constructs in place at the array's end. A class with template arguments names none.
fn emplaced_class(name: &str) -> Option<&str> {
    let class = name
        .strip_prefix("void CPdxArray<")?
        .strip_suffix(", int>::SetSizeAndEmplace<>(int, const&)")?;

    (!class.contains(['<', '>', ',', '*'])).then_some(class)
}

/// The resizing that constructs objects of `class` in place at a `CPdxArray`'s end.
fn emplacement(class: &str) -> String {
    format!("void CPdxArray<{class}, int>::SetSizeAndEmplace<>(int, const&)")
}

/// The array layout that an in-place resizing proves: its element pointer, its count and the
/// size of one element.
#[derive(Clone, Copy)]
struct EmplaceLayout {
    data: u64,
    count: u64,
    element_size: u64,
}

/// The layout of the resizing at `start`. The count is the receiver word that it increments.
/// The element size is the constant that `operator new[]`'s size multiplies (`mov wS,#size;
/// umull x0,wN,wS`), and the element pointer is the receiver member that keeps the new buffer.
/// Every `operator new[]` call must agree.
fn emplace_layout(text: &Text, start: u64, array_new: &BTreeSet<u64>) -> Option<EmplaceLayout> {
    let code = text.function_within(start, INSERTION_LIMIT)?;
    let rows = crate::engine::analysis::decode::decode_arm64(code, start).ok()?;

    resizing_layout(&rows, array_new)
}

/// The layout that the decoded resizing `rows` proves, as [`emplace_layout`] reads it.
fn resizing_layout(
    rows: &[crate::engine::analysis::decode::Instruction],
    array_new: &BTreeSet<u64>,
) -> Option<EmplaceLayout> {
    let receivers = receiver_states(rows)?;
    let flow = Flow::of(rows);
    let count = incremented_count(rows, &flow, &receivers)?;
    let mut layouts = BTreeSet::new();

    for (index, row) in rows.iter().enumerate() {
        let calls_array_new = row.operation == "bl"
            && branch_target(&row.operands).is_some_and(|target| array_new.contains(&target));

        if !calls_array_new {
            continue;
        }

        let element_size = allocation_multiplier(rows, &flow, index)?;
        let data = stored_allocation(rows, &flow, &receivers, index)?;

        layouts.insert((data, element_size));
    }

    let [(data, element_size)] = layouts.into_iter().collect::<Vec<_>>()[..] else {
        return None;
    };

    Some(EmplaceLayout {
        data,
        count,
        element_size,
    })
}

/// The control flow of a decoded function: the position of each instruction address, and the
/// addresses that a branch names.
struct Flow {
    positions: BTreeMap<u64, usize>,
    branch_targets: BTreeSet<u64>,
}

impl Flow {
    fn of(rows: &[crate::engine::analysis::decode::Instruction]) -> Self {
        Self {
            positions: rows
                .iter()
                .enumerate()
                .map(|(position, row)| (row.address, position))
                .collect(),
            branch_targets: rows
                .iter()
                .filter(|row| branches(row))
                .filter_map(|row| branch_target(&row.operands))
                .collect(),
        }
    }

    /// The positions that the instruction at `position` passes control to, or `None` when it
    /// leaves the function or branches to an address outside it.
    fn successors(
        &self,
        rows: &[crate::engine::analysis::decode::Instruction],
        position: usize,
    ) -> Option<Vec<usize>> {
        let row = &rows[position];
        let target =
            || branch_target(&row.operands).and_then(|target| self.positions.get(&target).copied());

        match row.operation.as_str() {
            "ret" | "br" => None,
            "b" => Some(vec![target()?]),
            _ if branches(row) => Some(vec![target()?, position + 1]),
            _ => Some(vec![position + 1]),
        }
    }
}

/// Whether `row` is a branch that may not go on to the next instruction.
fn branches(row: &crate::engine::analysis::decode::Instruction) -> bool {
    let operation = row.operation.as_str();

    operation.starts_with("b.")
        || matches!(
            operation,
            "b" | "br" | "ret" | "cbz" | "cbnz" | "tbz" | "tbnz"
        )
}

/// The one receiver word that `rows` increments: `ldr wA,[R,#c]`, `add wB,wA,#1` and
/// `str wB,[R',#c]` through receiver registers, with nothing between them that branches, is a
/// branch target or writes `wA` or `wB`.
fn incremented_count(
    rows: &[crate::engine::analysis::decode::Instruction],
    flow: &Flow,
    receivers: &[Option<BTreeSet<String>>],
) -> Option<u64> {
    let member = |position: usize, base: &str, offset: &str| {
        let base = base.strip_prefix('[')?;
        receivers[position]
            .as_ref()?
            .contains(base)
            .then(|| parse_offset(offset.trim_end_matches(']')))?
    };
    let mut counts = BTreeSet::new();

    for (store, row) in rows.iter().enumerate() {
        let operands: Vec<_> = row.operands.split(',').collect();
        let ("str", [sum, base, offset]) = (row.operation.as_str(), &operands[..]) else {
            continue;
        };
        let Some(count) = member(store, base, offset).filter(|_| sum.starts_with('w')) else {
            continue;
        };
        let straight = |from: usize| {
            (from + 1..=store).all(|position| {
                !flow.branch_targets.contains(&rows[position].address)
                    && !branches(&rows[position - 1])
            })
        };
        let writes = |position: usize, register: &str| {
            register_number(register).is_some_and(|number| {
                crate::engine::analysis::decode::written_registers(
                    &rows[position].operation,
                    &rows[position].operands,
                )
                .contains(&number)
            })
        };
        let Some(add) = (0..store).rev().find(|&position| writes(position, sum)) else {
            continue;
        };
        let added: Vec<_> = rows[add].operands.split(',').collect();
        let ("add", [_, loaded, "#1"]) = (rows[add].operation.as_str(), &added[..]) else {
            continue;
        };
        let Some(load) = (0..add).rev().find(|&position| writes(position, loaded)) else {
            continue;
        };
        let parts: Vec<_> = rows[load].operands.split(',').collect();
        let loads_count = rows[load].operation == "ldr"
            && matches!(parts[..], [_, base, offset] if member(load, base, offset) == Some(count));

        if loads_count && straight(load) {
            counts.insert(count);
        }
    }

    let [count] = counts.into_iter().collect::<Vec<_>>()[..] else {
        return None;
    };

    Some(count)
}

/// The constant that the size argument of the call at `call` multiplies: the instruction before
/// the call is `umull x0,wN,wS` or `mul x0,xN,xS`, and the write of `S` before it is a `mov` of an
/// immediate. Nothing from that `mov` to the call may branch or be a branch target, so every path
/// to the call runs the `mov`.
fn allocation_multiplier(
    rows: &[crate::engine::analysis::decode::Instruction],
    flow: &Flow,
    call: usize,
) -> Option<u64> {
    let multiply = rows.get(call.checked_sub(1)?)?;
    let ["x0", _, factor] = multiply.operands.split(',').collect::<Vec<_>>()[..] else {
        return None;
    };

    if !matches!(multiply.operation.as_str(), "umull" | "mul") {
        return None;
    }

    let factor = register_number(factor)?;
    let mut position = call;
    let definition = loop {
        if flow.branch_targets.contains(&rows[position].address) {
            return None;
        }

        position = position.checked_sub(1)?;
        let row = &rows[position];

        if branches(row) {
            return None;
        }

        if crate::engine::analysis::decode::written_registers(&row.operation, &row.operands)
            .contains(&factor)
            && position + 1 < call
        {
            break row;
        }
    };
    let [_, immediate] = definition.operands.split(',').collect::<Vec<_>>()[..] else {
        return None;
    };

    (definition.operation == "mov").then(|| parse_offset(immediate))?
}

/// The receiver member that keeps the result of the call at `call`. The instruction after the
/// call copies `x0` to a callee-saved register. On every path from there, that register is
/// stored through a receiver register before any instruction writes it or the function returns,
/// and every such store names one member.
fn stored_allocation(
    rows: &[crate::engine::analysis::decode::Instruction],
    flow: &Flow,
    receivers: &[Option<BTreeSet<String>>],
    call: usize,
) -> Option<u64> {
    let copy = rows.get(call + 1)?;
    let ["x0"] = copy.operands.split(',').skip(1).collect::<Vec<_>>()[..] else {
        return None;
    };
    let kept = copy.operands.split(',').next()?;
    let kept_number = register_number(kept).filter(|number| (19..=28).contains(number))?;

    if copy.operation != "mov" {
        return None;
    }

    let mut members = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut pending = vec![call + 2];

    while let Some(position) = pending.pop() {
        if !visited.insert(position) {
            continue;
        }

        let row = rows.get(position)?;
        let parts: Vec<_> = row.operands.split(',').collect();

        if row.operation == "str"
            && let [value, base, offset] = parts[..]
            && value == kept
            && receivers[position]
                .as_ref()
                .is_some_and(|receiver| receiver.contains(base.trim_start_matches('[')))
        {
            members.insert(parse_offset(offset.trim_end_matches(']'))?);
            continue;
        }

        if crate::engine::analysis::decode::written_registers(&row.operation, &row.operands)
            .contains(&kept_number)
        {
            return None;
        }

        pending.extend(flow.successors(rows, position)?);
    }

    let [member] = members.into_iter().collect::<Vec<_>>()[..] else {
        return None;
    };

    Some(member)
}

/// The number of a general register operand such as `w8` or `x21`.
fn register_number(operand: &str) -> Option<usize> {
    operand
        .strip_prefix('w')
        .or_else(|| operand.strip_prefix('x'))?
        .parse()
        .ok()
}

/// The targets of the direct calls in `rows`.
fn direct_calls(rows: &[crate::engine::analysis::decode::Instruction]) -> BTreeSet<u64> {
    rows.iter()
        .filter(|row| row.operation == "bl")
        .filter_map(|row| u64::from_str_radix(row.operands.trim_start_matches("#0x"), 16).ok())
        .collect()
}

/// The insertions of a `Class*` and of a `Class const*` into a `CPdxArray`.
fn pointer_insertions(class: &str) -> [String; 2] {
    [class.to_owned(), format!("{class} const")].map(|pointee| {
        format!(
            "{pointee}*& CPdxArray<{pointee}*, int>::InsertAtEmplace<{pointee}* const&>(int, {pointee}* const&)"
        )
    })
}

/// The insertion that moves a `CPdxScopedPtrImpl<Class, false>` into a `CPdxArray`.
fn scoped_insertion(class: &str) -> String {
    let scoped = format!("CPdxScopedPtrImpl<{class}, false>");

    format!("{scoped}& CPdxArray<{scoped}, int>::InsertAtEmplace<{scoped} >(int, {scoped}&&)")
}

/// The insertion that moves a `std::__1::shared_ptr<Class>` into a `CPdxArray`.
fn shared_insertion(class: &str) -> String {
    let shared = format!("std::__1::shared_ptr<{class}>");

    format!("{shared}& CPdxArray<{shared}, int>::InsertAtEmplace<{shared} >(int, {shared}&&)")
}

/// The control block that `std::make_shared<Class>` allocates, with the object inside it.
fn shared_emplace(class: &str) -> String {
    format!("std::__1::__shared_ptr_emplace<{class}, std::__1::allocator<{class}> >")
}

fn addresses(symbols: IndexedSymbols<'_>, name: &str) -> Vec<u64> {
    symbols.named(name).map(|symbol| symbol.address).collect()
}

/// Add a symbol's bounded body once; whether the body is available.
/// How many readers deep the stack serializers of serializers are collected.
const SERIALIZER_DEPTH: usize = 4;

/// The stack serializers whose constructors the input's functions call directly, by constructor
/// entry. A constructor counts when its body only installs the vtable at `+0`, stores the wrapped
/// member's address from `x1` at `+8` and stores further arguments beyond it
/// ([`serializer_constructor_point`]); its member reader is the vtable's member slot. Each member
/// reader joins the functions, and the serializers that it builds are collected in turn.
fn stack_serializers(
    image: &Image<'_>,
    text: &Text,
    recipe: &PersistentRecipe,
    functions: &mut Vec<Function>,
) -> BTreeMap<u64, StackSerializer> {
    let symbols = image.index.over(image.symbols);
    let names = Names::new(image.symbols);
    let mut serializers = BTreeMap::new();
    let mut callers: Vec<u64> = functions.iter().map(|function| function.address).collect();

    for _ in 0..SERIALIZER_DEPTH {
        let targets: BTreeSet<u64> = functions
            .iter()
            .filter(|function| callers.contains(&function.address))
            .flat_map(|function| {
                function
                    .code
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .enumerate()
                    .filter_map(|(index, word)| {
                        call_target(
                            u32::from_le_bytes(*word),
                            function.address + 4 * index as u64,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let constructors: Vec<Symbol> = symbols
            .at(&targets)
            .filter(|symbol| symbol.name.contains("::CSerializer::CSerializer("))
            .filter(|symbol| !serializers.contains_key(&symbol.address))
            .cloned()
            .collect();
        callers.clear();
        for constructor in constructors {
            let Ok((address, code)) = text.function(constructor.address) else {
                continue;
            };
            let Some(point) = crate::engine::analysis::decode::decode_arm64(code, address)
                .ok()
                .and_then(|rows| serializer_constructor_point(&rows))
            else {
                continue;
            };
            let Some(reader) = image
                .pointers
                .get(&(point + recipe.member_slot))
                .and_then(|&target| names.unique(target))
                .filter(|name| name.ends_with("::ReadMember(CReader&, int)"))
            else {
                continue;
            };
            let Some(member) = symbols.named(reader).next().cloned() else {
                continue;
            };
            push_function(text, functions, &member);
            callers.push(member.address);
            serializers.insert(
                address,
                StackSerializer {
                    owner_slot: SERIALIZER_OWNER_SLOT,
                    reader: reader.to_owned(),
                },
            );
        }
    }

    serializers
}

/// Where a serializer's constructor stores the wrapped member's address.
const SERIALIZER_OWNER_SLOT: i64 = 8;

/// The vtable address point that a serializer constructor installs, when its body is
/// `adrp x, page; add x, x, #off; stp x, x1, [x0]`, then only stores of other arguments at
/// `[x0, #16]` or beyond, then `ret`.
fn serializer_constructor_point(
    rows: &[crate::engine::analysis::decode::Instruction],
) -> Option<u64> {
    let [adrp, add, pair, rest @ .., ret] = rows else {
        return None;
    };
    let parts = |row: &crate::engine::analysis::decode::Instruction| {
        row.operands
            .split(',')
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let number = |text: &str| crate::engine::analysis::decode::number(text.trim_start_matches('#'));
    let (adrp_parts, add_parts, pair_parts) = (parts(adrp), parts(add), parts(pair));
    let [vtable, page] = adrp_parts.as_slice() else {
        return None;
    };
    let [added, base, offset] = add_parts.as_slice() else {
        return None;
    };
    let shape = adrp.operation == "adrp"
        && add.operation == "add"
        && added == vtable
        && base == vtable
        && pair.operation == "stp"
        && pair_parts == [vtable.clone(), "x1".to_owned(), "[x0]".to_owned()]
        && ret.operation == "ret";
    let beyond_owner = rest.iter().all(|row| {
        let parts = parts(row);
        matches!(row.operation.as_str(), "str" | "stur")
            && matches!(parts.as_slice(), [source, base, offset]
                if source.starts_with(['x', 'w', 'd', 's'])
                    && source.as_str() != "x0"
                    && base == "[x0"
                    && offset
                        .strip_suffix(']')
                        .and_then(&number)
                        .is_some_and(|offset| offset >= 16))
    });

    (shape && beyond_owner)
        .then(|| number(page)?.checked_add(number(offset)?))
        .flatten()
}

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

/// The bytes that the moving insertion at `start` clears at its source on every return.
fn moved_source_width_at(text: &Text, start: u64) -> Option<u64> {
    let code = text.function_within(start, INSERTION_LIMIT)?;
    let rows = crate::engine::analysis::decode::decode_arm64(code, start).ok()?;

    moved_source_width(&rows)
}

/// The bytes from the source that `x2` names on entry that every return leaves holding zero, as
/// the moving insertion of a scoped pointer (8) or a shared pointer (16) must. The source is
/// cleared by `str xzr` or `stp xzr, xzr` with no offset through a register that holds the entry
/// `x2` on every path. Any other store through a register that may hold an address computed from
/// it, or a call that may receive one, makes the source unknown again. Stores through other
/// registers are taken not to alias the caller's source, because the insertion writes only its
/// array and frame.
fn moved_source_width(rows: &[crate::engine::analysis::decode::Instruction]) -> Option<u64> {
    use crate::engine::analysis::decode::{general_register, reads_register, written_registers};

    #[derive(Clone, PartialEq)]
    struct Source {
        /// Registers that hold the entry `x2` on every path.
        holders: BTreeSet<usize>,
        /// Registers that may hold an address computed from the entry `x2` on some path.
        derived: BTreeSet<usize>,
        cleared: u64,
    }

    if rows.is_empty() {
        return None;
    }

    let indexes: BTreeMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.address, index))
        .collect();
    let mut states: Vec<Option<Source>> = vec![None; rows.len()];
    states[0] = Some(Source {
        holders: BTreeSet::from([2]),
        derived: BTreeSet::from([2]),
        cleared: 0,
    });
    let mut pending = std::collections::VecDeque::from([0]);
    let mut returned: Option<u64> = None;
    let mut steps = 0;

    while let Some(pc) = pending.pop_front() {
        steps += 1;
        if steps > rows.len() * 32 {
            return None;
        }

        let row = &rows[pc];
        let operation = row.operation.as_str();
        let mut source = states[pc].clone()?;
        let parts: Vec<_> = row.operands.split(',').collect();

        if operation.starts_with("ret") {
            if source.cleared == 0 {
                return None;
            }
            returned = Some(returned.map_or(source.cleared, |width| width.min(source.cleared)));
            continue;
        }

        if operation.starts_with("br") && operation != "brk" {
            return None;
        }

        let base = memory_base(&row.operands).and_then(general_register);
        let accesses_memory = base.is_some();

        if operation.starts_with("st") && base.is_some_and(|base| source.derived.contains(&base)) {
            let through_holder = base.is_some_and(|base| source.holders.contains(&base));
            let unindexed = parts.last().is_some_and(|last| last.ends_with(']'))
                && !parts.last().is_some_and(|last| last.contains('#'));

            source.cleared = match (operation, parts.as_slice(), through_holder && unindexed) {
                ("str", ["xzr", _], true) => 8,
                ("stp", ["xzr", "xzr", _], true) => 16,
                _ => 0,
            };
        }

        if (operation == "bl" || operation.starts_with("blr"))
            && (0..8).any(|index| source.derived.contains(&index))
        {
            source.cleared = 0;
        }

        // A `mov` between `x` registers copies the source address to its destination.
        let copied_to = match parts.as_slice() {
            [to, from] if operation == "mov" && to.starts_with('x') && from.starts_with('x') => {
                general_register(from)
                    .filter(|from| source.holders.contains(from))
                    .and(general_register(to))
            }
            _ => None,
        };
        let written = written_registers(operation, &row.operands);
        // A load or store derives only its written-back base; any other instruction derives
        // every register that it writes from a derived register that it reads.
        let derives: Vec<usize> = if accesses_memory {
            base.filter(|base| source.derived.contains(base))
                .into_iter()
                .collect()
        } else if source
            .derived
            .iter()
            .any(|&register| reads_register(operation, &row.operands, register))
        {
            written.clone()
        } else {
            Vec::new()
        };

        for register in &written {
            source.holders.remove(register);
            source.derived.remove(register);
        }

        for register in derives
            .into_iter()
            .filter(|register| written.contains(register))
        {
            source.derived.insert(register);
        }

        if let Some(to) = copied_to {
            source.holders.insert(to);
            source.derived.insert(to);
        }

        let mut next = Vec::new();

        if !matches!(operation, "b" | "brk") && pc + 1 < rows.len() {
            next.push(pc + 1);
        }

        if operation == "b"
            || operation.starts_with("b.")
            || matches!(operation, "cbz" | "cbnz" | "tbz" | "tbnz")
        {
            next.push(*indexes.get(&branch_target(&row.operands)?)?);
        }

        for next in next {
            let joined = match &states[next] {
                Some(previous) => Source {
                    holders: previous
                        .holders
                        .intersection(&source.holders)
                        .copied()
                        .collect(),
                    derived: previous.derived.union(&source.derived).copied().collect(),
                    cleared: previous.cleared.min(source.cleared),
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

/// The identities of the recipe's Boolean tokens, true then false.
pub(super) fn boolean_tokens(
    tokens: &BTreeMap<i64, fields::Token>,
    recipe: &PersistentRecipe,
) -> Result<[i64; 2], AnalysisError> {
    let [true_token, false_token] = recipe.boolean_tokens.map(|name| {
        named_token(tokens, name).ok_or_else(|| {
            AnalysisError::from(crate::engine::analysis::InputError(format!(
                "Boolean token {name} is missing or ambiguous"
            )))
        })
    });

    Ok([true_token?, false_token?])
}

/// The identity of the one token named `name`.
fn named_token(tokens: &BTreeMap<i64, fields::Token>, name: &str) -> Option<i64> {
    let mut matches = tokens
        .iter()
        .filter(|(_, token)| token.name.as_deref() == Some(name));

    match (matches.next(), matches.next()) {
        (Some((&id, _)), None) => Some(id),
        _ => None,
    }
}

/// Bind the compound reader boundaries used by both field and command dispatch.
pub(super) fn key_readers(
    symbols: &[Symbol],
    tokens: &BTreeMap<i64, fields::Token>,
    recipe: &PersistentRecipe,
    reader_token_offset: u64,
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
        true_token: named_token(tokens, recipe.boolean_tokens[0]),
        reader_token: Some(reader_token_offset),
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
    fn a_part_is_the_one_class_whose_methods_every_call_reaches() {
        let symbol = |name: &str, address| Symbol {
            name: name.into(),
            address,
        };
        let symbols = vec![
            symbol("Outer::Part<int>::operator=(Outer::Part<int>&&)", 0x10),
            symbol("Outer::Part<int>::Clear()", 0x20),
            symbol("Other::Clear()", 0x30),
            symbol("_memcpy", 0x40),
        ];
        let index = crate::binding::binary::symbols::SymbolIndex::new(&symbols);
        let indexed = index.over(&symbols);
        let class = |targets: &[u64]| method_class(indexed, &targets.iter().copied().collect());

        assert_eq!(class(&[0x10, 0x20]), Some("Outer::Part<int>"));
        assert_eq!(class(&[0x10, 0x30]), None);
        // A target that names no class, or has no symbol, may be a call on another object.
        assert_eq!(class(&[0x10, 0x40]), None);
        assert_eq!(class(&[0x10, 0x50]), None);
        assert_eq!(
            super::super::receivers::unqualified_base("Outer::Part<int>"),
            "Part"
        );
    }

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
    fn a_resizing_proves_its_element_size_and_where_it_keeps_its_objects_and_count() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;
        let resizing = arm64!(at 0x1000;
            stp x20, x19, [sp, #-0x20]!;
            mov x20, x0; // the array
            mov x19, x1; // the new count
            ldr w8, [x20, #0x10]; // its capacity
            mov w9, #0x278; // the element size
            umull x0, w8, w9;
            bl extern 0x2000; // operator new[]
            mov x21, x0;
            bl extern 0x3000; // a move of the old objects
            str x21, [x20, #8]; // the new objects
            ldr w8, [x20, #0x14]; // the count
            add w8, w8, #1;
            str w8, [x20, #0x14];
            ldp x20, x19, [sp], #0x20;
            ret
        );
        let rows = decode_arm64(&resizing, 0x1000).unwrap();
        let layout = resizing_layout(&rows, &BTreeSet::from([0x2000])).unwrap();

        assert_eq!(
            (layout.data, layout.count, layout.element_size),
            (8, 0x14, 0x278)
        );
        // A size that a call computes proves no element size.
        let mut computed = rows.clone();
        computed[4].operation = "bl".into();
        computed[4].operands = "#0x4000".into();
        assert!(resizing_layout(&computed, &BTreeSet::from([0x2000])).is_none());
        // A path that skips the store keeps the new objects nowhere that the layout names.
        let mut skipped = rows.clone();
        skipped[8].operation = "cbz".into();
        skipped[8].operands = "w8,#0x1034".into();
        assert!(resizing_layout(&skipped, &BTreeSet::from([0x2000])).is_none());
        // A path that enters between the size and its multiply may bring another size.
        let mut entered = rows.clone();
        entered[8].operation = "cbz".into();
        entered[8].operands = "w8,#0x1014".into();
        assert!(resizing_layout(&entered, &BTreeSet::from([0x2000])).is_none());
        // A path that enters between the count's load and its increment may add to another value.
        let mut joined = rows.clone();
        joined[13].operation = "cbz".into();
        joined[13].operands = "w8,#0x102c".into();
        assert!(resizing_layout(&joined, &BTreeSet::from([0x2000])).is_none());
    }

    #[test]
    fn a_moving_insertion_must_clear_its_source_on_every_return_and_says_how_much() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;
        let clears = |code: Vec<u8>| moved_source_width(&decode_arm64(&code, 0x1000).unwrap());
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

        let moved_pair = arm64!(at 0x1000;
            mov x22, x2;
            ldp x8, x9, [x22];
            stp xzr, xzr, [x22];
            stp x8, x9, [x0];
            ret
        );
        let pair_at_offset = arm64!(at 0x1000;
            stp xzr, xzr, [x2, #8];
            ret
        );
        let pair_with_writeback = arm64!(at 0x1000;
            stp xzr, xzr, [x2], #0x10;
            ret
        );
        let derived_store = arm64!(at 0x1000;
            stp xzr, xzr, [x2];
            add x9, x2, #8; // the source's second word
            str x8, [x9];
            ret
        );
        let alias_on_one_path = arm64!(at 0x1000;
            stp xzr, xzr, [x2];
            mov x9, x3;
            cbz w1, extern 0x1010;
            mov x9, x2;
            str x8, [x9]; // the source on one path
            ret
        );

        assert_eq!(clears(moved), Some(8));
        assert_eq!(clears(moved_pair), Some(16));
        for code in [
            bypassed,
            other_slot,
            kept,
            passed_on,
            pair_at_offset,
            pair_with_writeback,
            derived_store,
            alias_on_one_path,
        ] {
            assert_eq!(clears(code), None);
        }
    }

    #[test]
    fn a_serializer_constructor_installs_its_vtable_and_wraps_x1_at_offset_8() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;
        let point =
            |code: Vec<u8>| serializer_constructor_point(&decode_arm64(&code, 0x1000).unwrap());
        let with_scope = arm64!(at 0x1000;
            adrp x8, extern 0x5000;
            add x8, x8, #0x4b0;
            stp x8, x1, [x0];
            str x2, [x0, #0x10]; // the scope
            ret
        );
        let over_the_owner = arm64!(at 0x1000;
            adrp x8, extern 0x5000;
            add x8, x8, #0x4b0;
            stp x8, x1, [x0];
            str x2, [x0, #8];
            ret
        );
        let swapped = arm64!(at 0x1000;
            adrp x8, extern 0x5000;
            add x8, x8, #0x4b0;
            stp x1, x8, [x0];
            ret
        );
        let calls = arm64!(at 0x1000;
            adrp x8, extern 0x5000;
            add x8, x8, #0x4b0;
            stp x8, x1, [x0];
            bl extern 0x6000;
            ret
        );

        assert_eq!(point(with_scope), Some(0x54b0));
        for code in [over_the_owner, swapped, calls] {
            assert_eq!(point(code), None);
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
    fn an_insertion_names_the_class_of_the_pointer_it_takes() {
        let [pointer, const_pointer] = pointer_insertions("A<B>");
        assert_eq!(inserted_class(&pointer), Some("A<B>"));
        assert_eq!(inserted_class(&const_pointer), None);
        assert_eq!(inserted_class(&scoped_insertion("A")), Some("A"));
        assert_eq!(
            inserted_class("A*& CPdxArray<A*, int>::InsertAtEmplace<A*>(int, A*&&)"),
            None
        );
    }

    #[test]
    fn a_member_initializer_is_a_constructor_that_takes_no_address_but_its_object() {
        assert!(is_scalar_constructor("CRootTrigger::CRootTrigger(bool)"));
        assert!(is_scalar_constructor(
            "CScriptableValue::CScriptableValue(EScopeType, bool)"
        ));
        assert!(is_scalar_constructor("A<B*>::A()"));
        assert!(!is_scalar_constructor("CString::CString(char const*)"));
        assert!(!is_scalar_constructor("A::A(B const&)"));
        assert!(!is_scalar_constructor("A::A(COwnerHandle)"));
        assert!(!is_scalar_constructor("A::A(int, ...)"));
        assert!(!is_scalar_constructor("A::Read(bool)"));
    }

    #[test]
    fn an_inline_construction_runs_from_the_allocation_to_the_read_of_the_inserted_object() {
        use crate::engine::analysis::assembler::arm64;
        use crate::engine::analysis::decode::decode_arm64;

        let root = |code: Vec<u8>| Function {
            name: "C::ReadMember(CReader&, int)".into(),
            address: 0x1000,
            code,
        };
        let constructions = |root: &Function| {
            let rows = decode_arm64(&root.code, root.address).unwrap();
            inline_constructions(root, &rows, &BTreeSet::from([0x9000]), &[0x9300])
        };
        let straight = root(arm64!(at 0x1000;
            bl extern 0x9000; // operator new
            mov x21, x0;
            bl extern 0x9400; // member constructor
            str x8, [x21];
            blr x8; // the read
            mov x2, sp;
            bl extern 0x9300; // the insertion
            ret
        ));
        let bodies = constructions(&straight);
        assert_eq!(bodies.len(), 1);
        assert_eq!(bodies[0].address, 0x1004);
        assert_eq!(
            bodies[0].code,
            [&straight.code[4..16], &arm64!(at 0x1010; ret)[..]].concat()
        );

        let branching = root(arm64!(at 0x1000;
            bl extern 0x9000;
            cbz x0, extern 0x1010;
            blr x8;
            bl extern 0x9300;
            ret
        ));
        let call_after_the_read = root(arm64!(at 0x1000;
            bl extern 0x9000;
            blr x8;
            bl extern 0x9400;
            bl extern 0x9300;
            ret
        ));
        for (case, root) in [
            ("a branch in the construction", branching),
            (
                "a call between the read and the insertion",
                call_after_the_read,
            ),
        ] {
            assert!(constructions(&root).is_empty(), "{case}");
        }
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
