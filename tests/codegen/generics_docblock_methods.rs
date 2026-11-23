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

/// Compiles `source` (a class under a PHPDoc `@template`), asserts the docblock was bound to it
/// as a `DocView` (the class is never instantiated), and returns the program's output.
fn doc_class_output(source: &str) -> String {
    let (stderr, stdout, _) = compile_cli_files_with_env_and_run(
        &[("main.php", source)],
        "main.php",
        &[("ELEPHC_GENERICS_STATS", "1")],
    );
    let line = generics_stats_line(&stderr);
    assert!(line.contains(" doc_templates=1 "), "{line}");
    assert!(line.contains(" doc_class_instances=0 "), "{line}");
    stdout
}

/// Constructor and ordinary methods on the class line retain their class-scoped types.
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

/// A same-line class annotation determines the concrete class in both PHP modes.
#[test]
fn test_docblock_same_line_class_declaration() {
    let source = r#"<?php
/** @template T */ class Box {
    public function __construct(public $value) {}
}
$box = new Box(7);
echo get_class($box), ":", $box->value;
"#;
    assert_eq!(compile_and_run(source), "Box:7");
    let portable = source.replace("Box<int>(7)", "Box(7)");
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(&portable, flags), "Box:7");
    }
}

/// An ordinary class method specializes its returned container at each call's concrete type.
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

/// A class's type parameter and a method's type parameter bind independently.
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

/// Written method type arguments also work when the template was declared only in PHPDoc.
#[test]
fn test_docblock_method_templates_accept_explicit_type_arguments() {
    let out = compile_and_run(
        r#"<?php
class Identity {
    /**
     * @template T
     * @param T $value
     * @return T
     */
    public function id($value) { return $value; }
}
echo (new Identity())->id<string>("seven");
"#,
    );
    assert_eq!(out, "seven");
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

/// An unrelated object against a method-local PHPDoc bound is a warning in both strict modes:
/// php-src never reads the bound, so the call compiles and runs as it does there (issue #1899).
#[test]
fn test_docblock_method_template_bound_violation_warns_and_runs() {
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
        let warnings = compile_cli_file_with_flags_capturing_stderr(source, flags);
        assert!(
            warnings.contains(
                "binds type parameter <E> to Other, which does not satisfy its bound Entity; \
                 the bound is a PHPDoc annotation php-src does not enforce"
            ),
            "{warnings}"
        );
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), "1", "{flags:?}");
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

/// A doc comment closed on the declaration's own line still binds to it, and a member sharing
/// the class's line does not adopt the class's `@template`.
///
/// `collect` keyed every block to the line AFTER `*/`, so `/** @template T */ class Box` lost
/// its template and `new Box(7)` was refused. With the class and its constructor on one
/// line, the class block was also copied onto the constructor, which then expected no argument.
#[test]
fn test_docblock_on_the_declaration_line_binds_to_it() {
    let sources = [
        "<?php\n/** @template T */ class Box { public function __construct(public $v) {} }\n\
         $b = new Box(7);\necho get_class($b), \":\", $b->v;\n",
        "<?php\n/** @template T */\nclass Box { public function __construct(public $v) {} }\n\
         $b = new Box(7);\necho get_class($b), \":\", $b->v;\n",
    ];
    // Without `--strict-php` only: the written `new Box<int>` is an elephc extension it refuses.
    for source in sources {
        assert_eq!(doc_class_output(source), "Box:7", "{source}");
    }
}

/// An enum method's PHPDoc template specializes per call and reads its bound, like a class's.
///
/// Enum methods were skipped, so `@template T of int` was never read on `Id::A->id("seven")`.
/// The violation is a warning, and the call runs as php-src runs it.
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
        let violating = template.replace("ARG", "\"seven\"");
        let warnings = compile_cli_file_with_flags_capturing_stderr(&violating, flags);
        assert!(warnings.contains("does not satisfy its bound int"), "{warnings}");
        assert_eq!(compile_cli_file_and_run_with_flags(&violating, flags), "seven", "{flags:?}");
    }
}

/// Constructor PHPDoc templates affect class specialization in both PHP modes.
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
/// and `/** @template T */ // note` above `class …` left the class non-generic.
#[test]
fn test_docblock_closing_line_trailing_comment_or_attribute() {
    let sources = [
        "<?php\n#[Attribute]\nclass Marker {}\n/** @template T */ #[Marker]\n\
         class Box { public function __construct(public $v) {} }\n\
         $b = new Box(7);\necho get_class($b), \":\", $b->v;\n",
        "<?php\n/** @template T */ // kept\n\
         class Box { public function __construct(public $v) {} }\n\
         $b = new Box(7);\necho get_class($b), \":\", $b->v;\n",
    ];
    for source in sources {
        assert_eq!(doc_class_output(source), "Box:7", "{source}");
    }
}

/// An ordinary `/* … */` between a docblock and its declaration does not discard the docblock,
/// whether it shares the closing line, spans several lines or precedes an attribute group.
///
/// The gap scan replaced the docblock it had seen with whatever block comment came next, so the
/// declaration was compiled as non-generic or its `@template` bound was never checked.
#[test]
fn test_block_comment_between_docblock_and_declaration_keeps_the_template() {
    let class_sources = [
        "<?php\n/** @template T */ /* kept */ class Box { public function __construct(public $v) {} }\n\
         $b = new Box(7);\necho get_class($b), \":\", $b->v;\n",
        "<?php\n/** @template T */\n/*\n * kept\n */\nclass Box { public function __construct(public $v) {} }\n\
         $b = new Box(7);\necho get_class($b), \":\", $b->v;\n",
        "<?php\n#[Attribute]\nclass Marker {}\n/** @template T */\n/* kept */\n#[Marker]\n\
         class Box { public function __construct(public $v) {} }\n\
         $b = new Box(7);\necho get_class($b), \":\", $b->v;\n",
    ];
    for source in class_sources {
        assert_eq!(doc_class_output(source), "Box:7", "{source}");
    }
    let bounded = r#"<?php
/**
 * @template T of int
 * @param T $v
 * @return T
 */
/* kept */
function id($v) { return $v; }
echo id("seven");
"#;
    let warnings = compile_cli_file_with_flags_capturing_stderr(bounded, &[]);
    assert!(warnings.contains("does not satisfy its bound int"), "{warnings}");
}

/// A `/***` banner is an ordinary comment, as php's lexer reads it, so it does not replace the
/// docblock above it; a real docblock below still does.
///
/// Any comment starting `/**` counted as a docblock, so a `/*** kept */` line or a `/***` banner
/// dropped the `@template`: a function, static method or enum method lost its `of int` bound.
#[test]
fn test_star_banner_comment_keeps_the_docblock_above_it() {
    let banner = "<?php\n/** @template T */\n/***\n * banner\n */\n\
                  class Box { public function __construct(public $v) {} }\n\
                  $b = new Box(7);\necho get_class($b), \":\", $b->v;\n";
    assert_eq!(doc_class_output(banner), "Box:7");
    let bound = "/**\n * @template T of int\n * @param T $v\n * @return T\n */\n/*** kept */\n";
    for source in [
        format!("<?php\n{bound}function id($v) {{ return $v; }}\necho id(\"seven\");\n"),
        format!(
            "<?php\nclass R {{\n{bound}public static function id($v) {{ return $v; }}\n}}\n\
             echo R::id(\"seven\");\n"
        ),
        format!(
            "<?php\nenum Id {{\n    case A;\n{bound}public function id($v) {{ return $v; }}\n}}\n\
             echo Id::A->id(\"seven\");\n"
        ),
    ] {
        let warnings = compile_cli_file_with_flags_capturing_stderr(&source, &[]);
        assert!(warnings.contains("does not satisfy its bound int"), "{source}: {warnings}");
    }
    let second = "<?php\n/** @template T */\n/** plain */\n\
                  class Box { public function __construct(public $v) {} }\n\
                  $b = new Box(7);\necho get_class($b), \":\", $b->v;\n";
    assert_eq!(compile_cli_file_and_run_with_flags(second, &[]), "Box:7");
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
        let source =
            format!("<?php\n{bound}{comment}\nfunction id($v) {{ return $v; }}\necho id(\"seven\");\n");
        let warnings = compile_cli_file_with_flags_capturing_stderr(&source, &[]);
        assert!(warnings.contains("does not satisfy its bound int"), "{comment}: {warnings}");
    }
    for comment in ["/*/*/", "/**\u{0c}*/", "/**\u{0b}*/", "/**\u{a0}*/"] {
        let source = format!(
            "<?php\n/** @template T */\n{comment}\n\
             class Box {{ public function __construct(public $v) {{}} }}\n\
             $b = new Box(7);\necho get_class($b), \":\", $b->v;\n"
        );
        assert_eq!(
            doc_class_output(&source),
            "Box:7",
            "{comment:?}"
        );
    }
}

/// An interface method's PHPDoc `@template` is erased: the interface keeps its native signature,
/// so `TInstance` is not an unknown type and the implementing class still conforms.
#[test]
fn test_docblock_template_on_interface_method_is_erased() {
    let out = compile_and_run(
        r#"<?php
interface Container {
    /**
     * @template TInstance of object
     * @param TInstance $instance
     * @return TInstance
     */
    public function instance(string $abstract, $instance);
}
class Box { public int $v = 7; }
class C implements Container {
    public function instance(string $abstract, $instance) { return $instance; }
}
function viaContract(Container $c) { return $c->instance("a", new Box())->v; }
echo (new C())->instance("a", new Box())->v, "|", viaContract(new C());
"#,
    );
    assert_eq!(out, "7|7");
}

