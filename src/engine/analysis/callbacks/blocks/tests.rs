//! Authored-input tests of the block method, with negative controls.
use std::collections::BTreeMap;

use super::super::tests::{
    COPY, COPY_CONSTRUCT, COPY_INTERNAL, COUNTRY, DESTROY, FACTORY, FACTORY_FROM, FRESH, LEADER,
    PASSES_ON, Rows, SET_COUNTRY, SET_LEADER, layout, rows, scope_code,
};
use super::*;
use crate::engine::analysis::callbacks::{Context, Slot};
use crate::engine::analysis::declarations::number;

// Rows call the evaluator at 0x9400, a tooltip builder at 0x9410, a function that never returns
// at 0x9420 and an unknown function at 0x9900. Owner methods evaluate
// `potential` at `this + 0x40` and `allow` at `this + 0x48`.
const EVALUATE: u64 = 0x9400;
const TOOLTIP: u64 = 0x9410;
const NEVER_RETURNS: u64 = 0x9420;
const OWNER: &str = "COwner";
const POTENTIAL: i64 = 0x40;
const ALLOW: i64 = 0x48;

const COUNTRY_TYPE: (&str, &str) = ("bl", "#0x8100");
const LEADER_TYPE: (&str, &str) = ("bl", "#0x8200");
const NO_TYPE: (&str, &str) = ("nop", "");

type Line = (u64, &'static str, &'static str);

/// Owner methods, whose evaluator calls are sites, and other functions. A direct call to a
/// function of the program is a caller of it.
struct Program {
    functions: BTreeMap<u64, Vec<Instruction>>,
    methods: BTreeSet<u64>,
    arguments: BTreeMap<u64, usize>,
    call_arguments: BTreeMap<u64, usize>,
    receivers: BTreeSet<u64>,
    readers: BTreeSet<u64>,
    instances: BTreeMap<u64, u64>,
    words: BTreeMap<u64, u64>,
    factories: BTreeMap<u64, Vec<Instruction>>,
}

impl Program {
    fn new() -> Self {
        Self {
            functions: BTreeMap::new(),
            methods: BTreeSet::new(),
            arguments: BTreeMap::new(),
            call_arguments: BTreeMap::new(),
            receivers: BTreeSet::new(),
            readers: BTreeSet::new(),
            instances: BTreeMap::new(),
            words: BTreeMap::new(),
            factories: BTreeMap::new(),
        }
    }

    /// A function that receives a scope and changes no type or link.
    fn reader(mut self, function: u64) -> Self {
        self.readers.insert(function);
        self
    }

    /// The instance pointer at `pointer` holds an object with the vtable at `point`, and `words`
    /// are the loaded words that a virtual call on it reads.
    fn instance(mut self, pointer: u64, point: u64, words: &[(u64, u64)]) -> Self {
        self.instances.insert(pointer, point);
        self.words.extend(words.iter().copied());
        self
    }

    /// The call through a pointer at `call` reads `count` argument registers.
    fn call_arguments(mut self, call: u64, count: usize) -> Self {
        self.call_arguments.insert(call, count);
        self
    }

    /// A function that receives a scope, which the pass runs on the path.
    fn receiver(mut self, lines: Rows<'_>) -> Self {
        self.receivers.insert(lines[0].0);
        self.function(lines)
    }

    /// `function` reads `count` argument registers.
    fn arguments(mut self, function: u64, count: usize) -> Self {
        self.arguments.insert(function, count);
        self
    }

    /// A function that builds a scope in the object that `x8` addresses.
    fn factory(mut self, lines: Rows<'_>) -> Self {
        self.factories.insert(lines[0].0, rows(lines));
        self
    }

    fn method(mut self, lines: Rows<'_>) -> Self {
        self.methods.insert(lines[0].0);
        self.function(lines)
    }

    fn function(mut self, lines: Rows<'_>) -> Self {
        self.functions.insert(lines[0].0, rows(lines));
        self
    }

    fn analyze(self) -> BlockEntries {
        let direct_calls = |rows: &[Instruction]| -> Vec<(u64, u64)> {
            rows.iter()
                .filter(|row| matches!(row.operation.as_str(), "bl" | "b"))
                .filter_map(|row| Some((row.address, number(&row.operands)?)))
                .collect()
        };

        let mut sites = Vec::new();
        let mut callers: BTreeMap<u64, Vec<CallSite>> = BTreeMap::new();
        for (&function, rows) in &self.functions {
            for (address, target) in direct_calls(rows) {
                if target == EVALUATE && self.methods.contains(&function) {
                    sites.push(EvaluationSite {
                        address,
                        function,
                        owner: OWNER.into(),
                    });
                }
                if self.functions.contains_key(&target) {
                    callers
                        .entry(target)
                        .or_default()
                        .push(CallSite { address, function });
                }
            }
        }

        analyze_blocks(&BlockInput {
            sites,
            evaluators: BTreeSet::from([EVALUATE]),
            scope_users: BTreeSet::from([TOOLTIP]),
            functions: self.functions,
            callers,
            scope_code: scope_code()
                .into_iter()
                .chain(self.factories.values().flatten().cloned())
                .collect(),
            scope_functions: ScopeFunctions {
                fresh_constructors: BTreeSet::from([FRESH]),
                setters: BTreeSet::from([SET_COUNTRY, SET_LEADER, PASSES_ON]),
                copy_constructors: BTreeSet::from([COPY_CONSTRUCT]),
                internal_copies: BTreeSet::from([COPY_INTERNAL]),
                copies: BTreeSet::from([COPY]),
                factories: self.factories.into_keys().collect(),
                destructors: BTreeSet::from([DESTROY]),
                readers: self.readers,
            },
            strings: StringFunctions::default(),
            data: ReadOnlyData::default().with_words(&self.words),
            layout: layout(),
            scope_names: None,
            arguments: self.arguments,
            call_arguments: self.call_arguments,
            ignores_x8: BTreeSet::new(),
            instances: self.instances,
            receivers: self.receivers,
            never_return: BTreeSet::from([NEVER_RETURNS]),
        })
    }
}

