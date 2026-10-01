//! Bind embedded fixture storage and inline loader boundaries from one verified executable.
use super::{BoundAnalysis, VerifiedAnalysis, binary, fixture_decoder};
use crate::engine::analysis::{
    discovery::CandidateRecord,
    evaluate::{Call, Code, Exit, Machine, ReadOnlyData},
    fields::{self, FieldInput, ReaderJoin, Value},
    stop::Unresolved,
};
use crate::protocol::observation::{
    FixtureInlineLoader, FixtureNestedField, FixtureOutcomeRegistryBinding, FixtureQuestionSetup,
    FixtureStorageBinding, FixtureStorageDecoder,
};

impl BoundAnalysis {
    pub(crate) fn has_inline_fixture(&self, directory: &str) -> bool {
        self.declarations.is_some_and(|recipe| {
            recipe
                .inline_fixtures
                .iter()
                .any(|loader| loader.directory == directory)
        })
    }

    pub(crate) fn inline_fixture(
        &self,
        request: &crate::FixtureRequest,
        base: &FixtureOutcomeRegistryBinding,
    ) -> Result<Option<(FixtureOutcomeRegistryBinding, Vec<FixtureQuestionSetup>)>, String> {
        let Some(recipe) = self.declarations.and_then(|recipe| {
            recipe
                .inline_fixtures
                .iter()
                .find(|loader| loader.directory == request.registry())
        }) else {
            return Ok(None);
        };
        if request.window != crate::FixtureWindow::InitialFileLoad {
            return Err("Inline fixture loaders support the initial file-load window only".into());
        }
        if !request.observations.is_empty() {
            return Err("Inline fixture loaders support field outcomes only".into());
        }
        let verified = self.verified().map_err(|error| error.to_string())?;
        let loader = verified.verify_inline_loader(recipe)?;
        let input = verified.fixture_owner_input(recipe.owner, recipe)?;
        let root = fields::analyze_owner(&input).map_err(|error| error.to_string())?;
        let key = root
            .fields
            .iter()
            .find(|field| field.name == recipe.key_field)
            .ok_or("Inline definition key is not established")?;
        let (key_storage, _) = member_storage(&input, key.token)?;
        if key_storage.decoder != FixtureStorageDecoder::String {
            return Err("Inline definition key is not a string".into());
        }
        let member_entry =
            verified.symbol(&format!("{}::ReadMember(CReader&, int)", recipe.owner))?;
        let mut binding = base.clone();
        binding.registry = request.registry().into();
        binding.load_entry = loader + recipe.reader_call;
        binding.reader_entry = loader + recipe.root_call;
        binding.reader_return = loader + recipe.file_end;
        binding.constructor_entry = loader + recipe.root_call + 4;
        binding.member_entry = member_entry;
        binding.inline = Some(FixtureInlineLoader {
            root_return: binding.constructor_entry,
            key_storage,
        });
        binding.fields.clear();
        let mut questions = Vec::new();
        for (index, question) in request.field_questions.iter().enumerate() {
            let mut setup = FixtureQuestionSetup {
                index: index as u64,
                definition: question.definition.clone(),
                field: question.field.clone(),
                nested: None,
                parent_field: question.parent_field.clone(),
                parsing: question.parsing,
                diagnostics: question.diagnostics,
                runtime: question.runtime,
                reader_id: None,
                reader_kind: crate::ReaderKind::Unknown,
                reader_family: crate::BlockFamily::Unknown,
                token: None,
                storage: None,
                storage_unavailable: None,
            };
            match verified.nested_fixture_storage(question, &root, recipe) {
                Ok((token, storage, callee, nested)) => {
                    setup.token = Some(token);
                    setup.reader_id = Some(crate::ReaderId::from_callee(&callee).0);
                    setup.reader_kind = storage.decoder.reader_kind();
                    setup.reader_family = crate::BlockFamily::NotApplicable;
                    setup.storage = Some(FixtureStorageBinding {
                        offset: nested
                            .owner_offset
                            .checked_add(storage.offset)
                            .ok_or("Fixture storage offset overflow")?,
                        decoder: storage.decoder,
                    });
                    setup.nested = Some(nested);
                }
                Err(reason) => setup.storage_unavailable = Some(reason),
            }
            questions.push(setup);
        }
        Ok(Some((binding, questions)))
    }
}