/// A class method that declares `@template` while implementing a plain interface method keeps
/// a vtable slot, so the class still implements the interface and dispatch reaches it.
#[test]
fn test_docblock_template_on_interface_implementation_keeps_conformance() {
    let out = compile_and_run(
        r#"<?php
interface Repo { public function get(string $key, $default = null); }
class ArrayRepo implements Repo {
    /**
     * @template TDefault
     * @param TDefault $default
     * @return mixed|TDefault
     */
    public function get(string $key, $default = null) { return $default; }
}
function read(Repo $r) { return $r->get("k", "contract"); }
echo (new ArrayRepo())->get("k", "direct"), "|", read(new ArrayRepo());
"#,
    );
    assert_eq!(out, "direct|contract");
}

/// A generic method that a subclass overrides is dispatched dynamically: calling it through the
/// parent type runs the child's body, as php-src does.
#[test]
fn test_docblock_template_method_override_dispatches_to_the_child() {
    let out = compile_and_run(
        r#"<?php
class Base {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function pick($v) { echo "Base "; return $v; }
}
class Child extends Base {
    public function pick($v) { echo "Child "; return $v; }
}
function run(Base $b) { echo $b->pick(5); }
run(new Child());
"#,
    );
    assert_eq!(out, "Child 5");
}

/// Control (also passes without this fix): `@template T of Model` documenting a native `Model`
/// parameter and return refines both, so the call returns the argument's own class.
#[test]
fn test_docblock_bounded_template_refines_the_native_bound() {
    let out = compile_and_run(
        r#"<?php
class Model { public string $n = "m"; }
class User extends Model { public function hi() { return "hi"; } }
class Repo {
    /**
     * @template T of Model
     * @param T $m
     * @return T
     */
    public function keep(Model $m): Model { return $m; }
    /**
     * @template T of Model
     * @param T $m
     * @return T
     */
    public function keepUntyped($m): Model { return $m; }
}
$r = new Repo();
echo $r->keep(new User())->hi(), "|", $r->keepUntyped(new User())->hi();
"#,
    );
    assert_eq!(out, "hi|hi");
}

/// A method template that no position adopts (every annotation lost to a native hint) is
/// dropped, so the method compiles with its native signature.
#[test]
fn test_docblock_template_no_position_adopts_is_dropped() {
    let out = compile_and_run(
        r#"<?php
class Repo {
    /**
     * @template T of object
     * @param T $m
     * @return T
     */
    public function keep(int $m): int { return $m + 1; }
}
echo (new Repo())->keep(4);
"#,
    );
    assert_eq!(out, "5");
}

/// A method template on a method implementing a built-in interface (`ArrayAccess::offsetGet`,
/// `ArrayAccess::offsetSet`, `JsonSerializable::jsonSerialize`) is dispatched dynamically and
/// erased, so the class still implements the interface.
#[test]
fn test_docblock_template_on_builtin_interface_method_is_erased() {
    let out = compile_and_run(
        r#"<?php
class Bag implements ArrayAccess, JsonSerializable, Countable {
    private array $items = ["x" => 1];
    public function offsetExists(mixed $k): bool { return isset($this->items[$k]); }
    /**
     * @template TK
     * @param TK $k
     * @return mixed
     */
    public function offsetGet($k): mixed { return $this->items[$k]; }
    /**
     * @template TK
     * @template TV
     * @param TK $k
     * @param TV $v
     */
    public function offsetSet($k, $v): void { $this->items[$k] = $v; }
    public function offsetUnset(mixed $k): void {}
    /**
     * @template T
     * @return T
     */
    public function jsonSerialize(): mixed { return $this->items; }
    public function count(): int { return count($this->items); }
}
function rd(ArrayAccess $a) { return $a["x"]; }
function js(JsonSerializable $j) { return count($j->jsonSerialize()); }
$b = new Bag(); $b["y"] = 2;
echo $b["x"], " ", rd($b), " ", $b->offsetGet("y"), " ", json_encode($b), " ", count($b), " ", js($b);
"#,
    );
    assert_eq!(out, "1 1 2 {\"x\":1,\"y\":2} 2 2");
}

/// A one-line block parses its OPENING tag only — the tags after it are that tag's description
/// (phpDocumentor semantics, review finding: a tag MENTIONED in a description must not bind).
/// With `@param` unread, no position adopts `T`, so the declaration is not a template at all:
/// it compiles untyped and silently, as php-src runs it. The multi-line form binds every tag
/// and reports the bound violation as a warning.
#[test]
fn test_one_line_block_reads_its_opening_tag() {
    let bound = "/** @template T of int @param T $v @return T */\n";
    // `docblock.rs` and `docblock/members.rs`: a template that no parameter or return position
    // adopts binds nothing at a call, so it is dropped and the declaration compiles untyped, as
    // php-src runs it, instead of failing as an undetermined call.
    let unadopted_cases = [
        format!("<?php\n{bound}function id($v) {{ return $v; }}\necho id(\"seven\");\n"),
        format!(
            "<?php\nclass R {{\n{bound}public static function id($v) {{ return $v; }}\n}}\n\
             echo R::id(\"seven\");\n"
        ),
        format!(
            "<?php\ntrait Tr {{\n{bound}public function id($v) {{ return $v; }}\n}}\n\
             class K {{ use Tr; }}\necho (new K)->id(\"seven\");\n"
        ),
        format!(
            "<?php\nenum Id {{\n    case A;\n{bound}public function id($v) {{ return $v; }}\n}}\n\
             echo Id::A->id(\"seven\");\n"
        ),
    ];
    for source in &unadopted_cases {
        let warnings = compile_cli_file_with_flags_capturing_stderr(source, &[]);
        assert!(!warnings.contains("its bound"), "{source}: {warnings}");
        assert_eq!(compile_cli_file_and_run_with_flags(source, &[]), "seven", "{source}");
    }
    let multiline = "<?php\n/** @template T of int\n * @param T $v\n * @return T\n */\n\
                     function id($v) { return $v; }\necho id(\"seven\");\n";
    let warnings = compile_cli_file_with_flags_capturing_stderr(multiline, &[]);
    assert!(
        warnings.contains("does not satisfy its bound int"),
        "{multiline}: {warnings}"
    );
    assert_eq!(compile_cli_file_and_run_with_flags(multiline, &[]), "seven");
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

/// A method template whose only `@param` loses to a native `mixed` is bound by no argument, so
/// an ordinary call reads it as `mixed`: the native signature php-src runs. A sibling whose
/// `@param` is adopted still specializes, and a written `id<int>(7)` still selects its own
/// instantiation (native syntax, so outside `--strict-php`).
#[test]
fn test_docblock_method_template_lost_to_native_mixed_still_compiles() {
    let declaration = r#"<?php
class Repo {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function id(mixed $v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function same($v) { return $v; }
}
$repo = new Repo();
"#;
    let source = format!(
        "{declaration}echo $repo->id(7), \"|\", $repo->id(\"x\"), \"|\", $repo->same(8) + 1, \"|\", $repo->same(\"y\");\n"
    );
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(&source, flags), "7|x|9|y");
    }
    let written = format!("{declaration}echo $repo->id<int>(7) + 1, \"|\", $repo->id(\"x\");\n");
    assert_eq!(compile_cli_file_and_run_with_flags(&written, &[]), "8|x");
}

/// A PHPDoc type parameter the call cannot determine is read as `mixed` instead of failing a
/// program php-src runs (issue #1899): `class-string<T>` has no elephc type, so nothing binds
/// `T` on `make()`; two arguments bind `T` to two types on `pick()`; and `null` says nothing
/// about `T` on `same()`. A bound on such a parameter is not reported either.
#[test]
fn test_docblock_undetermined_template_reads_as_mixed() {
    let source = r#"<?php
class Model { public function hi() { return "hi"; } }
class Locator {
    /**
     * @template T of Model
     * @param class-string<T> $class
     * @return T
     */
    public function make(string $class) { return new $class(); }
}
/**
 * @template T
 * @param T $a
 * @param T $b
 * @return T
 */
function pick($a, $b) { return $b; }
/**
 * @template T
 * @param T $v
 * @return T
 */
function same($v) { return $v; }
echo (new Locator())->make("Model")->hi(), "|", pick(1, "two"), "|", var_export(same(null), true);
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        let warnings = compile_cli_file_with_flags_capturing_stderr(source, flags);
        assert!(!warnings.contains("its bound"), "{warnings}");
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), "hi|two|NULL", "{flags:?}");
    }
}

/// A PHPDoc bound on a FUNCTION template is a warning when a call breaks it, and a native hint
/// wins over the `@param` that documents it, as php-src enforces the hint and not the comment
/// (issue #1899, case 6).
#[test]
fn test_docblock_function_bound_violation_warns_and_native_hint_wins() {
    let bounded = r#"<?php
/**
 * @template T of int
 * @param T $v
 * @return T
 */
function id($v) { return $v; }
echo id("seven");
"#;
    let warnings = compile_cli_file_with_flags_capturing_stderr(bounded, &[]);
    assert!(
        warnings.contains(
            "Call to generic function 'id' binds type parameter <T> to string, which does not \
             satisfy its bound int; the bound is a PHPDoc annotation php-src does not enforce"
        ),
        "{warnings}"
    );
    assert_eq!(compile_cli_file_and_run_with_flags(bounded, &[]), "seven");

    let native_hint = r#"<?php
/**
 * @template T of object
 * @param T $m
 * @return T
 */
function keep(int $m): int { return $m + 1; }
/**
 * @template T
 * @param T $m
 * @param array<T> $rest
 * @return T
 */
function first(int $m, array $rest) { return $rest[0]; }
echo keep(4), "|", keep("4"), "|", first(1, ["a", "b"]);
"#;
    let warnings = compile_cli_file_with_flags_capturing_stderr(native_hint, &[]);
    assert!(!warnings.contains("its bound"), "{warnings}");
    assert_eq!(compile_cli_file_and_run_with_flags(native_hint, &[]), "5|5|a");
}