/// The start of a function at `base` that keeps its receiver in `x21` and builds scope A at
/// sp+0x100 with `typed`. The next instruction is at `base + 0x18`.
fn builds_a(base: u64, typed: (&'static str, &'static str)) -> Vec<Line> {
    vec![
        (base, "sub", "sp,sp,#0x200"),
        (base + 0x4, "mov", "x21,x0"),
        (base + 0x8, "add", "x0,sp,#0x100"),
        (base + 0xc, "bl", "#0x8000"),
        (base + 0x10, "add", "x0,sp,#0x100"),
        (base + 0x14, typed.0, typed.1),
    ]
}

/// A call at `at` to `callee` with the receiver and scope A.
fn calls_with_a(at: u64, callee: &'static str) -> [Line; 3] {
    [
        (at, "mov", "x0,x21"),
        (at + 4, "add", "x1,sp,#0x100"),
        (at + 8, "bl", callee),
    ]
}

/// A function at `base` that builds scope A with `typed`, calls `callee` with it and returns.
fn caller(base: u64, typed: (&'static str, &'static str), callee: &'static str) -> Vec<Line> {
    let mut lines = builds_a(base, typed);
    lines.extend(calls_with_a(base + 0x18, callee));
    lines.push((base + 0x24, "ret", ""));
    lines
}

/// A wrapper at `base` that evaluates the block at `this + offset` with its scope parameter.
fn wrapper(base: u64, offset: &'static str) -> Vec<Line> {
    vec![
        (base, "mov", "x19,x0"),
        (base + 0x4, "mov", "x20,x1"),
        (base + 0x8, "add", offset),
        (base + 0xc, "mov", "x1,x20"),
        (base + 0x10, "bl", "#0x9400"),
        (base + 0x14, "ret", ""),
    ]
}

const POTENTIAL_OF_X19: &str = "x0,x19,#0x40";
const ALLOW_OF_X19: &str = "x0,x19,#0x48";

fn block(offset: i64) -> Block {
    Block {
        owner: OWNER.into(),
        offset,
    }
}

fn contexts(result: &BlockEntries, offset: i64) -> Vec<Context> {
    result
        .blocks
        .get(&block(offset))
        .map(|findings| findings.contexts.iter().cloned().collect())
        .unwrap_or_default()
}

fn unresolved(result: &BlockEntries, offset: i64) -> Vec<&'static str> {
    result
        .blocks
        .get(&block(offset))
        .map(|findings| findings.unresolved.iter().copied().collect())
        .unwrap_or_default()
}

/// A scope of type `this` whose links all point back to itself.
fn fresh(this: Slot) -> Context {
    Context {
        this,
        root: Slot::SelfLink,
        from: vec![Slot::SelfLink],
        prev: vec![Slot::SelfLink],
    }
}

fn unreadable() -> Context {
    Context {
        this: Slot::Unresolved,
        root: Slot::Unresolved,
        from: vec![Slot::Unresolved],
        prev: vec![Slot::Unresolved],
    }
}

#[test]
fn a_scope_that_the_method_builds_reaches_its_block() {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "add", "x0,x21,#0x40"),
        (0x101c, "add", "x1,sp,#0x100"),
        (0x1020, "bl", "#0x9400"),
        (0x1024, "ret", ""),
    ]);
    let result = Program::new().method(&lines).analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
    assert!(result.unattributed.is_empty());
}

#[test]
fn a_scope_that_a_factory_returns_reaches_its_block() {
    let lines = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "mov", "x21,x0"),
        (0x1008, "add", "x8,sp,#0x100"),
        (0x100c, "bl", "#0x8d00"),
        (0x1010, "add", "x0,x21,#0x40"),
        (0x1014, "add", "x1,sp,#0x100"),
        (0x1018, "bl", "#0x9400"),
        (0x101c, "ret", ""),
    ];
    let followed = Program::new().factory(FACTORY).method(&lines).analyze();
    let unfollowed = Program::new().method(&lines).analyze();

    assert_eq!(
        contexts(&followed, POTENTIAL),
        [Context {
            from: FACTORY_FROM.to_vec(),
            ..fresh(COUNTRY)
        }]
    );
    assert_eq!(contexts(&unfollowed, POTENTIAL), [unreadable()]);
}

#[test]
fn a_block_that_is_not_this_plus_an_offset_is_unattributed() {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "ldr", "x0,[x21,#0x40]"),
        (0x101c, "add", "x1,sp,#0x100"),
        (0x1020, "bl", "#0x9400"),
        (0x1024, "ret", ""),
    ]);
    let result = Program::new().method(&lines).analyze();

    assert!(result.blocks.is_empty());
    assert_eq!(result.unattributed, BTreeMap::from([(OWNER.into(), 1)]));
}

#[test]
fn two_branches_that_select_different_blocks_are_unattributed() {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "cbz", "x2,#0x1024"),
        (0x101c, "add", "x0,x21,#0x40"),
        (0x1020, "b", "#0x1028"),
        (0x1024, "add", "x0,x21,#0x48"),
        (0x1028, "add", "x1,sp,#0x100"),
        (0x102c, "bl", "#0x9400"),
        (0x1030, "ret", ""),
    ]);
    let result = Program::new().method(&lines).analyze();

    assert!(result.blocks.is_empty());
    assert_eq!(result.unattributed, BTreeMap::from([(OWNER.into(), 1)]));
}

#[test]
fn a_scope_parameter_takes_the_scope_that_the_caller_built_and_linked() {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "add", "x0,sp,#0x180"),
        (0x101c, "bl", "#0x8000"),
        (0x1020, "add", "x0,sp,#0x180"),
        (0x1024, "bl", "#0x8200"),
        (0x1028, "add", "x8,sp,#0x180"),
        (0x102c, "str", "x8,[sp,#0x138]"), // A.from = B
    ]);
    lines.extend(calls_with_a(0x1030, "#0x2000"));
    lines.push((0x103c, "ret", ""));
    let result = Program::new()
        .function(&lines)
        .method(&wrapper(0x2000, POTENTIAL_OF_X19))
        .analyze();

    assert_eq!(
        contexts(&result, POTENTIAL),
        [Context {
            from: vec![LEADER, Slot::SelfLink],
            ..fresh(COUNTRY)
        }]
    );
    assert!(unresolved(&result, POTENTIAL).is_empty());
}

