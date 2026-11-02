//! Purpose:
//! Exercises method-local PHPDoc templates through specialization and native execution.
//! Covers scalar and container storage, class scope, traits, bounds and call argument forms.
//!
//! Called from:
//! - `cargo test --test codegen_tests generics_docblock_methods`.
//!
//! Key details:
//! - The CLI regression checks that PHPDoc stays active with and without strict PHP.
//! - Runtime class names distinguish concrete specializations from an ignored annotation.

use crate::support::*;

/// A `@template` above a class is not adopted, so the class and its same-line constructor and
/// methods are ordinary php: `get_class` prints `Box`, as `php -n` does.
#[test]
fn test_docblock_shared_class_and_method_line() {
    let source = r#"<?php
/** @template T */
class Box { public function __construct(public $value) {} public function id($v) { return $v; } }
$box = new Box(7);
echo get_class($box), ":", $box->value, "|", $box->id(8);
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), "Box:7|8");
    }
}

/// A same-line class annotation does not make the class generic in either PHP mode: the class
/// keeps its name, and a written `new Box<int>` is refused because `Box` declares no parameter.
#[test]
fn test_docblock_same_line_class_declaration() {
    let source = r#"<?php
/** @template T */ class Box {
    public function __construct(public $value) {}
}
$box = new Box(7);
echo get_class($box), ":", $box->value;
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), "Box:7", "{flags:?}");
    }
    let written = source.replace("new Box(7)", "new Box<int>(7)");
    let error = compile_cli_file_with_flags_expect_failure(&written, &[]);
    assert!(error.contains("declares no type parameters"), "{error}");
}

/// An ordinary class method specializes its returned container at each call's concrete type; the
/// container's own class-level `@template` is not adopted, so both objects are plain `Box`.
#[test]
fn test_docblock_method_templates_specialize_with_and_without_strict_php() {
    let source = r#"<?php
/** @template T */
class Box {
    /** @param T $value */
    public function __construct(private $value) {}
    /** @return T */
    public function get() { return $this->value; }
}
class Factory {
    /**
     * @template T
     * @param T $value
     * @return Box<T>
     */
    #[Marker("]")]
    public function wrap($value) { return new Box($value); }
}
$f = new Factory();
$number = $f->wrap(7);
$text = $f->wrap("seven");
echo get_class($number), ":", $number->get() + 1, "|",
    get_class($text), ":", strtoupper($text->get());
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(
            compile_cli_file_and_run_with_flags(source, flags),
            "Box:8|Box:SEVEN",
            "{flags:?}"
        );
    }
}

/// A class-level `@template` is ignored while a method's own template still binds per call. The
/// output is what `php -n` prints, with one `Box` class.
#[test]
fn test_docblock_method_templates_inside_generic_classes() {
    let out = compile_and_run(
        r#"<?php
/** @template T */
class Box {
    /** @param T $value */
    public function __construct(private $value) {}
    /** @return T */
    public function get() { return $this->value; }
    /**
     * @template U
     * @param U $value
     * @return Box<U>
     */
    public function map($value) { return new Box($value); }
}
$left = new Box(7);
$right = new Box("seven");
$mapped = $left->map("eight");
$back = $right->map(9);
echo $left->get(), "|", $right->get(), "|", get_class($mapped), ":", $mapped->get(),
    "|", get_class($back), ":", $back->get();
"#,
    );
    assert_eq!(out, "7|seven|Box:eight|Box:9");
}

/// Static, named, variadic and defaulted calls use the existing method-template argument rules.
#[test]
fn test_docblock_method_templates_cover_static_named_variadic_and_default_calls() {
    let out = compile_and_run(
        r#"<?php
class Picker {
    /**
     * @template T
     * @param array<T> $values
     * @param T $fallback
     * @return T
     */
    public static function choose(array $values, $fallback) { return $values[0] ?? $fallback; }
    /**
     * @template U
     * @param U ...$values
     * @return U
     */
    public function first(...$values) { return $values[0]; }
    /**
     * @template V = string
     * @return V
     */
    public static function emptyValue() { return "empty"; }
}
$p = new Picker();
echo Picker::choose(fallback: 0, values: [7]), "|",
    Picker::choose(fallback: "none", values: ["seven"]), "|",
    $p->first(8, 9), "|", $p->first("eight", "nine"), "|", Picker::emptyValue();
"#,
    );
    assert_eq!(out, "7|seven|8|eight|empty");
}

