//! Purpose:
//! Compile-time test discovery for `elephc test`: finds every concrete class whose ancestry
//! reaches `Elephc\Testing\TestCase`, lists its test methods, and appends the synthesized
//! driver (`Runner::run` per test, `exit(finish())`) to the program's top level.
//!
//! Called from:
//! - `crate::pipeline::compile_inner` for a test-suite entry, right after `autoload::run`.
//!
//! Key details:
//! - Runs AFTER name resolution and autoloading: class names are canonical, `extends` is
//!   resolved, and every base class the tests reach, including one autoloaded from `src/`
//!   or `vendor/`, is in the AST. Running any earlier would miss those parents.
//! - The driver is PHP TEXT parsed and name-resolved like a prelude, not hand-built AST: it
//!   then has exactly the node shapes the rest of the pipeline expects for `new`, a closure
//!   and `exit()`, and the one place that knows the driver's shape is the string below.
//! - Every test is called through a closure (`fn() => $case->testX()`): declaration
//!   reachability compiles only what the program reaches, and that call is what keeps the
//!   method, and its class, in the binary. The `new` expressions root the classes.
//! - A test method is public, non-static, has a body, and either starts with `test` or
//!   carries an attribute whose unqualified name is `Test` (PHPUnit's attribute, matched by
//!   name so suites need no import of `PHPUnit\Framework\Attributes\Test`). Inherited
//!   test methods run on the concrete class, own methods first then each ancestor's, an
//!   overridden name once: the order PHPUnit's reflection walk produces.
//! - `--filter` is a case-sensitive substring over `Class::method`, applied here, so a
//!   filtered-out test is never compiled. A test with a data provider is kept whenever a
//!   filter is given, because its run-time labels (`with data set "x"`) are what the filter
//!   really matches; the runner applies the same filter to those labels.
//! - ONE INSTANCE PER TEST, as PHPUnit creates: inline property initializers start fresh
//!   for every test, not only what `setUp()` resets.
//! - A `#[DataProvider('name')]` test runs once per row of the static provider `name()`,
//!   and once per row of EACH provider when it declares several; the driver passes the row's
//!   elements BY INDEX, one per declared parameter, because the backend does not spread a
//!   runtime array into a typed call (journal 88d26b54). Provider calls go through
//!   `Runner::rows()`, so a provider that throws is that test's ERROR, not the suite's end.
//! - Test methods supplied by TRAITS count: trait bodies are only merged into classes during
//!   type checking, after this pass, so the merge is reproduced here for the test rule:
//!   nested trait uses, `insteadof` exclusions, `as` aliases (an alias is a second method
//!   name) and `as` visibility changes, with a class's own methods overriding trait ones.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::names::Name;
use crate::parser::ast::{
    ClassMethod, ExprKind, Program, Stmt, StmtKind, TraitAdaptation, TraitUse, Visibility,
};
use crate::span::Span;

/// The canonical root every test class descends from (the PHPUnit alias is its subclass).
const TEST_CASE_ROOT: &str = "Elephc\\Testing\\TestCase";

/// One discovered test: the class as declared, the method name, its declared parameter
/// count, and the static data providers it draws rows from (empty for a plain test).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredTest {
    pub class: String,
    pub method: String,
    pub parameter_count: usize,
    pub providers: Vec<String>,
}

/// Runner options the driver bakes into the `Runner` constructor call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriverOptions {
    /// `--filter PATTERN`: compile only the matching tests and hand the pattern to the runner
    /// for provider-generated labels.
    pub filter: Option<String>,
    /// `--no-isolation` turns this off: every test then runs in the runner's own process.
    pub isolate: bool,
    /// Seconds a test may run inside its isolated child before `SIGALRM` ends it; 0 disables.
    pub timeout: u32,
    /// `--junit FILE`: the JUnit XML report the runner writes in `finish()`.
    pub junit: Option<String>,
}