#[test]
fn a_scope_parameter_with_no_caller_has_no_context() {
    let result = Program::new()
        .method(&wrapper(0x2000, POTENTIAL_OF_X19))
        .analyze();

    assert!(contexts(&result, POTENTIAL).is_empty());
    assert_eq!(unresolved(&result, POTENTIAL), ["no-caller"]);
}

/// A wrapper at 0x2000 that runs `before` on its scope in `x20` and then evaluates `potential`.
fn wrapper_doing(before: &[Line]) -> Vec<Line> {
    let mut lines = vec![(0x2000, "mov", "x19,x0"), (0x2004, "mov", "x20,x1")];
    lines.extend_from_slice(before);
    let next = lines.last().unwrap().0 + 4;
    lines.extend([
        (next, "add", "x0,x19,#0x40"),
        (next + 4, "mov", "x1,x20"),
        (next + 8, "bl", "#0x9400"),
        (next + 12, "ret", ""),
    ]);
    lines
}

#[test]
fn a_setter_in_the_wrapper_gives_the_type_that_the_block_receives() {
    let before = [(0x2008, "mov", "x0,x20"), (0x200c, "bl", "#0x8200")];
    let result = Program::new()
        .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
        .method(&wrapper_doing(&before))
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(LEADER)]);
}

#[test]
fn a_scope_that_the_wrapper_passes_to_an_unknown_call_is_unresolved() {
    let before = [(0x2008, "mov", "x0,x20"), (0x200c, "bl", "#0x9900")];
    let result = Program::new()
        .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
        .method(&wrapper_doing(&before))
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [unreadable()]);
}

/// A caller at 0x1000 that links scope R, a leader, as A's root and scope C, a country, as R's
/// prev, then calls 0x2000 with A.
fn links_root_with_its_own_prev() -> Vec<Line> {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "add", "x0,sp,#0x180"),
        (0x101c, "bl", "#0x8000"),
        (0x1020, "add", "x0,sp,#0x180"),
        (0x1024, "bl", "#0x8200"),
        (0x1028, "add", "x0,sp,#0x80"),
        (0x102c, "bl", "#0x8000"),
        (0x1030, "add", "x0,sp,#0x80"),
        (0x1034, "bl", "#0x8100"),
        (0x1038, "add", "x8,sp,#0x180"),
        (0x103c, "str", "x8,[sp,#0x130]"), // A.root = R
        (0x1040, "add", "x8,sp,#0x80"),
        (0x1044, "str", "x8,[sp,#0x1c0]"), // R.prev = C
    ]);
    lines.extend(calls_with_a(0x1048, "#0x2000"));
    lines.push((0x1054, "ret", ""));
    lines
}

#[test]
fn a_link_that_the_wrapper_writes_is_read_through_the_linked_object() {
    let before = [
        (0x2008, "ldr", "x8,[x20,#0x30]"),
        (0x200c, "str", "x8,[x20,#0x40]"), // A.prev = A.root
    ];
    let result = Program::new()
        .function(&links_root_with_its_own_prev())
        .method(&wrapper_doing(&before))
        .analyze();

    assert_eq!(
        contexts(&result, POTENTIAL),
        [Context {
            this: COUNTRY,
            root: LEADER,
            from: vec![Slot::SelfLink],
            prev: vec![LEADER, COUNTRY, Slot::SelfLink],
        }]
    );
}

#[test]
fn link_writes_on_two_paths_of_the_wrapper_give_two_contexts() {
    let before = [
        (0x2008, "cbz", "x2,#0x2014"),
        (0x200c, "ldr", "x8,[x20,#0x30]"),
        (0x2010, "str", "x8,[x20,#0x40]"), // A.prev = A.root
    ];
    let result = Program::new()
        .function(&links_root_with_its_own_prev())
        .method(&wrapper_doing(&before))
        .analyze();
    let unchanged = Context {
        this: COUNTRY,
        root: LEADER,
        from: vec![Slot::SelfLink],
        prev: vec![Slot::SelfLink],
    };
    let linked = Context {
        prev: vec![LEADER, COUNTRY, Slot::SelfLink],
        ..unchanged.clone()
    };

    assert_eq!(contexts(&result, POTENTIAL), [linked, unchanged]);
}

/// A wrapper that runs `test` on its scope, which branches past a leader setter to 0x201c,
/// before it evaluates `potential`.
fn sets_leader_when(test: [Line; 3]) -> Vec<Line> {
    let mut before = test.to_vec();
    before.extend([(0x2014, "mov", "x0,x20"), (0x2018, "bl", "#0x8200")]);
    wrapper_doing(&before)
}

#[test]
fn the_callers_type_selects_a_branch_in_the_wrapper() {
    let wrapper = sets_leader_when([
        (0x2008, "ldr", "x8,[x20,#0x8]"),
        (0x200c, "cmp", "x8,#4"),
        (0x2010, "b.ne", "#0x201c"),
    ]);
    let country = Program::new()
        .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
        .method(&wrapper)
        .analyze();
    let untyped = Program::new()
        .function(&caller(0x1000, NO_TYPE, "#0x2000"))
        .method(&wrapper)
        .analyze();

    assert_eq!(contexts(&country, POTENTIAL), [fresh(LEADER)]);
    assert_eq!(contexts(&untyped, POTENTIAL), [fresh(Slot::NotSet)]);
}

#[test]
fn the_callers_self_link_selects_a_branch_in_the_wrapper() {
    let wrapper = sets_leader_when([
        (0x2008, "ldr", "x8,[x20,#0x30]"),
        (0x200c, "cmp", "x8,x20"),
        (0x2010, "b.ne", "#0x201c"),
    ]);
    let self_linked = Program::new()
        .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
        .method(&wrapper)
        .analyze();
    let linked = Program::new()
        .function(&links_root_with_its_own_prev())
        .method(&wrapper)
        .analyze();

    assert_eq!(contexts(&self_linked, POTENTIAL), [fresh(LEADER)]);
    assert_eq!(
        contexts(&linked, POTENTIAL),
        [Context {
            this: COUNTRY,
            root: LEADER,
            from: vec![Slot::SelfLink],
            prev: vec![Slot::SelfLink],
        }]
    );
}