/// Method template bounds resolve namespace aliases and keep concrete return annotations.
#[test]
fn test_docblock_method_templates_resolve_namespaced_bounds() {
    let out = compile_and_run(
        r#"<?php
namespace Model {
    class Entity { public function __construct(public int $id) {} }
    class User extends Entity {}
}
namespace Service {
    use Model\Entity as Base;
    class Repo {
        /**
         * @template E of Base
         * @param E $entity
         * @param Base ...$others
         * @return int
         */
        public function idOf($entity, ...$others) { return $entity->id + $others[0]->id; }
    }
    echo (new Repo())->idOf(new \Model\User(42), new \Model\User(8));
}
"#,
    );
    assert_eq!(out, "50");
}

/// Trait templates specialize for multiple using classes and remain callable through inheritance.
#[test]
fn test_docblock_method_templates_in_traits_and_subclasses() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
trait Identity {
    /**
     * @template T
     * @param T $value
     * @return T
     */
    public function id($value) { return $value; }
}
class A { use Identity; }
class B { use Identity; }
class C extends A {}
echo (new A())->id(7), "|", (new A())->id("seven"), "|",
    (new B())->id(8), "|", (new C())->id("eight");
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "7|seven|8|eight");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A method's doc template is not adopted, so a written method type argument is refused as the
/// elephc extension it is; the same template on a FUNCTION accepts written type arguments, and
/// the method itself runs as php runs it.
#[test]
fn test_docblock_method_templates_accept_explicit_type_arguments() {
    let method = r#"<?php
class Identity {
    /**
     * @template T
     * @param T $value
     * @return T
     */
    public function id($value) { return $value; }
}
echo (new Identity())->id("seven");
"#;
    assert_eq!(compile_and_run(method), "seven");
    let written = method.replace("->id(", "->id<string>(");
    let error = compile_cli_file_with_flags_expect_failure(&written, &[]);
    assert!(error.contains("id<string>"), "{error}");
    let function = r#"<?php
/**
 * @template T
 * @param T $value
 * @return T
 */
function id($value) { return $value; }
echo id<string>("seven");
"#;
    assert_eq!(compile_and_run(function), "seven");
}

/// Native method templates retain both class and method parameter scope inside a namespace.
#[test]
fn test_native_namespaced_method_templates_keep_parameter_scope() {
    let out = compile_and_run(
        r#"<?php
namespace Domain;
class Holder<T> {
    public function __construct(private T $value) {}
    public function get(): T { return $this->value; }
    public function id<U>(U $value): U { return $value; }
}
$holder = new Holder<int>(7);
echo $holder->get(), "|", strtoupper($holder->id("seven"));
"#,
    );
    assert_eq!(out, "7|SEVEN");
}

/// An unrelated object no longer fails a function-local PHPDoc bound (php never reads it): the
/// template is rebound to `mixed` and the call runs. A written argument shows the bound was read.
#[test]
fn test_docblock_method_templates_tolerate_bound_violations() {
    let source = r#"<?php
class Entity {}
class Other {}
/**
 * @template E of Entity
 * @param E $entity
 */
function accept($entity): int { return 1; }
echo accept(new Other());
"#;
    let written = source.replace("accept(new Other())", "accept<Entity>(ARG)");
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), "1", "{flags:?}");
        assert_eq!(
            compile_cli_file_and_run_with_flags(&written.replace("ARG", "new Entity()"), flags),
            "1",
            "{flags:?}"
        );
        // The written `E = Entity` makes the parameter a real `Entity`, enforced at runtime.
        let caught = written
            .replace("echo accept<Entity>(ARG);", "try { accept<Entity>(ARG); } catch (TypeError $e) { echo 'TypeError'; }")
            .replace("ARG", "new Other()");
        assert_eq!(compile_cli_file_and_run_with_flags(&caught, flags), "TypeError", "{flags:?}");
    }
}

