//! A source-level boundary check for Atlas's Native consumer.

#[path = "support/source.rs"]
mod source;

use proc_macro2::{TokenStream, TokenTree};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use syn::{
    Attribute, ExprLit, ExprMethodCall, ExprUnsafe, ItemForeignMod, ItemImpl, ItemTrait, ItemUse,
    Lit, LitInt, Macro, Meta, Signature, UseTree, parse::Parser, punctuated::Punctuated,
    visit::Visit,
};

const EXPORTS: &[&str] = &[
    "Answer",
    "Basis",
    "BuildId",
    "Completeness",
    "ContextScopes",
    "Declaration",
    "DeclarationKind",
    "DeclaredScopes",
    "DeclaredTags",
    "Define",
    "DefineValueType",
    "Disposal",
    "EntryContext",
    "EntryScope",
    "Error",
    "Field",
    "Gap",
    "GameRule",
    "GapKind",
    "GapSubject",
    "GeneratedName",
    "GenerationCondition",
    "LinkData",
    "LoadedContent",
    "LoadedModifier",
    "LoadedModifiers",
    "LocalizationCommand",
    "LocalizationContext",
    "LocalizationContextId",
    "LocalizationContextReference",
    "LocalizationDeclarations",
    "LocalizationLink",
    "LocalizationOutput",
    "ModifierCategory",
    "ModifierDeclaration",
    "ModifierFamily",
    "NamePart",
    "OnAction",
    "Operation",
    "OutputScope",
    "Reader",
    "ReaderId",
    "ReaderKind",
    "Registry",
    "RuleKind",
    "ScopeDeclaration",
    "ScopeGroup",
    "ScopeId",
    "ScopeInventory",
    "ScopeLink",
    "ScopeReference",
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

/// The `pdx_native::supervisor` members that Atlas may use.
const SUPERVISOR_EXPORTS: &[&str] = &["serve", "SupervisorError"];

/// The hidden `GameOptions` methods that inject observation faults for Native's own tests.
const FAULT_HOOKS: &[&str] = &["fault", "fixture_fault", "modifier_fault"];

/// A fault hook or one of its hidden control types.
fn is_hidden_test_hook(name: &str) -> bool {
    FAULT_HOOKS.contains(&name) || matches!(name, "ObservationControl" | "ObservationTarget")
}

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
                if SUPERVISOR_EXPORTS.contains(&name.ident.to_string().as_str()) => {}
            UseTree::Rename(rename)
                if SUPERVISOR_EXPORTS.contains(&rename.ident.to_string().as_str()) => {}
            UseTree::Group(group) => {
                for item in &group.items {
                    self.check_supervisor_tree(item);
                }
            }
            _ => self.reject(
                "unsupported supervisor export",
                format!("only {} are supported", SUPERVISOR_EXPORTS.join(" and ")),
            ),
        }
    }

    fn check_literal(&mut self, literal: &Lit) {
        match literal {
            Lit::Int(value) if is_native_integer(value) => {
                self.reject("native constant", value.to_string());
            }
            Lit::Str(value) if source::has_build_shape(&value.value()) => {
                self.reject("build/version literal", value.value());
            }
            _ => {}
        }
    }

    fn scan_macro_tokens(&mut self, tokens: TokenStream) {
        let tokens: Vec<_> = tokens.into_iter().collect();
        for (index, token) in tokens.iter().enumerate() {
            match token {
                TokenTree::Group(group) => self.scan_macro_tokens(group.stream()),
                TokenTree::Literal(literal) => {
                    if let Ok(value) = syn::parse_str::<Lit>(&literal.to_string()) {
                        self.check_literal(&value);
                    }
                }
                TokenTree::Ident(ident) => {
                    let name = ident.to_string();
                    if is_hidden_test_hook(&name) {
                        self.reject("hidden test hook", &name);
                    }
                    if name == "unsafe" {
                        self.reject("native mechanism", "unsafe macro token");
                    }
                    if name == "pdx_native" && is_path_separator(&tokens, index + 1) {
                        match tokens.get(index + 3) {
                            Some(TokenTree::Ident(export)) => {
                                self.check_export(&export.to_string());
                                if export == "supervisor"
                                    && let Some(member) = path_ident(&tokens, index + 6)
                                    && !SUPERVISOR_EXPORTS.contains(&member.as_str())
                                {
                                    self.reject("unsupported supervisor export", member);
                                }
                            }
                            Some(TokenTree::Group(_)) => self.reject(
                                "unsupported export",
                                "grouped Native imports inside macros are not checked",
                            ),
                            _ => {}
                        }
                    }
                    if name == "std"
                        && path_ident(&tokens, index + 3).as_deref() == Some("env")
                        && path_ident(&tokens, index + 6).as_deref() == Some("consts")
                    {
                        self.reject("platform branch", "std::env::consts in macro");
                    }
                    if name == "cfg"
                        && matches!(tokens.get(index + 1), Some(TokenTree::Punct(p)) if p.as_char() == '!')
                    {
                        self.reject("platform branch", "cfg! macro token");
                    }
                }
                TokenTree::Punct(_) => {}
            }
        }
    }
}