/// A caller at 0x1000 that builds a country scope A, runs `between`, then calls the `potential`
/// wrapper at 0x2000 and the `allow` wrapper at 0x2100 with A.
fn evaluates_both(between: &[Line]) -> Vec<Line> {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend_from_slice(between);
    let next = lines.last().unwrap().0 + 4;
    lines.extend(calls_with_a(next, "#0x2000"));
    lines.extend(calls_with_a(next + 0xc, "#0x2100"));
    lines.push((next + 0x18, "ret", ""));
    lines
}

#[test]
fn a_scope_stays_known_from_one_evaluation_to_the_next() {
    let result = Program::new()
        .function(&evaluates_both(&[]))
        .method(&wrapper(0x2000, POTENTIAL_OF_X19))
        .method(&wrapper(0x2100, ALLOW_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
    assert_eq!(contexts(&result, ALLOW), [fresh(COUNTRY)]);
}

#[test]
fn a_scope_passed_to_an_unknown_call_between_evaluations_is_unresolved() {
    let between = [(0x1018, "add", "x0,sp,#0x100"), (0x101c, "bl", "#0x9900")];
    let result = Program::new()
        .function(&evaluates_both(&between))
        .method(&wrapper(0x2000, POTENTIAL_OF_X19))
        .method(&wrapper(0x2100, ALLOW_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [unreadable()]);
    assert_eq!(contexts(&result, ALLOW), [unreadable()]);
}

#[test]
fn a_scope_that_a_reader_receives_between_evaluations_stays_known() {
    let between = [(0x1018, "add", "x2,sp,#0x100"), (0x101c, "bl", "#0x9700")];
    let run = |program: Program| {
        program
            .function(&evaluates_both(&between))
            .method(&wrapper(0x2000, POTENTIAL_OF_X19))
            .method(&wrapper(0x2100, ALLOW_OF_X19))
            .analyze()
    };

    assert_eq!(
        contexts(&run(Program::new().reader(0x9700)), ALLOW),
        [fresh(COUNTRY)]
    );
    assert_eq!(contexts(&run(Program::new()), ALLOW), [unreadable()]);
}

#[test]
fn a_change_after_the_evaluation_reaches_the_next_one() {
    let mut potential = wrapper(0x2000, POTENTIAL_OF_X19);
    potential.truncate(5);
    potential.extend([
        (0x2014, "mov", "x0,x20"),
        (0x2018, "bl", "#0x8200"),
        (0x201c, "ret", ""),
    ]);
    let result = Program::new()
        .function(&evaluates_both(&[]))
        .method(&potential)
        .method(&wrapper(0x2100, ALLOW_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
    assert_eq!(contexts(&result, ALLOW), [fresh(LEADER)]);
}

#[test]
fn a_change_on_an_early_return_reaches_the_next_evaluation() {
    let potential = [
        (0x2000, "mov", "x19,x0"),
        (0x2004, "mov", "x20,x1"),
        (0x2008, "cbz", "x2,#0x2020"),
        (0x200c, "add", "x0,x19,#0x40"),
        (0x2010, "mov", "x1,x20"),
        (0x2014, "bl", "#0x9400"),
        (0x2018, "ret", ""),
        (0x201c, "nop", ""),
        (0x2020, "mov", "x0,x20"),
        (0x2024, "bl", "#0x8200"),
        (0x2028, "ret", ""),
    ];
    let result = Program::new()
        .function(&evaluates_both(&[]))
        .method(&potential)
        .method(&wrapper(0x2100, ALLOW_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, ALLOW), [fresh(COUNTRY), fresh(LEADER)]);
}

/// A `potential` wrapper that passes its scope to an unknown call, such as a tooltip builder,
/// after the evaluation and only when its third argument is not null.
fn passes_scope_on_when_x2_is_set() -> Vec<Line> {
    vec![
        (0x2000, "mov", "x19,x0"),
        (0x2004, "mov", "x20,x1"),
        (0x2008, "mov", "x22,x2"),
        (0x200c, "add", "x0,x19,#0x40"),
        (0x2010, "mov", "x1,x20"),
        (0x2014, "bl", "#0x9400"),
        (0x2018, "cbz", "x22,#0x2024"),
        (0x201c, "mov", "x0,x20"),
        (0x2020, "bl", "#0x9900"),
        (0x2024, "ret", ""),
    ]
}

#[test]
fn a_null_argument_from_the_caller_keeps_the_scope_from_a_tooltip_call() {
    let result = Program::new()
        .function(&evaluates_both(&[(0x1018, "mov", "x2,#0")]))
        .method(&passes_scope_on_when_x2_is_set())
        .method(&wrapper(0x2100, ALLOW_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, ALLOW), [fresh(COUNTRY)]);
}

#[test]
fn a_set_argument_from_the_caller_passes_the_scope_to_a_tooltip_call() {
    let result = Program::new()
        .function(&evaluates_both(&[(0x1018, "add", "x2,sp,#0x20")]))
        .method(&passes_scope_on_when_x2_is_set())
        .method(&wrapper(0x2100, ALLOW_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
    assert_eq!(contexts(&result, ALLOW), [unreadable()]);
}

#[test]
fn a_flag_that_the_wrapper_writes_selects_the_callers_next_scope() {
    let potential = [
        (0x2000, "mov", "x19,x0"),
        (0x2004, "mov", "x20,x1"),
        (0x2008, "mov", "x9,#1"),
        (0x200c, "str", "x9,[x2]"),
        (0x2010, "add", "x0,x19,#0x40"),
        (0x2014, "mov", "x1,x20"),
        (0x2018, "bl", "#0x9400"),
        (0x201c, "ret", ""),
    ];
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "str", "xzr,[sp,#0x20]"),
        (0x101c, "mov", "x0,x21"),
        (0x1020, "add", "x1,sp,#0x100"),
        (0x1024, "add", "x2,sp,#0x20"),
        (0x1028, "bl", "#0x2000"),
        (0x102c, "ldr", "x8,[sp,#0x20]"),
        (0x1030, "cbz", "x8,#0x103c"),
        (0x1034, "add", "x0,sp,#0x100"),
        (0x1038, "bl", "#0x8200"),
    ]);
    lines.extend(calls_with_a(0x103c, "#0x2100"));
    lines.push((0x1048, "ret", ""));
    let result = Program::new()
        .arguments(EVALUATE, 2)
        .function(&lines)
        .method(&potential)
        .method(&wrapper(0x2100, ALLOW_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, ALLOW), [fresh(LEADER)]);
}

#[test]
fn an_evaluation_or_a_tooltip_call_forgets_the_local_that_it_receives() {
    let run = |call, passed| {
        let mut lines = builds_a(0x1000, COUNTRY_TYPE);
        lines.extend([
            (0x1018, "str", "xzr,[sp,#0x20]"),
            (0x101c, "add", passed),
            (0x1020, "bl", call),
            (0x1024, "ldr", "x8,[sp,#0x20]"),
            (0x1028, "cbz", "x8,#0x1034"),
            (0x102c, "add", "x0,sp,#0x100"),
            (0x1030, "bl", "#0x8200"),
        ]);
        lines.extend(calls_with_a(0x1034, "#0x2100"));
        lines.push((0x1040, "ret", ""));
        let result = Program::new()
            .function(&lines)
            .method(&wrapper(0x2100, ALLOW_OF_X19))
            .analyze();
        contexts(&result, ALLOW)
    };

    for call in ["#0x9400", "#0x9410"] {
        assert_eq!(run(call, "x1,sp,#0x20"), [fresh(COUNTRY), fresh(LEADER)]);
        assert_eq!(run(call, "x1,sp,#0x28"), [fresh(COUNTRY)]);
    }
}

/// A `potential` wrapper that stores its scope pointer where `x2` points when `keeps` is set.
fn stores_scope_pointer(keeps: bool) -> Vec<Line> {
    let store = if keeps {
        (0x2008, "str", "x20,[x2]")
    } else {
        (0x2008, "nop", "")
    };
    wrapper_doing(&[store])
}

#[test]
fn a_scope_pointer_that_the_wrapper_keeps_escapes_to_a_call_that_can_reach_it() {
    let mut caller = builds_a(0x1000, COUNTRY_TYPE);
    caller.extend([
        (0x1018, "add", "x2,sp,#0x20"),
        (0x101c, "mov", "x0,x21"),
        (0x1020, "add", "x1,sp,#0x100"),
        (0x1024, "bl", "#0x2000"),
        (0x1028, "add", "x0,sp,#0x20"), // where the wrapper kept the pointer
        (0x102c, "bl", "#0x9900"),
    ]);
    caller.extend(calls_with_a(0x1030, "#0x2100"));
    caller.push((0x103c, "ret", ""));
    let run = |keeps| {
        Program::new()
            .function(&caller)
            .method(&stores_scope_pointer(keeps))
            .method(&wrapper(0x2100, ALLOW_OF_X19))
            .analyze()
    };

    assert_eq!(contexts(&run(true), ALLOW), [unreadable()]);
    assert_eq!(contexts(&run(false), ALLOW), [fresh(COUNTRY)]);
}

#[test]
fn an_evaluation_before_the_entry_call_records_nothing_for_its_block() {
    let method = [
        (0x2000, "sub", "sp,sp,#0x100"),
        (0x2004, "mov", "x19,x0"),
        (0x2008, "mov", "x20,x1"),
        (0x200c, "add", "x0,x19,#0x40"),
        (0x2010, "mov", "x1,x20"),
        (0x2014, "bl", "#0x9400"), // potential, with the scope parameter
        (0x2018, "add", "x0,sp,#0x80"),
        (0x201c, "bl", "#0x8000"),
        (0x2020, "add", "x0,sp,#0x80"),
        (0x2024, "bl", "#0x8100"),
        (0x2028, "add", "x0,x19,#0x48"),
        (0x202c, "add", "x1,sp,#0x80"),
        (0x2030, "bl", "#0x9400"), // allow, with the method's own scope
        (0x2034, "add", "sp,sp,#0x100"),
        (0x2038, "ret", ""),
    ];
    let result = Program::new()
        .function(&caller(0x1000, LEADER_TYPE, "#0x2000"))
        .method(&method)
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(LEADER)]);
    assert!(unresolved(&result, POTENTIAL).is_empty());
    assert_eq!(contexts(&result, ALLOW), [fresh(COUNTRY)]);
}

/// A function at `base` that passes its receiver and scope on to `callee`.
fn forwards(base: u64, callee: &'static str) -> [Line; 2] {
    [(base, "bl", callee), (base + 4, "ret", "")]
}

#[test]
fn a_scope_forwarded_through_two_callers_is_read_where_it_is_built() {
    let result = Program::new()
        .function(&caller(0x1000, COUNTRY_TYPE, "#0x3000"))
        .function(&forwards(0x3000, "#0x2000"))
        .method(&wrapper(0x2000, POTENTIAL_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
}

#[test]
fn a_scope_forwarded_through_three_callers_is_past_the_caller_depth() {
    let result = Program::new()
        .function(&caller(0x1000, COUNTRY_TYPE, "#0x3100"))
        .function(&forwards(0x3100, "#0x3000"))
        .function(&forwards(0x3000, "#0x2000"))
        .method(&wrapper(0x2000, POTENTIAL_OF_X19))
        .analyze();

    assert!(contexts(&result, POTENTIAL).is_empty());
    assert_eq!(unresolved(&result, POTENTIAL), ["caller-depth"]);
}

#[test]
fn callers_with_different_scopes_give_two_contexts_and_equal_ones_merge() {
    let run = |second: (&'static str, &'static str)| {
        Program::new()
            .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
            .function(&caller(0x1100, second, "#0x2000"))
            .method(&wrapper(0x2000, POTENTIAL_OF_X19))
            .analyze()
    };

    assert_eq!(
        contexts(&run(LEADER_TYPE), POTENTIAL),
        [fresh(COUNTRY), fresh(LEADER)]
    );
    assert_eq!(contexts(&run(COUNTRY_TYPE), POTENTIAL), [fresh(COUNTRY)]);

    let runs: Vec<_> = run(LEADER_TYPE)
        .runs
        .into_iter()
        .map(|run| {
            let reached: Vec<_> = run.reached.into_iter().collect();
            (run.function, run.site, run.wrapper, reached)
        })
        .collect();
    assert_eq!(
        runs,
        [
            (
                0x1000,
                0x1020,
                Some(0x2000),
                vec![(0x2010, block(POTENTIAL), fresh(COUNTRY))]
            ),
            (
                0x1100,
                0x1120,
                Some(0x2000),
                vec![(0x2010, block(POTENTIAL), fresh(LEADER))]
            ),
        ]
    );
}

#[test]
fn a_call_does_not_receive_a_scope_left_in_a_register_that_its_signature_does_not_use() {
    let before = [(0x2008, "bl", "#0x9500")]; // x1 still holds the scope
    let run = |program: Program| {
        program
            .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
            .method(&wrapper_doing(&before))
            .analyze()
    };

    assert_eq!(
        contexts(&run(Program::new().arguments(0x9500, 0)), POTENTIAL),
        [fresh(COUNTRY)]
    );
    assert_eq!(
        contexts(&run(Program::new().arguments(0x9500, 1)), POTENTIAL),
        [fresh(COUNTRY)]
    );
    assert_eq!(
        contexts(&run(Program::new().arguments(0x9500, 2)), POTENTIAL),
        [unreadable()]
    );
    assert_eq!(contexts(&run(Program::new()), POTENTIAL), [unreadable()]);
}

#[test]
fn a_tooltip_builder_that_receives_the_scope_keeps_it_known() {
    let mut potential = wrapper(0x2000, POTENTIAL_OF_X19);
    potential.truncate(5);
    potential.extend([
        (0x2014, "mov", "x1,x20"),
        (0x2018, "bl", "#0x9410"),
        (0x201c, "ret", ""),
    ]);
    let unknown: Vec<Line> = potential
        .iter()
        .map(|&line| match line {
            (0x2018, "bl", _) => (0x2018, "bl", "#0x9900"),
            line => line,
        })
        .collect();
    let run = |potential: &[Line]| {
        Program::new()
            .function(&evaluates_both(&[]))
            .method(potential)
            .method(&wrapper(0x2100, ALLOW_OF_X19))
            .analyze()
    };

    assert_eq!(contexts(&run(&potential), ALLOW), [fresh(COUNTRY)]);
    assert_eq!(contexts(&run(&unknown), ALLOW), [unreadable()]);
}

/// A function at 0x3000 that receives a scope in `x1` and runs `body` on it in `x20`.
fn receives_scope(body: &[Line]) -> Vec<Line> {
    let mut lines = vec![(0x3000, "mov", "x20,x1")];
    lines.extend_from_slice(body);
    let next = lines.last().unwrap().0 + 4;
    lines.push((next, "ret", ""));
    lines
}

/// A caller at 0x1000 that builds a country scope A, passes it to the function at 0x3000 and
/// then to the `potential` wrapper at 0x2000.
fn passes_a_on_first() -> Vec<Line> {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([(0x1018, "add", "x1,sp,#0x100"), (0x101c, "bl", "#0x3000")]);
    lines.extend(calls_with_a(0x1020, "#0x2000"));
    lines.push((0x102c, "ret", ""));
    lines
}

#[test]
fn a_function_that_receives_the_scope_runs_on_the_path() {
    let evaluates = receives_scope(&[(0x3004, "mov", "x1,x20"), (0x3008, "bl", "#0x9400")]);
    let sets_leader = receives_scope(&[(0x3004, "mov", "x0,x20"), (0x3008, "bl", "#0x8200")]);
    let run = |program: Program| {
        program
            .function(&passes_a_on_first())
            .method(&wrapper(0x2000, POTENTIAL_OF_X19))
            .analyze()
    };

    assert_eq!(
        contexts(&run(Program::new().receiver(&evaluates)), POTENTIAL),
        [fresh(COUNTRY)]
    );
    assert_eq!(
        contexts(&run(Program::new().receiver(&sets_leader)), POTENTIAL),
        [fresh(LEADER)]
    );
    assert_eq!(
        contexts(&run(Program::new().function(&evaluates)), POTENTIAL),
        [unreadable()]
    );
}

#[test]
fn a_call_that_never_returns_ends_its_path() {
    let ends = |call: &'static str| -> Vec<Line> {
        vec![
            (0x2000, "mov", "x19,x0"),
            (0x2004, "mov", "x20,x1"),
            (0x2008, "cbz", "x2,#0x201c"),
            (0x200c, "add", "x0,x19,#0x40"),
            (0x2010, "mov", "x1,x20"),
            (0x2014, "bl", "#0x9400"),
            (0x2018, "ret", ""),
            (0x201c, "bl", call), // the last instruction of the function
        ]
    };
    let run = |potential: Vec<Line>| {
        Program::new()
            .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
            .method(&potential)
            .analyze()
    };

    let never_returns = run(ends("#0x9420"));
    assert_eq!(contexts(&never_returns, POTENTIAL), [fresh(COUNTRY)]);
    assert!(unresolved(&never_returns, POTENTIAL).is_empty());
    assert_eq!(
        unresolved(&run(ends("#0x9900")), POTENTIAL),
        ["outside-code"]
    );
}

#[test]
fn a_call_through_a_pointer_reads_the_registers_that_the_binding_states() {
    let before = [(0x2008, "mov", "x8,x20"), (0x200c, "blr", "x16")]; // x1 and x8 hold the scope
    let run = |program: Program| {
        program
            .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
            .method(&wrapper_doing(&before))
            .analyze()
    };

    assert_eq!(
        contexts(&run(Program::new().call_arguments(0x200c, 1)), POTENTIAL),
        [fresh(COUNTRY)]
    );
    assert_eq!(contexts(&run(Program::new()), POTENTIAL), [unreadable()]);
}

/// A `potential` wrapper with seven branches on unknown values before the evaluation, which
/// make more paths than a run follows. When `escapes` is set, the side of a last branch that the
/// search takes first passes the scope to an unknown call.
fn branches_before_evaluating(escapes: bool) -> Vec<Line> {
    let escape = if escapes {
        [(0x2028, "mov", "x0,x20"), (0x202c, "bl", "#0x9900")]
    } else {
        [(0x2028, "nop", ""), (0x202c, "nop", "")]
    };
    let mut lines = vec![
        (0x2000, "mov", "x19,x0"),
        (0x2004, "mov", "x20,x1"),
        (0x2008, "cbz", "x3,#0x200c"),
        (0x200c, "cbz", "x4,#0x2010"),
        (0x2010, "cbz", "x5,#0x2014"),
        (0x2014, "cbz", "x6,#0x2018"),
        (0x2018, "cbz", "x7,#0x201c"),
        (0x201c, "cbz", "x9,#0x2020"),
        (0x2020, "cbz", "x2,#0x2028"),
        (0x2024, "b", "#0x2030"),
    ];
    lines.extend(escape);
    lines.extend([
        (0x2030, "add", "x0,x19,#0x40"),
        (0x2034, "mov", "x1,x20"),
        (0x2038, "bl", "#0x9400"),
        (0x203c, "ret", ""),
    ]);
    lines
}

#[test]
fn a_search_that_ends_at_a_bound_with_no_contradiction_has_no_gap() {
    let run = |escapes| {
        Program::new()
            .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
            .method(&branches_before_evaluating(escapes))
            .analyze()
    };

    let bounds = |result: &BlockEntries| -> BTreeSet<(&'static str, bool, bool)> {
        result
            .runs
            .iter()
            .flat_map(|run| {
                run.bounded
                    .iter()
                    .map(|bound| (bound.reason, bound.stop.is_some(), run.contradicted))
            })
            .collect()
    };

    let clean = run(false);
    assert_eq!(contexts(&clean, POTENTIAL), [fresh(COUNTRY)]);
    assert!(unresolved(&clean, POTENTIAL).is_empty());
    assert_eq!(
        bounds(&clean),
        BTreeSet::from([("path-limit", true, false)])
    );

    let contradicted = run(true);
    assert_eq!(
        contexts(&contradicted, POTENTIAL),
        [fresh(COUNTRY), unreadable()]
    );
    assert_eq!(unresolved(&contradicted, POTENTIAL), ["path-limit"]);
    assert_eq!(
        bounds(&contradicted),
        BTreeSet::from([("path-limit", true, true)])
    );
}

#[test]
fn a_scope_pointer_kept_in_the_callers_stack_escapes_only_to_a_call_that_can_reach_it() {
    let run = |unknown_call_receives: (&'static str, &'static str)| {
        let mut lines = builds_a(0x1000, COUNTRY_TYPE);
        lines.extend([
            (0x1018, "add", "x9,sp,#0x100"),
            (0x101c, "str", "x9,[sp,#0x20]"), // a copy of A's address in the caller's frame
            (0x1020, unknown_call_receives.0, unknown_call_receives.1),
            (0x1024, "mov", "x8,#0"),
            (0x1028, "bl", "#0x9900"),
        ]);
        lines.extend(calls_with_a(0x102c, "#0x2000"));
        lines.push((0x1038, "ret", ""));
        Program::new()
            .function(&lines)
            .method(&wrapper(0x2000, POTENTIAL_OF_X19))
            .analyze()
    };

    assert_eq!(
        contexts(&run(("mov", "x0,#0")), POTENTIAL),
        [fresh(COUNTRY)]
    );
    assert_eq!(
        contexts(&run(("add", "x0,sp,#0x10")), POTENTIAL),
        [unreadable()]
    );
}

#[test]
fn a_call_that_receives_an_address_in_an_inner_frame_does_not_reach_an_outer_frame() {
    let run = |passed: (&'static str, &'static str)| {
        let mut caller = builds_a(0x1000, COUNTRY_TYPE);
        caller.extend([
            (0x1018, "add", "x9,sp,#0x100"),
            (0x101c, "str", "x9,[sp,#0x20]"), // a copy of A's address in the caller's frame
            (0x1020, "add", "x2,sp,#0x10"),
        ]);
        caller.extend(calls_with_a(0x1024, "#0x2000"));
        caller.push((0x1030, "ret", ""));
        let before = [
            (0x2008, "sub", "sp,sp,#0x40"),
            (0x200c, passed.0, passed.1),
            (0x2010, "mov", "x1,#0"),
            (0x2014, "mov", "x2,#0"),
            (0x2018, "mov", "x8,#0"),
            (0x201c, "bl", "#0x9900"),
            (0x2020, "add", "sp,sp,#0x40"),
        ];
        Program::new()
            .function(&caller)
            .method(&wrapper_doing(&before))
            .analyze()
    };

    assert_eq!(
        contexts(&run(("add", "x0,sp,#0x10")), POTENTIAL),
        [fresh(COUNTRY)]
    );
    assert_eq!(contexts(&run(("mov", "x0,x2")), POTENTIAL), [unreadable()]);
}

/// A caller that builds scope A, keeps its address in `x20` and calls the wrapper at 0x2000 with
/// it.
fn caller_keeping_a_in_x20() -> Vec<Line> {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "add", "x20,sp,#0x100"),
        (0x101c, "mov", "x0,x21"),
        (0x1020, "mov", "x1,x20"),
        (0x1024, "bl", "#0x2000"),
        (0x1028, "ret", ""),
    ]);
    lines
}