/// A method's specialized array return contract cannot silently return another element type.
#[test]
fn test_docblock_method_templates_enforce_return_storage() {
    // A return annotation naming `T` only inside a container is not adopted: the body fills that
    // storage, and php runs the written `array` (Symfony's `getAttributesOfType()` builds its
    // `@return array<T>` from untyped data). The program runs as php runs it.
    let source = r#"<?php
class Repo {
    /**
     * @template T
     * @param T $value
     * @return array<T>
     */
    public function values($value): array { return ["wrong"]; }
}
echo implode(",", (new Repo())->values(7));
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), "wrong");
    }
}

/// The four declaration forms of one `id` (function, static method, trait method, enum method)
/// carrying `doc` above it, with `body` as its body, called with `arg`.
fn id_forms(doc: &str, body: &str, arg: &str) -> [String; 4] {
    [
        format!("<?php\n{doc}function id($v) {{ {body} }}\necho id({arg});\n"),
        format!(
            "<?php\nclass R {{\n{doc}public static function id($v) {{ {body} }}\n}}\n\
             echo R::id({arg});\n"
        ),
        format!(
            "<?php\ntrait Tr {{\n{doc}public function id($v) {{ {body} }}\n}}\n\
             class K {{ use Tr; }}\necho (new K)->id({arg});\n"
        ),
        format!(
            "<?php\nenum Id {{\n    case A;\n{doc}public function id($v) {{ {body} }}\n}}\n\
             echo Id::A->id({arg});\n"
        ),
    ]
}

/// The note the compiler prints when a file's ADOPTED docblock generics refused the program and
/// the file was compiled again with its written types (`docblock::erasure`).
const ERASED_NOTE: &str = "is compiled with its written types";

/// A bounded `@template T of int` docblock above a FUNCTION is tolerant, not enforced: every
/// program runs as php runs it. That `doc` was READ shows in a call where the adopted reading
/// refuses the body (`T` binds to `int` from `5`, and `return "x"` breaks the instantiated
/// `@return T`): the compiler notes it fell back to the written types, then the program prints
/// `x` like php. The same doc above a METHOD (static, trait or enum) is never adopted: it runs
/// as written with no note, because a method is overridable and reachable by name.
fn assert_bound_docblock_read(doc: &str) {
    let [function, methods @ ..] = id_forms(doc, "return $v;", "\"seven\"");
    assert_eq!(compile_cli_file_and_run_with_flags(&function, &[]), "seven", "{function}");
    for source in methods {
        assert_eq!(compile_cli_file_and_run_with_flags(&source, &[]), "seven", "{source}");
    }
    let [function, methods @ ..] = id_forms(doc, "return \"x\";", "5");
    let notes = compile_cli_file_with_flags_capturing_stderr(&function, &[]);
    assert!(notes.contains(ERASED_NOTE), "{function}: {notes}");
    assert_eq!(compile_cli_file_and_run_with_flags(&function, &[]), "x", "{function}");
    for source in methods {
        let notes = compile_cli_file_with_flags_capturing_stderr(&source, &[]);
        assert!(!notes.contains(ERASED_NOTE), "{source}: {notes}");
        assert_eq!(compile_cli_file_and_run_with_flags(&source, &[]), "x", "{source}");
    }
}

/// The program of `assert_bound_docblock_read` when `doc` was NOT read: `5` binds nothing, so
/// nothing is refused or erased and the mismatched body prints `x`.
fn assert_bound_docblock_unread(doc: &str) {
    for source in id_forms(doc, "return \"x\";", "5") {
        let notes = compile_cli_file_with_flags_capturing_stderr(&source, &[]);
        assert!(!notes.contains(ERASED_NOTE), "{source}: {notes}");
        assert_eq!(compile_cli_file_and_run_with_flags(&source, &[]), "x", "{source}");
    }
}