/// One test method as declared: name, parameter count, providers, and whether it ends in a
/// variadic parameter (with its span for the diagnostic that rejects it under a provider).
#[derive(Clone)]
struct MethodInfo {
    name: String,
    parameter_count: usize,
    providers: Vec<String>,
    variadic: bool,
    span: Span,
}

/// A class declaration as discovery sees it: its test methods, own and trait-supplied.
struct ClassInfo {
    name: String,
    parent: Option<String>,
    is_abstract: bool,
    tests: Vec<MethodInfo>,
}

/// A class as declared, before its trait uses are merged: own methods and trait uses.
struct RawClass {
    name: String,
    parent: Option<String>,
    is_abstract: bool,
    methods: Vec<ClassMethod>,
    trait_uses: Vec<TraitUse>,
}

/// A trait as declared: its own methods and the traits it uses in turn.
struct RawTrait {
    methods: Vec<ClassMethod>,
    trait_uses: Vec<TraitUse>,
}

/// Appends the synthesized driver for every discovered test to `program`.
///
/// Fails with a diagnostic when no test class is declared or when `filter` matches nothing,
/// so an empty run is a compile error rather than a binary that reports "No tests executed".
pub fn append_driver(
    mut program: Program,
    options: &DriverOptions,
) -> Result<Program, CompileError> {
    let tests = discover(&program, options.filter.as_deref())?;
    let source = driver_source(&tests, options);
    let tokens = crate::lexer::tokenize(&source)
        .map_err(|error| error.with_file("<elephc test driver>".to_string()))?;
    let driver = crate::parser::parse_internal(&tokens)
        .map_err(|error| error.with_file("<elephc test driver>".to_string()))?;
    let driver = crate::name_resolver::resolve(driver)?;
    program.extend(driver);
    Ok(program)
}

/// Lists every test in declaration order, filtered by `filter` when given.
pub fn discover(
    program: &Program,
    filter: Option<&str>,
) -> Result<Vec<DiscoveredTest>, CompileError> {
    let mut raw_classes = Vec::new();
    let mut traits = HashMap::new();
    collect_declarations(program, &mut raw_classes, &mut traits);
    let classes: Vec<ClassInfo> = raw_classes
        .iter()
        .map(|class| ClassInfo {
            name: class.name.clone(),
            parent: class.parent.clone(),
            is_abstract: class.is_abstract,
            tests: merged_methods(&class.methods, &class.trait_uses, &traits)
                .iter()
                .filter(|method| is_test_method(method))
                .map(|method| MethodInfo {
                    name: method.name.clone(),
                    parameter_count: method.params.len(),
                    providers: data_providers(method),
                    variadic: method.variadic.is_some(),
                    span: method.span,
                })
                .collect(),
        })
        .collect();
    let by_key: HashMap<String, usize> = classes
        .iter()
        .enumerate()
        .map(|(index, class)| (php_symbol_key(&class.name), index))
        .collect();
    let root = php_symbol_key(TEST_CASE_ROOT);

    let mut tests = Vec::new();
    let mut declared_classes = 0usize;
    for class in &classes {
        if class.is_abstract || !descends_from_root(class, &classes, &by_key, &root) {
            continue;
        }
        declared_classes += 1;
        for method in inherited_tests(class, &classes, &by_key) {
            if method.variadic && !method.providers.is_empty() {
                // A row's tail would have to be spread into the variadic, which the backend
                // cannot do with a runtime array; dropping it would let the test pass on
                // data it never saw.
                return Err(CompileError::new(
                    method.span,
                    &format!(
                        "{}::{} takes a variadic parameter and has a data provider: \
                         elephc test passes provider rows by position and cannot fill a \
                         variadic parameter from them. Declare one parameter per row element.",
                        class.name, method.name
                    ),
                ));
            }
            tests.push(DiscoveredTest {
                class: class.name.clone(),
                method: method.name,
                parameter_count: method.parameter_count,
                providers: method.providers,
            });
        }
    }
    if declared_classes == 0 {
        return Err(CompileError::new(
            Span::dummy(),
            "no test classes found: a test class is a concrete class extending \
             Elephc\\Testing\\TestCase or PHPUnit\\Framework\\TestCase",
        ));
    }
    if tests.is_empty() {
        return Err(CompileError::new(
            Span::dummy(),
            "no tests found: a test is a public method whose name starts with `test` or \
             that carries the #[Test] attribute",
        ));
    }
    if let Some(pattern) = filter {
        tests.retain(|test| {
            !test.providers.is_empty()
                || format!("{}::{}", test.class, test.method).contains(pattern)
        });
        if tests.is_empty() {
            return Err(CompileError::new(
                Span::dummy(),
                &format!("no tests match --filter '{pattern}'"),
            ));
        }
    }
    Ok(tests)
}