/// A wrapper at 0x2000 that runs `entry`, keeps its receiver and scope, passes the local at
/// sp+0x8 to an unknown call that reads one argument, and evaluates `potential`.
fn wrapper_with_a_local(entry: &[(&'static str, &'static str)]) -> Vec<Line> {
    let mut lines: Vec<Line> = entry
        .iter()
        .map(|(operation, operands)| (0, *operation, *operands))
        .collect();
    lines.extend([
        (0, "mov", "x19,x0"),
        (0, "mov", "x20,x1"),
        (0, "add", "x0,sp,#0x8"), // a local below the saves
        (0, "mov", "x8,#0"),
        (0, "bl", "#0x9900"),
        (0, "add", "x0,x19,#0x40"),
        (0, "mov", "x1,x20"),
        (0, "bl", "#0x9400"),
        (0, "ldp", "x29,x30,[sp,#0x30]"),
        (0, "ldp", "x20,x19,[sp,#0x20]"),
        (0, "add", "sp,sp,#0x40"),
        (0, "ret", ""),
    ]);
    for (index, line) in lines.iter_mut().enumerate() {
        line.0 = 0x2000 + 4 * index as u64;
    }
    lines
}

const PROLOGUE: &[(&str, &str)] = &[
    ("sub", "sp,sp,#0x40"),
    ("stp", "x20,x19,[sp,#0x20]"), // the caller's x20: scope A
    ("stp", "x29,x30,[sp,#0x30]"),
    ("add", "x29,sp,#0x30"),
];