/// The portable example keeps function, class and method PHPDoc active in both CLI modes.
#[test]
fn test_docblock_method_templates_example_runs_in_both_modes() {
    let source = include_str!("../../examples/generics-docblock/main.php");
    // php -n prints the same: the class keeps its written name.
    let expected = "10|x\n3|2\n7|seven|ok\n3|a\n8|eight\nIdentity|true\n";
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), expected, "{flags:?}");
    }
}

/// A doc comment closed on the declaration's own line still binds to it. Checked on a function,
/// a static method, a trait method and an enum method, whose bound `T of int` is observable
/// through the erasure note (`assert_bound_docblock_read`).
///
/// `collect` keyed every block to the line AFTER `*/`, so `/** @template T */ function id` lost
/// its template when the declaration shared the closing line.
#[test]
fn test_docblock_on_the_declaration_line_binds_to_it() {
    assert_bound_docblock_read("/**\n * @template T of int\n * @param T $v\n * @return T\n */ ");
    assert_bound_docblock_read("/**\n * @template T of int\n * @param T $v\n * @return T\n */\n");
}

/// An enum method's PHPDoc template is not adopted: it keeps its written signature, accepts any
/// argument as php does, and the written `id<int>` extension is refused on it.
#[test]
fn test_docblock_method_templates_on_enum_methods() {
    let template = r#"<?php
enum Id {
    case A;
    /**
     * @template T of int
     * @param T $v
     * @return T
     */
    public function id($v) { return $v; }
}
echo Id::A->id(ARG);
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(
            compile_cli_file_and_run_with_flags(&template.replace("ARG", "7"), flags),
            "7",
            "{flags:?}"
        );
        assert_eq!(
            compile_cli_file_and_run_with_flags(&template.replace("ARG", "\"seven\""), flags),
            "seven",
            "{flags:?}"
        );
    }
    let written = template.replace("id(ARG)", "id<int>(7)");
    let error = compile_cli_file_with_flags_expect_failure(&written, &[]);
    assert!(error.contains("id<int>"), "{error}");
}

/// A constructor's PHPDoc `@template` does NOT make the class a template: `get_class` prints
/// `Box` for both objects, exactly as `php -n` does.
#[test]
fn test_docblock_template_on_a_constructor_specializes_the_class() {
    let source = r#"<?php
class Box {
    public $value;
    /**
     * @template T
     * @param T $value
     */
    public function __construct($value) { $this->value = $value; }
}
$number = new Box(5);
$text = new Box("x");
echo get_class($number), ":", $number->value, "|", get_class($text), ":", $text->value;
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(
            compile_cli_file_and_run_with_flags(source, flags),
            "Box:5|Box:x",
            "{flags:?}"
        );
    }
}

/// Only a declaration after `*/` takes the doc comment's line: a trailing comment does not, and a
/// trailing attribute group leads to the declaration that follows it.
///
/// Any text after `*/` used to key the block to the closing line, so `/** @template T */ #[Marker]`
/// and `/** @template T */ // note` above a declaration left it non-generic.
#[test]
fn test_docblock_closing_line_trailing_comment_or_attribute() {
    let bound = "/**\n * @template T of int\n * @param T $v\n * @return T\n */";
    assert_bound_docblock_read(&format!("{bound} #[Marker]\n"));
    assert_bound_docblock_read(&format!("{bound} // kept\n"));
}

