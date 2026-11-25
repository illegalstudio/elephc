//! Purpose:
//! Checks EvalIR produced through the Chopper AST against the existing eval parser.
//!
//! Called from:
//! - Magician's eval parser test target.
//!
//! Key details:
//! - Comparisons include the caller-scope mask and CLI superglobal metadata.

use crate::eval_ir::{EvalExpr, EvalStmt};
use crate::lexer::{tokenize, TokenKind};
use crate::errors::EvalParseError;
use crate::parser::{parse_fragment, state::FragmentInput};

/// Checks that lexical namespaces, aliases, and global fallback use the shared AST path.
#[test]
fn chopper_ast_preserves_eval_namespace_resolution() {
    for source in [
        r"namespace Eval\Ns; function local() { return __NAMESPACE__; } return local();",
        r"namespace Eval\Outer; use function Lib\outer as alias; namespace Inner { use function Lib\inner as alias; alias(); } return alias();",
        r"namespace Eval\Ns; use Lib\{Box as B, function Go as Alias, const VALUE as Local,}; function local(B $obj) { return ALIAS(Local, LOCAL, new b\Inner()); }",
        r"namespace Eval\Ns { class Box {} return new Box(); } namespace { return Box; }",
        r"namespace Eval\Ns; use Lib\Box as Box; namespace\helper(); return [namespace\LIMIT, new namespace\Box(), \strlen(...), helper(...)];",
        r"namespace Eval\Ns; use function Lib\One as Run; Run(); namespace Eval\Ns; return Run();",
        r"namespace Eval\Ns; use Lib\Err as Oops; try { throw new Oops(); } catch (Oops $e) { return __NAMESPACE__; }",
        r"namespace Eval\Ns; use Lib\Box as Alias; Alias::$value = 1; return [Alias::class, Alias::VALUE, Alias::read(...), Alias::read()];",
    ] {
        let program = parse_fragment(source.as_bytes()).unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert!(!program.statements().is_empty(), "{source}");
    }
    let source = br"namespace App; use function Lib\run as alias; return alias();";
    let program = parse_fragment(source).unwrap();
    assert!(matches!(program.statements(), [EvalStmt::Return(Some(EvalExpr::Call { name, .. }))] if name == "Lib\\run"));
}

/// Verifies computed members keep the receiver, member name and call arguments in EvalIR.
#[test]
fn chopper_ast_preserves_eval_computed_members() {
    for source in [
        r"return $object->$method();",
        r"return $object->{name()}(input: $value);",
        r"return $object?->{$method = 'read'}($argument = 5);",
        r"return $object->$method(...);",
        r"return $object->read(...);",
        r"return Service::$method(input: 2);",
        r"return Service::{$method}(...);",
        r"return $class::method();",
        r"return $class::{$method}(input: 2);",
        r"return $class::$method(...);",
        r"return [$class::class, $class::VALUE, $class::{name()}, $class::$property];",
        r"return [Service::${$property}, (factory())::${'items'}];",
        r"return +$argument;",
        r"if ($condition) +$argument;",
        r"return +2 ** 3 - +$argument;",
        r#"class RuntimeCounter { function add(&$value, $step) { $value += $step; return $value; } } $o = new RuntimeCounter(); $m = "add"; return $o->{$m}(step: 3, value: $seed);"#,
        r"$object = null; $object?->{$seed = 10}($seed = 11); return $seed;",
    ] {
        let program = parse_fragment(source.as_bytes()).unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert!(!program.statements().is_empty(), "{source}");
    }
}

/// Keeps computed nullsafe callables on the established error path.
#[test]
fn chopper_ast_rejects_eval_nullsafe_computed_callable() {
    let source = b"return $object?->$method(...);";
    assert_eq!(parse_fragment(source), Err(EvalParseError::UnsupportedConstruct));
}

/// Confirms Chopper retains the operands required by the runtime write kernels.
#[test]
fn chopper_ast_preserves_eval_member_writes() {
    for source in [
        r"$obj->{$member} = 3;",
        r"$obj->$member += 3;",
        r"$obj->{$member}[] = 3;",
        r"$obj->$member[$key] *= 3;",
        r"$obj->{$member} =& $other;",
        r"$obj->$member++;",
        r"++$obj->{$member};",
        r"echo $obj->{$member} += 3, $obj->$member++, ++$obj->{$member};",
        r"Service::${$property} = 2; ++$class::${name_expr()}; (factory())::${'items'}[] = 4;",
        r"$class::${$property}[$key] *= 6;",
        r"$class::${$property} =& $other;",
        r"$class::${$property}--;",
        r"$class::$property += 3;",
        r"Service::$property[$key] += 3;",
        r"$target[$key] += 2;",
    ] {
        let program = parse_fragment(source.as_bytes()).unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert!(!program.statements().is_empty(), "{source}");
    }
}