/// The test methods `class` runs: its own in declaration order, then each ancestor's, with a
/// name that appears again further up skipped (the override already ran).
fn inherited_tests(
    class: &ClassInfo,
    classes: &[ClassInfo],
    by_key: &HashMap<String, usize>,
) -> Vec<MethodInfo> {
    let mut seen = HashSet::new();
    let mut tests = Vec::new();
    let mut current = Some(class);
    let mut visited = HashSet::new();
    while let Some(info) = current {
        if !visited.insert(php_symbol_key(&info.name)) {
            break;
        }
        for method in &info.tests {
            if seen.insert(php_symbol_key(&method.name)) {
                tests.push(method.clone());
            }
        }
        current = info
            .parent
            .as_deref()
            .and_then(|parent| by_key.get(&php_symbol_key(parent)))
            .map(|index| &classes[*index]);
    }
    tests
}

/// Walks the parent chain of `class` through the declared classes, answering whether it
/// reaches the runtime root. A chain that leaves the declared set (an unknown parent) or
/// cycles answers `false`.
fn descends_from_root(
    class: &ClassInfo,
    classes: &[ClassInfo],
    by_key: &HashMap<String, usize>,
    root: &str,
) -> bool {
    let mut seen = HashSet::new();
    let mut current = class.parent.as_deref().map(php_symbol_key);
    while let Some(parent) = current {
        if parent == root {
            return true;
        }
        if !seen.insert(parent.clone()) {
            return false;
        }
        current = by_key
            .get(&parent)
            .and_then(|index| classes[*index].parent.as_deref())
            .map(php_symbol_key);
    }
    false
}

/// Collects class and trait declarations in declaration order, descending into the
/// wrappers an included or conditionally declared file leaves around them.
fn collect_declarations(
    statements: &[Stmt],
    classes: &mut Vec<RawClass>,
    traits: &mut HashMap<String, RawTrait>,
) {
    for statement in statements {
        match &statement.kind {
            StmtKind::ClassDecl {
                name,
                extends,
                is_abstract,
                methods,
                trait_uses,
                ..
            } => classes.push(RawClass {
                name: name.clone(),
                parent: extends.as_ref().map(|parent| parent.as_canonical()),
                is_abstract: *is_abstract,
                methods: methods.clone(),
                trait_uses: trait_uses.clone(),
            }),
            StmtKind::TraitDecl {
                name,
                methods,
                trait_uses,
                ..
            } => {
                traits.insert(
                    php_symbol_key(name),
                    RawTrait {
                        methods: methods.clone(),
                        trait_uses: trait_uses.clone(),
                    },
                );
            }
            StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. }
            | StmtKind::NamespaceBlock { body, .. } => collect_declarations(body, classes, traits),
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_declarations(then_body, classes, traits);
                for (_, body) in elseif_clauses {
                    collect_declarations(body, classes, traits);
                }
                if let Some(body) = else_body {
                    collect_declarations(body, classes, traits);
                }
            }
            _ => {}
        }
    }
}

