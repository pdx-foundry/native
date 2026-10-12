//! The stated definition-name rules of the loaders of a registry.
//!
//! **Recorded manual exception (stated definition names).** Claim: a template loader
//! (`TSingleObjectGameDatabase<…>`) names each root definition by its key; the events loader
//! names a `namespace = x` statement by its value and every other accepted root statement by its
//! `id` field; a tradition item names each `tradition_swap` block by its `name` field.
//! Conditions: the functions that each rule names are bound on the build; otherwise that rule's
//! form is missing. Obstacle: the name reaches the item, the namespace table or the swap's member
//! through stack copies, inlined construction and a later lookup, which no method follows.
//! Removal route: a reader that follows a key or field value into the member that the loader or
//! a lookup compares. `docs/native/loader-rules.md` records the disassembly and the checks.
use std::collections::BTreeSet;

use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::loader_rules::StatedForm;
use crate::{DefinitionForm, DefinitionName, DefinitionStatement};

/// A template loader's function that reads the root statements of one file.
const LOAD_FROM_READER: &str = "::LoadFromReader(CReader&, bool)";

/// The events loader's database, and the functions that register a namespace by its value and
/// an event by its id.
const EVENT_MANAGER: &str = "CEventManager";
const EVENT_NAMESPACE_RULE: [&str; 2] = [
    "CEventManager::ReadEventsFile(CReader&, int&, bool)",
    "CHashTable<CString, CNameSpace, ClassicHashKeyTraits<CString, CNameSpace> >::Find(CString const&)",
];
const EVENT_ID_RULE: [&str; 3] = [
    "CEventManager::ReadEventsFile(CReader&, int&, bool)",
    "CEvent::ReadMember(CReader&, int)",
    "CEventManager::GetIDInNameSpace(CString const&) const",
];

/// The item class that holds tradition swaps, and the functions that read a swap and its name.
const TRADITION: &str = "CTraditionType";
const TRADITION_SWAP_RULE: [&str; 2] = [
    "CTraditionType::ReadMember(CReader&, int)",
    "CTraditionSwap::ReadMember(CReader&, int)",
];

/// A template loader of the registry: its loader class and its item class.
pub(in crate::binding) struct TemplateLoader<'a> {
    pub loader_class: &'a str,
    pub item: &'a str,
}

/// The stated forms of the registry whose loaders are `templates` and the scheduled `databases`,
/// in rule order: root definitions first, then the forms they hold.
pub(in crate::binding) fn stated_forms(
    symbols: &[Symbol],
    templates: &[TemplateLoader<'_>],
    databases: &BTreeSet<String>,
) -> Vec<StatedForm> {
    let bound: BTreeSet<&str> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();
    let holds = |functions: &[&str]| functions.iter().all(|name| bound.contains(name));
    let mut stated = Vec::new();

    for template in templates {
        let loader = format!("{}{LOAD_FROM_READER}", template.loader_class);
        stated
            .push(holds(&[&loader]).then(|| form(DefinitionStatement::Root, DefinitionName::Key)));

        if template.item == TRADITION {
            stated.push(holds(&TRADITION_SWAP_RULE).then(tradition_swap));
        }
    }

    if databases.contains(EVENT_MANAGER) {
        stated.push(holds(&EVENT_NAMESPACE_RULE).then(event_namespace));
        stated.push(holds(&EVENT_ID_RULE).then(event));
    }

    stated
}

fn event_namespace() -> DefinitionForm {
    form(
        DefinitionStatement::RootKey("namespace".into()),
        DefinitionName::Value,
    )
}

fn event() -> DefinitionForm {
    form(
        DefinitionStatement::Root,
        DefinitionName::Field("id".into()),
    )
}

fn tradition_swap() -> DefinitionForm {
    form(
        DefinitionStatement::Nested {
            field_path: vec!["tradition_swap".into()],
        },
        DefinitionName::Field("name".into()),
    )
}

fn form(statement: DefinitionStatement, name: DefinitionName) -> DefinitionForm {
    DefinitionForm { statement, name }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOADER: &str = "TSingleObjectGameDatabase<CTraditionTypeDatabase, CTraditionType, false>";

    fn symbols(names: &[&str]) -> Vec<Symbol> {
        names
            .iter()
            .enumerate()
            .map(|(index, name)| Symbol {
                name: (*name).to_owned(),
                address: 0x1000 + index as u64 * 4,
            })
            .collect()
    }

    fn template(item: &str) -> TemplateLoader<'_> {
        TemplateLoader {
            loader_class: LOADER,
            item,
        }
    }

    fn load_from_reader() -> String {
        format!("{LOADER}{LOAD_FROM_READER}")
    }

    #[test]
    fn a_template_loader_names_root_definitions_by_their_key() {
        let symbols = symbols(&[&load_from_reader()]);

        let stated = stated_forms(&symbols, &[template("CExample")], &BTreeSet::new());

        assert_eq!(
            stated,
            [Some(form(DefinitionStatement::Root, DefinitionName::Key))]
        );
    }

    #[test]
    fn a_template_loader_without_its_reader_states_no_form() {
        let stated = stated_forms(&[], &[template("CExample")], &BTreeSet::new());

        assert_eq!(stated, [None]);
    }

    #[test]
    fn a_tradition_item_names_its_swaps_by_their_name_field() {
        let loader = load_from_reader();
        let mut names = vec![loader.as_str()];
        names.extend(TRADITION_SWAP_RULE);

        let stated = stated_forms(&symbols(&names), &[template(TRADITION)], &BTreeSet::new());

        assert_eq!(
            stated,
            [
                Some(form(DefinitionStatement::Root, DefinitionName::Key)),
                Some(tradition_swap()),
            ]
        );
    }

    #[test]
    fn a_tradition_item_without_the_swap_reader_misses_the_swap_form() {
        let loader = load_from_reader();
        let symbols = symbols(&[&loader, TRADITION_SWAP_RULE[0]]);

        let stated = stated_forms(&symbols, &[template(TRADITION)], &BTreeSet::new());

        assert_eq!(stated[1], None);
    }

    fn event_functions() -> Vec<Symbol> {
        let mut names = EVENT_NAMESPACE_RULE.to_vec();
        names.extend(EVENT_ID_RULE);

        symbols(&names)
    }

    #[test]
    fn the_events_loader_names_namespaces_by_value_and_events_by_id() {
        let databases = BTreeSet::from([EVENT_MANAGER.to_owned()]);

        let stated = stated_forms(&event_functions(), &[], &databases);

        assert_eq!(stated, [Some(event_namespace()), Some(event())]);
    }

    #[test]
    fn the_events_loader_without_the_id_reader_misses_only_the_event_form() {
        let databases = BTreeSet::from([EVENT_MANAGER.to_owned()]);

        let stated = stated_forms(&symbols(&EVENT_NAMESPACE_RULE), &[], &databases);

        assert_eq!(stated, [Some(event_namespace()), None]);
    }

    #[test]
    fn the_events_loader_without_the_namespace_table_misses_only_the_namespace_form() {
        let databases = BTreeSet::from([EVENT_MANAGER.to_owned()]);

        let stated = stated_forms(&symbols(&EVENT_ID_RULE), &[], &databases);

        assert_eq!(stated, [None, Some(event())]);
    }

    #[test]
    fn another_custom_loader_states_no_form() {
        let databases = BTreeSet::from(["CTraitDatabase".to_owned()]);

        assert!(stated_forms(&event_functions(), &[], &databases).is_empty());
    }
}