#[test]
fn a_prologue_save_of_the_callers_scope_does_not_reach_a_call_that_gets_a_lower_local() {
    let result = Program::new()
        .function(&caller_keeping_a_in_x20())
        .method(&wrapper_with_a_local(PROLOGUE))
        .arguments(0x9900, 1)
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
}

#[test]
fn a_save_after_the_prologue_reaches_a_call_that_gets_a_lower_local() {
    let late_save = [
        ("sub", "sp,sp,#0x40"),
        ("mov", "x9,x0"),
        ("stp", "x20,x19,[sp,#0x20]"), // after a body row: an ordinary store
        ("stp", "x29,x30,[sp,#0x30]"),
    ];
    let result = Program::new()
        .function(&caller_keeping_a_in_x20())
        .method(&wrapper_with_a_local(&late_save))
        .arguments(0x9900, 1)
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [unreadable()]);
}

#[test]
fn a_body_spill_of_the_scope_reaches_a_call_that_gets_a_lower_local() {
    let mut spills = PROLOGUE.to_vec();
    spills.extend([("mov", "x22,x1"), ("str", "x22,[sp,#0x18]")]); // the scope in the frame
    let result = Program::new()
        .function(&caller_keeping_a_in_x20())
        .method(&wrapper_with_a_local(&spills))
        .arguments(0x9900, 1)
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [unreadable()]);
}