/// A template that takes over a native hint keeps it binding (issue #1899 review): a type
/// argument the hint accepts refines it, and every other call is compiled at the hint itself,
/// so it is rejected exactly as php-src rejects it, whatever the PHPDoc bound says.
#[test]
fn test_docblock_template_keeps_the_native_hint_it_takes_over() {
    let declarations = r#"<?php
class Model { public function name() { return "model"; } }
class User extends Model { public function hi() { return "hi"; } }
class Admin extends Model {}
class Other {}
class Repo {
    /**
     * @template T of Model
     * @param T $m
     * @return T
     */
    public function keep(Model $m): Model { return $m; }
}
/**
 * @template T of Model
 * @param T $a
 * @param T $b
 * @return T
 */
function pair(Model $a, Model $b): Model { return $b; }
/**
 * @template T of Model
 * @param T $m
 */
function opt(?Model $m): int { return $m === null ? 0 : 1; }
/**
 * @template T
 * @param T $o
 * @return T
 */
function same(object $o): object { return $o; }
"#;
    let valid = format!(
        "{declarations}{}",
        r#"echo (new Repo())->keep(new User())->hi(), "|", pair(new User(), new Admin())->name(),
    "|", opt(new User()), opt(null), "|", same(new User())->hi();
"#
    );
    for flags in [&[][..], &["--strict-php"][..]] {
        let warnings = compile_cli_file_with_flags_capturing_stderr(&valid, flags);
        assert!(!warnings.contains("its bound"), "{warnings}");
        assert_eq!(
            compile_cli_file_and_run_with_flags(&valid, flags),
            "hi|model|10|hi",
            "{flags:?}"
        );
    }
    // php enforces the written hint at run time: the call compiles and throws.
    for call in ["(new Repo())->keep(new Other());", "pair(new User(), new Other());"] {
        let source = format!(
            "{declarations}try {{ {call} }} catch (TypeError $e) {{ echo \"TypeError\"; }}\n"
        );
        assert_eq!(compile_cli_file_and_run_with_flags(&source, &[]), "TypeError", "{call}");
    }
    // A scalar literal the hint rejects is refused at compile time, as for any plain function.
    let error = compile_cli_file_with_flags_expect_failure(&format!("{declarations}same(5);\n"), &[]);
    assert!(error.contains("expects object, got int"), "{error}");
}

/// A template that takes over a native hint carries it to the untyped parameters it annotates
/// too: the type argument is one type, and the hint php-src enforces has to accept it.
#[test]
fn test_docblock_template_spanning_typed_and_untyped_parameters_keeps_the_hint() {
    let declarations = r#"<?php
class Model {}
class User extends Model { public function hi() { return "hi"; } }
class Other {}
/**
 * @template T of Model
 * @param T $a
 * @param T $b
 * @return T
 */
function both(Model $a, $b) { return $b; }
"#;
    let valid = format!("{declarations}echo both(new User(), new User())->hi();\n");
    assert_eq!(compile_cli_file_and_run_with_flags(&valid, &[]), "hi");
    // php enforces the written hint at run time: the call compiles and throws.
    for call in ["both(new Other(), new Other());"] {
        let source = format!(
            "{declarations}try {{ {call} }} catch (TypeError $e) {{ echo \"TypeError\"; }}\n"
        );
        assert_eq!(compile_cli_file_and_run_with_flags(&source, &[]), "TypeError", "{call}");
    }
}

/// The native hint alone decides whether a position accepts `null`: `@param ?T $m` (or
/// `T|null`) on a native `Model $m` does not make it nullable, on a free function or on a
/// method erased for dispatch, while `?Model $m` keeps accepting `null` under `@param T $m`.
#[test]
fn test_docblock_template_keeps_the_native_nullability() {
    let declarations = r#"<?php
class Model {}
class User extends Model {}
interface Sink { public function accept(Model $m): int; }
class Repo implements Sink {
    /**
     * @template T of Model
     * @param ?T $m
     */
    public function accept(Model $m): int { return 1; }
}
/**
 * @template T of Model
 * @param ?T $m
 */
function strict(Model $m): int { return 1; }
/**
 * @template T of Model
 * @param T|null $m
 */
function union(Model $m): int { return 1; }
/**
 * @template T of Model
 * @param T $m
 */
function loose(?Model $m): int { return $m === null ? 0 : 1; }
"#;
    let valid = format!(
        "{declarations}echo strict(new User()), union(new User()), loose(null), loose(new User()), (new Repo())->accept(new User());\n"
    );
    assert_eq!(compile_cli_file_and_run_with_flags(&valid, &[]), "11011");
    // php enforces the written hint at run time: the call compiles and throws.
    for call in ["strict(null);", "union(null);", "(new Repo())->accept(null);"] {
        let source = format!(
            "{declarations}try {{ {call} }} catch (TypeError $e) {{ echo \"TypeError\"; }}\n"
        );
        assert_eq!(compile_cli_file_and_run_with_flags(&source, &[]), "TypeError", "{call}");
    }
}

/// A native return type `@return T` documents stays enforced: the type argument has to be one
/// the return type accepts, so a value php-src refuses to return is still rejected, and a
/// template only the `@return` mentions is read as the native return type.
#[test]
fn test_docblock_template_keeps_the_native_return_type() {
    let declarations = r#"<?php
class Model { public function name() { return "model"; } }
class User extends Model { public function hi() { return "hi"; } }
class Other {}
/**
 * @template T of Model
 * @param T $m
 * @return T
 */
function id($m): Model { return $m; }
/**
 * @template T of Model
 * @return T
 */
function make(): Model { return new User(); }
/**
 * @template T
 * @param T $m
 * @return T
 */
function thing($m): object { return $m; }
/**
 * @template T of Model
 * @param T $m
 * @return T
 */
function wide(object $m): Model { return $m; }
"#;
    let valid = format!(
        "{declarations}echo id(new User())->hi(), \"|\", make()->name(), \"|\", thing(new User())->hi(), \"|\", wide(new User())->hi();\n"
    );
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(
            compile_cli_file_and_run_with_flags(&valid, flags),
            "hi|model|hi|hi",
            "{flags:?}"
        );
    }
    // php enforces the written hint at run time: the call compiles and throws.
    for call in ["id(new Other());", "wide(new Other());"] {
        let source = format!(
            "{declarations}try {{ {call} }} catch (TypeError $e) {{ echo \"TypeError\"; }}\n"
        );
        assert_eq!(compile_cli_file_and_run_with_flags(&source, &[]), "TypeError", "{call}");
    }
    let error = compile_cli_file_with_flags_expect_failure(&format!("{declarations}thing(1);\n"), &[]);
    assert!(error.contains("expects object, got int"), "{error}");
    let wrong_body = r#"<?php
class Model {}
class Other {}
/**
 * @template T of Model
 * @return T
 */
function make(): Model { return new Other(); }
try { make(); } catch (TypeError $e) { echo "TypeError"; }
"#;
    // php enforces the written return type at run time: the program compiles and throws.
    assert_eq!(compile_cli_file_and_run_with_flags(wrong_body, &[]), "TypeError");
}

/// A dispatched method is erased back to the signature php-src enforces: the native hints a
/// template took over return, on the parameter and on the return type, so the method still
/// implements its interface and an unrelated argument is still rejected.
#[test]
fn test_docblock_erased_method_restores_the_native_hint() {
    let declarations = r#"<?php
class Model {}
class User extends Model {}
class Other {}
interface Sink { public function accept(Model $m, $tag): Model; }
class Repo implements Sink {
    /**
     * @template T of Model
     * @param T $m
     * @param T $tag
     * @return T
     */
    public function accept(Model $m, $tag): Model { return $m; }
}
function through(Sink $sink): string { return get_class($sink->accept(new User(), "untyped")); }
"#;
    let valid = format!(
        "{declarations}echo get_class((new Repo())->accept(new User(), 7)), \"|\", through(new Repo());\n"
    );
    assert_eq!(compile_and_run(&valid), "User|User");
    // php enforces the restored `Model` hint at run time: the call compiles and throws.
    let mismatch = format!(
        "{declarations}try {{ (new Repo())->accept(new Other(), 1); }} catch (TypeError $e) {{ echo \"TypeError\"; }}\n"
    );
    assert_eq!(compile_cli_file_and_run_with_flags(&mismatch, &[]), "TypeError");
}

/// Every php-visible spelling of a PHPDoc-template class is the written name: the instantiation
/// the compiler monomorphizes (`Box<int>`) never leaks through `get_class`, `::class`,
/// `static::class`, `var_dump` or `print_r`, and `instanceof`/`is_a` answer for `Box`. The
/// expected output is what `php -n` prints for the same program.
#[test]
fn test_docblock_template_instantiation_keeps_the_written_class_name_everywhere() {
    let source = r#"<?php
/** @template T */
class Box {
    /** @param T $v */
    public function __construct(public $v) {}
    /** @return T */
    public function get() { return $this->v; }
    public function me() { return [get_class($this), static::class, self::class, __CLASS__]; }
}
$a = new Box(42);
$b = new Box("hi");
echo get_class($a), "|", $a::class, "|", get_class($b), "\n";
echo implode(",", $a->me()), "\n";
var_dump($a);
print_r($b);
var_dump($a instanceof Box, is_a($a, 'Box'));
"#;
    let expected = "Box|Box|Box\nBox,Box,Box,Box\nobject(Box)#1 (1) {\n  [\"v\"]=>\n  int(42)\n}\nBox Object\n(\n    [v] => hi\n)\nbool(true)\nbool(true)\n";
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), expected, "{flags:?}");
    }
}

