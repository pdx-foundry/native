//! The locality gate: method, session and operation code keeps each engine fact in its home.
//!
//! A shared method reads the executable and has no branch on a registry, a command, an engine
//! class, a field or a build (`docs/development-policy.md`). A per-build fact lives in the binding
//! authority, `src/binding`, which this gate does not scan. A fact no method reaches is a recorded
//! manual exception, listed in `EXCEPTIONS` with its removal route.
//!
//! The gate scans all other production code. Test code may name build-specific registries,
//! fields and counts: those are regression expectations, not behavior.

#[path = "support/source.rs"]
mod source;

use proc_macro2::{TokenStream, TokenTree};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use syn::{
    Attribute, BinOp, Expr, ExprBinary, ExprCall, ExprLit, ExprMatch, ExprMethodCall, ImplItem,
    Item, Lit, Macro, Member, Meta, Pat, TraitItem, parse::Parser, punctuated::Punctuated,
    visit::Visit,
};

/// A recorded manual exception: one function where scanned code names an engine fact because no
/// method reaches it yet.
struct Exception {
    file: &'static str,
    /// The function that holds the exception. The same text elsewhere in the file still fails.
    function: &'static str,
    text: &'static str,
    reason: &'static str,
    removal: &'static str,
}

const EXCEPTIONS: &[Exception] = &[Exception {
    file: "src/fixture.rs",
    function: "validate",
    text: "common/tradition_categories",
    reason: "Manual category read-entry exception (docs/native/early-observations.md): the public \
             InitialCategoryLoad window and CategoryFieldReads kind exist only for the category \
             registry whose read entries the M45 binding supplies.",
    removal: "Replace both names and this check when root-field analysis selects the read-entry \
              hook and field tokens from the exact-build binding and passes an \
              unfamiliar-category transfer.",
}];

/// Words that name a value holding an engine subject, alone or in a compound name such as
/// `field_name`. A literal compared with such a value selects behavior for one registry, class,
/// field or command.
const ENGINE_SUBJECTS: &[&str] = &[
    "owner",
    "class",
    "registry",
    "directory",
    "field",
    "token",
    "command",
    "effect",
    "trigger",
];

/// Conversions that keep the value they convert, so a comparison sees through them.
const TRANSPARENT_METHODS: &[&str] = &[
    "as_str",
    "as_ref",
    "as_deref",
    "to_string",
    "to_owned",
    "into",
];

const COMPARISON_METHODS: &[&str] = &[
    "eq",
    "ne",
    "starts_with",
    "ends_with",
    "contains",
    "strip_prefix",
    "strip_suffix",
];

/// Build-specific facts that the gate recognizes in source. They come from the registry
/// expectations of the catalogued builds, so production code cannot restate them.
struct Reference {
    roots: BTreeSet<String>,
    registry_counts: BTreeSet<u64>,
}

impl Reference {
    /// Reads `tests/expected/<build>/registries.json` for every build with expectations.
    fn catalogued() -> Self {
        let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/expected");
        let builds: Vec<Vec<String>> = std::fs::read_dir(expected)
            .expect("read the build expectations")
            .map(|entry| {
                entry
                    .expect("expectation entry")
                    .path()
                    .join("registries.json")
            })
            .filter(|path| path.is_file())
            .map(|path| {
                let text = std::fs::read_to_string(&path).expect("read a registry expectation");

                serde_json::from_str(&text).expect("parse a registry expectation")
            })
            .collect();
        assert!(!builds.is_empty(), "no registry expectation found");

        Self::new(&builds)
    }

    fn new(builds: &[Vec<String>]) -> Self {
        let roots = builds
            .iter()
            .flatten()
            .map(|registry| registry.split('/').next().unwrap().to_string())
            .collect();
        let registry_counts = builds
            .iter()
            .map(|registries| registries.len() as u64)
            .collect();

        Self {
            roots,
            registry_counts,
        }
    }