impl VerifiedAnalysis<'_> {
    fn verify_inline_loader(
        &self,
        recipe: &super::super::targets::InlineFixtureRecipe,
    ) -> Result<u64, String> {
        let loader = self.symbol(recipe.loader)?;
        let body = self.fixture_function(loader)?;
        let rows = super::decode_arm64(&body, loader).map_err(|error| error.to_string())?;
        for (offset, callee) in [
            (recipe.reader_call, "CReader::CReader(CLexer&)"),
            (recipe.file_end, "CReader::~CReader()"),
        ] {
            if !rows.iter().any(|row| {
                row.address == loader + offset
                    && row.operation == "bl"
                    && self.catalog.symbols.iter().any(|symbol| {
                        symbol.name == callee && row.operands == format!("#0x{:x}", symbol.address)
                    })
            }) {
                return Err("Inline file boundary does not match its exact-build recipe".into());
            }
        }
        if !rows.iter().any(|row| {
            row.address == loader + recipe.root_call
                && row.operation == "blr"
                && row.operands == "x8"
        }) {
            return Err("Inline object read does not match its exact-build recipe".into());
        }
        Ok(loader)
    }

    fn nested_fixture_storage(
        &self,
        question: &crate::FixtureFieldQuestion,
        root: &fields::RegistryFieldResult,
        recipe: &super::super::targets::InlineFixtureRecipe,
    ) -> Result<(u64, FixtureStorageBinding, String, FixtureNestedField), String> {
        let parent_name = question
            .parent_field
            .as_ref()
            .ok_or("Inline fixture storage requires an embedded parent field")?;
        let parent = root
            .fields
            .iter()
            .find(|field| &field.name == parent_name)
            .ok_or("Parent field is not established")?;
        let [
            ReaderJoin::Joined {
                callee, arguments, ..
            },
        ] = parent.readers.as_slice()
        else {
            return Err("Parent has no single persistent reader".into());
        };
        if callee != "CReader::Read(CPersistent&)"
            || arguments.get("x0") != Some(&Value::Reader(0))
            || parent.paths.is_empty()
            || parent
                .paths
                .iter()
                .any(|&path| !root.paths[path].conditions.is_empty())
        {
            return Err("Parent has no unconditional embedded reader".into());
        }
        let Some(Value::Owner(offset)) = arguments.get("x1") else {
            return Err("Parent owner is not embedded".into());
        };
        let concrete = root
            .persistent
            .get(offset)
            .ok_or("Nested constructor and member reader are not joined")?;
        if concrete.read != "CPersistent::Read(CReader&)" {
            return Err("Nested reader has an unsupported read boundary".into());
        }
        let owner = concrete
            .member
            .strip_suffix("::ReadMember(CReader&, int)")
            .ok_or("Nested member signature is unsupported")?;
        let nested_input = self.fixture_owner_input(owner, recipe)?;
        let nested_result =
            fields::analyze_owner(&nested_input).map_err(|error| error.to_string())?;
        let leaf = nested_result
            .fields
            .iter()
            .find(|field| field.name == question.field)
            .ok_or("Nested field token is not established")?;
        let (storage, callee) = member_storage(&nested_input, leaf.token)?;
        let nested = FixtureNestedField {
            parent_token: parent
                .token
                .try_into()
                .map_err(|_| "Negative parent token")?,
            owner_offset: (*offset).try_into().map_err(|_| "Negative owner offset")?,
            member_entry: self.symbol(&concrete.member)?,
        };
        Ok((leaf.token as u64, storage, callee, nested))
    }

    fn fixture_function(&self, address: u64) -> Result<Vec<u8>, String> {
        let end = self
            .catalog
            .symbols
            .iter()
            .filter(|symbol| symbol.address > address)
            .map(|symbol| symbol.address)
            .min()
            .ok_or("Unbounded fixture function")?;
        let length = end - address;
        if length == 0 || length > 65536 {
            return Err("Fixture function exceeds its bound".into());
        }
        binary::code_range(&self.executable, address, length).map_err(|error| error.to_string())
    }

    fn fixture_owner_input(
        &self,
        owner: &str,
        recipe: &super::super::targets::InlineFixtureRecipe,
    ) -> Result<FieldInput, String> {
        let selection = CandidateRecord {
            database: recipe
                .loader
                .split_once("::")
                .ok_or("Loader has no class")?
                .0
                .into(),
            owner_candidate: owner.into(),
            loader: recipe.loader.into(),
            address: format!("{:#x}", self.symbol(recipe.loader)?),
            initial_loader: None,
            has_named_member_reader: true,
        };
        binary::fields::read(
            &self.executable,
            &self.catalog.symbols,
            &self.catalog.strings,
            &self.catalog.pointers,
            &self.catalog.bound_slots,
            selection,
            self.persistent,
        )
        .map_err(|error| error.to_string())
    }
}