/// Every php-visible relation of a PHPDoc-template class is the written class's: `class_exists`
/// (the generic marker is an interface internally), `get_parent_class`, `instanceof` and `is_a`
/// across two instantiations, `class_implements`, `class_parents` and the declared-class lists.
/// The expected output is what `php -n` prints for the same program.
#[test]
fn test_docblock_template_instantiation_relations_equal_the_written_class() {
    let source = r#"<?php
/** @template T */
class Box {
    /** @param T $v */
    public function __construct(public $v) {}
}
/** @extends Box<string> */
class SBox extends Box {}
/** @template T */
interface Coll {}
/** @implements Coll<int> */
class IC implements Coll {}
$a = new Box(1);
$b = new Box("s");
$s = new SBox("x");
$i = new IC();
echo implode(",", array_map('intval', [class_exists('Box'), interface_exists('Box'), class_exists('Coll'), interface_exists('Coll')])), "\n";
echo var_export((bool) get_parent_class($a), true), "|", get_parent_class($s), "\n";
echo implode(",", array_map('intval', [$b instanceof $a, $a instanceof Box, $s instanceof $a, $a instanceof $s, $i instanceof Coll])), "\n";
echo implode(",", array_map('intval', [is_a($a, 'Box'), is_a($s, 'Box'), is_subclass_of($s, 'Box'), is_subclass_of($a, 'Box'), is_a('SBox', 'Box', true), is_subclass_of('SBox', 'Box')])), "\n";
echo count(class_implements($a)), count(class_implements($s)), count(class_parents($a)), implode(",", class_parents($s)), implode(",", class_implements($i)), "\n";
echo implode(",", array_map('intval', [in_array('Box', get_declared_classes(), true), in_array('Coll', get_declared_interfaces(), true), in_array('Coll', get_declared_classes(), true)])), "\n";
echo implode(",", array_map('intval', [method_exists('Box', '__construct'), property_exists('Box', 'v')])), implode(",", get_class_methods('Box')), implode(",", array_keys(get_object_vars($a))), "\n";
"#;
    let expected = "1,0,0,1\nfalse|Box\n1,1,1,0,1\n1,1,1,0,1,1\n000BoxColl\n1,1,0\n1,1__constructv\n";
    assert_eq!(compile_and_run(source), expected);
    // A bare `Box` parameter accepting a subclass is refused as a template reading, so the file is
    // compiled with its written types: the same program, the same answers.
    let with_parameter = format!(
        "{source}function take(Box $x): string {{ return get_class($x); }}\necho take($b), take($s), \"\\n\";\n"
    );
    assert_eq!(
        compile_cli_file_and_run_with_managed_pcre2(&with_parameter, &[]),
        format!("{expected}BoxSBox\n")
    );
}

/// Reflection and a runtime class name see the written class too, not the instantiation.
#[test]
#[ignore = "elephc-h17m.31: the reflection table keys a template by its marker interface and instantiation rows"]
fn test_docblock_template_instantiation_reflection_equals_the_written_class() {
    let source = r#"<?php
/** @template T */
class Box {
    /** @param T $v */
    public function __construct(public $v) {}
}
/** @extends Box<string> */
class SBox extends Box {}
/** @template T */
interface Coll {}
/** @implements Coll<int> */
class IC implements Coll {}
$a = new Box(1);
$b = new Box("s");
$s = new SBox("x");
$i = new IC();
$r = new ReflectionClass($a);
echo $r->getName(), "|", var_export($r->isInterface(), true), "|", var_export($r->getParentClass(), true), "|", count($r->getInterfaceNames()), "\n";
$r = new ReflectionClass($s);
echo $r->getName(), "|", $r->getParentClass()->getName(), "|", count($r->getInterfaceNames()), "|", var_export($r->isSubclassOf('Box'), true), "\n";
$r = new ReflectionClass('Box');
echo $r->getName(), "|", var_export($r->isInterface(), true), "|", var_export($r->isInstantiable(), true), "|", $r->newInstance(5)->v, "\n";
$name = 'Box';
$c = new $name(3);
echo get_class($c), (int) ($c instanceof Box), (int) ($c instanceof $b), (int) is_a($c, $name), (int) is_subclass_of($s, $name), "\n";
"#;
    let expected = "Box|false|false|0\nSBox|Box|0|true\nBox|false|true|5\nBox1111\n";
    assert_eq!(compile_and_run(source), expected);
}

/// A class whose `@template` bound this reader cannot parse (`class-string`, Symfony's type-info
/// `ObjectType`) is no template, php sees an ordinary class, and a child annotated
/// `@extends ObjectType<T>` must not be refused for it: the inherited annotation is forgotten, the
/// program compiles at once with no second round that falls back to written types.
#[test]
fn test_docblock_extends_of_a_class_with_an_unreadable_template_is_forgotten() {
    let source = r#"<?php
/**
 * @template T of class-string
 */
class ObjectType {
    public function __construct(public string $name) {}
}
/**
 * @template T of class-string<\UnitEnum>
 *
 * @extends ObjectType<T>
 */
class EnumType extends ObjectType {}
/**
 * @template T of class-string<\BackedEnum>
 *
 * @extends EnumType<T>
 */
final class BackedEnumType extends EnumType {}
$t = new BackedEnumType("Suit");
echo get_class($t), ":", $t->name, ":", $t instanceof ObjectType ? "y" : "n";
"#;
    let notes = compile_cli_file_with_flags_capturing_stderr(source, &[]);
    assert!(!notes.contains(ERASED_NOTE), "{notes}");
    assert_eq!(compile_cli_file_and_run_with_flags(source, &[]), "BackedEnumType:Suit:y");
}

/// A class whose `@template` bound this reader cannot parse (`Builtin<Id::A>|Obj`, Symfony
/// type-info's `GenericType`) is still a generic declaration to the people who mention it: the
/// mention is forgotten, not warned about as a bare mention of a template.
#[test]
fn test_unreadable_template_bound_does_not_warn_at_a_mention_of_a_template() {
    let source = r#"<?php
enum Id: string { case A = 'a'; case B = 'b'; }
/** @template T of Id */
final class Builtin {
    public function __construct(public readonly Id $id) {}
}
final class Obj {}
/**
 * @template T of Builtin<Id::A>|Builtin<Id::B>|Obj
 */
final class Generic {
    /** @param T $type */
    public function __construct(private readonly Builtin|Obj $type) {}
    public function kind(): string { return $this->type instanceof Obj ? 'obj' : 'builtin'; }
}
echo (new Generic(new Obj))->kind(), (new Generic(new Builtin(Id::A)))->kind(), "\n";
"#;
    let notes = compile_cli_file_with_flags_capturing_stderr(source, &[]);
    assert!(!notes.contains("names a template"), "{notes}");
    assert_eq!(compile_cli_file_and_run_with_flags(source, &[]), "objbuiltin\n");
}

/// A method `@template T of object` whose `@return array<T>` no argument determines
/// (`class-string<T>` is no type of the grammar) reads `T` as its bound, and a typed
/// `array<object>` is not the boxed array the body builds. The array position goes back to the
/// plain `array` php declares, so the method compiles at once (Symfony `ArgumentMetadata`).
#[test]
fn test_undetermined_doc_template_does_not_type_an_array_return() {
    let source = r#"<?php
final class Meta {
    /** @param list<object> $attributes */
    public function __construct(private array $attributes) {}
    /**
     * @template T of object
     *
     * @param class-string<T> $name
     *
     * @return array<T>
     */
    public function ofType(string $name): array {
        $out = [];
        foreach ($this->attributes as $a) {
            if ($a::class === $name) {
                $out[] = $a;
            }
        }
        return $out;
    }
}
class A {}
class B {}
$m = new Meta([new A, new B, new A]);
echo count($m->ofType(A::class)), count($m->ofType(B::class)), "\n";
"#;
    let notes = compile_cli_file_with_flags_capturing_stderr(source, &[]);
    assert!(!notes.contains(ERASED_NOTE), "{notes}");
    assert_eq!(compile_cli_file_and_run_with_flags(source, &[]), "21\n");
}

/// The same undetermined template in a parameter and in a nested return
/// (`list<self<TEntities>>`), called with a plain `array` (Doctrine `InsertBatch`).
#[test]
fn test_undetermined_doc_template_does_not_type_an_array_parameter() {
    let source = r#"<?php
/** @template TEntity of object */
final class Batch {
    /** @param non-empty-list<TEntity> $entities */
    public function __construct(public readonly string $class, public array $entities) {}

    /**
     * @param list<TEntities> $entities
     *
     * @return list<self<TEntities>>
     *
     * @template TEntities of object
     */
    public static function byType(array $entities): array {
        $current = null;
        $batches = [];
        $index = -1;
        foreach ($entities as $entity) {
            if ($current?->class !== $entity::class) {
                $current = new self($entity::class, []);
                $batches[] = $current;
                $index++;
            }
            $batches[$index]->entities[] = $entity;
        }
        return $batches;
    }
}
final class Uow {
    /** @return list<object> */
    private function order(): array { return [new stdClass, new stdClass, new ArrayObject([])]; }
    public function run(): string {
        $out = [];
        foreach (Batch::byType($this->order()) as $b) { $out[] = $b->class . ':' . count($b->entities); }
        return implode(',', $out);
    }
}
echo (new Uow)->run(), "\n";
"#;
    let notes = compile_cli_file_with_flags_capturing_stderr(source, &[]);
    assert!(!notes.contains(ERASED_NOTE), "{notes}");
    assert_eq!(
        compile_cli_file_and_run_with_flags(source, &[]),
        "stdClass:2,ArrayObject:1\n"
    );
}