fn is_path_separator(tokens: &[TokenTree], start: usize) -> bool {
    matches!(tokens.get(start), Some(TokenTree::Punct(p)) if p.as_char() == ':')
        && matches!(tokens.get(start + 1), Some(TokenTree::Punct(p)) if p.as_char() == ':')
}

fn path_ident(tokens: &[TokenTree], start: usize) -> Option<String> {
    if !is_path_separator(tokens, start - 2) {
        return None;
    }
    match tokens.get(start) {
        Some(TokenTree::Ident(ident)) => Some(ident.to_string()),
        _ => None,
    }
}

impl<'ast> Visit<'ast> for Checker {
    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        self.check_use_tree(&item.tree, false);
        if imports_platform_consts(&item.tree, 0) {
            self.reject("platform branch", "std::env::consts import");
        }
        syn::visit::visit_item_use(self, item);
    }

    fn visit_attribute(&mut self, attribute: &'ast Attribute) {
        if (attribute.path().is_ident("cfg") || attribute.path().is_ident("cfg_attr"))
            && is_platform_meta(&attribute.meta)
        {
            self.reject("platform branch", "cfg/cfg_attr predicate");
        }
        syn::visit::visit_attribute(self, attribute);
    }

    fn visit_macro(&mut self, item: &'ast Macro) {
        if item.path.is_ident("cfg") {
            self.reject("platform branch", "cfg! macro");
        }
        self.scan_macro_tokens(item.tokens.clone());
        syn::visit::visit_macro(self, item);
    }

    fn visit_expr_method_call(&mut self, expression: &'ast ExprMethodCall) {
        let method = expression.method.to_string();
        if FAULT_HOOKS.contains(&method.as_str()) {
            self.reject("hidden test hook", method);
        }
        syn::visit::visit_expr_method_call(self, expression);
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        check_path(self, path);
        syn::visit::visit_path(self, path);
    }

    fn visit_signature(&mut self, signature: &'ast Signature) {
        if matches!(&signature.safety, syn::Safety::Unsafe(_)) {
            self.reject("native mechanism", "unsafe function");
        }
        syn::visit::visit_signature(self, signature);
    }

    fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
        if item.unsafety.is_some() {
            self.reject("native mechanism", "unsafe trait");
        }
        syn::visit::visit_item_trait(self, item);
    }

    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        if item.unsafety.is_some() {
            self.reject("native mechanism", "unsafe impl");
        }
        syn::visit::visit_item_impl(self, item);
    }

    fn visit_item_foreign_mod(&mut self, item: &'ast ItemForeignMod) {
        if item.unsafety.is_some() {
            self.reject("native mechanism", "unsafe extern block");
        }
        syn::visit::visit_item_foreign_mod(self, item);
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
            && !SUPERVISOR_EXPORTS.contains(&segments[2].as_str())
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
    for name in &segments {
        if is_hidden_test_hook(name) {
            checker.reject("hidden test hook", name);
        }
    }
}

fn is_platform_predicate(word: &str) -> bool {
    matches!(
        word,
        "windows"
            | "unix"
            | "target_os"
            | "target_arch"
            | "target_family"
            | "target_env"
            | "target_vendor"
            | "target_pointer_width"
            | "target_endian"
            | "target_feature"
            | "target_abi"
    )
}

fn is_platform_meta(meta: &Meta) -> bool {
    let path = meta.path();
    if path
        .get_ident()
        .is_some_and(|ident| is_platform_predicate(&ident.to_string()))
    {
        return true;
    }
    let Meta::List(list) = meta else {
        return false;
    };
    let parser = Punctuated::<Meta, syn::Token![,]>::parse_terminated;
    match parser.parse2(list.tokens.clone()) {
        Ok(items) => items.iter().any(is_platform_meta),
        Err(_) => true,
    }
}