    fn names_content_directory(&self, text: &str) -> bool {
        text.split(|character: char| {
            !(character.is_ascii_alphanumeric() || matches!(character, '_' | '/' | '.' | '-'))
        })
        .any(|word| {
            self.roots.iter().any(|root| {
                word.strip_prefix(root.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
            })
        })
    }
}

#[derive(Debug)]
struct Violation {
    file: PathBuf,
    /// The innermost named function around the violation, or empty at module level.
    function: String,
    rule: &'static str,
    detail: String,
}

impl Violation {
    fn describe(&self) -> String {
        format!(
            "{}: {}: {}: {}",
            self.file.display(),
            self.function,
            self.rule,
            self.detail
        )
    }
}

struct Checker<'a> {
    file: PathBuf,
    functions: Vec<String>,
    reference: &'a Reference,
    violations: Vec<Violation>,
}

impl Checker<'_> {
    fn reject(&mut self, rule: &'static str, detail: impl Into<String>) {
        self.violations.push(Violation {
            file: self.file.clone(),
            function: self.functions.last().cloned().unwrap_or_default(),
            rule,
            detail: detail.into(),
        });
    }

    /// Checks a literal wherever it appears.
    fn check_literal(&mut self, literal: &Lit) {
        if let Lit::Int(value) = literal
            && let Ok(number) = value.base10_parse::<u64>()
            && self.reference.registry_counts.contains(&number)
        {
            self.reject("registry count", value.to_string());
        }

        let Some(text) = literal_text(literal) else {
            return;
        };

        if self.reference.names_content_directory(&text) {
            self.reject("content directory", text);
        } else if is_class_name(&text) {
            self.reject("engine class", text);
        } else if source::has_build_shape(&text) {
            self.reject("build version", text);
        }
    }

    /// Checks a literal that a comparison tests against `subject`.
    fn check_compared(&mut self, subject: &Expr, literal: &Lit) {
        let subject = subject_name(subject).unwrap_or_default();
        let engine_subject = subject
            .split('_')
            .any(|word| ENGINE_SUBJECTS.contains(&word));
        let build_subject = subject.contains("build") || subject.contains("version");
        let text = match literal {
            Lit::Int(value) => value.to_string(),
            literal => match literal_text(literal) {
                Some(text) => text,
                None => return,
            },
        };

        if engine_subject && !matches!(literal, Lit::Int(_)) {
            self.reject("engine subject", format!("{subject} compared with {text}"));
        } else if build_subject {
            self.reject("build version", format!("{subject} compared with {text}"));
        }
    }

    fn check_pattern(&mut self, subject: &Expr, pattern: &Pat) {
        for literal in pattern_literals(pattern) {
            self.check_compared(subject, literal);
        }
    }

    fn check_macro(&mut self, item: &Macro) {
        if item.path.is_ident("matches") {
            let parser = |input: syn::parse::ParseStream| {
                let subject: Expr = input.parse()?;
                input.parse::<syn::Token![,]>()?;
                let pattern = Pat::parse_multi_with_leading_vert(input)?;
                let guard = match input.parse::<Option<syn::Token![if]>>()? {
                    Some(_) => Some(input.parse::<Expr>()?),
                    None => None,
                };
                input.parse::<Option<syn::Token![,]>>()?;

                Ok((subject, pattern, guard))
            };

            if let Ok((subject, pattern, guard)) = parser.parse2(item.tokens.clone()) {
                self.check_pattern(&subject, &pattern);
                self.visit_expr(&subject);
                if let Some(guard) = &guard {
                    self.visit_expr(guard);
                }
                return;
            }
        }

        let arguments = Punctuated::<Expr, syn::Token![,]>::parse_terminated;
        match arguments.parse2(item.tokens.clone()) {
            Ok(expressions) => {
                for expression in &expressions {
                    self.visit_expr(expression);
                }
            }
            Err(_) => self.check_tokens(item.tokens.clone()),
        }
    }

    /// Checks the literals of a macro whose arguments are not Rust expressions, such as `json!`.
    fn check_tokens(&mut self, tokens: TokenStream) {
        for token in tokens {
            match token {
                TokenTree::Group(group) => self.check_tokens(group.stream()),
                TokenTree::Literal(literal) => {
                    if let Ok(literal) = syn::parse_str::<Lit>(&literal.to_string()) {
                        self.check_literal(&literal);
                    }
                }
                TokenTree::Ident(_) | TokenTree::Punct(_) => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Checker<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        if !is_test_code(item_attributes(item)) {
            syn::visit::visit_item(self, item);
        }
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.functions.push(item.sig.ident.to_string());
        syn::visit::visit_item_fn(self, item);
        self.functions.pop();
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.functions.push(item.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, item);
        self.functions.pop();
    }

    fn visit_impl_item(&mut self, item: &'ast ImplItem) {
        let attributes = match item {
            ImplItem::Const(item) => &item.attrs,
            ImplItem::Fn(item) => &item.attrs,
            ImplItem::Type(item) => &item.attrs,
            ImplItem::Macro(item) => &item.attrs,
            _ => return syn::visit::visit_impl_item(self, item),
        };

        if !is_test_code(attributes) {
            syn::visit::visit_impl_item(self, item);
        }
    }

    fn visit_trait_item(&mut self, item: &'ast TraitItem) {
        let attributes = match item {
            TraitItem::Const(item) => &item.attrs,
            TraitItem::Fn(item) => &item.attrs,
            TraitItem::Type(item) => &item.attrs,
            TraitItem::Macro(item) => &item.attrs,
            _ => return syn::visit::visit_trait_item(self, item),
        };

        if !is_test_code(attributes) {
            syn::visit::visit_trait_item(self, item);
        }
    }

    fn visit_attribute(&mut self, attribute: &'ast Attribute) {
        if !attribute.path().is_ident("doc") {
            syn::visit::visit_attribute(self, attribute);
        }
    }

    fn visit_expr_lit(&mut self, item: &'ast ExprLit) {
        self.check_literal(&item.lit);
        syn::visit::visit_expr_lit(self, item);
    }

    fn visit_expr_binary(&mut self, item: &'ast ExprBinary) {
        if matches!(
            item.op,
            BinOp::Eq(_) | BinOp::Ne(_) | BinOp::Lt(_) | BinOp::Le(_) | BinOp::Gt(_) | BinOp::Ge(_)
        ) {
            if let Some(literal) = direct_literal(&item.right) {
                self.check_compared(&item.left, literal);
            }
            if let Some(literal) = direct_literal(&item.left) {
                self.check_compared(&item.right, literal);
            }
        }

        syn::visit::visit_expr_binary(self, item);
    }

    fn visit_expr_match(&mut self, item: &'ast ExprMatch) {
        for arm in &item.arms {
            self.check_pattern(&item.expr, &arm.pat);
        }

        syn::visit::visit_expr_match(self, item);
    }

    fn visit_expr_method_call(&mut self, item: &'ast ExprMethodCall) {
        if COMPARISON_METHODS.contains(&item.method.to_string().as_str())
            && let [argument] = item.args.iter().collect::<Vec<_>>().as_slice()
            && let Some(literal) = direct_literal(argument)
        {
            self.check_compared(&item.receiver, literal);
        }

        syn::visit::visit_expr_method_call(self, item);
    }

    fn visit_expr_call(&mut self, item: &'ast ExprCall) {
        let names_build = matches!(&*item.func, Expr::Path(path)
            if path.path.segments.iter().any(|segment| segment.ident == "BuildId"));

        if names_build
            && item
                .args
                .iter()
                .any(|argument| direct_literal(argument).is_some())
        {
            self.reject("build version", "BuildId built from a literal");
        }

        syn::visit::visit_expr_call(self, item);
    }

    fn visit_macro(&mut self, item: &'ast Macro) {
        self.check_macro(item);
        syn::visit::visit_macro(self, item);
    }
}

/// The text of a string, byte-string or C-string literal.
fn literal_text(literal: &Lit) -> Option<String> {
    match literal {
        Lit::Str(value) => Some(value.value()),
        Lit::ByteStr(value) => String::from_utf8(value.value()).ok(),
        Lit::CStr(value) => value.value().into_string().ok(),
        _ => None,
    }
}

/// The literal an expression evaluates to, seen through references, parentheses and conversions.
fn direct_literal(expression: &Expr) -> Option<&Lit> {
    match expression {
        Expr::Lit(literal) => Some(&literal.lit),
        Expr::Reference(reference) => direct_literal(&reference.expr),
        Expr::Paren(paren) => direct_literal(&paren.expr),
        Expr::Group(group) => direct_literal(&group.expr),
        Expr::MethodCall(call)
            if call.args.is_empty()
                && TRANSPARENT_METHODS.contains(&call.method.to_string().as_str()) =>
        {
            direct_literal(&call.receiver)
        }
        Expr::Call(call) if call.args.len() == 1 && is_path_ending(&call.func, "from") => {
            direct_literal(&call.args[0])
        }
        _ => None,
    }
}

fn is_path_ending(expression: &Expr, name: &str) -> bool {
    matches!(expression, Expr::Path(path)
        if path.path.segments.last().is_some_and(|segment| segment.ident == name))
}

/// The name that says what a compared value is: `request.registry()` is a `registry`, and
/// `build.0` is a `build`.
fn subject_name(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        Expr::Field(field) => match &field.member {
            Member::Named(name) => Some(name.to_string()),
            Member::Unnamed(_) => subject_name(&field.base),
        },
        Expr::MethodCall(call)
            if TRANSPARENT_METHODS.contains(&call.method.to_string().as_str()) =>
        {
            subject_name(&call.receiver)
        }
        Expr::MethodCall(call) => Some(call.method.to_string()),
        Expr::Reference(reference) => subject_name(&reference.expr),
        Expr::Unary(unary) => subject_name(&unary.expr),
        Expr::Paren(paren) => subject_name(&paren.expr),
        Expr::Group(group) => subject_name(&group.expr),
        _ => None,
    }
}