/// Verifies declaration metadata produced by Chopper survives conversion for reflection.
#[test]
fn chopper_ast_preserves_eval_class_like_declarations() {
    for source in [
        r"#[Example(7)] function example(#[Argument('text')] int $value): int { return $value; }",
        r"namespace A; use Lib\Hint as Hint; #[Hint('class')] class Example { #[Hint] public int $n = 3; #[Hint(name: 'method')] public function f(#[Hint] $v) { return $v; } }",
        r"#[Example] interface Contract { public function f(int $value): int; }",
        r"interface Contract { public int $value { get; set; } }",
        r"trait Shared { public function run() { return 1; } } class Person { use Shared { run as public start; } }",
        r"abstract class Base { abstract public function work(): int; }",
        r"abstract class Base { abstract public int $value { get; set; } }",
        r"readonly class Read { public int $x; }",
        r"class Hook { public int $x = 5 { get => $this->x; set(int $new) { $this->x = $new; } } }",
        r"class Hook { public string $x = 'start' { get => $this->x; } }",
        r"class Hook { public array $x = [1] { get => $this->x; } }",
        r"class Hook { public int $x { get => 5; } }",
        r"class Promote { function __construct(public int $x = 5) {} }",
        r"class Promote { function __construct(public int &$x) {} }",
        r"namespace Example; class Box extends Base { public ?self $copy; public function derive(self $other): static { return new static(); } public function base(): parent { return new parent(); } public function original(): self { return new self(); } }",
        r"#[Example] enum Color: string { case Red = 'r'; const SIZE = 1; #[Tag] case Blue = 'b'; }",
    ] {
        let program = parse_fragment(source.as_bytes()).unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert!(!program.statements().is_empty(), "{source}");
        for statement in program.statements() {
            let location = match statement {
                EvalStmt::ClassDecl(value) => value.source_location(),
                EvalStmt::InterfaceDecl(value) => value.source_location(),
                EvalStmt::TraitDecl(value) => value.source_location(),
                EvalStmt::EnumDecl(value) => {
                    assert_eq!(value.member_order(), ["Red", "SIZE", "Blue"]);
                    value.source_location()
                }
                _ => continue,
            };
            assert_eq!(location, Some(crate::eval_ir::EvalSourceLocation::single_line(1)));
        }
    }
}

/// Keeps specialization parsing in the original namespace and strict-types mode.
#[test]
fn chopper_ast_preserves_specialization_scope() {
    let source = r"function specialized(int $value) { return [Alias::class, namespace\one(), $value]; }";
    let mut input = FragmentInput::new(tokenize(source).unwrap(), source.len());
    input.namespace = "Project".to_string();
    input.strict_types = true;
    input.imports.insert_class("Alias".to_string(), "Lib\\Thing".to_string());
    let program = super::super::lower_shared_program(&input).unwrap();
    assert!(program.strict_types());
    assert!(matches!(program.statements(),
        [EvalStmt::FunctionDecl { name, body, .. }]
            if name == "Project\\specialized" && matches!(body.as_slice(),
                [EvalStmt::Return(Some(EvalExpr::Array(_)))])));
}

/// Verifies type-position substitutions without renaming same-spelled values.
#[test]
fn chopper_ast_preserves_eval_generic_headers() {
    for source in [
        r"namespace App; function identity<T>(T $value): T { trace(T); return $value; }",
        r"namespace App; function create<T>(T $input): T { return new T(); }",
        r"function name<T>(): string { return T::class; }",
        r"function matches<T>($value): bool { return $value instanceof T; }",
        r"class Box<T> { public T $value; public function get(): T { return $this->value; } }",
        r"interface Read<T> { public function get(): T; }",
        r"trait Store<T> { public T $value; }",
        r"class Service { public function run<T>(T $value): T { return $value; } }",
        r"class Service { public function __construct<T>(public T $value) {} }",
        r"namespace App; use function Lib\Identity as Alias; aLIAS<string>('ok'); ALias<int>(...); Missing<string>(...);",
        r"interface Contract<T> {} class Impl implements Contract<array<string, int>> {}",
        r"array<int> $values = [1, 2]; return $values;",
        r"namespace App; use Lib\Resource as Alias; return [$object->run<Alias>($value), Service::run<Alias>(...), $class::run<Alias>($value)];",
    ] {
        let program = parse_fragment(source.as_bytes()).unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert!(program.native_generic_syntax(), "{source}");
    }
}