/// Erasing a PHPDoc template's file takes the files that write type arguments on it in the SAME
/// re-run. The refusal here (a bare `Box` parameter given a subclass) implicates `Box.php` and
/// `SBox.php`; `Holder.php` annotates `@var Box<T>` and would be refused once `Box` is no
/// template, which used to cost another whole compile per file. `ELEPHC_DOCBLOCK_ERASURE_ROUND=7`
/// leaves a build exactly one re-run, so only the closure can finish.
#[test]
fn test_docblock_erasure_takes_dependents_in_one_rerun() {
    let autoloader = "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . $class . \".php\";\n    if (is_file($file)) { require $file; }\n});\n";
    let main = format!(
        "{autoloader}function take(Box $x): string {{ return get_class($x); }}\n$h = new Holder();\n$h->box = new Box(1);\necho take(new Box(2)), take(new SBox('x')), get_class($h->box), \"\\n\";\n"
    );
    let files = [
        ("main.php", main.as_str()),
        (
            "src/Box.php",
            "<?php\n/** @template T */\nclass Box {\n    /** @param T $v */\n    public function __construct(public $v) {}\n}\n",
        ),
        (
            "src/SBox.php",
            "<?php\n/** @extends Box<string> */\nclass SBox extends Box {}\n",
        ),
        (
            "src/Holder.php",
            "<?php\n/**\n * @template T\n */\nclass Holder {\n    /** @var Box<T> */\n    public $box;\n}\n",
        ),
    ];
    let (compile_stderr, stdout, _) = compile_cli_files_with_env_and_run(
        &files,
        "main.php",
        &[("ELEPHC_DOCBLOCK_ERASURE_ROUND", "7")],
    );
    assert_eq!(stdout, "BoxSBoxBox\n", "{compile_stderr}");
    // A PHPDoc template is no longer instantiated (`docblock::DocView`), so the refusal this
    // fixture was built around cannot happen: nothing is erased and nothing re-runs.
    assert!(!compile_stderr.contains(ERASED_NOTE), "{compile_stderr}");
}

/// The last `[elephc-generics]` totals line of a compile's stderr (`ELEPHC_GENERICS_STATS`).
fn generics_stats_line(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| line.starts_with("[elephc-generics] templates="))
        .last()
        .unwrap_or_else(|| panic!("no generics stats line in:\n{stderr}"))
        .to_string()
}

/// `ELEPHC_GENERICS_STATS` tells a native `<T>` template from a PHPDoc one: the native class is
/// monomorphized (one instance), the PHPDoc class is a `DocView` and never instantiated.
#[test]
fn test_generics_stats_tell_native_templates_from_doc_templates() {
    let main = "<?php\nclass NativeBox<T> { public function __construct(public T $v) {} }\n/** @template T */\nclass DocBox { /** @param T $v */ public function __construct(public $v) {} }\n$n = new NativeBox<int>(1);\n$d = new DocBox(2);\necho $n->v, $d->v, \"\\n\";\n";
    let files = [("main.php", main)];
    let (stderr, stdout, _) = compile_cli_files_with_env_and_run(
        &files,
        "main.php",
        &[("ELEPHC_GENERICS_STATS", "1")],
    );
    assert_eq!(stdout, "12\n", "{stderr}");
    let line = generics_stats_line(&stderr);
    assert!(line.contains("templates=1 doc_templates=1 instances=1 doc_class_instances=0 "), "{line}");
    assert!(line.contains(" erasure_reruns=0 "), "{line}");
}

/// The three-file autoload shape of the Demo's refusals (`Box` is a PHPDoc template, `SBox`
/// extends it with arguments, `Holder` annotates `Box<T>`). Nothing is instantiated, so nothing
/// is refused and no erasure re-run (a whole extra compile) is ever needed.
#[test]
fn test_generics_stats_count_the_erasure_rerun_of_an_autoload_fixture() {
    let autoloader = "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . $class . \".php\";\n    if (is_file($file)) { require $file; }\n});\n";
    let main = format!(
        "{autoloader}function take(Box $x): string {{ return get_class($x); }}\n$h = new Holder();\n$h->box = new Box(1);\necho take(new Box(2)), take(new SBox('x')), get_class($h->box), \"\\n\";\n"
    );
    let files = [
        ("main.php", main.as_str()),
        (
            "src/Box.php",
            "<?php\n/** @template T */\nclass Box {\n    /** @param T $v */\n    public function __construct(public $v) {}\n}\n",
        ),
        (
            "src/SBox.php",
            "<?php\n/** @extends Box<string> */\nclass SBox extends Box {}\n",
        ),
        (
            "src/Holder.php",
            "<?php\n/**\n * @template T\n */\nclass Holder {\n    /** @var Box<T> */\n    public $box;\n}\n",
        ),
    ];
    let (stderr, stdout, _) = compile_cli_files_with_env_and_run(
        &files,
        "main.php",
        &[("ELEPHC_GENERICS_STATS", "1")],
    );
    assert_eq!(stdout, "BoxSBoxBox\n", "{stderr}");
    let line = generics_stats_line(&stderr);
    assert!(line.contains(" erasure_reruns=0 "), "{line}");
    assert!(line.contains("doc_class_instances=0 "), "{line}");
    assert!(line.contains(" rounds=0 "), "{line}");
}

/// The refusals the Symfony Demo's first build met, as one namespaced program: a bare `new` of
/// a PHPDoc template inside a trait factory (`Cannot instantiate interface: ...BuiltinType`), an
/// `EntityRepository<mixed>` returned where `EntityRepository<object>` is declared, and a
/// subclass annotated `@extends EntityRepository<User>`. php reads none of the annotations, so
/// the output (taken from `php -n`) is that of plain classes: one `BuiltinType` class with one
/// shared static, `get_class` without type arguments, and no instantiation, round or re-run.
#[test]
fn test_doc_templates_compile_as_plain_classes_with_no_instantiation() {
    let source = r#"<?php
namespace App;

enum Id: string { case INT = 'int'; case STR = 'string'; }

/** @template T of Id */
final class BuiltinType {
    public static int $made = 0;
    /** @param T $id */
    public function __construct(private readonly Id $id) { self::$made++; }
    /** @return T */
    public function get(): Id { return $this->id; }
}

trait Factory {
    /**
     * @template T of Id
     * @template U value-of<T>
     * @param T|U $identifier
     * @return BuiltinType<T>
     */
    public static function builtin(Id|string $identifier): BuiltinType {
        /** @var T $identifier */
        $identifier = \is_string($identifier) ? Id::from($identifier) : $identifier;
        return new BuiltinType($identifier);
    }
    /** @return BuiltinType<Id::INT> */
    public static function int(): BuiltinType { return static::builtin(Id::INT); }
}
abstract class Type { use Factory; }

/** @template T of object */
class EntityRepository {
    public function __construct(protected string $class) {}
    /** @return EntityRepository<object> */
    public function widen(): EntityRepository { return $this; }
    /** @return T|null */
    public function make(): ?object { return new ($this->class)(); }
}
class User {}
/** @extends EntityRepository<User> */
class UserRepository extends EntityRepository {
    public function __construct() { parent::__construct(User::class); }
}
/** @return EntityRepository<mixed> */
function repo(): EntityRepository { return new UserRepository(); }
function take(EntityRepository $r): string { return get_class($r->widen()->make()); }

$a = Type::int();
$b = Type::builtin('string');
$c = new BuiltinType(Id::STR);
echo get_class($a), ' ', $a->get()->value, ' ', $b->get()->value, ' ', BuiltinType::$made, "\n";
echo $c instanceof BuiltinType ? 'y' : 'n', ' ', take(repo()), ' ', get_class(repo()), "\n";
"#;
    let files = [("main.php", source)];
    let (stderr, stdout, _) = compile_cli_files_with_env_and_run(
        &files,
        "main.php",
        &[("ELEPHC_GENERICS_STATS", "1")],
    );
    assert_eq!(
        stdout, "App\\BuiltinType int string 3\ny App\\User App\\UserRepository\n",
        "{stderr}"
    );
    let line = generics_stats_line(&stderr);
    assert!(line.contains(" doc_class_instances=0 "), "{line}");
    assert!(line.contains(" erasure_reruns=0 "), "{line}");
    assert!(!stderr.contains(ERASED_NOTE), "{stderr}");
}

/// An annotated `@extends` whose bound the reader cannot parse (`class-string<\UnitEnum>`) used to
/// need the erasure fallback; as a plain class chain it never refuses, and costs no re-run.
#[test]
fn test_unparsable_doc_bounds_cost_no_erasure_rerun() {
    let source = r#"<?php
/** @template T of class-string */
class ObjectType {
    public function __construct(public string $name) {}
}
/**
 * @template T of class-string<\UnitEnum>
 *
 * @extends ObjectType<T>
 */
class EnumType extends ObjectType {}
/**
 * @template T of class-string<\BackedEnum>
 *
 * @extends EnumType<T>
 */
final class BackedEnumType extends EnumType {}
$t = new BackedEnumType("Suit");
echo get_class($t), ":", $t->name, ":", $t instanceof ObjectType ? "y" : "n";
"#;
    let files = [("main.php", source)];
    let (stderr, stdout, _) = compile_cli_files_with_env_and_run(
        &files,
        "main.php",
        &[("ELEPHC_GENERICS_STATS", "1")],
    );
    assert_eq!(stdout, "BackedEnumType:Suit:y", "{stderr}");
    let line = generics_stats_line(&stderr);
    assert!(line.contains(" doc_class_instances=0 "), "{line}");
    assert!(line.contains(" erasure_reruns=0 "), "{line}");
}

