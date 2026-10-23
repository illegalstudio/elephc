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

/// An unrelated object is rejected against a method-local PHPDoc bound in both strict modes.
#[test]
fn test_docblock_method_templates_enforce_bounds() {
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
        let error = compile_cli_file_with_flags_expect_failure(source, flags);
        assert!(error.contains("does not satisfy its bound"), "{error}");
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

/// An enum method's PHPDoc template specializes per call and enforces its bound, like a class's.
///
/// Enum methods were skipped, so `@template T of int` was never checked on `Id::A->id("seven")`.
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
        let error =
            compile_cli_file_with_flags_expect_failure(&template.replace("ARG", "\"seven\""), flags);
        assert!(error.contains("does not satisfy its bound int"), "{error}");
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
    let error = compile_cli_file_with_flags_expect_failure(bounded, &[]);
    assert!(error.contains("does not satisfy its bound int"), "{error}");
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
        let error = compile_cli_file_with_flags_expect_failure(&source, &[]);
        assert!(error.contains("does not satisfy its bound int"), "{source}: {error}");
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
        let error = compile_cli_file_with_flags_expect_failure(&source, &[]);
        assert!(error.contains("does not satisfy its bound int"), "{comment}: {error}");
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
/// The violating argument is still refused, by a different route: with `@param` unread, `T`
/// stays undetermined and the call errors. The multi-line form binds every tag and reports the
/// bound violation.
#[test]
fn test_one_line_block_reads_its_opening_tag() {
    let bound = "/** @template T of int @param T $v @return T */\n";
    let function_case =
        format!("<?php\n{bound}function id($v) {{ return $v; }}\necho id(\"seven\");\n");
    let error = compile_cli_file_with_flags_expect_failure(&function_case, &[]);
    assert!(
        error.contains("does not determine type parameter <T>"),
        "{function_case}: {error}"
    );
    // `docblock/members.rs`: a METHOD template that no parameter
    // or return position adopts binds nothing at a call, so it is dropped and the method compiles
    // untyped, as php-src runs it, instead of failing as an undetermined call.
    let method_cases = [
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
    for source in &method_cases {
        assert_eq!(compile_cli_file_and_run_with_flags(source, &[]), "seven", "{source}");
    }
    let multiline = "<?php\n/** @template T of int\n * @param T $v\n * @return T\n */\n\
                     function id($v) { return $v; }\necho id(\"seven\");\n";
    let error = compile_cli_file_with_flags_expect_failure(multiline, &[]);
    assert!(
        error.contains("does not satisfy its bound int"),
        "{multiline}: {error}"
    );
    let ok = "<?php\n/** @template T of int\n * @param T $v\n * @return T\n */\n\
              function id($v) { return $v; }\necho id(7);\n";
    assert_eq!(compile_cli_file_and_run_with_flags(ok, &[]), "7");
}

/// A method template whose only `@param` loses to a native `mixed` binds nothing at a call, so
/// it is dropped together with the `@return T` it was adopted into: ordinary calls keep
/// compiling with the native signature, as php-src runs them. A sibling whose `@param` is adopted
/// still specializes.
#[test]
fn test_docblock_method_template_lost_to_native_mixed_still_compiles() {
    let source = r#"<?php
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
echo $repo->id(7), "|", $repo->id("x"), "|", $repo->same(8) + 1, "|", $repo->same("y");
"#;
    for flags in [&[][..], &["--strict-php"][..]] {
        assert_eq!(compile_cli_file_and_run_with_flags(source, flags), "7|x|9|y");
    }
}
