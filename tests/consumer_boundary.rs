//! A source-level boundary check for Atlas's frozen Native consumer.

use std::path::{Path, PathBuf};
use syn::{
    Attribute, ExprLit, ExprMethodCall, ExprPath, ExprUnsafe, ItemFn, ItemUse, Lit, LitInt, LitStr,
    Macro, UseTree, visit::Visit,
};

const EXPORTS: &[&str] = &[
    "Answer",
    "Basis",
    "BuildId",
    "Completeness",
    "Disposal",
    "Error",
    "Field",
    "Gap",
    "GapKind",
    "Operation",
    "Reader",
    "ReaderId",
    "ReaderKind",
    "Registry",
    "Source",
    "Support",
    "OpenError",
    "GameReadiness",
    "DiagnosticCoverage",
    "DiagnosticJoin",
    "DiagnosticWindow",
    "FieldRead",
    "FixtureDiagnostic",
    "FixtureFieldOutcome",
    "FixtureFieldQuestion",
    "FixtureObservation",
    "FixtureObservationKind",
    "FixtureOwnerId",
    "FixtureRequest",
    "FixtureRuntime",
    "FixtureStorage",
    "FixtureWindow",
    "ProcessingStage",
    "RegistrationEntry",
    "StoredStringOccurrence",
    "Game",
    "GameOptions",
    "Native",
    "supervisor",
];

#[derive(Debug)]
struct Violation {
    file: PathBuf,
    rule: &'static str,
    detail: String,
}

struct Checker {
    file: PathBuf,
    violations: Vec<Violation>,
}

impl Checker {
    fn reject(&mut self, rule: &'static str, detail: impl Into<String>) {
        self.violations.push(Violation {
            file: self.file.clone(),
            rule,
            detail: detail.into(),
        });
    }

    fn check_export(&mut self, name: &str) {
        if !EXPORTS.contains(&name) {
            self.reject("unsupported export", name);
        }
    }

    fn check_use_tree(&mut self, tree: &UseTree, native: bool) {
        match tree {
            UseTree::Path(path) if path.ident == "pdx_native" => {
                self.check_use_tree(&path.tree, true);
            }
            UseTree::Path(path) if native => {
                self.check_export(&path.ident.to_string());
                if path.ident == "supervisor" {
                    self.check_supervisor_tree(&path.tree);
                }
            }
            UseTree::Group(group) => {
                for tree in &group.items {
                    self.check_use_tree(tree, native);
                }
            }
            UseTree::Name(name) if native => self.check_export(&name.ident.to_string()),
            UseTree::Rename(rename) if native => self.check_export(&rename.ident.to_string()),
            UseTree::Rename(rename) if rename.ident == "pdx_native" => {
                self.reject(
                    "unsupported export",
                    "renaming pdx_native hides its imports",
                );
            }
            UseTree::Glob(_) if native => self.reject("unsupported export", "glob import"),
            _ => {}
        }
    }

    fn check_supervisor_tree(&mut self, tree: &UseTree) {
        match tree {
            UseTree::Name(name)
                if matches!(name.ident.to_string().as_str(), "serve" | "SupervisorError") => {}
            UseTree::Group(group) => {
                for item in &group.items {
                    self.check_supervisor_tree(item);
                }
            }
            _ => self.reject(
                "unsupported supervisor export",
                "only serve and SupervisorError are supported",
            ),
        }
    }

    fn check_literal(&mut self, literal: &Lit) {
        match literal {
            Lit::Int(value) if is_native_integer(value) => {
                self.reject("native constant", value.to_string());
            }
            Lit::Str(value) if is_build_literal(value) => {
                self.reject("build/version literal", value.value());
            }
            _ => {}
        }
    }
}

impl<'ast> Visit<'ast> for Checker {
    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        self.check_use_tree(&item.tree, false);
        syn::visit::visit_item_use(self, item);
    }

    fn visit_attribute(&mut self, attribute: &'ast Attribute) {
        if attribute.path().is_ident("cfg")
            && let syn::Meta::List(list) = &attribute.meta
        {
            let text = list.tokens.to_string();
            if text.contains("target_os") || text.contains("target_arch") {
                self.reject("platform branch", text);
            }
        }
        syn::visit::visit_attribute(self, attribute);
    }

    fn visit_macro(&mut self, item: &'ast Macro) {
        if item.path.is_ident("cfg") {
            self.reject("platform branch", "cfg! macro");
        }
        syn::visit::visit_macro(self, item);
    }

    fn visit_expr_method_call(&mut self, expression: &'ast ExprMethodCall) {
        if matches!(
            expression.method.to_string().as_str(),
            "fault" | "fixture_fault"
        ) {
            self.reject("hidden test hook", expression.method.to_string());
        }
        syn::visit::visit_expr_method_call(self, expression);
    }

    fn visit_expr_path(&mut self, expression: &'ast ExprPath) {
        check_path(self, &expression.path);
        syn::visit::visit_expr_path(self, expression);
    }

    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        if matches!(&item.sig.safety, syn::Safety::Unsafe(_)) {
            self.reject("native mechanism", "unsafe function");
        }
        syn::visit::visit_item_fn(self, item);
    }

    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        if item.ident == "pdx_native" && item.rename.is_some() {
            self.reject(
                "unsupported export",
                "renaming pdx_native hides its imports",
            );
        }
        syn::visit::visit_item_extern_crate(self, item);
    }

    fn visit_expr_unsafe(&mut self, item: &'ast ExprUnsafe) {
        self.reject("native mechanism", "unsafe block");
        syn::visit::visit_expr_unsafe(self, item);
    }

    fn visit_expr_lit(&mut self, item: &'ast ExprLit) {
        self.check_literal(&item.lit);
        syn::visit::visit_expr_lit(self, item);
    }
}