/// A call to a PHPDoc function template whose argument is typed `int` before the method bodies
/// settle and `mixed` after (`(new C(7))->get()` over an untyped promoted property) compiles: the
/// two top-level passes re-decide the call site instead of reporting "two different generic
/// instantiations" for it. The output is what `php -n` prints.
#[test]
fn test_doc_function_template_call_over_a_widening_property_compiles() {
    let out = compile_and_run(
        r#"<?php
/**
 * @template U
 * @param U $value
 * @return U
 */
function identity($value) { return $value; }
/** @template T */
class C {
    /** @param T $value */
    public function __construct(private $value) {}
    /** @return T */
    public function get() { return $this->value; }
}
$number = identity((new C(7))->get());
$text = identity((new C("seven"))->get());
var_dump($number, $text);
"#,
    );
    assert_eq!(out, "int(7)\nstring(5) \"seven\"\n");
}

/// A program whose PHPDoc method templates are called at several types. `php -n` prints
/// `foo bar foo 6 3 2 5 get|4 id` for it.
const DOC_METHOD_PROGRAM: &str = r#"<?php
class Foo { public function hi(): string { return "foo"; } }
class Bar { public function hi(): string { return "bar"; } }
class Reg {
    /**
     * @template T of object
     * @param class-string<T> $c
     * @return T
     */
    public function get(string $c) { return new $c(); }
    /**
     * @template T
     * @param T $x
     * @return T
     */
    public function id($x) { return $x; }
    /**
     * @template T
     * @param array<T> $xs
     * @return T
     */
    public function first(array $xs) { return $xs[0]; }
    /**
     * @template T of object
     * @param class-string<T> $c
     * @return T
     */
    public static function make(string $c): object { return new $c(); }
}
function run(Reg $r, int $n, string $s, float $f) {
    echo $r->get(Foo::class)->hi(), "\n";
    echo $r->get(Bar::class)->hi(), "\n";
    echo Reg::make(Foo::class)->hi(), "\n";
    echo $r->id($n) + 1, "\n";
    echo strlen($r->id($s)), "\n";
    echo $r->first([1, 2, 3]) * 2, "\n";
    echo $r->id($f) * 2, "\n";
}
run(new Reg(), 5, "abc", 2.5);
echo get_class_methods('Reg')[0], "|", count(get_class_methods('Reg')), "\n";
$m = new ReflectionMethod('Reg', 'id');
echo $m->getName(), "\n";
"#;

/// A method whose type parameters all come from PHPDoc is one method under its written name:
/// nothing is instantiated, no extra check round runs, and each call is still typed by its own
/// binding (`$r->id($n) + 1` is an int sum, `get(Foo::class)->hi()` a call on `Foo`).
#[test]
fn test_docblock_method_templates_are_one_method_called_at_precise_types() {
    let (stderr, stdout, _) = compile_cli_files_with_env_and_run(
        &[("main.php", DOC_METHOD_PROGRAM)],
        "main.php",
        &[("ELEPHC_GENERICS_STATS", "1")],
    );
    assert_eq!(stdout, "foo\nbar\nfoo\n6\n3\n2\n5\nget|4\nid\n", "{stderr}");
    let line = generics_stats_line(&stderr);
    for expected in [
        " instances=0 ",
        " doc_class_instances=0 ",
        " doc_method_instances=0 ",
        " rounds=0 ",
        " checks=1 ",
        " erasure_reruns=0 ",
    ] {
        assert!(line.contains(expected), "{expected}: {line}");
    }
}

/// The narrowed call results own their boxes and payloads: no leak, no double release.
#[test]
fn test_docblock_method_templates_are_heap_debug_clean() {
    let output = compile_and_run_with_heap_debug(DOC_METHOD_PROGRAM);
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(output.stdout, "foo\nbar\nfoo\n6\n3\n2\n5\nget|4\nid\n");
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        output.stderr
    );
}

/// php sees a PHPDoc template method under its written name only, wherever a name is observable.
#[test]
fn test_docblock_method_templates_keep_their_written_name() {
    let out = compile_and_run(
        r#"<?php
class Reg {
    /**
     * @template T
     * @param T $x
     * @return T
     */
    public function id($x) { return debug_backtrace()[0]['function']; }
}
$r = new Reg();
echo $r->id('x'), "|", implode(",", get_class_methods($r)), "|",
    (new ReflectionClass('Reg'))->getMethods()[0]->name, "|",
    method_exists($r, 'id') ? "y" : "n", "|", is_callable([$r, 'id']) ? "y" : "n", "|",
    (string) call_user_func([$r, 'id'], 6), "\n";
"#,
    );
    assert_eq!(out, "id|id|id|y|y|id\n");
}