/// The instance pointer at 0x9d50, named by the slot at 0x7248, holds an object whose vtable
/// point is 0x6010; that vtable's slot +0x40 holds the function at 0x3000.
const INSTANCE_POINTER: u64 = 0x9d50;
const INSTANCE_WORDS: &[(u64, u64)] = &[(0x7248, INSTANCE_POINTER), (0x6050, 0x3000)];

/// Before evaluating, the wrapper calls slot +0x40 of the object that the instance pointer holds,
/// while `x1` holds its scope.
const CALLS_THE_INSTANCE: [Line; 7] = [
    (0x2008, "adrp", "x9,#0x7000"),
    (0x200c, "ldr", "x9,[x9,#0x248]"),
    (0x2010, "ldr", "x0,[x9]"),
    (0x2014, "ldr", "x8,[x0]"),
    (0x2018, "ldr", "x8,[x8,#0x40]"),
    (0x201c, "mov", "x1,x20"),
    (0x2020, "blr", "x8"),
];

#[test]
fn a_virtual_call_on_a_proven_instance_reads_the_registers_of_its_target() {
    let run = |program: Program| {
        program
            .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
            .method(&wrapper_doing(&CALLS_THE_INSTANCE))
            .analyze()
    };
    let proven = || Program::new().instance(INSTANCE_POINTER, 0x6010, INSTANCE_WORDS);

    assert_eq!(
        contexts(&run(proven().arguments(0x3000, 1)), POTENTIAL),
        [fresh(COUNTRY)]
    );
    assert_eq!(
        contexts(&run(proven().arguments(0x3000, 2)), POTENTIAL),
        [unreadable()]
    );
    assert_eq!(
        contexts(&run(Program::new().arguments(0x3000, 1)), POTENTIAL),
        [unreadable()]
    );
}