fn pattern_literals(pattern: &Pat) -> Vec<&Lit> {
    match pattern {
        Pat::Lit(literal) => vec![&literal.lit],
        Pat::Or(or) => or.cases.iter().flat_map(pattern_literals).collect(),
        Pat::Guard(guard) => pattern_literals(&guard.pat),
        Pat::Paren(paren) => pattern_literals(&paren.pat),
        Pat::Reference(reference) => pattern_literals(&reference.pat),
        _ => Vec::new(),
    }
}

/// A bare CamelCase engine class name such as `CCouncilAgenda`. A demangled signature, which
/// names a symbol shape, is not one, and neither is a constant name such as `CARGO_PKG_VERSION`.
fn is_class_name(text: &str) -> bool {
    let mut characters = text.chars();

    characters.next() == Some('C')
        && characters
            .next()
            .is_some_and(|character| character.is_ascii_uppercase())
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
        && text.chars().any(|character| character.is_ascii_lowercase())
}

fn item_attributes(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

/// Whether the attributes make an item exist only in test builds.
fn is_test_code(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute
            .path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "test")
            || (attribute.path().is_ident("cfg")
                && attribute
                    .parse_args::<Meta>()
                    .is_ok_and(|predicate| requires_test(&predicate)))
    })
}

/// Whether a `cfg` predicate holds only when `test` holds.
fn requires_test(predicate: &Meta) -> bool {
    if predicate.path().is_ident("test") {
        return true;
    }

    let Meta::List(list) = predicate else {
        return false;
    };
    if !list.path.is_ident("all") {
        return false;
    }

    Punctuated::<Meta, syn::Token![,]>::parse_terminated
        .parse2(list.tokens.clone())
        .is_ok_and(|items| items.iter().any(requires_test))
}