/// php enforces no annotation: a method whose body returns something else than its docblock says
/// returns it, untouched, to a typed use or not. `php -n` prints `bar`, `string(1) "7"`, `NULL`,
/// `Bar` and `string(1) "7"` for this program, and so do we; nothing is converted or thrown.
#[test]
fn test_docblock_method_template_lies_follow_php() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class Foo { public function name(): string { return "foo"; } }
class Bar { public function name(): string { return "bar"; } }
class Reg {
    /**
     * @template T
     * @param class-string<T> $c
     * @return T
     */
    public function get(string $c): object { return new Bar(); }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function id($v) { return is_int($v) ? "7" : $v; }
}
class Liar {
    /**
     * @template T of object
     * @param class-string<T> $c
     * @return T
     */
    public function pick(string $c) { return unserialize(serialize(new Bar())); }
    /**
     * @template T
     * @param T $x
     * @return T
     */
    public function seven($x) { return unserialize(serialize("7")); }
    /**
     * @template T
     * @param T $x
     * @return T
     */
    public function nothing($x) { return unserialize(serialize(null)); }
}
function run(Reg $r, Liar $l) {
    echo (new Foo())->name(), "\n";
    echo $r->get(Foo::class)->name(), "\n";
    var_dump($r->id(5));
    echo get_class($l->pick(Foo::class)), "\n";
    var_dump($l->seven(3));
    var_dump($l->nothing("x"));
}
run(new Reg(), new Liar());
"#,
    );
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(
        output.stdout,
        "foo\nbar\nstring(1) \"7\"\nBar\nstring(1) \"7\"\nNULL\n"
    );
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// The result of a PHPDoc method template call is an ordinary value in every use php gives it a
/// rule for: a typed property store, a typed parameter, an array key, arithmetic, a loop. Proven
/// calls (`return $x;`, `return new $c;`) keep their precise type; the rest keep the native one
/// and php's own rules apply to what the body really returned. `php -n` prints the same.
#[test]
fn test_docblock_method_template_results_in_typed_uses() {
    let source = r#"<?php
class Foo { public int $n = 3; public function name(): string { return "foo"; } }
class Bar { public function name(): string { return "bar"; } }
class Holder { public int $v = 0; public ?Foo $f = null; }
class Reg {
    /**
     * @template T
     * @param class-string<T> $c
     * @return T
     */
    public function get(string $c): object { return new Bar(); }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function id($v) { return is_int($v) ? "7" : $v; }
    /**
     * @template T
     * @param T $x
     * @return T
     */
    public function same($x) { return $x; }
    /**
     * @template T of object
     * @param class-string<T> $c
     * @return T
     */
    public function make($c) { return new $c(); }
}
function takes(Foo $f): string { return $f->name(); }
function takesInt(int $i): int { return $i + 1; }
function run(Reg $r, int $i, string $s) {
    $f = new Foo();
    echo $f->name(), "
";
    echo $r->get(Foo::class)->name(), "
";
    var_dump($r->id(5));
    var_dump($r->same($i) + 1);
    var_dump(strlen($r->same($s)));
    echo $r->make(Foo::class)->n, "
";
    $h = new Holder();
    $h->v = $r->same(41);
    $h->f = $r->make(Foo::class);
    echo $h->v + 1, $h->f->name(), "
";
    $a = [];
    $a[$r->same("k")] = $r->same(2.5);
    var_dump($a);
    echo intdiv($r->same(9), 2), "
";
    foreach ($r->same([1, 2]) as $x) { echo $x; }
    echo "
";
    echo takes($r->make(Foo::class)), takesInt($r->same(4)), "
";
    try { echo takes($r->get(Foo::class)), "
"; } catch (\TypeError $e) { echo "TE
"; }
}
run(new Reg(), 6, "hello");
"#;
    let (stderr, stdout, _) = compile_cli_files_with_env_and_run(
        &[("main.php", source)],
        "main.php",
        &[("ELEPHC_GENERICS_STATS", "1")],
    );
    assert_eq!(
        stdout,
        "foo\nbar\nstring(1) \"7\"\nint(7)\nint(5)\n3\n42foo\narray(1) {\n  [\"k\"]=>\n  float(2.5)\n}\n4\n12\nfoo5\nTE\n",
        "{stderr}"
    );
    assert!(generics_stats_line(&stderr).contains(" checks=1 "), "{stderr}");
}

/// A method's PHPDoc bound a call breaks is still only a warning, and it names the method as
/// written (not the hidden twin the annotation is read from).
#[test]
fn test_docblock_method_bound_warning_names_the_written_method() {
    let source = r#"<?php
class Entity {}
class Other {}
class Gate {
    /**
     * @template E of Entity
     * @param E $entity
     * @return E
     */
    public function accept($entity) { return $entity; }
}
$g = new Gate();
echo get_class($g->accept(new Other()));
"#;
    let warnings = compile_cli_file_with_flags_capturing_stderr(source, &[]);
    assert!(
        warnings.contains("Call to generic method 'Gate::accept' binds type parameter <E> to Other"),
        "{warnings}"
    );
    assert!(!warnings.contains("__elephc_doc_template"), "{warnings}");
    assert_eq!(compile_cli_file_and_run_with_flags(source, &[]), "Other");
}

/// An identity call of a PHPDoc method template is a value like any call's result in every context:
/// by-reference parameters (function, method, closure, dynamic callee, `call_user_func`), spreads,
/// loops, generators, arrow functions, evaluation order against `$x++` and `($x = ...)`, array
/// operands, ternary and `match` arms, copy-on-write, destructors, private and static methods, an
/// uninitialized typed property, and a receiver reassigned by an argument. `php -n` prints the
/// expected output below.
#[test]
fn test_docblock_method_identity_call_is_a_value_contexts() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class D { public function __destruct() { echo "[D]"; } }
class P { public $p = 0; public ?int $t; }
class Reg {
    /**
     * @template T
     * @param T $x
     * @return T
     */
    public function same($x) { return $x; }
}
function byref(&$v) { $v = $v + 100; return $v; }
function byval($v) { return $v + 1; }
function twice($a, $b) { return "$a,$b"; }
function &refret(array &$a) { return $a; }
function gen(Reg $r, int $n) { yield $r->same($n); yield $r->same($n) + 1; }
function run(Reg $r) {
    // 1. by-ref contexts
    $x = 5;
    echo @byref($r->same($x)), "|$x\n";
    $m = new class { public function bump(&$v) { $v++; return $v; } };
    $y = 7;
    echo @$m->bump($r->same($y)), "|$y\n";
    $c = function (&$v) { $v = 0; return 1; };
    $z = 9;
    echo @$c($r->same($z)), "|$z\n";
    $arr = [3, 1, 2];
    $s = 4;
    echo @call_user_func('byval', $r->same($s)), "|$s\n";
    $f = 'byref';
    $w = 11;
    echo @$f($r->same($w)), "|$w\n";
    echo twice(...[$r->same($x), 2]), "\n";
    // 2. loops and generators
    $i = 3;
    foreach ([1, 2] as $k => $v) { echo $r->same($i) + $v, ","; $i++; }
    echo "\n";
    foreach (gen($r, 4) as $g) { echo $g, ","; }
    echo "\n";
    $af = fn($q) => $r->same($q) * 2;
    echo $af(21), "\n";
    // 3. writes and probes through the result
    $e = $r->same(5);
    $e++;
    echo $e, "\n";
    echo isset($r->same(1)['k']) ? "set" : "unset", "\n";
    // 4. evaluation order and snapshot
    $a = 1;
    echo $r->same($a) + $a++, "\n";
    $a = 1;
    echo $r->same($a) . ($a = 7), "\n";
    $a = 1;
    echo json_encode([$r->same($a), $a = 2]), "\n";
    $a = 1;
    echo twice($r->same($a), $a = 2), "\n";
    $a = 1;
    echo ($a > 0 ? $r->same($a) : 0) + ($a = 5), "\n";
    echo match (true) { $a > 100 => 0, default => $r->same($a) }, "\n";
    // 5. undefined / destructors / cow
    echo $r->same($undef ?? 'dflt'), "\n";
    echo $r->same(@$nothere) === null ? "null" : "other", "\n";
    $b = $r->same(10);
    $b += 1;
    echo $b, "\n";
    $r->same(new D);
    echo "after1\n";
    echo "x" . $r->same(new D) instanceof D ? "T" : "F", "\n";
    echo "after2\n";
    // 6. receiver tricks
    echo $r->same($r = 3), "\n";
}
run(new Reg());
"#);
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(output.stdout, "105|5\n8|7\n1|9\n5|4\n111|11\n5,2\n4,6,\n4,5,\n42\n6\nunset\n2\n17\n[1,2]\n1,2\n6\n5\ndflt\nnull\n11\n[D]after1\n[D]T\nafter2\n3\n");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// An identity call of a PHPDoc method template is a value like any call's result in every context:
/// by-reference parameters (function, method, closure, dynamic callee, `call_user_func`), spreads,
/// loops, generators, arrow functions, evaluation order against `$x++` and `($x = ...)`, array
/// operands, ternary and `match` arms, copy-on-write, destructors, private and static methods, an
/// uninitialized typed property, and a receiver reassigned by an argument. `php -n` prints the
/// expected output below.
#[test]
fn test_docblock_method_identity_call_is_a_value_contexts_more() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class P { public int $t; public int $u = 4; public static function make(int $n) { return $n; } }
class Reg {
    /**
     * @template T
     * @param T $x
     * @return T
     */
    public function same($x) { return $x; }
    /**
     * @template T
     * @param T $x
     * @return T
     */
    private function hid($x) { return $x; }
    public function viaPrivate(int $n) { return $this->hid($n) + 1; }
    /**
     * @template T
     * @param T $x
     * @return T
     */
    public static function st($x) { return $x; }
}
function bump(&$v) { $v++; return $v; }
function getr(Reg &$r) { $r = new Reg(); return 4; }
function run(Reg $r, P $p) {
    $a = [3, 1, 2];
    $n = 2;
    echo $r->same($n), @bump($r->same($n)), $n, "\n";
    echo Reg::st($n) + 1, "\n";
    echo $r->viaPrivate(5), "\n";
    $v = 1.5;
    echo $r->same($v) * 2, "|", @bump($r->same($v)), "|", $v, "\n";
    echo $r->same($p->u) + 1, "\n";
    try { echo $r->same($p->t), "\n"; } catch (\Error $e) { echo get_class($e), ":", $e->getMessage(), "\n"; }
    $k = 1;
    echo $r->same($k) + $r->same($k++) + $k, "\n";
    $k = 1;
    echo $r->same($k++) . $r->same($k), "\n";
    $arr = [10, 20];
    $i = 0;
    echo $arr[$r->same($i)], $arr[$r->same($i++) + 1], $i, "\n";
    $q = $r->same($i);
    $i = 99;
    echo $q, "\n";
    $s = $r->same(7);
    $s++;
    echo $s, "\n";
    $rr = new Reg();
    echo $rr->same(getr($rr)), "\n";
    try { echo P::make($r->same(5)), "\n"; } catch (\Error $e) { echo "E\n"; }
    $cl = function () use (&$n) { return $n; };
    $n = 40;
    echo $r->same($cl()), "\n";
    foreach ([1, 2, 3] as $z) { echo $r->same($z) * $r->same($z); }
    echo "\n";
}
run(new Reg(), new P());
"#);
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(output.stdout, "232\n3\n6\n3|2.5|1.5\n5\nError:Typed property P::$t must not be accessed before initialization\n4\n12\n10201\n1\n8\n4\n5\n40\n149\n");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Drops the file path of a diagnostic (`... in /path/main.php on line 7` becomes `... on line 7`),
/// which is the one part of a php message a test directory changes.
fn strip_diagnostic_paths(output: &str) -> String {
    output
        .lines()
        .map(|line| match (line.rfind(" in "), line.rfind(" on line ")) {
            (Some(start), Some(end)) if start < end => format!("{}{}", &line[..start], &line[end..]),
            _ => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The identity-call contexts of `foreachref`: the output equals `php -n`'s, line numbers of notices
/// included (visibility, by-reference assignment, string by-reference parameters, generators,
/// loops, writes through the result, null receivers, inherited, trait, enum and override receivers).
#[test]
fn test_docblock_method_identity_call_matches_php_foreachref() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class P { public int $p = 1; public int $u; }
class Reg {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function same($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    private function hid($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public static function st($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function gen($v) { yield $v; }
}
$r = new Reg;
$n = $argc;
$a = [1, 2]; foreach ($r->same($a) as &$v) { $v += $n; } unset($v); echo json_encode($a), "\n";
foreach ($r->same($a) as $k => $w) { $a[] = $w; } echo json_encode($a), "\n";
"#);
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(
        strip_diagnostic_paths(&output.stdout).trim_end(),
        "[1,2]\n[1,2,1,2]"
    );
    let diagnostics = strip_diagnostic_paths(&output.stderr);
    for expected in [] as [&str; 0] {
        assert!(diagnostics.contains(expected), "{expected} missing from {diagnostics}");
    }
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// The identity-call contexts of `gener`: the output equals `php -n`'s, line numbers of notices
/// included (visibility, by-reference assignment, string by-reference parameters, generators,
/// loops, writes through the result, null receivers, inherited, trait, enum and override receivers).
#[test]
fn test_docblock_method_identity_call_matches_php_gener() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class P { public int $p = 1; public int $u; }
class Reg {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function same($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    private function hid($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public static function st($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function gen($v) { yield $v; }
}
$r = new Reg;
$n = $argc;
foreach ($r->gen($n) as $v) { var_dump($v); }
"#);
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(
        strip_diagnostic_paths(&output.stdout).trim_end(),
        "int(1)"
    );
    let diagnostics = strip_diagnostic_paths(&output.stderr);
    for expected in [] as [&str; 0] {
        assert!(diagnostics.contains(expected), "{expected} missing from {diagnostics}");
    }
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// The identity-call contexts of `loop`: the output equals `php -n`'s, line numbers of notices
/// included (visibility, by-reference assignment, string by-reference parameters, generators,
/// loops, writes through the result, null receivers, inherited, trait, enum and override receivers).
#[test]
fn test_docblock_method_identity_call_matches_php_loop() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class P { public int $p = 1; public int $u; }
class Reg {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function same($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    private function hid($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public static function st($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function gen($v) { yield $v; }
}
$r = new Reg;
$n = $argc;
$i = 0; while ($r->same($i++) < 3) { echo $i; } for ($j = 0; $r->same($j) < 3; $j = $r->same($j) + 1) echo $j; echo "\n";
function rec(Reg $r, int $k): int { return $k ? $r->same($k) + rec($r, $k - 1) + $r->same($k) : 0; } echo rec($r, 4), "\n";
$c = fn(int $z) => $r->same($z) * 2; echo $c($n), "\n";
try { echo $r->same(intdiv($n, $n - 1)); } catch (Error $e) { echo $e->getMessage(), "\n"; } finally { echo $r->same($n), "\n"; }
"#);
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(
        strip_diagnostic_paths(&output.stdout).trim_end(),
        "123012\n20\n2\nDivision by zero\n1"
    );
    let diagnostics = strip_diagnostic_paths(&output.stderr);
    for expected in [] as [&str; 0] {
        assert!(diagnostics.contains(expected), "{expected} missing from {diagnostics}");
    }
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// The identity-call contexts of `nullrecv`: the output equals `php -n`'s, line numbers of notices
/// included (visibility, by-reference assignment, string by-reference parameters, generators,
/// loops, writes through the result, null receivers, inherited, trait, enum and override receivers).
#[test]
fn test_docblock_method_identity_call_matches_php_nullrecv() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class P { public int $p = 1; public int $u; }
class Reg {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function same($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    private function hid($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public static function st($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function gen($v) { yield $v; }
}
$r = new Reg;
$n = $argc;
$q = $n > 5 ? new Reg : null;
try { echo $q->same($n); } catch (Error $e) { echo $e->getMessage(), "\n"; }
echo $q?->same($n) ?? 'nul', "\n";
"#);
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(
        strip_diagnostic_paths(&output.stdout).trim_end(),
        "Call to a member function same() on null\nnul"
    );
    let diagnostics = strip_diagnostic_paths(&output.stderr);
    for expected in [] as [&str; 0] {
        assert!(diagnostics.contains(expected), "{expected} missing from {diagnostics}");
    }
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// The identity-call contexts of `propwrite`: the output equals `php -n`'s, line numbers of notices
/// included (visibility, by-reference assignment, string by-reference parameters, generators,
/// loops, writes through the result, null receivers, inherited, trait, enum and override receivers).
#[test]
fn test_docblock_method_identity_call_matches_php_propwrite() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class P { public int $p = 1; public int $u; }
class Reg {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function same($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    private function hid($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public static function st($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function gen($v) { yield $v; }
}
$r = new Reg;
$n = $argc;
$o = new P; $r->same($o)->p = 4; $r->same($o)->p++; echo $o->p, "\n";
$a = ['k' => $n]; echo $r->same($a)['k'] ?? 'no', isset($r->same($a)['z']) ? 'y' : 'n', "\n";
$b = $r->same($a); $b['k'] = 3; echo $a['k'], $b['k'], "\n";
"#);
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(
        strip_diagnostic_paths(&output.stdout).trim_end(),
        "5\n1n\n13"
    );
    let diagnostics = strip_diagnostic_paths(&output.stderr);
    for expected in [] as [&str; 0] {
        assert!(diagnostics.contains(expected), "{expected} missing from {diagnostics}");
    }
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// The identity-call contexts of `refassign`: the output equals `php -n`'s, line numbers of notices
/// included (visibility, by-reference assignment, string by-reference parameters, generators,
/// loops, writes through the result, null receivers, inherited, trait, enum and override receivers).
#[test]
fn test_docblock_method_identity_call_matches_php_refassign() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class P { public int $p = 1; public int $u; }
class Reg {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function same($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    private function hid($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public static function st($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function gen($v) { yield $v; }
}
$r = new Reg;
$n = $argc;
$x = $n; $y = &$r->same($x); $y = 8; echo $x, $y, "\n";
"#);
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(
        strip_diagnostic_paths(&output.stdout).trim_end(),
        "18"
    );
    let diagnostics = strip_diagnostic_paths(&output.stderr);
    for expected in ["Notice: Only variables should be assigned by reference on line 31"] {
        assert!(diagnostics.contains(expected), "{expected} missing from {diagnostics}");
    }
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// The identity-call contexts of `strs`: the output equals `php -n`'s, line numbers of notices
/// included (visibility, by-reference assignment, string by-reference parameters, generators,
/// loops, writes through the result, null receivers, inherited, trait, enum and override receivers).
#[test]
fn test_docblock_method_identity_call_matches_php_strs() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class P { public int $p = 1; public int $u; }
class Reg {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function same($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    private function hid($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public static function st($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function gen($v) { yield $v; }
}
$r = new Reg;
$n = $argc;
$s = str_repeat("a", $n); function app(string &$z) { $z .= "!"; }
app($r->same($s)); echo $s, "\n"; $t = $r->same($s); $t[0] = 'z'; echo $s, $t, "\n";
$f = $r->same(1.5 * $n); var_dump($f); var_dump($r->same((bool)$n)); var_dump($r->same(null));
"#);
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(
        strip_diagnostic_paths(&output.stdout).trim_end(),
        "a\naz\nfloat(1.5)\nbool(true)\nNULL"
    );
    let diagnostics = strip_diagnostic_paths(&output.stderr);
    for expected in ["Notice: Only variables should be passed by reference on line 32"] {
        assert!(diagnostics.contains(expected), "{expected} missing from {diagnostics}");
    }
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// The identity-call contexts of `visib`: the output equals `php -n`'s, line numbers of notices
/// included (visibility, by-reference assignment, string by-reference parameters, generators,
/// loops, writes through the result, null receivers, inherited, trait, enum and override receivers).
#[test]
fn test_docblock_method_identity_call_matches_php_visib() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class P { public int $p = 1; public int $u; }
class Reg {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function same($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    private function hid($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public static function st($v) { return $v; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function gen($v) { yield $v; }
}
$r = new Reg;
$n = $argc;
try { echo $r->hid($n); } catch (Error $e) { echo get_class($e), $e->getMessage(), "\n"; }
"#);
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(
        strip_diagnostic_paths(&output.stdout).trim_end(),
        "ErrorCall to private method Reg::hid() from global scope"
    );
    let diagnostics = strip_diagnostic_paths(&output.stderr);
    for expected in [] as [&str; 0] {
        assert!(diagnostics.contains(expected), "{expected} missing from {diagnostics}");
    }
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// The identity-call contexts of `audit`: the output equals `php -n`'s, line numbers of notices
/// included (visibility, by-reference assignment, string by-reference parameters, generators,
/// loops, writes through the result, null receivers, inherited, trait, enum and override receivers).
#[test]
fn test_docblock_method_identity_call_matches_php_audit() {
    let output = compile_and_run_with_heap_debug(r#"<?php
declare(strict_types=1);
abstract class Base {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function same($v) { return $v; }
}
class Kid extends Base {}
class Over extends Base { public function same($v) { return 99; } }
enum E: int {
    case A = 1;
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function same($v) { return $v; }
}
trait T {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function tsame($v) { return $v; }
}
class UT { use T; }
class UT2 { use T; public function tsame($v) { return -$v; } }
class Magic {
    public function __call($name, $args) { return "magic" . $name; }
    /**
     * @template T
     * @param T $v
     * @return T
     */
    private function hid($v) { return $v; }
}
class Prot {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    protected function pr($v) { return $v; }
    public function viaInside(int $n) { return $this->pr($n); }
}
class Sub extends Prot { public function viaSub(int $n) { return $this->pr($n); } }
class Misc {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    /**
     * @template T
     * @param T $v
     * @return T
     */
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function same($v) { return $v; }
}
function run(Base $b, Kid $k, Over $o, UT $u, UT2 $u2, Magic $m, Prot $p, Sub $s, Misc $x, ?Misc $q, int $n) {
    echo $b->same($n), $k->same($n), $o->same($n), "\n";
    echo E::A->same($n), $u->tsame($n), $u2->tsame($n), "\n";
    try { echo $p->pr($n), "\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
    echo $p->viaInside($n), $s->viaSub($n), "\n";
    $f = $x->same(...);
    echo $f($n), "\n";
    echo $x->same(v: $n), "\n";
    echo $x->same(...[$n]), "\n";
    echo $x->same($n, 2), "\n";
    echo $q?->same($n) ?? 'nul', "\n";
    echo $x->same($n) + 1, "\n";
}
run(new Kid(), new Kid(), new Over(), new UT(), new UT2(), new Magic(), new Prot(), new Sub(), new Misc(), new Misc(), 3);
"#);
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(
        strip_diagnostic_paths(&output.stdout).trim_end(),
        "3399\n33-3\nCall to protected method Prot::pr() from global scope\n33\n3\n3\n3\n3\n3\n4"
    );
    let diagnostics = strip_diagnostic_paths(&output.stderr);
    for expected in [] as [&str; 0] {
        assert!(diagnostics.contains(expected), "{expected} missing from {diagnostics}");
    }
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