#[test]
fn a_caller_that_builds_one_scope_and_forwards_another_adds_nothing_to_the_forwarded_block() {
    let wrapper = [
        (0x2000, "mov", "x19,x0"),
        (0x2004, "mov", "x20,x1"),
        (0x2008, "mov", "x21,x2"),
        (0x200c, "add", "x0,x19,#0x40"),
        (0x2010, "mov", "x1,x20"),
        (0x2014, "bl", "#0x9400"),
        (0x2018, "add", "x0,x19,#0x48"),
        (0x201c, "mov", "x1,x21"),
        (0x2020, "bl", "#0x9400"),
        (0x2024, "ret", ""),
    ];
    let builds_potential_forwards_allow = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "mov", "x21,x0"),
        (0x1008, "mov", "x22,x2"),
        (0x100c, "add", "x0,sp,#0x100"),
        (0x1010, "bl", "#0x8000"),
        (0x1014, "add", "x0,sp,#0x100"),
        (0x1018, "bl", "#0x8100"),
        (0x101c, "mov", "x0,x21"),
        (0x1020, "add", "x1,sp,#0x100"),
        (0x1024, "mov", "x2,x22"),
        (0x1028, "bl", "#0x2000"),
        (0x102c, "ret", ""),
    ];
    let mut builds_allow = builds_a(0x3000, LEADER_TYPE);
    builds_allow.extend([
        (0x3018, "mov", "x0,x21"),
        (0x301c, "add", "x2,sp,#0x100"),
        (0x3020, "bl", "#0x1000"),
        (0x3024, "ret", ""),
    ]);
    let result = Program::new()
        .function(&builds_allow)
        .function(&builds_potential_forwards_allow)
        .method(&wrapper)
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
    assert_eq!(contexts(&result, ALLOW), [fresh(LEADER)]);
    assert!(unresolved(&result, ALLOW).is_empty());
}