fn check_path(checker: &mut Checker, path: &syn::Path) {
    let segments: Vec<_> = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    if segments.first().map(String::as_str) == Some("pdx_native")
        && let Some(name) = segments.get(1)
    {
        checker.check_export(name);
        if name == "supervisor"
            && segments.len() > 2
            && !matches!(segments[2].as_str(), "serve" | "SupervisorError")
        {
            checker.reject("unsupported supervisor export", &segments[2]);
        }
    }
    if segments.len() >= 3
        && segments[0] == "std"
        && segments[1] == "env"
        && segments[2] == "consts"
    {
        checker.reject("platform branch", "std::env::consts");
    }
    if segments.iter().any(|name| name == "ObservationControl") {
        checker.reject("hidden test hook", "ObservationControl");
    }
}

fn is_native_integer(value: &LitInt) -> bool {
    let text = value.to_string();
    let digits = text.strip_prefix("0x").unwrap_or("");
    digits
        .chars()
        .take_while(|character| character.is_ascii_hexdigit() || *character == '_')
        .filter(|character| *character != '_')
        .count()
        >= 4
}

fn is_build_literal(value: &LitStr) -> bool {
    let text = value.value();
    let version = text.split('.').count() >= 2
        && text
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
        && text.chars().any(|character| character == '.');
    let hash = text.len() == 64 && text.chars().all(|character| character.is_ascii_hexdigit());
    version || hash
}

fn violations(file: &Path, source: &str) -> Vec<Violation> {
    let syntax = syn::parse_file(source).expect("caller source is Rust");
    let mut checker = Checker {
        file: file.into(),
        violations: Vec::new(),
    };
    checker.visit_file(&syntax);
    checker.violations
}

fn scan(root: &Path) -> Vec<Violation> {
    let mut found = Vec::new();
    for directory in ["src", "tests"] {
        visit_files(&root.join(directory), &mut found);
    }
    found
}

fn visit_files(path: &Path, found: &mut Vec<Violation>) {
    if !path.exists() {
        return;
    }
    for entry in std::fs::read_dir(path).expect("read caller directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            visit_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let source = std::fs::read_to_string(&path).expect("read caller source");
            found.extend(violations(&path, &source));
        }
    }
}

#[test]
fn boundary_rules_accept_public_calls_and_reject_hidden_details() {
    let cases = [
        (
            "use pdx_native::{Native, supervisor::serve}; fn f() {}",
            true,
        ),
        ("use pdx_native::{Native, internals};", false),
        ("use pdx_native as native; fn f() {}", false),
        ("extern crate pdx_native as native; fn f() {}", false),
        ("fn f() { pdx_native::internals::x(); }", false),
        ("fn f() { let _ = pdx_native::supervisor::serve; }", true),
        ("fn f() { let _ = pdx_native::supervisor::hidden; }", false),
        ("fn f() { let _ = 0x1234; }", false),
        ("fn f() { let _ = 0x12; }", true),
        ("fn f() { let _ = \"4.5\"; }", false),
        ("fn f() { let _ = \"not a version\"; }", true),
        ("#[cfg(target_os = \"macos\")] fn f() {}", false),
        ("fn f() { let _ = cfg!(target_arch = \"aarch64\"); }", false),
        ("fn f() { let _ = std::env::consts::OS; }", false),
        ("fn f() { x.fault(); }", false),
        ("fn f() { x.fixture_fault(); }", false),
        (
            "fn f() { let _ = pdx_native::Operation::Registries; }",
            true,
        ),
        ("// pdx_native::internals\nfn f() {}", true),
    ];
    for (source, accepted) in cases {
        assert_eq!(
            violations(Path::new("case.rs"), source).is_empty(),
            accepted,
            "{source}"
        );
    }
}

#[test]
#[ignore = "set ATLAS_CALLER_PATH to the frozen Atlas caller"]
fn frozen_atlas_caller_uses_only_supported_exports() {
    let root = std::env::var_os("ATLAS_CALLER_PATH").expect("ATLAS_CALLER_PATH is required");
    let found = scan(Path::new(&root));
    let descriptions: Vec<_> = found
        .iter()
        .map(|item| format!("{}: {}: {}", item.file.display(), item.rule, item.detail))
        .collect();
    assert!(found.is_empty(), "{}", descriptions.join("\n"));
}
