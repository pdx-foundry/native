//! Shared exact-name selection for scoped numeric and duration measurements.
use pdx_native::internals::command_grammar_stops::{self, Run};
use pdx_native::{DeclarationKind, Error, Native};
use std::collections::BTreeSet;

/// CLI syntax shared by both population examples.
pub const USAGE: &str = "[--command Effect/NAME | --command Trigger/NAME | --registry DIRECTORY]...\n\
    With no filters, measure every command and registry. Filters select their union.";

/// Exact command and registry names to measure. The default selects the full population;
/// supplied filters select only the union of the requested commands and registries.
#[derive(Debug, Default)]
pub struct Selection {
    commands: Vec<(DeclarationKind, String)>,
    registries: BTreeSet<String>,
}

impl Selection {
    /// Parse repeatable exact names. An empty selection means the whole population.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut selection = Self::default();
        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            if !matches!(flag.as_str(), "--command" | "--registry") {
                return Err(format!("unknown option {flag}; usage: {USAGE}"));
            }
            let name = args
                .next()
                .filter(|name| !name.is_empty() && !name.starts_with('-'))
                .ok_or_else(|| format!("missing value for {flag}"))?;
            if flag == "--registry" {
                selection.registries.insert(name);
                continue;
            }
            let (kind, name) = name
                .split_once('/')
                .ok_or("expected Effect/NAME or Trigger/NAME")?;
            let kind = match kind {
                "Effect" => DeclarationKind::Effect,
                "Trigger" => DeclarationKind::Trigger,
                _ => return Err("expected Effect/NAME or Trigger/NAME".into()),
            };
            if name.is_empty() || name.contains('/') {
                return Err("expected a nonempty command name without '/'".into());
            }
            let command = (kind, name.into());
            if !selection.commands.contains(&command) {
                selection.commands.push(command);
            }
        }
        Ok(selection)
    }

    fn all(&self) -> bool {
        self.commands.is_empty() && self.registries.is_empty()
    }

    /// Select registry names and reject typos before running any registry method.
    /// A command-only selection returns no registries.
    pub fn registries<'a>(
        &self,
        names: impl IntoIterator<Item = &'a str>,
    ) -> Result<Vec<&'a str>, String> {
        let names: Vec<_> = names.into_iter().collect();
        for requested in &self.registries {
            if !names.contains(&requested.as_str()) {
                return Err(format!("unknown registry: {requested}"));
            }
        }
        Ok(names
            .into_iter()
            .filter(|name| self.all() || self.registries.contains(*name))
            .collect())
    }

    /// Visit only requested commands, or use the shared full-inventory walk without filters.
    pub fn visit_commands(
        &self,
        native: &Native,
        mut visit: impl FnMut(String, Run),
    ) -> Result<(), Error> {
        if self.all() {
            for kind in [DeclarationKind::Effect, DeclarationKind::Trigger] {
                command_grammar_stops::population(native, kind, |name, run| {
                    visit(format!("{kind:?}/{name}"), run);
                })?;
            }
        } else {
            for (kind, name) in &self.commands {
                let run = command_grammar_stops::run(native, *kind, name)?;
                visit(format!("{kind:?}/{name}"), run);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(args: &[&str]) -> Selection {
        Selection::parse(args.iter().map(|arg| (*arg).into())).unwrap()
    }

    #[test]
    fn filters_select_a_union_and_deduplicate_names() {
        let all = selection(&[]);
        assert!(all.all());
        assert_eq!(all.registries(["one", "two"]).unwrap(), ["one", "two"]);
        let commands = selection(&["--command", "Effect/test", "--command", "Effect/test"]);
        assert_eq!(commands.commands.len(), 1);
        assert!(commands.registries(["one"]).unwrap().is_empty());
        let mixed = selection(&["--command", "Trigger/test", "--registry", "two"]);
        assert_eq!(mixed.registries(["one", "two"]).unwrap(), ["two"]);
        let registries = selection(&["--registry", "two"]);
        assert!(registries.commands.is_empty());
        assert_eq!(registries.registries(["one", "two"]).unwrap(), ["two"]);
        assert!(registries.registries(["one"]).is_err());
    }

    #[test]
    fn invalid_filters_fail_instead_of_silently_measuring_everything() {
        for args in [
            vec!["--other"],
            vec!["--command"],
            vec!["--registry", ""],
            vec!["--command", "effect/test"],
            vec!["--command", "Effect/"],
            vec!["--command", "Effect/test/extra"],
            vec!["--registry", "--command"],
        ] {
            assert!(Selection::parse(args.into_iter().map(String::from)).is_err());
        }
    }
}