fn violations(file: &Path, source: &str, reference: &Reference) -> Vec<Violation> {
    let syntax = syn::parse_file(source).expect("crate source is Rust");
    let mut checker = Checker {
        file: file.into(),
        functions: Vec::new(),
        reference,
        violations: Vec::new(),
    };

    checker.visit_file(&syntax);

    checker.violations
}

/// The crate's source files, found by following its module tree from `lib.rs`.
#[derive(Debug, Default)]
struct ModuleTree {
    production: Vec<PathBuf>,
    tests: Vec<PathBuf>,
}

impl ModuleTree {
    fn read(src: &Path) -> Self {
        let mut tree = Self::default();

        tree.visit_file(&src.join("lib.rs"), false);

        tree
    }

    fn visit_file(&mut self, file: &Path, test: bool) {
        let source = std::fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("read module {}: {error}", file.display()));
        let syntax = syn::parse_file(&source).expect("crate source is Rust");
        let directory = file.parent().unwrap();
        let children = match file.file_name().and_then(|name| name.to_str()) {
            Some("lib.rs" | "mod.rs" | "main.rs") => directory.to_path_buf(),
            _ => directory.join(file.file_stem().unwrap()),
        };

        if test {
            self.tests.push(file.into());
        } else {
            self.production.push(file.into());
        }