/// Prove a single tail reader destination for a known token, including a preceding comparison read.
fn member_storage(
    input: &FieldInput,
    token: i64,
) -> Result<(FixtureStorageBinding, String), String> {
    let name = format!(
        "{}::ReadMember(CReader&, int)",
        input.selection.owner_candidate
    );
    let function = input
        .functions
        .iter()
        .find(|function| function.name == name)
        .ok_or("Member function is absent")?;
    let code = Code::decode(&[(function.address, function.code.as_slice())])
        .map_err(|error| error.to_string())?;
    let data = ReadOnlyData::new(
        input
            .read_only_data
            .iter()
            .map(|section| (section.address, section.bytes.clone()))
            .collect(),
    );
    let mut machine = Machine::new(&code, &data);
    let owner = machine.reserve(0x10000);
    let reader = machine.reserve(0x10000);
    machine.set_register(0, owner);
    machine.set_register(1, reader);
    machine.set_register(2, token as u64);
    let paths = machine.run_paths(function.address, &mut |target, machine| {
        let symbol = input
            .symbols
            .iter()
            .find(|symbol| Some(symbol.address) == target)
            .ok_or(Unresolved::new("fixture-unknown-call"))?;
        if symbol.name == "CCompareOperator::Read(CReader&)"
            && machine.register(1) == Some(reader)
            && !machine.is_tail_call()
        {
            return Ok(Call::Return(None));
        }
        if fixture_decoder(&symbol.name).is_some()
            && machine.is_tail_call()
            && machine.register(0) == Some(reader)
        {
            return Ok(Call::Stop);
        }
        Err(Unresolved::new("fixture-unproven-reader"))
    });
    let [path] = paths.as_slice() else {
        return Err("Fixture storage has conditional or incomplete paths".into());
    };
    let Ok(Exit::Stopped(target)) = path.end else {
        return Err("Fixture storage read is not proven".into());
    };
    let callee = &input
        .symbols
        .iter()
        .find(|symbol| symbol.address == target)
        .ok_or("Missing reader symbol")?
        .name;
    let offset = path
        .machine
        .register(1)
        .and_then(|destination| destination.checked_sub(owner))
        .filter(|offset| *offset < 0x10000 - 40)
        .ok_or("Fixture destination is not owner-relative")?;
    Ok((
        FixtureStorageBinding {
            offset,
            decoder: fixture_decoder(callee).ok_or("Unsupported storage reader")?,
        },
        callee.clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::{assembler::arm64, discovery::Symbol, fields::Function};

    fn input(code: Vec<u8>) -> FieldInput {
        FieldInput {
            selection: CandidateRecord { database: "Database".into(), owner_candidate: "Owner".into(), loader: "Database::Init()".into(), address: "0x1000".into(), initial_loader: None, has_named_member_reader: true },
            symbols: vec![
                Symbol { address: 0x2000, name: "CCompareOperator::Read(CReader&)".into() },
                Symbol { address: 0x3000, name: "CReader::Read(fpml::fixed_point<long long, (unsigned char)48, (unsigned char)15>&)".into() },
            ],
            functions: vec![Function { address: 0x1000, name: "Owner::ReadMember(CReader&, int)".into(), code }],
            strings: Default::default(), read_only_data: vec![], objects: vec![], persistent: None, gaps: vec![], key_readers: Default::default(),
        }
    }

    #[test]
    fn storage_follows_selected_token_across_comparison_to_tail_reader() {
        let input = input(arm64!(at 0x1000;
            cmp w2, #7;
            b.ne >rejected;
            stp x19, x20, [sp, #-16]!;
            mov x19, x0;
            mov x20, x1;
            add x0, x19, #64;
            bl extern 0x2000;
            add x1, x19, #56;
            mov x0, x20;
            ldp x19, x20, [sp], #16;
            b extern 0x3000;
            rejected:; ret
        ));
        let (storage, _) = member_storage(&input, 7).unwrap();
        assert_eq!(storage.offset, 56);
        assert_eq!(
            storage.decoder,
            FixtureStorageDecoder::FixedPoint { scale: 32768 }
        );
        assert!(member_storage(&input, 8).is_err());
    }

    #[test]
    fn float_and_short_storage_require_a_tail_reader_and_owner_destination() {
        for (callee, decoder) in [
            ("CReader::Read(float&)", FixtureStorageDecoder::Float),
            ("CReader::Read(short&)", FixtureStorageDecoder::Integer16),
        ] {
            let mut positive = input(arm64!(at 0x1000;
                cmp w2, #7; b.ne >rejected;
                mov x8, x0; mov x0, x1; add x1, x8, #56;
                b extern 0x3000;
                rejected:; ret
            ));
            positive.symbols[1].name = callee.into();
            let (storage, joined) = member_storage(&positive, 7).unwrap();
            assert_eq!(storage.offset, 56);
            assert_eq!(storage.decoder, decoder);
            assert_eq!(joined, callee);
            assert!(member_storage(&positive, 8).is_err());
            for code in [
                arm64!(at 0x1000; add x1, x0, #56; b extern 0x3000),
                arm64!(at 0x1000; mov x0, x1; mov x1, #56; b extern 0x3000),
                arm64!(at 0x1000; mov x8, x0; mov x0, x1; add x1, x8, #56; bl extern 0x3000; ret),
            ] {
                let mut negative = input(code);
                negative.symbols[1].name = callee.into();
                assert!(member_storage(&negative, 7).is_err());
            }
        }
    }

    #[test]
    fn storage_refuses_unknown_calls_wrong_receivers_and_non_tail_readers() {
        let controls = [
            arm64!(at 0x1000; bl extern 0x4000; b extern 0x3000),
            arm64!(at 0x1000; add x1, x0, #56; b extern 0x3000),
            arm64!(at 0x1000; mov x0, x1; mov x1, #56; b extern 0x3000),
            arm64!(at 0x1000; mov x8, x0; mov x0, x1; add x1, x8, #56; bl extern 0x3000; ret),
            arm64!(at 0x1000; cbz x3, >other; ret; other:; mov x8, x0; mov x0, x1; add x1, x8, #56; b extern 0x3000),
        ];
        for code in controls {
            assert!(member_storage(&input(code), 7).is_err());
        }
    }
}