/// An ordinary `/* … */` between a docblock and its declaration does not discard the docblock,
/// whether it shares the closing line, spans several lines or precedes an attribute group.
///
/// The gap scan replaced the docblock it had seen with whatever block comment came next, so the
/// declaration was compiled as non-generic or its `@template` bound was never checked.
#[test]
fn test_block_comment_between_docblock_and_declaration_keeps_the_template() {
    let bound = "/**\n * @template T of int\n * @param T $v\n * @return T\n */";
    assert_bound_docblock_read(&format!("{bound} /* kept */ "));
    assert_bound_docblock_read(&format!("{bound}\n/*\n * kept\n */\n"));
    assert_bound_docblock_read(&format!("{bound}\n/* kept */\n#[Marker]\n"));
    assert_bound_docblock_read(&format!("{bound}\n/* kept */\n"));
}

/// A `/***` banner is an ordinary comment, as php's lexer reads it, so it does not replace the
/// docblock above it; a real docblock below still does.
///
/// Any comment starting `/**` counted as a docblock, so a `/*** kept */` line or a `/***` banner
/// dropped the `@template`: a function, static method or enum method lost its `of int` bound.
#[test]
fn test_star_banner_comment_keeps_the_docblock_above_it() {
    let bound = "/**\n * @template T of int\n * @param T $v\n * @return T\n */\n";
    assert_bound_docblock_read(&format!("{bound}/***\n * banner\n */\n"));
    assert_bound_docblock_read(&format!("{bound}/*** kept */\n"));
    // A real docblock below replaces the bounded one: nothing is read any more.
    assert_bound_docblock_unread(&format!("{bound}/** plain */\n"));
}

/// A comment is scanned and classified the way php's lexer does: the opener is consumed before
/// the closer is searched for, and only a space, tab, LF or CR after `/**` makes a docblock.
///
/// `/*/ kept */` ended at its own opener and its tail was read as code, and a form feed, vertical
/// tab or non-breaking space after `/**` was taken for a docblock; both dropped the `@template`
/// above them.
#[test]
fn test_comment_scan_follows_the_php_lexer() {
    let bound = "/**\n * @template T of int\n * @param T $v\n * @return T\n */\n";
    for comment in ["/*/ kept */", "/*/** kept */"] {
        assert_bound_docblock_read(&format!("{bound}{comment}\n"));
    }
    for comment in ["/*/*/", "/**\u{0c}*/", "/**\u{0b}*/", "/**\u{a0}*/"] {
        assert_bound_docblock_read(&format!("{bound}{comment}\n"));
    }
}

/// A one-line block parses its OPENING tag only: the tags after it are that tag's description
/// (phpDocumentor semantics, review finding: a tag MENTIONED in a description must not bind).
///
/// Every form still runs a violating argument as php does. The opening `@template` is read for a
/// function (`id<int>(1)` exists), while `@param` and `@return` are not: a written `id<int>("s")`
/// is accepted and a `5` argument binds nothing, so a body returning "x" runs. A method with no
/// read annotation naming `T` adopts no template, so `id<int>` is "not generic" there. The
/// multi-line form binds every tag.
#[test]
fn test_one_line_block_reads_its_opening_tag() {
    let bound = "/** @template T of int @param T $v @return T */\n";
    for source in id_forms(bound, "return $v;", "\"seven\"") {
        assert_eq!(compile_cli_file_and_run_with_flags(&source, &[]), "seven", "{source}");
    }
    // `@param T` / `@return T` mentioned in the description are not read: nothing binds `T`.
    assert_bound_docblock_unread(bound);
    // A function or method whose only mention of `T` is in a description adopts no template:
    // nothing it reads binds `T`.
    let method = format!(
        "<?php\nclass R {{\n{bound}public function id($v) {{ return $v; }}\n}}\necho (new R)->id<int>(1);\n"
    );
    let error = compile_cli_file_with_flags_expect_failure(&method, &[]);
    assert!(error.contains("R::id<int>"), "{error}");
    let multiline = "/** @template T of int\n * @param T $v\n * @return T\n */\n";
    assert_bound_docblock_read(multiline);
    let ok = "<?php\n/** @template T of int\n * @param T $v\n * @return T\n */\n\
              function id($v) { return $v; }\necho id(7);\n";
    assert_eq!(compile_cli_file_and_run_with_flags(ok, &[]), "7");
}