/// Parses a fragment beyond the previous size limits.
#[test]
fn chopper_ast_handles_long_eval_fragments() {
    let mut source = String::from("$sum = 0;\n");
    source.push_str(&"$sum += 1; // enough text to cross the old byte limit\n".repeat(256));
    source.push_str("return $sum;");
    assert!(source.len() > 4096);
    assert!(tokenize(&source).unwrap().len() > 128);
    let program = parse_fragment(source.as_bytes()).unwrap();
    assert_eq!(program.statements().len(), 258);
}

/// Lowers a varied corpus of runtime fragments through Chopper.
#[test]
fn chopper_ast_lowers_supported_eval_fragments() {
    let strict = parse_fragment(b"declare(strict_types=1);").unwrap();
    assert!(strict.strict_types());
    assert!(strict.statements().is_empty());

    for source in [
        "$value = 3; echo $value, 5; return ($value + 1) * 2;",
        "$balance = ($balance - 1.5) / 2; return $balance;",
        "declare(strict_types=1); declare(strict_types=0); return 3;",
        "declare(strict_types=1); namespace App; class Example { public function number(int $n): int { return $n; } } return 0;",
        "return $_SERVER;",
        "return;",
        "return 'text';",
        "$n = 2; while ($n) { echo 'x'; $n = $n - 1; } return -$n;",
        "if (!$x) { echo 'zero'; } elseif ($x) { return true; } else { return null; }",
        "if ($a) { ?>\nHTML<?= 2 ?><?php }",
        "if ($a): echo 'a'; elseif ($b): echo 'b'; else: echo 'c'; endif;",
        "while ($running): continue; endwhile;",
        "foreach ([1, 2] as $point): echo $point; endforeach;",
        "return 'a' . $x + 2;",
        "return 1 << 2 . '3';",
        "return \"\\xff\";",
        "return \"hello $name, result {$a[0]}\";",
        "\nreturn [__LINE__, \"line={$a[__LINE__]}\"];",
        "return '\u{e000}';",
        "return 1 + 2 ** 3 ** 2;",
        "return -2 ** 2;",
        "return $x > 2 && ($x << 2) !== 8 || false;",
        "return $a <=> $b < $c;",
        "return (int) $value . 'x';",
        "print $a and $b;",
        "return clone $object;",
        "return $object instanceof Box;",
        "return $object instanceof $class;",
        "return include 'input' . '.php';",
        "require_once('input.php');",
        "return $a & $b ^ $c | $d;",
        "return $a and $b xor $c or $d;",
        "return $a ?? $b ?: $c;",
        "return $a ? $b : $c;",
        "return PHP_VERSION;",
        "return strlen('hello');",
        "return $fn();",
        "return $fn(...);",
        "return strlen(...);",
        "return custom(a: $value, b: [1, 2]);",
        "return custom(...$args);",
        "f(); return [1, 2][0];",
        "return [5 => 1, 2];",
        "return [2, $key => 3, 4];",
        "return array(2, $key => 3, 4);",
        "return [&$x, 'other' => &$y];",
        "$items = array(&$x, 'other' => &$x); $items[0] = 12; return $items['other'];",
        "$target += 2; return ($target -= 1);",
        "$target++; --$target; return $target;",
        "$a[7] = 19; $a[] = 20; return $a;",
        "global $source; $local =& $source; return $local;",
        "$seed = 1 and $flag;",
        "$sum = 0; for ($i = 0; $i < 3; $i = $i + 1) { if ($sum) break; $sum = $sum + $i; } return $sum;",
        "while ($running) { continue; } throw $failure;",
        "try { throw new Exception('stop'); } catch (Exception|Error $failure) { echo 'caught'; } finally { echo 'done'; }",
        "function sample($x = 4, &$y = null, ...$rest) {\n return $x;\n } return sample();",
        "function typed(?int $x, string ...$words): int { return $x ?? 0; } return typed(3, 'word');",
        "function typed(int|null|string $x): void { return; }",
        "function typed(Input&Record $x): int { return 1; }",
        "function typed(null $x): null { return null; }",
        "function typed(true $x) {} function nullable(TRUE|null $x) {}",
        "function object_arg(Widget $w): Widget { return $w; }",
        "$o->ECHO = 42; echo $o?->ECHO; return $o->query('x');",
        "return $o?->next()->VALUE;",
        "return $o->$member;",
        "return new $class(4);",
        "echo new C < $limit; echo $value instanceof C < 1;",
        "function tick() { static $ticks = 0; $ticks = $ticks + 1; return $ticks; } return tick() + tick();",
        "class StaticSample { public const FACTOR = 2; public static int $value = 1; public static function read(): int { return self::$value * self::FACTOR; } } StaticSample::$value = 5; return StaticSample::read();",
        "final class Sample {\n private int $base = 4;\n public function add(int $step): int {\n return $this->base + $step;\n }\n } $record = new Sample(); return $record->add(3);",
        "return \\strlen('length') + Tools\\step(2);",
        "return match($key) { 1, 2 => 'first', default => 'other' };",
        "switch ($key) { case 1: case 2: echo 'one'; break; default: case 3: echo 'other'; }",
        "switch ($key): case 1: echo 'one'; default: echo 'other'; endswitch;",
        "goto done; echo 'unreached'; done: return $value;",
        "function jump() { goto done; done: return 2; } return jump();",
        "unset($name, $array[0], $object->VALUE); return isset($name);",
        "return eval('return 3;');",
        "function example() {\n return __FUNCTION__;\n }\n return [__FILE__, __NAMESPACE__, example()];",
        "foreach ([1, 2] as $key => $item) { echo $key, $item; }",
        "do { $balance = $balance - 1; } while ($balance > 0);",
    ] {
        let program = parse_fragment(source.as_bytes()).unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert!(!program.statements().is_empty(), "{source}");
    }
}