        self.visit_items(&syntax.items, directory, &children, test);
    }

    /// `path_base` is where a `#[path]` in these items resolves: the file's directory at the top
    /// level, and the module's directory inside an inline module.
    fn visit_items(&mut self, items: &[Item], path_base: &Path, children: &Path, test: bool) {
        for item in items {
            let Item::Mod(module) = item else {
                continue;
            };
            let test = test || is_test_code(&module.attrs);
            let name = module.ident.to_string();

            if let Some((_, items)) = &module.content {
                let directory = children.join(&name);

                self.visit_items(items, &directory, &directory, test);
                continue;
            }

            let file = match path_attribute(&module.attrs) {
                Some(path) => path_base.join(path),
                None if children.join(format!("{name}.rs")).is_file() => {
                    children.join(format!("{name}.rs"))
                }
                None => children.join(&name).join("mod.rs"),
            };

            self.visit_file(&file, test);
        }
    }
}

fn path_attribute(attributes: &[Attribute]) -> Option<String> {
    attributes
        .iter()
        .find_map(|attribute| match &attribute.meta {
            Meta::NameValue(value) if value.path.is_ident("path") => match &value.value {
                Expr::Lit(ExprLit {
                    lit: Lit::Str(path),
                    ..
                }) => Some(path.value()),
                _ => None,
            },
            _ => None,
        })
}

fn rust_files(directory: &Path, found: &mut BTreeSet<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("read source directory") {
        let path = entry.expect("directory entry").path();

        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.insert(path);
        }
    }
}

/// Source files that the module tree does not reach. The compiler ignores them, so a gate that
/// skipped them would still pass.
fn unreached(src: &Path, tree: &ModuleTree) -> Vec<PathBuf> {
    let mut files = BTreeSet::new();
    let reached: BTreeSet<_> = tree
        .production
        .iter()
        .chain(&tree.tests)
        .map(|path| path.canonicalize().expect("canonical module path"))
        .collect();

    rust_files(src, &mut files);

    files
        .into_iter()
        .filter(|file| !reached.contains(&file.canonicalize().expect("canonical source path")))
        .collect()
}

fn is_binding_authority(src: &Path, file: &Path) -> bool {
    file == src.join("binding.rs") || file.starts_with(src.join("binding"))
}

/// Violations in the crate's production code outside the binding authority, relative to `root`.
fn scan(root: &Path, reference: &Reference) -> Vec<Violation> {
    let src = root.join("src");
    let tree = ModuleTree::read(&src);

    tree.production
        .iter()
        .filter(|file| !is_binding_authority(&src, file))
        .flat_map(|file| {
            let source = std::fs::read_to_string(file).expect("read crate source");
            let relative = file.strip_prefix(root).unwrap_or(file);

            violations(relative, &source, reference)
        })
        .collect()
}

fn excuses(exception: &Exception, violation: &Violation) -> bool {
    violation.file == Path::new(exception.file)
        && violation.function == exception.function
        && violation.detail.contains(exception.text)
}