/// The Twig shape (`CoreExtension::enum`): a static method carrying a doc `@template` must stay
/// a plain php method. Reflection, `method_exists` and callables by name see it exactly as under
/// `php -n`, because a PHPDoc template must never remove or rename a php symbol.
#[test]
fn test_docblock_template_method_stays_a_plain_php_method() {
    let source = r#"<?php
class Ext {
    /**
     * @template T of \UnitEnum
     * @param class-string<T> $enum
     * @return T
     */
    public static function enum(string $enum): \UnitEnum { return Suit::Hearts; }

    /**
     * @template T
     * @param T $value
     * @return T
     */
    public function keep($value) { return $value; }
}
enum Suit { case Hearts; case Spades; }

$m = new ReflectionMethod(Ext::class, 'enum');
echo $m->getName(), '|', $m->getNumberOfParameters(), '|', $m->getParameters()[0]->getName(), '|', $m->getParameters()[0]->getType(), "\n";
$p = (new ReflectionClass(Ext::class))->getMethod('enum')->getParameters();
echo count($p), '|', $p[0]->getName(), "\n";
echo var_export(method_exists(Ext::class, 'enum'), true), '|', var_export(method_exists(Ext::class, 'keep'), true), "\n";
echo call_user_func([Ext::class, 'enum'], Suit::class)->name, '|', call_user_func([new Ext(), 'keep'], 3), "\n";
echo (new Ext())->keep(5), '|', get_class(new Ext()), "\n";
"#;
    // Identical to `php -n`.
    let expected = "enum|1|enum|string\n1|enum\ntrue|true\nHearts|3\n5|Ext\n";
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), expected, "{flags:?}");
    }
}

/// A free function with a doc `@template` is still a php function: `function_exists`, a string
/// callable, `array_map('id', …)` and `ReflectionFunction` all find it with its written
/// signature, while a direct call may still use the annotated instantiation.
#[test]
fn test_docblock_template_function_stays_a_plain_php_function() {
    let source = r#"<?php
/**
 * @template T
 * @param T $value
 * @return T
 */
function id($value) { return $value; }

$name = 'id';
echo $name(7), '|', call_user_func('id', 9), '|', id(8), '|', id('y'), "\n";
echo var_export(function_exists('id'), true), '|', (new ReflectionFunction('id'))->getNumberOfParameters(), "\n";
echo implode(',', array_map('id', [1, 2])), "\n";
"#;
    let expected = "7|9|8|y\ntrue|1\n1,2\n";
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), expected, "{flags:?}");
    }
    // The direct call still reads the annotation: a written argument exists only if it was.
    let written = source.replace("id(8)", "id<int>(8)");
    assert_eq!(compile_cli_file_and_run_with_flags(&written, &[]), expected);
}

/// An interface method implemented by a method carrying a doc `@template`: the implementation
/// keeps its written signature, so the interface contract, dispatch through the interface and an
/// overriding subclass all behave as under `php -n`.
#[test]
fn test_docblock_template_method_implements_an_interface() {
    let source = r#"<?php
interface Source {
    public function fetch(string $key);
}
class Impl implements Source {
    /**
     * @template T
     * @param T $key
     * @return T
     */
    public function fetch(string $key) { return $key; }
}
class Child extends Impl {
    public function fetch(string $key) { return strtoupper($key); }
}
function use_it(Source $s, string $k) { return $s->fetch($k); }
echo use_it(new Impl(), 'a'), '|', use_it(new Child(), 'b'), '|', (new Impl())->fetch('c'), "\n";
echo var_export((new Impl()) instanceof Source, true), '|', implode(',', class_implements(new Impl())), "\n";
echo (new ReflectionMethod(Impl::class, 'fetch'))->getNumberOfParameters(), "\n";
"#;
    let expected = "a|B|c\ntrue|Source\n1\n";
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), expected, "{flags:?}");
    }
}