/// The methods a class or trait ends up with: its own, then those its traits supply that
/// it does not declare itself, with `insteadof` exclusions, `as` aliases and `as` visibility
/// changes applied. Trait uses nest; a trait cycle or an unknown trait contributes nothing.
fn merged_methods(
    own: &[ClassMethod],
    trait_uses: &[TraitUse],
    traits: &HashMap<String, RawTrait>,
) -> Vec<ClassMethod> {
    merged_methods_guarded(own, trait_uses, traits, &mut HashSet::new())
}

/// [`merged_methods`] with the set of traits already on the expansion path, so a cycle stops.
fn merged_methods_guarded(
    own: &[ClassMethod],
    trait_uses: &[TraitUse],
    traits: &HashMap<String, RawTrait>,
    expanding: &mut HashSet<String>,
) -> Vec<ClassMethod> {
    let mut merged: Vec<ClassMethod> = own.to_vec();
    let mut declared: HashSet<String> = own.iter().map(|m| php_symbol_key(&m.name)).collect();
    for trait_use in trait_uses {
        for trait_name in &trait_use.trait_names {
            let trait_key = php_symbol_key(trait_name.as_canonical().trim_start_matches('\\'));
            let Some(raw) = traits.get(&trait_key) else {
                continue;
            };
            if !expanding.insert(trait_key.clone()) {
                continue;
            }
            let supplied =
                merged_methods_guarded(&raw.methods, &raw.trait_uses, traits, expanding);
            expanding.remove(&trait_key);
            for method in supplied {
                // `insteadof` drops the method under its own name only: an `as` alias of
                // the excluded method still reaches the class, as in PHP.
                let excluded = excluded_by_insteadof(trait_use, trait_name, &method.name);
                let (original, aliases) = adapted_methods(trait_use, trait_name, method);
                let kept = (!excluded).then_some(original).into_iter().chain(aliases);
                for adapted in kept {
                    if declared.insert(php_symbol_key(&adapted.name)) {
                        merged.push(adapted);
                    }
                }
            }
        }
    }
    merged
}

/// Whether an `insteadof` rule in this use drops `method` as supplied by `trait_name`.
fn excluded_by_insteadof(trait_use: &TraitUse, trait_name: &Name, method: &str) -> bool {
    trait_use.adaptations.iter().any(|adaptation| match adaptation {
        TraitAdaptation::InsteadOf {
            method: chosen,
            instead_of,
            ..
        } => {
            chosen.eq_ignore_ascii_case(method)
                && instead_of.iter().any(|excluded| same_trait(excluded, trait_name))
        }
        TraitAdaptation::Alias { .. } => false,
    })
}

/// The method as the class sees it after this use's `as` rules: the original (with any
/// visibility-only change applied) and one copy per alias, carrying the alias's visibility.
fn adapted_methods(
    trait_use: &TraitUse,
    trait_name: &Name,
    method: ClassMethod,
) -> (ClassMethod, Vec<ClassMethod>) {
    let mut original = method.clone();
    let mut aliases = Vec::new();
    for adaptation in &trait_use.adaptations {
        let TraitAdaptation::Alias {
            trait_name: scope,
            method: target,
            alias,
            visibility,
        } = adaptation
        else {
            continue;
        };
        if !target.eq_ignore_ascii_case(&method.name)
            || scope.as_ref().is_some_and(|scope| !same_trait(scope, trait_name))
        {
            continue;
        }
        match alias {
            Some(alias) => {
                let mut copy = method.clone();
                copy.name = alias.clone();
                if let Some(visibility) = visibility {
                    copy.visibility = visibility.clone();
                }
                aliases.push(copy);
            }
            None => {
                if let Some(visibility) = visibility {
                    original.visibility = visibility.clone();
                }
            }
        }
    }
    (original, aliases)
}