/// Accepts the trailing commas PHP 8 allows in parameter, closure `use` and `unset()` lists.
#[test]
fn chopper_accepts_trailing_commas_in_runtime_lists() {
    for source in [
        "function sample($x,) {}",
        "$f = function ($x,) {};",
        "unset($name,);",
        "class Sample { public function call($a,) {} }",
        "$f = function () use ($value,) {};",
        "$f = function () use ($first, &$second,) {};",
    ] {
        parse_fragment(source.as_bytes()).unwrap_or_else(|error| panic!("{source}: {error:?}"));
    }
}

/// Rejects invalid syntax and unsupported runtime constructs.
#[test]
fn chopper_rejects_invalid_runtime_fragments() {
    for source in [
        "while ($a) break 1;",
        "for ($i = 0, $j = 0; $i < 3; $i = $i + 1) {}",
        "try { echo 1; } catch (Exception) echo 2;",
        "function sample($x = f()) {}",
        "enum();",
        "switch ($key) { case 1; echo 'one'; }",
        "class Sample { public callable $value; }",
        "function tick() { static $a = 1, $b = 2; }",
        "$f = function () use ($value, $value) {};",
        "$f = function () use () {};",
        "$f = function () use (,) {};",
        "function sample(,) {}",
        "declare(ticks=1); return 8;",
        "declare(strict_types=1, ticks=1); return 8;",
        "declare(strict_types=1) { return 8; }",
    ] {
        assert!(parse_fragment(source.as_bytes()).is_err(), "{source}");
    }
    for source in ["{}", "return 1 +;", "<?php return 1;"] {
        assert!(parse_fragment(source.as_bytes()).is_err(), "{source}");
    }
    for source in ["goto missing;", "done: done: return;", "function bad_jump() { goto missing; }"] {
        assert_eq!(parse_fragment(source.as_bytes()), Err(EvalParseError::InvalidJump), "{source}");
    }
}

/// Verifies that only the actual end-of-input terminator can be repaired by the REPL.
#[test]
fn chopper_reports_missing_semicolon_at_the_unconsumed_token() {
    for (source, at_eof) in [("echo 1", true), ("echo 1 echo 2;", false)] {
        let parser = FragmentInput::new(tokenize(source).unwrap(), source.len());
        let Err(failure) = super::super::lower_shared_program(&parser) else {
            panic!("expected a missing semicolon for {source}");
        };
        assert_eq!(failure.missing_final_semicolon(&parser.tokens), at_eof);
        assert_eq!(parse_fragment(source.as_bytes()), Err(crate::errors::EvalParseError::ExpectedSemicolon));
    }
}