#[test]
fn locality_rules_accept_methods_and_reject_shortcuts() {
    let reference = Reference {
        roots: ["common".into(), "map".into()].into(),
        registry_counts: [164, 201].into(),
    };
    let cases = [
        // The shortcuts the Milestone 3 review found, and the shapes of their relatives.
        (
            "fn f(owner: &str) -> bool { owner == \"CCouncilAgenda\" }",
            false,
        ),
        (
            "fn f(name: &str) -> bool { name.starts_with(\"CCouncil\") }",
            false,
        ),
        (
            "fn f(r: &R) -> bool { r.registry() != \"common/tradition_categories\" }",
            false,
        ),
        (
            "fn f(field: &str) -> bool { matches!(field, \"tree_template\" | \"traditions\") }",
            false,
        ),
        (
            "fn f(field: &str) { match field { \"x\" => {}, _ => {} } }",
            false,
        ),
        (
            "fn f(t: &T) -> bool { t.token.as_str() == \"agenda_cost\" }",
            false,
        ),
        ("fn f(names: &[String]) -> bool { names.len() <= 2 }", true),
        (
            "fn f(names: &[String]) -> bool { names.len() <= 164 }",
            false,
        ),
        ("const MAXIMUM: usize = 164;", false),
        (
            "fn f(names: &[String]) -> bool { names.len() > 201 }",
            false,
        ),
        ("const AGENDA: &str = \"CCouncilAgenda\";", false),
        ("const V: &str = env!(\"CARGO_PKG_VERSION\");", true),
        (
            "fn f(field_name: &str) -> bool { field_name == \"tree_template\" }",
            false,
        ),
        ("fn f(q: &Q) -> bool { q.registry_name.eq(\"x\") }", false),
        (
            "fn f(v: u8, field: &str) -> bool { matches!(v, _ if field == \"tree_template\") }",
            false,
        ),
        (
            "fn f(d: &[u8]) -> bool { d == b\"common/traditions\" }",
            false,
        ),
        (
            "fn f() -> &'static std::ffi::CStr { c\"map/galaxy\" }",
            false,
        ),
        (
            "fn f() -> String { format!(\"{}/x\", \"map/galaxy\") }",
            false,
        ),
        (
            "fn f() -> &'static str { \"see map/galaxy for this\" }",
            false,
        ),
        ("fn f() -> &'static str { \"common\" }", true),
        ("fn f() -> &'static str { \"uncommon/x\" }", true),
        (
            "fn f() { let _ = json!({\"registry\": \"common/x\"}); }",
            false,
        ),
        // A build or version test.
        (
            "fn f(build: &BuildId) -> bool { build.0 == \"4.5.0\" }",
            false,
        ),
        ("fn f() -> BuildId { BuildId(\"m45\".into()) }", false),
        ("fn f() -> BuildId { BuildId::new(\"m45\") }", false),
        (
            "fn f(build_number: u32) -> bool { build_number >= 450 }",
            false,
        ),
        ("fn f(version: u32) { if version == 3 {} }", false),
        (
            "fn f(s: &Source) -> bool { s.build.as_str().starts_with(\"m45\") }",
            false,
        ),
        ("fn f() -> &'static str { \"4.5\" }", false),
        // Uniform methods, internal vocabulary and identity checks.
        (
            "fn f(name: &str) -> bool { name == \"CReader::Read(bool&)\" }",
            true,
        ),
        (
            "fn f(name: &str) -> bool { name.starts_with(\"void NParserUtil::ReadEffect<\") }",
            true,
        ),
        (
            "fn f(op: &str) -> bool { op == \"bl\" || matches!(op, \"cbz\" | \"cbnz\") }",
            true,
        ),
        (
            "fn f(kind: &str) { match kind { \"reader-join\" => {}, _ => {} } }",
            true,
        ),
        (
            "fn f(h: &Hello) -> bool { h.version != VERSION || h.build != BUILD }",
            true,
        ),
        (
            "fn f(p: &Plan, r: &Request) -> bool { p.build() != r.build }",
            true,
        ),
        (
            "fn f(token: R) -> bool { token == Err(Unresolved(\"site\")) }",
            true,
        ),
        (
            "fn f(owner: &str, other: &str) -> bool { owner == other }",
            true,
        ),
        (
            "fn f(field_count: usize) -> bool { field_count == 3 }",
            true,
        ),
        (
            "fn f(instruction: &str) -> bool { instruction == \"bl\" }",
            true,
        ),
        // Test code, documentation and conditional compilation.
        (
            "#[cfg(test)] mod tests { const R: &str = \"common/x\"; }",
            true,
        ),
        ("#[test] fn f() { assert_eq!(names.len(), 164); }", true),
        (
            "#[cfg(all(test, unix))] mod tests { fn f() -> &'static str { \"map/x\" } }",
            true,
        ),
        (
            "#[cfg(any(test, unix))] fn f() -> &'static str { \"map/x\" }",
            false,
        ),
        (
            "mod inner { #[cfg(test)] fn f(owner: &str) -> bool { owner == \"CX\" } }",
            true,
        ),
        (
            "impl T { #[cfg(test)] fn f(field: &str) -> bool { field == \"x\" } }",
            true,
        ),
        (
            "/// Choose names from `common/traditions`.\nfn f() {}",
            true,
        ),
        (
            "#[cfg(target_os = \"macos\")] fn f() -> &'static str { \"common/x\" }",
            false,
        ),
    ];

    for (source, accepted) in cases {
        let found = violations(Path::new("case.rs"), source, &reference);

        assert_eq!(found.is_empty(), accepted, "{source}: {found:?}");
    }
}