/// Whether two spellings name the same trait, ignoring case and a leading `\`.
fn same_trait(left: &Name, right: &Name) -> bool {
    php_symbol_key(left.as_canonical().trim_start_matches('\\'))
        == php_symbol_key(right.as_canonical().trim_start_matches('\\'))
}

/// A test method is public, non-static, concrete, and named `test*` or attributed `#[Test]`.
fn is_test_method(method: &ClassMethod) -> bool {
    if method.visibility != Visibility::Public
        || method.is_static
        || method.is_abstract
        || !method.has_body
    {
        return false;
    }
    method.name.starts_with("test") || has_attribute(method, "Test")
}

/// Whether the method carries an attribute whose unqualified name is `name`, in any case.
fn has_attribute(method: &ClassMethod, name: &str) -> bool {
    method.attributes.iter().any(|group| {
        group.attributes.iter().any(|attribute| {
            attribute
                .name
                .last_segment()
                .is_some_and(|segment| segment.eq_ignore_ascii_case(name))
        })
    })
}

/// The providers named by the method's `#[DataProvider('method')]` attributes, in
/// declaration order: the first argument of each, a string literal given positionally or as
/// PHPUnit's named `methodName:` argument. Any other shape is ignored; a method with no
/// provider runs once as usual.
fn data_providers(method: &ClassMethod) -> Vec<String> {
    method
        .attributes
        .iter()
        .flat_map(|group| group.attributes.iter())
        .filter(|attribute| {
            attribute
                .name
                .last_segment()
                .is_some_and(|segment| segment.eq_ignore_ascii_case("DataProvider"))
        })
        .filter_map(|attribute| match attribute.args.first().map(|arg| &arg.kind) {
            Some(ExprKind::StringLiteral(name)) => Some(name.clone()),
            Some(ExprKind::NamedArg { name, value }) if name.eq_ignore_ascii_case("methodName") => {
                match &value.kind {
                    ExprKind::StringLiteral(method) => Some(method.clone()),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

/// Renders the driver: one runner built with the options, a fresh instance per test (or per
/// provider row), one `run` call per test, and the runner's verdict as the exit code.
pub fn driver_source(tests: &[DiscoveredTest], options: &DriverOptions) -> String {
    let mut source = format!(
        "<?php\n$__elephc_runner = new \\Elephc\\Testing\\Runner({}, {}, '{}', '{}');\n",
        if options.isolate { "true" } else { "false" },
        options.timeout,
        php_single_quoted(options.filter.as_deref().unwrap_or("")),
        php_single_quoted(options.junit.as_deref().unwrap_or("")),
    );
    for (index, test) in tests.iter().enumerate() {
        let class = &test.class;
        let method = &test.method;
        if test.providers.is_empty() {
            source.push_str(&format!(
                "$__elephc_case_{index} = new \\{class}();\n\
                 $__elephc_runner->run($__elephc_case_{index}, '{method}', fn() => $__elephc_case_{index}->{method}());\n"
            ));
            continue;
        }
        let arguments = (0..test.parameter_count)
            .map(|position| format!("$__elephc_args_{index}[{position}]"))
            .collect::<Vec<_>>()
            .join(", ");
        let quoted_class = php_single_quoted(class);
        for provider in &test.providers {
            source.push_str(&format!(
                "foreach ($__elephc_runner->rows('{quoted_class}', '{method}', fn() => \\{class}::{provider}()) as $__elephc_key_{index} => $__elephc_args_{index}) {{\n\
                 \x20   $__elephc_case_{index} = new \\{class}();\n\
                 \x20   $__elephc_runner->runWithData($__elephc_case_{index}, '{method}', fn() => $__elephc_case_{index}->{method}({arguments}), $__elephc_key_{index});\n\
                 }}\n"
            ));
        }
    }
    source.push_str("exit($__elephc_runner->finish());\n");
    source
}

/// Escapes `text` for a single-quoted PHP string: only `\` and `'` are special there.
fn php_single_quoted(text: &str) -> String {
    text.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for test discovery: the ancestry walk, the method rules, the filter, the
    //! diagnostics for empty runs, and the rendered driver.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Fixtures are resolved the way the pipeline resolves them: the prelude is injected
    //!   and names are canonicalized before `discover` runs, so `extends` is a resolved name.

    use super::*;

    /// Parses, injects the prelude and name-resolves `source`, as the pipeline would have
    /// by the time discovery runs.
    fn resolved(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        let program = crate::parser::parse(&tokens).expect("fixture must parse");
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = crate::testing_prelude::inject_if_used(program, true, &mut inventory);
        crate::name_resolver::resolve(program).expect("fixture must resolve")
    }

    /// Options with isolation on, no filter, no report.
    fn options() -> DriverOptions {
        DriverOptions { filter: None, isolate: true, timeout: 60, junit: None }
    }

    /// Lists discovered tests as `Class::method` strings.
    fn names(source: &str, filter: Option<&str>) -> Vec<String> {
        discover(&resolved(source), filter)
            .expect("discovery must succeed")
            .into_iter()
            .map(|test| format!("{}::{}", test.class, test.method))
            .collect()
    }

    /// Direct and aliased subclasses are found, with their `test*` and `#[Test]` methods
    /// in declaration order; private, static and helper methods are skipped.
    #[test]
    fn finds_test_methods_on_both_roots() {
        let found = names(
            r#"<?php
namespace App\Tests;
use Elephc\Testing\TestCase;
final class AlphaTest extends TestCase {
    public function testOne(): void {}
    private function testHidden(): void {}
    public static function testStatic(): void {}
    public function helper(): void {}
    #[Test]
    public function runsWithAttribute(): void {}
    #[\PHPUnit\Framework\Attributes\Test]
    public function runsWithQualifiedAttribute(): void {}
}
final class BetaTest extends \PHPUnit\Framework\TestCase {
    public function testTwo(): void {}
}
"#,
            None,
        );
        assert_eq!(
            found,
            vec![
                "App\\Tests\\AlphaTest::testOne",
                "App\\Tests\\AlphaTest::runsWithAttribute",
                "App\\Tests\\AlphaTest::runsWithQualifiedAttribute",
                "App\\Tests\\BetaTest::testTwo",
            ]
        );
    }

    /// An intermediate abstract base is walked through but never run itself, its test
    /// methods run on the concrete class after that class's own (an override once), and a
    /// class unrelated to the runtime is ignored even when its methods are named `test*`.
    #[test]
    fn walks_intermediate_bases_and_ignores_unrelated_classes() {
        let found = names(
            r#"<?php
namespace App\Tests;
abstract class IntegrationTestCase extends \PHPUnit\Framework\TestCase {
    public function testShared(): void {}
    public function testOverridden(): void {}
}
final class DbTest extends IntegrationTestCase {
    public function testQuery(): void {}
    public function testOverridden(): void {}
}
final class NotATest {
    public function testNothing(): void {}
}
"#,
            None,
        );
        assert_eq!(
            found,
            vec![
                "App\\Tests\\DbTest::testQuery",
                "App\\Tests\\DbTest::testOverridden",
                "App\\Tests\\DbTest::testShared",
            ]
        );
    }

    /// `--filter` keeps the `Class::method` pairs containing the pattern.
    #[test]
    fn filter_is_a_substring_over_class_and_method() {
        let source = r#"<?php
use Elephc\Testing\TestCase;
final class AlphaTest extends TestCase { public function testOne(): void {} public function testTwo(): void {} }
final class BetaTest extends TestCase { public function testOne(): void {} }
"#;
        assert_eq!(
            names(source, Some("testOne")),
            vec!["AlphaTest::testOne", "BetaTest::testOne"]
        );
        assert_eq!(names(source, Some("Beta")), vec!["BetaTest::testOne"]);
        assert_eq!(names(source, Some("AlphaTest::testTwo")), vec!["AlphaTest::testTwo"]);
    }

    /// Empty runs are diagnosed with a message that names the rule that was not met.
    #[test]
    fn empty_runs_are_diagnosed() {
        let no_classes = discover(&resolved("<?php class Plain {}"), None).unwrap_err();
        assert!(no_classes.message.contains("no test classes found"), "{}", no_classes.message);
        let no_methods = discover(
            &resolved("<?php final class T extends \\Elephc\\Testing\\TestCase { public function helper(): void {} }"),
            None,
        )
        .unwrap_err();
        assert!(no_methods.message.contains("no tests found"), "{}", no_methods.message);
        let no_match = discover(
            &resolved("<?php final class T extends \\Elephc\\Testing\\TestCase { public function testIt(): void {} }"),
            Some("nope"),
        )
        .unwrap_err();
        assert!(no_match.message.contains("no tests match --filter 'nope'"), "{}", no_match.message);
    }

    /// The driver bakes the options into the runner, instantiates a fresh object per test,
    /// expands a provider test into one `foreach` per provider over `Runner::rows()` with the
    /// elements passed by index, and exits with the runner's verdict.
    #[test]
    fn driver_source_has_the_documented_shape() {
        let tests = vec![
            DiscoveredTest { class: "App\\Tests\\AlphaTest".into(), method: "testOne".into(), parameter_count: 0, providers: vec![] },
            DiscoveredTest { class: "App\\Tests\\AlphaTest".into(), method: "testTwo".into(), parameter_count: 0, providers: vec![] },
            DiscoveredTest { class: "BetaTest".into(), method: "testAdds".into(), parameter_count: 2, providers: vec!["sums".into(), "more".into()] },
        ];
        let options = DriverOptions {
            filter: Some("it's".into()),
            isolate: false,
            timeout: 5,
            junit: Some("out/report.xml".into()),
        };
        assert_eq!(
            driver_source(&tests, &options),
            "<?php\n\
             $__elephc_runner = new \\Elephc\\Testing\\Runner(false, 5, 'it\\'s', 'out/report.xml');\n\
             $__elephc_case_0 = new \\App\\Tests\\AlphaTest();\n\
             $__elephc_runner->run($__elephc_case_0, 'testOne', fn() => $__elephc_case_0->testOne());\n\
             $__elephc_case_1 = new \\App\\Tests\\AlphaTest();\n\
             $__elephc_runner->run($__elephc_case_1, 'testTwo', fn() => $__elephc_case_1->testTwo());\n\
             foreach ($__elephc_runner->rows('BetaTest', 'testAdds', fn() => \\BetaTest::sums()) as $__elephc_key_2 => $__elephc_args_2) {\n\
             \x20   $__elephc_case_2 = new \\BetaTest();\n\
             \x20   $__elephc_runner->runWithData($__elephc_case_2, 'testAdds', fn() => $__elephc_case_2->testAdds($__elephc_args_2[0], $__elephc_args_2[1]), $__elephc_key_2);\n\
             }\n\
             foreach ($__elephc_runner->rows('BetaTest', 'testAdds', fn() => \\BetaTest::more()) as $__elephc_key_2 => $__elephc_args_2) {\n\
             \x20   $__elephc_case_2 = new \\BetaTest();\n\
             \x20   $__elephc_runner->runWithData($__elephc_case_2, 'testAdds', fn() => $__elephc_case_2->testAdds($__elephc_args_2[0], $__elephc_args_2[1]), $__elephc_key_2);\n\
             }\n\
             exit($__elephc_runner->finish());\n"
        );
    }

    /// PHPUnit's named form `#[DataProvider(methodName: 'rows')]` names the provider too.
    #[test]
    fn named_method_name_argument_names_the_provider() {
        let program = resolved(
            r#"<?php
use Elephc\Testing\TestCase;
final class T extends TestCase {
    public static function rows(): array { return [[1, 1]]; }
    #[DataProvider(methodName: 'rows')]
    public function testAdds(int $a, int $b): void {}
}
"#,
        );
        let tests = discover(&program, None).expect("discovery");
        assert_eq!(tests[0].providers, vec!["rows".to_string()]);
    }

    /// A provider test with a variadic parameter is a diagnostic, not a silently truncated
    /// call that would pass on data it never received.
    #[test]
    fn variadic_provider_tests_are_rejected() {
        let program = resolved(
            r#"<?php
use Elephc\Testing\TestCase;
final class T extends TestCase {
    public static function rows(): array { return [[1, 2, 3]]; }
    #[DataProvider('rows')]
    public function testOnlyFirst(int $a, int ...$rest): void {}
    public function testPlainVariadic(int ...$rest): void {}
}
"#,
        );
        let error = discover(&program, None).unwrap_err();
        assert!(
            error.message.contains("T::testOnlyFirst takes a variadic parameter and has a data provider"),
            "{}",
            error.message
        );
    }

    /// Every `#[DataProvider]` attribute names a provider, in order; a filter keeps provider
    /// tests for the runner to decide on their run-time labels.
    #[test]
    fn data_providers_are_discovered_and_survive_the_filter() {
        let program = resolved(
            r#"<?php
use Elephc\Testing\TestCase;
final class T extends TestCase {
    public static function sums(): array { return [[1, 2, 3]]; }
    public static function more(): array { return [[2, 2, 4]]; }
    #[DataProvider('sums')]
    #[DataProvider('more')]
    public function testAdds(int $a, int $b, int $sum): void {}
    public function testPlain(): void {}
}
"#,
        );
        let tests = discover(&program, None).expect("discovery");
        assert_eq!(tests[0].providers, vec!["sums".to_string(), "more".to_string()]);
        assert_eq!(tests[0].parameter_count, 3);
        assert!(tests[1].providers.is_empty());
        let filtered = discover(&program, Some("testPlain")).expect("discovery");
        assert_eq!(
            filtered.iter().map(|test| test.method.as_str()).collect::<Vec<_>>(),
            vec!["testAdds", "testPlain"],
            "the provider test stays for the runner's own filter"
        );
    }

    /// The appended driver parses, resolves, and lands after the user's declarations.
    #[test]
    fn append_driver_extends_the_program() {
        let program = resolved(
            "<?php use Elephc\\Testing\\TestCase; final class T extends TestCase { public function testIt(): void {} }",
        );
        let before = program.len();
        let appended = append_driver(program, &options()).expect("driver must append");
        // One runner assignment, one instance, one run call, one exit.
        assert_eq!(appended.len(), before + 4);
    }

    /// Test methods supplied by traits are discovered, through nested trait uses, with
    /// `insteadof` exclusions, aliases, visibility changes, and the class's own override.
    #[test]
    fn trait_supplied_tests_are_discovered() {
        let found = names(
            r#"<?php
use Elephc\Testing\TestCase;
trait Base { public function testFromBase(): void {} }
trait Shared {
    use Base;
    public function testShared(): void {}
    public function testHidden(): void {}
    public function helper(): void {}
    public function testOverridden(): void {}
}
trait Other { public function testShared(): void {} }
final class T extends TestCase {
    use Shared, Other {
        Shared::testShared insteadof Other;
        Other::testShared as testOtherShared;
        testHidden as private;
        helper as public testAliasedHelper;
    }
    public function testOverridden(): void {}
    public function testOwn(): void {}
}
"#,
            None,
        );
        assert_eq!(
            found,
            vec![
                "T::testOverridden",
                "T::testOwn",
                "T::testShared",
                "T::testAliasedHelper",
                "T::testFromBase",
                "T::testOtherShared",
            ]
        );
    }
}