/// Verifies Chopper's hoisted declaration is lowered at its lexical construction site.
#[test]
fn chopper_ast_preserves_eval_anonymous_class() {
    let source = r"namespace App;
use Package\Base as Alias;
return new class(3) extends Alias implements Task {
    private int $count = 1;
    public function run(): string { return __NAMESPACE__; }
};";
    let program = parse_fragment(source.as_bytes()).unwrap();
    let [EvalStmt::Return(Some(EvalExpr::NewAnonymousClass { class, args }))] = program.statements() else {
        panic!("expected one anonymous class construction");
    };
    assert_eq!(args.len(), 1);
    assert!(class.is_anonymous());
    assert_eq!(class.source_location(), Some(crate::eval_ir::EvalSourceLocation::new(3, 6)));
    assert_eq!(class.parent(), Some("Package\\Base"));
    assert_eq!(class.interfaces(), ["App\\Task"]);
    assert_eq!(class.properties()[0].name(), "count");
    assert_eq!(class.methods()[0].name(), "run");
    assert!(class.name().starts_with("class@anonymous#eval"));
}

/// Retains closure signatures, captures, bodies and reflection ranges.
#[test]
fn chopper_ast_preserves_eval_closure_captures() {
    let source = "\n$f = static function(int $step) use ($value, &$counter): int {\n
        $counter += $step;\n
        return $value + $counter;\n
    }; return $f(2);";
    let program = parse_fragment(source.as_bytes()).unwrap();
    let [EvalStmt::StoreVar { name, value: EvalExpr::Closure { function, captures, is_static } },
        EvalStmt::Return(Some(_))] = program.statements() else {
        panic!("expected a closure store followed by its call");
    };
    assert_eq!(name, "f");
    assert!(*is_static);
    assert_eq!(captures, &[
        crate::eval_ir::EvalClosureCapture::new("value", false),
        crate::eval_ir::EvalClosureCapture::new("counter", true),
    ]);
    assert_eq!(function.params(), ["step"]);
    assert_eq!(function.body().len(), 2);
    assert_eq!(function.source_location(), Some(crate::eval_ir::EvalSourceLocation::new(2, 8)));
}

/// Compares the binding and operand order of adjacent shared binary operators.
#[test]
fn chopper_ast_preserves_eval_binary_precedence() {
    use crate::eval_ir::EvalBinOp;
    for (source, root, nested, nested_on_right) in [
        ("return $a + $b * $c;", EvalBinOp::Add, EvalBinOp::Mul, true),
        ("return $a . $b + $c;", EvalBinOp::Concat, EvalBinOp::Add, true),
        ("return $a ** $b ** $c;", EvalBinOp::Pow, EvalBinOp::Pow, true),
        ("return $a - $b - $c;", EvalBinOp::Sub, EvalBinOp::Sub, false),
        ("return $a <=> $b == $c;", EvalBinOp::LooseEq, EvalBinOp::Spaceship, false),
    ] {
        let program = parse_fragment(source.as_bytes()).unwrap();
        let [EvalStmt::Return(Some(EvalExpr::Binary { op, left, right }))] = program.statements()
            else { panic!("expected binary expression: {source}"); };
        assert_eq!(*op, root);
        let child = if nested_on_right { right } else { left };
        assert!(matches!(child.as_ref(), EvalExpr::Binary { op, .. } if *op == nested), "{source}");
    }
}

/// Checks runtime-specific trivia, source lines and numeric token boundaries.
#[test]
fn chopper_scans_tagless_eval_with_runtime_token_boundaries() {
    use crate::eval_ir::EvalMagicConst;
    let tokens = tokenize("// comment\rmore\necho\u{00a0}__LINE__;").unwrap();
    assert_eq!(tokens, [
        crate::lexer::Token::new(TokenKind::Ident("echo".into()), 2),
        crate::lexer::Token::new(TokenKind::Magic(EvalMagicConst::Line(2)), 2),
        crate::lexer::Token::new(TokenKind::Semicolon, 2),
        crate::lexer::Token::new(TokenKind::Eof, 2),
    ]);
    for source in ["return .5;", "return 1.;"] {
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.iter().filter(|token| matches!(token.kind(), TokenKind::Dot)).count(), 1);
    }
    assert_eq!(tokenize("return $città;"), Err(EvalParseError::UnexpectedToken));
}
