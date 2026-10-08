//! Read which registry owner each register of a function leads to at entry: the type pointers of
//! the block method (`engine::analysis::callbacks::blocks`).
//!
//! A register leads to the owner type `O` when it holds:
//!
//! - the receiver of a member function of `O`. The class is the function's exact qualifier, so a
//!   member of a nested class such as `CAnomalyType::COutcomeEffect` is not a member of
//!   `CAnomalyType`;
//! - a parameter of type `O const*`, `O*`, `O const&` or `O&`;
//! - an object of a class `C` whose word at offset `k` leads to `O`: some constructor of `C`
//!   stores there a parameter of one of those types, no constructor stores a parameter of another
//!   type there, and every constructor of `C` decodes.
//!
//! A parameter's register is known up to the first parameter that one general register may not
//! pass: each pointer, reference, integer, `bool` and `TPdxRef<…>` takes the next register,
//! after the receiver of a member function. A by-value class, a floating-point number or an
//! enumeration, which a demangled name does not tell from a class, ends the known registers. A
//! function whose qualifier is not a known class or namespace has no known registers: a class has
//! a vtable, a type info, a constructor or a destructor symbol, and a `const` function is a
//! member. A demangled name does not say whether a member function is static; a function of a
//! class is read as taking its receiver in `x0`.
//!
//! Stated assumption: every function that writes a member word that leads to `O` keeps the
//! registry owner's layout: it stores an `O`, a null object of `O`, or null. Checked by hand on
//! M452 for `CMission` (its constructors, and `ReadMember`, which stores a lookup result or
//! `TPdxNullObject<CMissionType>::_pInstance` at `+0x18`), `CResolution` (its constructor from a
//! `CResolutionType const&`, its default constructor, which stores
//! `TPdxNullObject<CResolutionType>::_pInstance`, and `ReadMember`, which stores a lookup result)
//! and `CCosmicStorm` (`+0x328`, which `ReadMember`
//! fills with a lookup result or `TPdxNullObject<CCosmicStormType>::_pInstance`). Remove the
//! assumption with a writer check that covers every function that stores the word.
use std::collections::{BTreeMap, BTreeSet};

use crate::engine::analysis::callbacks::blocks::{TypePointers, receiver_stores};
use crate::engine::analysis::discovery::Symbol;

use super::callbacks::{decoded, signature, top_level};
use super::declarations::Text;

/// The integer types that one general register passes.
const INTEGERS: &[&str] = &[
    "bool",
    "char",
    "signed char",
    "unsigned char",
    "wchar_t",
    "short",
    "unsigned short",
    "int",
    "unsigned int",
    "long",
    "unsigned long",
    "long long",
    "unsigned long long",
];

/// The registry owners and the classes whose members lead to one.
pub(super) struct TypeOwners<'a> {
    owners: &'a BTreeSet<&'a str>,
    /// The classes that the symbols name.
    classes: BTreeSet<&'a str>,
    /// The owner that each member word of a class leads to, by class and offset.
    members: BTreeMap<&'a str, BTreeMap<i64, &'a str>>,
}

impl<'a> TypeOwners<'a> {
    /// Read the classes and the member words that lead to one of `owners`.
    pub(super) fn read(text: &Text, symbols: &'a [Symbol], owners: &'a BTreeSet<&'a str>) -> Self {
        let functions = || {
            symbols
                .iter()
                .filter(|symbol| !symbol.name.contains(".cold."))
        };
        let mut classes: BTreeSet<&str> = symbols
            .iter()
            .filter_map(|symbol| {
                let name = symbol.name.as_str();
                name.strip_prefix("vtable for ")
                    .or_else(|| name.strip_prefix("typeinfo for "))
            })
            .collect();
        let mut constructors: BTreeMap<&str, Vec<(u64, &str)>> = BTreeMap::new();
        for symbol in functions() {
            let Some((class, destructor)) = constructed_class(&symbol.name) else {
                continue;
            };
            classes.insert(class);
            if !destructor {
                constructors
                    .entry(class)
                    .or_default()
                    .push((symbol.address, &symbol.name));
            }
        }

        let members = constructors
            .into_iter()
            .filter(|(_, constructors)| {
                constructors
                    .iter()
                    .any(|(_, name)| !constructor_owners(name, owners).is_empty())
            })
            .filter_map(|(class, constructors)| {
                let members = member_owners(text, &constructors, owners)?;
                (!members.is_empty()).then_some((class, members))
            })
            .collect();
        Self {
            owners,
            classes,
            members,
        }
    }