#[test]
fn module_walk_reaches_every_source_file() {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("src");
    let write = |relative: &str, text: &str| {
        let path = src.join(relative);

        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };

    write(
        "lib.rs",
        "mod engine; #[cfg(test)] mod checks; mod binding; mod nested { mod leaf; }",
    );
    write(
        "engine.rs",
        "mod analysis; #[cfg(test)] #[path = \"engine/support.rs\"] mod support;",
    );
    write(
        "engine/analysis/mod.rs",
        "fn f() -> &'static str { \"common/x\" }",
    );
    write("engine/support.rs", "const R: &str = \"common/x\";");
    write("checks.rs", "const R: &str = \"common/x\";");
    write("binding.rs", "const R: &str = \"common/x\";");
    write("nested/leaf.rs", "fn f() {}");

    let tree = ModuleTree::read(&src);
    assert_eq!(tree.production.len(), 5, "{tree:?}");
    assert_eq!(tree.tests.len(), 2, "{tree:?}");
    assert!(unreached(&src, &tree).is_empty());

    let reference = Reference::new(&[vec!["common/x".into()]]);
    let found = scan(root.path(), &reference);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].file, Path::new("src/engine/analysis/mod.rs"));

    write("engine/orphan.rs", "fn f() {}");
    let tree = ModuleTree::read(&src);
    assert_eq!(unreached(&src, &tree), [src.join("engine/orphan.rs")]);

    write("lib.rs", "mod missing;");
    assert!(std::panic::catch_unwind(|| ModuleTree::read(&src)).is_err());
}

#[test]
fn an_exception_excuses_only_its_own_function() {
    let exception = Exception {
        file: "case.rs",
        function: "validate",
        text: "common/x",
        reason: "test",
        removal: "test",
    };
    let reference = Reference::new(&[vec!["common/x".into()]]);
    let source = "impl R { fn validate(r: &str) -> bool { r == \"common/x\" } }\n\
                  fn select(r: &str) -> bool { r == \"common/x\" }";
    let found = violations(Path::new("case.rs"), source, &reference);
    let unexcused: Vec<_> = found
        .iter()
        .filter(|violation| !excuses(&exception, violation))
        .map(|violation| violation.function.as_str())
        .collect();

    assert!(
        found
            .iter()
            .any(|violation| violation.function == "validate")
    );
    assert_eq!(unexcused, ["select"]);
}

#[test]
fn exceptions_name_their_reason_and_removal_route() {
    for exception in EXCEPTIONS {
        assert!(
            !exception.reason.trim().is_empty() && !exception.removal.trim().is_empty(),
            "{}: {}",
            exception.file,
            exception.text
        );
    }
}

#[test]
fn methods_sessions_and_operations_keep_engine_facts_in_their_home() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = root.join("src");
    let tree = ModuleTree::read(&src);
    let unreached = unreached(&src, &tree);
    assert!(
        unreached.is_empty(),
        "files outside the module tree: {unreached:?}"
    );

    let found = scan(root, &Reference::catalogued());
    let unexcused: Vec<_> = found
        .iter()
        .filter(|violation| {
            !EXCEPTIONS
                .iter()
                .any(|exception| excuses(exception, violation))
        })
        .map(Violation::describe)
        .collect();
    let unused: Vec<_> = EXCEPTIONS
        .iter()
        .filter(|exception| !found.iter().any(|violation| excuses(exception, violation)))
        .map(|exception| {
            format!(
                "{}: {}: {}",
                exception.file, exception.function, exception.text
            )
        })
        .collect();

    assert!(unexcused.is_empty(), "\n{}", unexcused.join("\n"));
    assert!(unused.is_empty(), "stale exceptions: {unused:?}");
}
