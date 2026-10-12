//! Bind embedded fixture storage and inline loader boundaries from one verified executable.
use super::{BoundAnalysis, VerifiedAnalysis, binary, fixture_decoder};
use crate::engine::analysis::{
    discovery::CandidateRecord,
    evaluate::{Call, Code, Exit, Machine, ReadOnlyData},
    fields::{self, FieldInput, ReaderJoin, Value},
    numeric::NumericFacts,
    readers::PERSISTENT_READ,
    stop::Unresolved,
};
use crate::protocol::observation::{
    FixtureInlineLoader, FixtureNestedField, FixtureOutcomeRegistryBinding, FixtureQuestionSetup,
    FixtureQuestionStorage, FixtureStorageBinding, FixtureStorageDecoder,
};

impl BoundAnalysis {
    pub(crate) fn has_inline_fixture(&self, directory: &str) -> bool {
        self.declarations
            .inline_fixtures
            .iter()
            .any(|loader| loader.directory == directory)
    }

    pub(crate) fn inline_fixture(
        &self,
        request: &crate::FixtureRequest,
    ) -> Result<Option<(FixtureOutcomeRegistryBinding, Vec<FixtureQuestionSetup>)>, String> {
        let Some(recipe) = self
            .declarations
            .inline_fixtures
            .iter()
            .find(|loader| loader.directory == request.registry())
        else {
            return Ok(None);
        };
        if request.window != crate::FixtureWindow::InitialFileLoad {
            return Err("Inline fixture loaders support the initial file-load window only".into());
        }
        let verified = self.verified().map_err(|error| error.to_string())?;
        let numeric = self.numeric_facts().map_err(|error| error.to_string())?;
        let loader = verified.verify_inline_loader(recipe)?;
        let input = verified.fixture_owner_input(recipe.owner, recipe)?;
        let root = fields::analyze(&input).map_err(|error| error.to_string())?;
        let key = root
            .fields
            .iter()
            .find(|field| field.name == recipe.key_field)
            .ok_or("Inline definition key is not established")?;
        let (key_storage, _) = member_storage(&input, key.token, numeric)?;
        if key_storage.decoder != FixtureStorageDecoder::String {
            return Err("Inline definition key is not a string".into());
        }
        let member_entry =
            verified.symbol(&format!("{}::ReadMember(CReader&, int)", recipe.owner))?;
        let root_return = loader + recipe.root_call + 4;
        let binding = FixtureOutcomeRegistryBinding {
            load_entry: loader + recipe.reader_call,
            reader_entry: loader + recipe.root_call,
            reader_return: loader + recipe.file_end,
            constructor_entry: root_return,
            member_entry,
            inline: Some(FixtureInlineLoader {
                root_return,
                key_storage,
            }),
        };
        let mut questions = Vec::new();
        for (index, question) in request.field_questions.iter().enumerate() {
            let setup = match verified.nested_fixture_storage(question, &root, recipe, numeric) {
                Ok((token, storage, callee, nested)) => {
                    let bound = FixtureStorageBinding {
                        offset: nested
                            .owner_offset
                            .checked_add(storage.offset)
                            .ok_or("Fixture storage offset overflow")?,
                        decoder: storage.decoder,
                    };
                    let reader = crate::ReaderId::from_callee(&callee);
                    let storage = FixtureQuestionStorage::for_reader(
                        Some(&reader),
                        bound.decoder.reader_kind(),
                        bound,
                    );
                    FixtureQuestionSetup {
                        token: Some(token),
                        nested: Some(nested),
                        ..FixtureQuestionSetup::new(index, question, storage)
                    }
                }
                Err(reason) => FixtureQuestionSetup::new(
                    index,
                    question,
                    FixtureQuestionStorage::UnresolvedReader(reason),
                ),
            };
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
        numeric: &NumericFacts,
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
        if callee != PERSISTENT_READ
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
        let nested_result = fields::analyze(&nested_input).map_err(|error| error.to_string())?;
        let leaf = nested_result
            .fields
            .iter()
            .find(|field| field.name == question.field)
            .ok_or("Nested field token is not established")?;
        let (storage, callee) = member_storage(&nested_input, leaf.token, numeric)?;
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
        let (class, _) = recipe
            .loader
            .split_once("::")
            .ok_or("Loader has no class")?;
        let selection = CandidateRecord {
            database: class.into(),
            owner_candidate: owner.into(),
            loader_class: class.into(),
            address: self.symbol(recipe.loader)?,
            initial_loader: None,
        };
        self.read_field_input(selection)
            .map_err(|error| error.to_string())
    }
}

/// Prove a single tail reader destination for a known token, including a preceding comparison read.
fn member_storage(
    input: &FieldInput,
    token: i64,
    numeric: &NumericFacts,
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
        if fixture_decoder(&symbol.name, numeric).is_some()
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
            decoder: fixture_decoder(callee, numeric).ok_or("Unsupported storage reader")?,
        },
        callee.clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{FPML_SCALE, fixed_point_numeric};
    use super::*;
    use crate::GrammarProperty;
    use crate::engine::analysis::{assembler::arm64, discovery::Symbol, fields::Function};

    fn input(code: Vec<u8>) -> FieldInput {
        FieldInput {
            selection: CandidateRecord { database: "Database".into(), owner_candidate: "Owner".into(), loader_class: "Database".into(), address: 0x1000, initial_loader: None },
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
        let numeric = fixed_point_numeric(
            GrammarProperty::Unresolved,
            GrammarProperty::Known(Some(FPML_SCALE)),
        );
        let (storage, _) = member_storage(&input, 7, &numeric).unwrap();
        assert_eq!(storage.offset, 56);
        assert_eq!(
            storage.decoder,
            FixtureStorageDecoder::FixedPoint { scale: FPML_SCALE }
        );
        assert!(member_storage(&input, 8, &numeric).is_err());
    }

    #[test]
    fn a_fixed_point_read_without_a_known_scale_is_unproven() {
        let input = input(arm64!(at 0x1000;
            cmp w2, #7; b.ne >rejected;
            mov x8, x0; mov x0, x1; add x1, x8, #56;
            b extern 0x3000;
            rejected:; ret
        ));
        for scale in [
            GrammarProperty::Partial(Some(FPML_SCALE)),
            GrammarProperty::Unresolved,
        ] {
            let numeric = fixed_point_numeric(GrammarProperty::Unresolved, scale);
            assert_eq!(
                member_storage(&input, 7, &numeric).unwrap_err(),
                "Fixture storage read is not proven"
            );
        }
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
            let (storage, joined) = member_storage(&positive, 7, &NumericFacts::default()).unwrap();
            assert_eq!(storage.offset, 56);
            assert_eq!(storage.decoder, decoder);
            assert_eq!(joined, callee);
            assert!(member_storage(&positive, 8, &NumericFacts::default()).is_err());
            for code in [
                arm64!(at 0x1000; add x1, x0, #56; b extern 0x3000),
                arm64!(at 0x1000; mov x0, x1; mov x1, #56; b extern 0x3000),
                arm64!(at 0x1000; mov x8, x0; mov x0, x1; add x1, x8, #56; bl extern 0x3000; ret),
            ] {
                let mut negative = input(code);
                negative.symbols[1].name = callee.into();
                assert!(member_storage(&negative, 7, &NumericFacts::default()).is_err());
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
            assert!(member_storage(&input(code), 7, &NumericFacts::default()).is_err());
        }
    }
}