    /// The type pointers of the function that `names` name, when every name gives the same.
    pub(super) fn of<'n>(&self, mut names: impl Iterator<Item = &'n str>) -> Option<TypePointers> {
        let first = self.function(names.next()?)?;
        names
            .all(|name| self.function(name).as_ref() == Some(&first))
            .then_some(first)
    }

    /// The type pointers of the function `name`, when it has one.
    fn function(&self, name: &str) -> Option<TypePointers> {
        if name.contains(".cold.") {
            return None;
        }
        let (qualified, parameters) = signature(name)?;
        let class = qualifier(qualified);
        let member = match class {
            None => false,
            Some(class) => self.classes.contains(class) || is_const(name),
        };
        if class.is_some() && !member {
            return None;
        }

        let mut pointers = TypePointers::default();
        if let Some(class) = class {
            self.lead(&mut pointers, 0, class);
            if self.owners.contains(class) {
                pointers.method_of = Some(class.to_owned());
            }
        }
        for (register, parameter) in parameter_registers(parameters, member) {
            if let Some(class) = pointee(parameter) {
                self.lead(&mut pointers, register, class);
            }
        }
        (pointers != TypePointers::default()).then_some(pointers)
    }

    /// Record the owners that a register holding an object of `class` leads to.
    fn lead(&self, pointers: &mut TypePointers, register: usize, class: &str) {
        if let Some(owner) = self.owners.get(class) {
            pointers.registers.insert(register, (*owner).to_owned());
        }
        for (&offset, owner) in self.members.get(class).into_iter().flatten() {
            pointers
                .members
                .insert((register, offset), (*owner).to_owned());
        }
    }
}

/// The member words of a class that lead to an owner, from all of its `constructors`. `None` when
/// a constructor does not decode.
fn member_owners<'a>(
    text: &Text,
    constructors: &[(u64, &str)],
    owners: &BTreeSet<&'a str>,
) -> Option<BTreeMap<i64, &'a str>> {
    let mut stores = Vec::new();
    for &(address, name) in constructors {
        let rows = decoded(text, address)?;
        stores.push(ConstructorStores {
            registers: receiver_stores(&rows),
            owners: constructor_owners(name, owners),
        });
    }
    Some(sole_owners(&stores))
}

/// What one constructor stores in its object.
struct ConstructorStores<'a> {
    /// The parameter registers that it stores, by offset.
    registers: BTreeMap<i64, BTreeSet<usize>>,
    /// The owner that each of its parameter registers points at.
    owners: BTreeMap<usize, &'a str>,
}

/// By offset, the owner of each member word that some constructor fills with a parameter that
/// points at that owner and no constructor fills with another parameter.
fn sole_owners<'a>(constructors: &[ConstructorStores<'a>]) -> BTreeMap<i64, &'a str> {
    let mut stored: BTreeMap<i64, BTreeSet<Option<&'a str>>> = BTreeMap::new();
    for constructor in constructors {
        for (&offset, registers) in &constructor.registers {
            let owners = registers
                .iter()
                .map(|register| constructor.owners.get(register).copied());
            stored.entry(offset).or_default().extend(owners);
        }
    }

    stored
        .into_iter()
        .filter_map(
            |(offset, owners)| match owners.into_iter().collect::<Vec<_>>()[..] {
                [Some(owner)] => Some((offset, owner)),
                _ => None,
            },
        )
        .collect()
}

/// The owner that each parameter register of the constructor `name` points at.
fn constructor_owners<'a>(name: &str, owners: &BTreeSet<&'a str>) -> BTreeMap<usize, &'a str> {
    let Some((_, parameters)) = signature(name) else {
        return BTreeMap::new();
    };
    parameter_registers(parameters, true)
        .into_iter()
        .filter_map(|(register, parameter)| Some((register, *owners.get(pointee(parameter)?)?)))
        .collect()
}