fn imports_platform_consts(tree: &UseTree, depth: usize) -> bool {
    match tree {
        UseTree::Path(path) => {
            let expected = ["std", "env", "consts"];
            if depth < expected.len() && path.ident == expected[depth] {
                if depth == expected.len() - 1 {
                    true
                } else {
                    imports_platform_consts(&path.tree, depth + 1)
                }
            } else {
                false
            }
        }
        UseTree::Group(group) => group
            .items
            .iter()
            .any(|item| imports_platform_consts(item, depth)),
        UseTree::Name(name) if depth == 2 => name.ident == "consts",
        UseTree::Rename(rename) if depth == 2 => rename.ident == "consts",
        _ => false,
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

fn violations(file: &Path, source: &str) -> Vec<Violation> {
    let syntax = syn::parse_file(source).expect("caller source is Rust");
    let mut checker = Checker {
        file: file.into(),
        violations: Vec::new(),
    };
    checker.visit_file(&syntax);
    checker.violations
}

fn scan(root: &Path, include_tests: bool) -> Vec<Violation> {
    assert!(root.is_dir(), "caller root is missing: {}", root.display());
    assert!(
        root.join("src").is_dir(),
        "caller src directory is missing: {}",
        root.display()
    );
    let mut found = Vec::new();
    let mut scanned = 0;
    visit_files(&root.join("src"), &mut found, &mut scanned);
    if include_tests {
        visit_files(&root.join("tests"), &mut found, &mut scanned);
    }
    assert!(
        scanned > 0,
        "no Rust caller source found: {}",
        root.display()
    );
    found.extend(dependency_aliases(root, include_tests));
    found
}

fn visit_files(path: &Path, found: &mut Vec<Violation>, scanned: &mut usize) {
    if !path.exists() {
        return;
    }
    for entry in std::fs::read_dir(path).expect("read caller directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            visit_files(&path, found, scanned);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            *scanned += 1;
            let source = std::fs::read_to_string(&path).expect("read caller source");
            found.extend(violations(&path, &source));
        }
    }
}

fn dependency_aliases(root: &Path, include_tests: bool) -> Vec<Violation> {
    let manifest = root.join("Cargo.toml");
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--locked", "--format-version", "1"])
        .arg("--manifest-path")
        .arg(&manifest)
        .output()
        .expect("run cargo metadata for caller");
    assert!(
        output.status.success(),
        "caller metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse caller metadata");
    let manifest = manifest.canonicalize().expect("canonical caller manifest");
    aliases_in_metadata(&metadata, &manifest, include_tests)
}

fn aliases_in_metadata(
    metadata: &serde_json::Value,
    manifest: &Path,
    include_tests: bool,
) -> Vec<Violation> {
    metadata["packages"]
        .as_array()
        .expect("metadata packages")
        .iter()
        .filter(|package| {
            package["manifest_path"]
                .as_str()
                .is_some_and(|path| Path::new(path) == manifest)
        })
        .flat_map(|package| package["dependencies"].as_array().into_iter().flatten())
        .filter(|dependency| dependency["name"] == "pdx-native")
        .filter(|dependency| include_tests || dependency["kind"] != "dev")
        .filter_map(|dependency| dependency["rename"].as_str())
        .map(|alias| Violation {
            file: manifest.to_path_buf(),
            rule: "unsupported export",
            detail: format!("Cargo renames pdx-native as {alias}"),
        })
        .collect()
}

#[test]
fn boundary_rules_accept_public_calls_and_reject_hidden_details() {
    let cases = [
        (
            "use pdx_native::{Native, supervisor::serve}; fn f() {}",
            true,
        ),
        (
            "use pdx_native::{Define, DefineValueType}; fn f() { let _ = pdx_native::DefineValueType::Integer; }",
            true,
        ),
        (
            "use pdx_native::{GeneratedName, LoadedContent, LoadedModifier, LoadedModifiers};",
            true,
        ),
        ("use pdx_native::{Native, internals};", false),
        ("use pdx_native as native; fn f() {}", false),
        ("extern crate pdx_native as native; fn f() {}", false),
        ("fn f() { pdx_native::internals::x(); }", false),
        ("fn f() { let _ = pdx_native::supervisor::serve; }", true),
        (
            "use pdx_native::supervisor::{serve as run, SupervisorError as E};",
            true,
        ),
        ("fn f() { let _ = pdx_native::supervisor::hidden; }", false),
        ("type C = pdx_native::internals::ObservationControl;", false),
        ("use pdx_native::internals::inspect::Image;", false),
        ("fn f() { let _ = pdx_native::GameOptions::fault; }", false),
        ("fn f() { let _ = GameOptions::fixture_fault; }", false),
        ("fn f() { let _ = 0x1234; }", false),
        ("fn f() { let _ = 0x12; }", true),
        ("fn f() { let _ = \"4.5\"; }", false),
        ("fn f() { let _ = \"not a version\"; }", true),
        ("fn f() { let _ = \".\"; }", true),
        ("fn f() { let _ = \"1.\"; }", true),
        ("fn f() { let _ = \".1\"; }", true),
        ("fn f() { let _ = \"1..2\"; }", true),
        ("#[cfg(target_os = \"macos\")] fn f() {}", false),
        ("#[cfg(windows)] fn f() {}", false),
        ("#[cfg(unix)] fn f() {}", false),
        (
            "#[cfg_attr(target_family = \"unix\", allow(dead_code))] fn f() {}",
            false,
        ),
        ("#[cfg(feature = \"x\")] fn f() {}", true),
        ("#[cfg(feature = \"windows\")] fn f() {}", true),
        ("fn f() { let _ = cfg!(target_arch = \"aarch64\"); }", false),
        ("fn f() { let _ = std::env::consts::OS; }", false),
        ("use std::env::consts;", false),
        ("use std::env::{consts as host};", false),
        ("fn f() { x.fault(); }", false),
        ("fn f() { x.fixture_fault(); }", false),
        ("fn f() { x.modifier_fault(y); }", false),
        ("fn f() { let _ = GameOptions::modifier_fault; }", false),
        ("unsafe trait T {}", false),
        ("unsafe impl Send for T {}", false),
        ("impl T { unsafe fn f() {} }", false),
        ("trait T { unsafe fn f(); }", false),
        ("unsafe extern \"C\" {}", false),
        (
            "fn f() { dbg!(pdx_native::internals::reference_result()); }",
            false,
        ),
        ("fn f() { dbg!(pdx_native::supervisor::hidden()); }", false),
        (
            "fn f() { serde_json::json!({\"address\": 0x1234}); }",
            false,
        ),
        (
            "fn f() { let _ = pdx_native::Operation::Registries; }",
            true,
        ),
        (
            "fn f() { let _ = pdx_native::Operation::Declarations; }",
            true,
        ),
        ("fn f() { let _ = pdx_native::Operation::Defines; }", true),
        (
            "fn f() { let _ = pdx_native::Operation::ScopeLinks; }",
            true,
        ),
        (
            "fn f() { let _ = pdx_native::Operation::LocalizationDeclarations; }",
            true,
        ),
        (
            "fn f() { let _ = (pdx_native::Operation::OnActions, pdx_native::Operation::GameRules); }",
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
fn missing_caller_source_cannot_pass() {
    let root = tempfile::tempdir().unwrap();
    assert!(std::panic::catch_unwind(|| scan(root.path(), true)).is_err());
    std::fs::create_dir(root.path().join("src")).unwrap();
    assert!(std::panic::catch_unwind(|| scan(root.path(), true)).is_err());
}

#[test]
fn cargo_dependency_alias_is_rejected() {
    let manifest = Path::new("/caller/Cargo.toml");
    let metadata = serde_json::json!({"packages": [{
        "manifest_path": "/caller/Cargo.toml",
        "dependencies": [{"name": "pdx-native", "rename": "native"}]
    }]});
    let violations = aliases_in_metadata(&metadata, manifest, true);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].detail.contains("native"));
    assert!(aliases_in_metadata(&metadata, manifest, false).len() == 1);

    let dev_metadata = serde_json::json!({"packages": [{
        "manifest_path": "/caller/Cargo.toml",
        "dependencies": [{"name": "pdx-native", "rename": "native", "kind": "dev"}]
    }]});
    assert_eq!(aliases_in_metadata(&dev_metadata, manifest, true).len(), 1);
    assert!(aliases_in_metadata(&dev_metadata, manifest, false).is_empty());
}

#[test]
#[ignore = "set ATLAS_CALLER_PATH to the Atlas caller"]
fn atlas_caller_uses_only_supported_exports() {
    let root = std::env::var_os("ATLAS_CALLER_PATH").expect("ATLAS_CALLER_PATH is required");
    let found = scan(Path::new(&root), false);
    let descriptions: Vec<_> = found
        .iter()
        .map(|item| format!("{}: {}: {}", item.file.display(), item.rule, item.detail))
        .collect();
    assert!(found.is_empty(), "{}", descriptions.join("\n"));
}
