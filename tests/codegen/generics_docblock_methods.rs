//! Purpose:
//! Exercises method-local PHPDoc templates through specialization and native execution.
//! Covers scalar and container storage, class scope, traits, bounds and call argument forms.
//!
//! Called from:
//! - `cargo test --test codegen_tests_oop generics_docblock_methods`.
//!
//! Key details:
//! - The CLI regression checks that PHPDoc stays active with and without strict PHP.
//! - Runtime class names distinguish concrete specializations from an ignored annotation.

use crate::support::*;

/// Constructor and ordinary methods on the class line retain their class-scoped types.
#[test]
fn test_docblock_shared_class_and_method_line() {
    let source = r#"<?php
/** @template T */
class Box { public function __construct(public T $value) {} public function id(T $v): T { return $v; } }
$box = new Box(7);
echo get_class($box), ":", $box->value, "|", $box->id(8);
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), "Box<int>:7|8");
    }
}

/// A same-line class annotation determines the concrete class in both PHP modes.
#[test]
fn test_docblock_same_line_class_declaration() {
    let source = r#"<?php
/** @template T */ class Box {
    public function __construct(public T $value) {}
}
$box = new Box<int>(7);
echo get_class($box), ":", $box->value;
"#;
    assert_eq!(compile_and_run(source), "Box<int>:7");
    let portable = source.replace("Box<int>(7)", "Box(7)");
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(&portable, flags), "Box<int>:7");
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
            "Box<int>:8|Box<string>:SEVEN",
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
    assert_eq!(out, "7|seven|Box<string>:eight|Box<int>:9");
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
class Repo {
    /**
     * @template E of Entity
     * @param E $entity
     */
    public function accept($entity): int { return 1; }
}
echo (new Repo())->accept(new Other());
"#;
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
    let source = r#"<?php
class Repo {
    /**
     * @template T
     * @param T $value
     * @return array<T>
     */
    public function values($value): array { return ["wrong"]; }
}
(new Repo())->values(7);
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        let error = compile_cli_file_with_flags_expect_failure(source, flags);
        assert!(error.contains("array<int>") && error.contains("array<string>"), "{error}");
    }
}

/// The portable example keeps function, class and method PHPDoc active in both CLI modes.
#[test]
fn test_docblock_method_templates_example_runs_in_both_modes() {
    let source = include_str!("../../examples/generics-docblock/main.php");
    let expected = "42|hi|ok\n7|ab\n42|21\n10|x\n7|seven\n8|EIGHT\nBox<int>|Box<string>|Pair<int>\n";
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), expected, "{flags:?}");
    }
}