/// The class of the constructor or destructor `name`, and whether it is the destructor.
fn constructed_class(name: &str) -> Option<(&str, bool)> {
    let (qualified, _) = signature(name)?;
    let class = qualifier(qualified)?;
    let method = &qualified[class.len() + 2..];
    let base = last_component(class);
    let base = base.split_once('<').map_or(base, |(base, _)| base);
    match method.strip_prefix('~') {
        Some(destructor) => (destructor == base).then_some((class, true)),
        None => (method == base).then_some((class, false)),
    }
}

/// The argument register of each parameter in `parameters`, up to the first that one general
/// register may not pass. A member function's receiver takes `x0`.
fn parameter_registers(parameters: &str, member: bool) -> Vec<(usize, &str)> {
    let mut register = usize::from(member);
    let mut found = Vec::new();
    for parameter in top_level(parameters).map(str::trim) {
        if matches!(parameter, "" | "void") {
            continue;
        }
        if register > 7 || !is_one_register(parameter) {
            break;
        }

        found.push((register, parameter));
        register += 1;
    }
    found
}

/// A pointer, a reference, an integer, `bool` or a `TPdxRef<…>`, which one general register
/// passes.
fn is_one_register(parameter: &str) -> bool {
    parameter.ends_with('*')
        || parameter.ends_with('&')
        || parameter.ends_with("* const")
        || parameter.starts_with("TPdxRef<")
        || INTEGERS.contains(&parameter)
}

/// The class that a parameter of type `O const*`, `O*`, `O const&` or `O&` points at.
fn pointee(parameter: &str) -> Option<&str> {
    let class = parameter
        .strip_suffix('*')
        .or_else(|| parameter.strip_suffix('&'))?;
    let class = class.strip_suffix(" const").unwrap_or(class);
    (!class.ends_with(['*', '&'])).then_some(class)
}

/// The qualifier of a qualified name, before its last top-level `::`: `A::B::F` gives `A::B`.
fn qualifier(qualified: &str) -> Option<&str> {
    let mut depth = 0usize;
    let mut last = None;
    for (at, character) in qualified.char_indices() {
        match character {
            '<' | '(' => depth += 1,
            '>' | ')' => depth = depth.saturating_sub(1),
            ':' if depth == 0 && qualified[at..].starts_with("::") => last = Some(at),
            _ => {}
        }
    }
    last.map(|at| &qualified[..at])
}

/// The last component of a qualifier: `A::B<C::D>` gives `B<C::D>`.
fn last_component(qualifier: &str) -> &str {
    match self::qualifier(qualifier) {
        Some(outer) => &qualifier[outer.len() + 2..],
        None => qualifier,
    }
}

