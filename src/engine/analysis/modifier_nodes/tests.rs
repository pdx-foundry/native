use super::*;
use crate::engine::analysis::assembler::{Arm64, arm64};

const CALLER: u64 = 0x1000;
const CALCULATION: u64 = 0x1100;
const INITIALIZER: u64 = 0x1200;
const BASE_CONSTRUCTOR: u64 = 0x2000;
const CONSTANT_MASK: u64 = 0x8010;
const REPLICATED: u64 = 0x8020;
const ZERO_FILL_MASK: u64 = 0x9054;

/// The caller's code before the mask and closure: nothing, or a replicating load.
enum Prefix {
    None,
    ReplicatingLoad,
}

/// How the caller passes the category mask.
enum MaskArgument {
    Stack(u32),
    Constant,
    ZeroFill,
    OwnerField,
}

/// An owner constructor at `CALLER` that builds node 1 and returns the call site.
fn caller(prefix: Prefix, mask: MaskArgument) -> (Vec<u8>, u64) {
    let mut code = Arm64::at(CALLER);
    arm64!(code; sub sp, sp, #0x40);

    if let Prefix::ReplicatingLoad = prefix {
        code.address(9, REPLICATED);
        arm64!(code; ld1r {v0.d2}, [x9]; str q0, [x0, #0x40]);
    }

    match mask {
        MaskArgument::Stack(value) => {
            let (low, high) = (value & 0xffff, value >> 16);
            arm64!(code;
                movz w8, #low;
                movk w8, #high, lsl #16;
                str w8, [sp, #0x10];
                add x5, sp, #0x10
            );
        }
        MaskArgument::Constant => {
            code.address(5, CONSTANT_MASK);
        }
        MaskArgument::ZeroFill => {
            code.address(5, ZERO_FILL_MASK);
        }
        MaskArgument::OwnerField => {
            arm64!(code; ldr x5, [x0, #8]);
        }
    }

    code.address(9, CALCULATION);
    arm64!(code; stp x0, x9, [sp, #0x18]; add x3, sp, #0x18);
    let site = code.here();
    code.call(BASE_CONSTRUCTOR);
    arm64!(code; add sp, sp, #0x40; ret);
    (code.bytes(), site)
}

fn calculation_without_store() -> Vec<u8> {
    arm64!(at CALCULATION; ret)
}

/// A calculation that stores one of two masks, chosen by an unknown argument.
fn calculation_with_choice() -> Vec<u8> {
    arm64!(at CALCULATION;
        mov x19, x1;
        cmp w2, #0;
        mov w8, #0x10; // first mask
        mov w9, #0x20; // second mask
        csel w10, w8, w9, ne;
        str w10, [x19, #0xdc];
        ret
    )
}

/// A store at the mask's offset from the owner, not from the node.
fn calculation_storing_to_owner() -> Vec<u8> {
    arm64!(at CALCULATION;
        mov w8, #0x40;
        str w8, [x0, #0xdc];
        ret
    )
}

fn initializer() -> Vec<u8> {
    arm64!(at INITIALIZER;
        mov w8, #0x7e;
        adrp x9, extern 0x9000;
        str w8, [x9, #0x54]; // ZERO_FILL_MASK
        ret
    )
}

/// An initializer whose branch on an unknown value can skip the store.
fn initializer_that_can_skip() -> Vec<u8> {
    arm64!(at INITIALIZER;
        cbz x0, >skip;
        mov w8, #0x7e;
        adrp x9, extern 0x9000;
        str w8, [x9, #0x54]; // ZERO_FILL_MASK
        skip:;
        ret
    )
}

fn input(caller: (Vec<u8>, u64), calculation: Vec<u8>, initializer: Vec<u8>) -> ModifierNodeInput {
    let (caller, site) = caller;
    let functions = [
        (CALLER, caller),
        (CALCULATION, calculation),
        (INITIALIZER, initializer),
    ];
    let end = functions
        .iter()
        .map(|(start, bytes)| start + bytes.len() as u64)
        .max()
        .unwrap();
    let mut bytes = vec![0; (end - CALLER) as usize];
    for (start, code) in &functions {
        let offset = (start - CALLER) as usize;
        bytes[offset..offset + code.len()].copy_from_slice(code);
    }

    ModifierNodeInput {
        sources: BTreeMap::from([(1, Ok(vec![]))]),
        constructions: vec![Construction {
            node: 1,
            owner: "COwner".into(),
            caller: CALLER,
            site,
        }],
        initializers: vec![INITIALIZER],
        text: Text {
            address: CALLER,
            bytes,
            starts: functions.iter().map(|(start, _)| *start).collect(),
        },
        data: ReadOnlyData::new(vec![
            (CONSTANT_MASK, 0x0040_407cu32.to_le_bytes().to_vec()),
            (REPLICATED, 7u64.to_le_bytes().to_vec()),
        ]),
        layout: ModifierNodeLayout {
            category_offset: 0xdc,
            category_argument: 5,
            calculation_argument: 3,
            calculation_function_offset: 8,
        },
        categories: CategoryInput {
            category_name: 0,
            string_object_size: 24,
            short_length_offset: 0x17,
            assign_literal: BTreeSet::new(),
            code: Code::default(),
            data: ReadOnlyData::default(),
        },
    }
}

fn masks(input: &ModifierNodeInput) -> Result<Masks, &'static str> {
    let result = analyze(input);
    let owner = &result.nodes[&1].owners[0];
    assert_eq!(owner.owner, "COwner");
    owner.masks.clone().map_err(|unresolved| unresolved.reason)
}

#[test]
fn a_mask_on_the_stack_or_in_constant_data_is_the_constructors_mask() {
    let stack = input(
        caller(Prefix::None, MaskArgument::Stack(0x8_0200)),
        calculation_without_store(),
        initializer(),
    );
    let constant = input(
        caller(Prefix::None, MaskArgument::Constant),
        calculation_without_store(),
        initializer(),
    );

    assert_eq!(masks(&stack), Ok(Masks::Constant(0x8_0200)));
    assert_eq!(masks(&constant), Ok(Masks::Constant(0x40_407c)));
}

#[test]
fn a_replicating_load_before_the_arguments_does_not_stop_the_constructor() {
    let input = input(
        caller(Prefix::ReplicatingLoad, MaskArgument::Stack(0x400)),
        calculation_without_store(),
        initializer(),
    );

    assert_eq!(masks(&input), Ok(Masks::Constant(0x400)));
}

#[test]
fn a_zero_fill_mask_is_the_value_that_its_initializer_leaves() {
    let set = input(
        caller(Prefix::None, MaskArgument::ZeroFill),
        calculation_without_store(),
        initializer(),
    );
    let skippable = input(
        caller(Prefix::None, MaskArgument::ZeroFill),
        calculation_without_store(),
        initializer_that_can_skip(),
    );

    assert_eq!(masks(&set), Ok(Masks::Constant(0x7e)));
    assert_eq!(masks(&skippable), Err("initializer-value"));
}

#[test]
fn a_mask_that_the_caller_loads_from_its_object_is_unresolved() {
    let input = input(
        caller(Prefix::None, MaskArgument::OwnerField),
        calculation_without_store(),
        initializer(),
    );

    assert_eq!(masks(&input), Err("category-mask"));
}

#[test]
fn each_mask_that_the_calculation_can_store_joins_the_constructors_mask() {
    let input = input(
        caller(Prefix::None, MaskArgument::Stack(0x400)),
        calculation_with_choice(),
        initializer(),
    );

    assert_eq!(
        masks(&input),
        Ok(Masks::Recalculated(BTreeSet::from([0x10, 0x20, 0x400])))
    );
}

#[test]
fn a_calculation_that_stores_the_constructors_mask_keeps_it_constant() {
    let calculation = arm64!(at CALCULATION;
        mov w8, #0x20; // the constructor's mask
        str w8, [x1, #0xdc];
        ret
    );
    let input = input(
        caller(Prefix::None, MaskArgument::Stack(0x20)),
        calculation,
        initializer(),
    );

    assert_eq!(masks(&input), Ok(Masks::Constant(0x20)));
}

#[test]
fn a_store_at_the_mask_offset_from_another_object_is_not_a_mask() {
    let input = input(
        caller(Prefix::None, MaskArgument::Stack(0x400)),
        calculation_storing_to_owner(),
        initializer(),
    );

    assert_eq!(masks(&input), Ok(Masks::Constant(0x400)));
}

#[test]
fn a_constructed_node_that_no_node_type_names_has_unresolved_sources() {
    let mut input = input(
        caller(Prefix::None, MaskArgument::Stack(0x400)),
        calculation_without_store(),
        initializer(),
    );
    input.sources.clear();

    let result = analyze(&input);

    assert_eq!(
        result.nodes[&1].sources.as_ref().unwrap_err().reason,
        "node-type"
    );
}