/// A doc comment closed on the declaration's own line still binds to it, and a member sharing
/// the class's line does not adopt the class's `@template`.
///
/// `collect` keyed every block to the line AFTER `*/`, so `/** @template T */ class Box` lost
/// its template and `new Box<int>(7)` was refused. With the class and its constructor on one
/// line, the class block was also copied onto the constructor, which then expected no argument.
#[test]
fn test_docblock_on_the_declaration_line_binds_to_it() {
    let sources = [
        "<?php\n/** @template T */ class Box { public function __construct(public T $v) {} }\n\
         $b = new Box<int>(7);\necho get_class($b), \":\", $b->v;\n",
        "<?php\n/** @template T */\nclass Box { public function __construct(public T $v) {} }\n\
         $b = new Box<int>(7);\necho get_class($b), \":\", $b->v;\n",
    ];
    // Without `--strict-php` only: the written `new Box<int>` is an elephc extension it refuses.
    for source in sources {
        assert_eq!(compile_cli_file_and_run_with_flags(source, &[]), "Box<int>:7", "{source}");
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
            "Box<int>:5|Box<string>:x",
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
         class Box { public function __construct(public T $v) {} }\n\
         $b = new Box<int>(7);\necho get_class($b), \":\", $b->v;\n",
        "<?php\n/** @template T */ // kept\n\
         class Box { public function __construct(public T $v) {} }\n\
         $b = new Box<int>(7);\necho get_class($b), \":\", $b->v;\n",
    ];
    for source in sources {
        assert_eq!(compile_cli_file_and_run_with_flags(source, &[]), "Box<int>:7", "{source}");
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
        "<?php\n/** @template T */ /* kept */ class Box { public function __construct(public T $v) {} }\n\
         $b = new Box<int>(7);\necho get_class($b), \":\", $b->v;\n",
        "<?php\n/** @template T */\n/*\n * kept\n */\nclass Box { public function __construct(public T $v) {} }\n\
         $b = new Box<int>(7);\necho get_class($b), \":\", $b->v;\n",
        "<?php\n#[Attribute]\nclass Marker {}\n/** @template T */\n/* kept */\n#[Marker]\n\
         class Box { public function __construct(public T $v) {} }\n\
         $b = new Box<int>(7);\necho get_class($b), \":\", $b->v;\n",
    ];
    for source in class_sources {
        assert_eq!(compile_cli_file_and_run_with_flags(source, &[]), "Box<int>:7", "{source}");
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
/// dropped the `@template`: the class compiled as non-generic, and a function, static method or
/// enum method lost its `of int` bound.
#[test]
fn test_star_banner_comment_keeps_the_docblock_above_it() {
    let banner = "<?php\n/** @template T */\n/***\n * banner\n */\n\
                  class Box { public function __construct(public T $v) {} }\n\
                  $b = new Box<int>(7);\necho get_class($b), \":\", $b->v;\n";
    assert_eq!(compile_cli_file_and_run_with_flags(banner, &[]), "Box<int>:7");
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
             class Box {{ public function __construct(public T $v) {{}} }}\n\
             $b = new Box<int>(7);\necho get_class($b), \":\", $b->v;\n"
        );
        assert_eq!(
            compile_cli_file_and_run_with_flags(&source, &[]),
            "Box<int>:7",
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
    for (call, expected) in [
        ("(new Repo())->keep(new Other());", "expects Model, got Other"),
        ("pair(new User(), new Other());", "expects Model, got Other"),
        ("pair<Other>(new Other(), new Other());", "expects Model, got Other"),
        ("same(5);", "expects object, got int"),
    ] {
        let error = compile_cli_file_with_flags_expect_failure(&format!("{declarations}{call}\n"), &[]);
        assert!(error.contains(expected), "{call}: {error}");
    }
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
    let error = compile_cli_file_with_flags_expect_failure(
        &format!("{declarations}both(new Other(), new Other());\n"),
        &[],
    );
    assert!(error.contains("expects Model, got Other"), "{error}");
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
    for call in ["strict(null);", "union(null);", "(new Repo())->accept(null);"] {
        let error =
            compile_cli_file_with_flags_expect_failure(&format!("{declarations}{call}\n"), &[]);
        assert!(error.contains("expects Model"), "{call}: {error}");
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
    for (call, expected) in [
        ("id(new Other());", "expects Model, got Other"),
        ("thing(1);", "expects object, got int"),
        ("wide(new Other());", "expects Model, got Other"),
    ] {
        let error =
            compile_cli_file_with_flags_expect_failure(&format!("{declarations}{call}\n"), &[]);
        assert!(error.contains(expected), "{call}: {error}");
    }
    let wrong_body = r#"<?php
class Model {}
class Other {}
/**
 * @template T of Model
 * @return T
 */
function make(): Model { return new Other(); }
make();
"#;
    let error = compile_cli_file_with_flags_expect_failure(wrong_body, &[]);
    assert!(error.contains("return type expects Model, got Other"), "{error}");
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
    let error = compile_cli_file_with_flags_expect_failure(
        &format!("{declarations}(new Repo())->accept(new Other(), 1);\n"),
        &[],
    );
    assert!(error.contains("expects Model, got Other"), "{error}");
}

/// The method path keeps the native hint when a refused `@param` cannot bind the return
/// template, exactly like the function path.
#[test]
fn test_docblock_method_return_template_without_a_binding_keeps_the_hint() {
    let source = r#"<?php
class Holder {
    /**
     * @template T
     * @param T $v
     * @return T
     */
    public function id(int $v) { return $v; }
}
echo (new Holder)->id(1), "\n";
"#;
    assert_eq!(compile_cli_file_and_run_with_flags(source, &[]), "1\n");
    let rejected = source.replace("->id(1)", "->id(\"seven\")");
    let error = compile_cli_file_with_flags_expect_failure(&rejected, &[]);
    assert!(error.contains("expects int"), "{error}");
}

/// A return-only PHPDoc template stays usable through an explicit type argument or a default.
#[test]
fn test_docblock_return_only_template_binds_explicitly_or_by_default() {
    let source = r#"<?php
/**
 * @template T
 * @return T
 */
function make() { return 7; }
class Maker {
    /**
     * @template T
     * @return T
     */
    public function make() { return 8; }
}
/**
 * @template T = int
 * @return T
 */
function defaulted() { return 9; }
echo make<int>(), "|", (new Maker)->make<int>(), "|", defaulted(), "\n";
"#;
    assert_eq!(compile_cli_file_and_run_with_flags(source, &[]), "7|8|9\n");
}

/// A written `T` parameter binds a method template even without a matching `@param` tag.
#[test]
fn test_docblock_written_parameter_binds_the_return_template() {
    let source = r#"<?php
class Holder {
    /**
     * @template T
     * @return array<T>
     */
    public function values(T $value): array { return ["wrong"]; }
}
(new Holder)->values(7);
"#;
    let error = compile_cli_file_with_flags_expect_failure(source, &[]);
    assert!(error.contains("array<int>") && error.contains("array<string>"), "{error}");
}