/// Whether the demangled function `name` is a `const` member.
fn is_const(name: &str) -> bool {
    name.split(" [clone")
        .next()
        .is_some_and(|name| name.trim_end().ends_with(" const"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parameter_takes_the_next_register_until_one_that_a_register_may_not_pass() {
        let (_, parameters) = signature(
            "CMission::CMission(TPdxRef<CCountry>, CMissionType const*, CEventScope const&, CMetaRef<CMissionType>)",
        )
        .unwrap();
        assert_eq!(
            parameter_registers(parameters, true),
            [
                (1, "TPdxRef<CCountry>"),
                (2, "CMissionType const*"),
                (3, "CEventScope const&"),
            ]
        );

        let (_, parameters) =
            signature("PerformEvent(CString const&, CCountry*, CCountry*)").unwrap();
        assert_eq!(
            parameter_registers(parameters, false),
            [(0, "CString const&"), (1, "CCountry*"), (2, "CCountry*")]
        );

        let (_, parameters) = signature("CCountry::AddEdict(CEdict const*)").unwrap();
        assert_eq!(
            parameter_registers(parameters, true),
            [(1, "CEdict const*")]
        );

        let (_, parameters) =
            signature("CShip::SetSpeed(CFixedPoint, CEdict const*, float, int)").unwrap();
        assert!(parameter_registers(parameters, true).is_empty());
    }

    #[test]
    fn a_pointer_or_reference_parameter_points_at_its_class() {
        assert_eq!(pointee("CEdict const*"), Some("CEdict"));
        assert_eq!(pointee("CResolution&"), Some("CResolution"));
        assert_eq!(pointee("CEdict**"), None);
        assert_eq!(pointee("CEventScope&&"), None);
        assert_eq!(pointee("int"), None);
    }

    #[test]
    fn a_member_of_a_nested_class_is_not_a_member_of_the_enclosing_class() {
        let owners = BTreeSet::from(["CAnomalyType", "CEdict"]);
        let reader = TypeOwners {
            owners: &owners,
            classes: BTreeSet::from(["CAnomalyType", "CAnomalyType::COutcomeEffect", "CCountry"]),
            members: BTreeMap::new(),
        };

        assert_eq!(
            reader.function("CAnomalyType::COutcomeEffect::Execute(CEventScope&) const"),
            None
        );
        assert_eq!(
            reader.function("CAnomalyType::OnSuccess(CCountry*, CEventScope&) const"),
            Some(TypePointers {
                method_of: Some("CAnomalyType".into()),
                registers: BTreeMap::from([(0, "CAnomalyType".into())]),
                members: BTreeMap::new(),
            })
        );
        assert_eq!(
            reader.function("CCountry::AddEdict(CEdict const*)"),
            Some(TypePointers {
                method_of: None,
                registers: BTreeMap::from([(1, "CEdict".into())]),
                members: BTreeMap::new(),
            })
        );
        assert_eq!(reader.function("NUnknown::Run(CEdict const*)"), None);
        assert_eq!(
            reader.function("Run(CEdict const*)"),
            Some(TypePointers {
                method_of: None,
                registers: BTreeMap::from([(0, "CEdict".into())]),
                members: BTreeMap::new(),
            })
        );
    }

    #[test]
    fn a_member_word_leads_through_the_register_of_its_class() {
        let owners = BTreeSet::from(["CMissionType"]);
        let reader = TypeOwners {
            owners: &owners,
            classes: BTreeSet::from(["CMission"]),
            members: BTreeMap::from([("CMission", BTreeMap::from([(0x18, "CMissionType")]))]),
        };

        assert_eq!(
            reader.function("CMission::Start()"),
            Some(TypePointers {
                method_of: None,
                registers: BTreeMap::new(),
                members: BTreeMap::from([((0, 0x18), "CMissionType".into())]),
            })
        );
        assert_eq!(
            reader.function("CGalacticCommunity::Pass(CMission&, bool)"),
            None,
            "CGalacticCommunity is not a known class"
        );
    }

    #[test]
    fn a_member_word_leads_to_the_one_owner_that_its_constructors_store() {
        let constructor = |stores: &[(i64, usize)], owners: &[(usize, &'static str)]| {
            let mut stored: BTreeMap<i64, BTreeSet<usize>> = BTreeMap::new();
            for &(offset, register) in stores {
                stored.entry(offset).or_default().insert(register);
            }
            ConstructorStores {
                registers: stored,
                owners: owners.iter().copied().collect(),
            }
        };

        let mission = [
            constructor(&[(0x18, 2)], &[(2, "CMissionType")]),
            constructor(&[(0x18, 3)], &[(3, "CMissionType")]),
        ];
        assert_eq!(
            sole_owners(&mission),
            BTreeMap::from([(0x18, "CMissionType")])
        );

        let two_owners = [
            constructor(&[(0x18, 1)], &[(1, "CMissionType")]),
            constructor(&[(0x18, 1)], &[(1, "CEdict")]),
        ];
        assert!(sole_owners(&two_owners).is_empty());

        let another_type = [
            constructor(&[(0x18, 1)], &[(1, "CMissionType")]),
            constructor(&[(0x18, 2)], &[]),
        ];
        assert!(sole_owners(&another_type).is_empty());
    }

    #[test]
    fn a_constructor_or_destructor_names_its_class() {
        assert_eq!(
            constructed_class("CMission::CMission(TPdxRef<CCountry>)"),
            Some(("CMission", false))
        );
        assert_eq!(
            constructed_class("CAnomalyType::COutcomeEffect::~COutcomeEffect()"),
            Some(("CAnomalyType::COutcomeEffect", true))
        );
        assert_eq!(
            constructed_class("TPdxRef<CCountry>::TPdxRef(int)"),
            Some(("TPdxRef<CCountry>", false))
        );
        assert_eq!(constructed_class("CMission::Start()"), None);
    }
}
