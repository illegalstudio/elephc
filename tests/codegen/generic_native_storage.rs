//! Purpose:
//! Regression tests for native storage widths used as generic type arguments.
//!
//! Called from:
//! - `codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - Method values use PHP scalar carriers while buffers retain their declared stride.

use crate::support::*;

/// Verifies the original generic cell accepts an integer through its `u32` specialization.
#[test]
fn test_generic_native_storage_carriers_minimal() {
    let output = compile_and_run(
        r#"<?php
final class Cell<T> {
    private buffer<T> $data;
    public function __construct() { $this->data = buffer_new<T>(1); }
    public function set(T $value): void { $this->data[0] = $value; }
    public function get(): T { return $this->data[0]; }
}
$cell = new Cell<u32>();
$cell->set(17);
echo $cell->get(), "\n";
"#,
    );
    assert_eq!(output, "17\n");
}

/// Builds a width matrix covering native strides, scalar carriers, and storage conversions.
fn native_storage_matrix() -> (String, String) {
    let mut source = String::from(
        r#"<?php
namespace NativeStorage;
final class NativeBuffer<T> {
    private buffer<T> $data;
    public function __construct(private int $length, T $seed) {
        $this->data = buffer_new<T>($length);
        $this->fill($seed);
    }
    public function set(int $index, T $value): void { $this->data[$index] = $value; }
    public function get(int $index): T { return $this->data[$index]; }
    public function fill(T $value): void {
        for ($i = 0; $i < $this->length; $i++) { $this->data[$i] = $value; }
    }
    public function native(): buffer<T> { return $this->data; }
    public function identity(T $value): T { return $value; }
}
"#,
    );
    let mut expected = String::new();
    for (width, value, stored, stride, read) in [
        ("i8", "200", "-56", 1, "ptr_read8"),
        ("u8", "257", "1", 1, "ptr_read8"),
        ("i16", "40000", "-25536", 2, "ptr_read16"),
        ("u16", "70000", "4464", 2, "ptr_read16"),
        ("i32", "5000000000", "705032704", 4, "ptr_read32"),
        ("u32", "4294967295", "4294967295", 4, "ptr_read32"),
        ("i64", "-1", "-1", 8, "ptr_get"),
        ("u64", "-1", "-1", 8, "ptr_get"),
        ("f32", "1.1", "1.1000000238419", 4, "ptr_read_f32"),
        ("f64", "1.1", "1.1", 8, "ptr_read_f64"),
    ] {
        let is_float = width.starts_with('f');
        let replacement = if is_float { "1.5" } else { "17" };
        let carrier = if is_float { "double" } else { "integer" };
        source.push_str(&format!(
            r#"
$b_{width} = new NativeBuffer<{width}>(3, {value});
$b_{width}->set(1, {replacement});
$raw_{width} = $b_{width}->native();
echo $b_{width}->get(0), " ", $b_{width}->get(1), " ", $b_{width}->get(2), " ",
    {read}(ptr_offset(buffer_ptr($raw_{width}), {stride})), " ",
    $b_{width}->identity({value}), " ", gettype($b_{width}->get(0)), "\n";
"#,
        ));
        expected.push_str(&format!(
            "{stored} {replacement} {stored} {replacement} {value} {carrier}\n"
        ));
    }
    (source, expected)
}

/// Verifies all supported widths keep their native stride and PHP scalar method ABI.
#[test]
fn test_generic_native_storage_carriers_width_matrix() {
    let (source, expected) = native_storage_matrix();
    assert_eq!(compile_and_run(&source), expected);
}

/// Verifies unoptimized buffer accesses agree with the optimized generic width matrix.
#[test]
fn test_generic_native_storage_carriers_width_matrix_without_ir_opt() {
    let (source, expected) = native_storage_matrix();
    without_ir_opt(|| assert_eq!(compile_and_run(&source), expected));
}

/// Verifies scalar substitution reaches properties, locals, closures, inheritance and callables.
#[test]
fn test_generic_native_storage_carriers_value_signatures() {
    let output = compile_and_run(
        r#"<?php
interface NativeValue<T> { public function get(): T; }
class ValueBox<T> implements NativeValue<T> {
    public function __construct(private T $value) {}
    public function get(): T { return $this->value; }
    public function copy(): T {
        T $local = $this->value;
        $identity = fn(T $value): T => $value;
        return $identity($local);
    }
    public function maybe(): ?T { return $this->value; }
    public function choose(T ...$values): T { return $values[0]; }
    public function same<U>(U $value): U { return $value; }
    public static function of(T $value): self { return new self($value); }
}

class ChildValueBox<T> extends ValueBox<T> {}
function carry<T>(T $value): T { return $value; }
function singleton<T>(T $value): array<T> { T $copy = $value; return [$copy]; }
$box = new ChildValueBox<u8>(257);
echo $box->get(), " ", $box->copy(), " ", $box->maybe(), " ", $box->choose(258, 259), "\n";
echo $box instanceof ValueBox<u8> ? "width" : "wrong", " ",
    $box instanceof ValueBox<int> ? "wrong" : "distinct", "\n";
echo ValueBox<u32>::of(4294967295)->get(), " ", carry<u32>(4294967295), "\n";
echo $box->same<f32>(1.1), " ", carry<f32>(1.1), " ", singleton<u16>(70000)[0], "\n";
echo is_float(carry<u8>(PHP_INT_MAX) + 1) ? "float\n" : "wrong\n";
"#,
    );
    assert_eq!(
        output,
        "257 257 257 258\nwidth distinct\n4294967295 4294967295\n1.1 1.1 70000\nfloat\n"
    );
}

/// Verifies classes named like widths remain object types when used as generic arguments.
#[test]
fn test_generic_native_storage_carriers_shadowing() {
    let output = compile_and_run(
        r#"<?php
final class u8 { public function __construct(public int $value) {} }
final class WidthNamedBox<T> {
    public function __construct(private T $value) {}
    public function get(): T { return $this->value; }
    public function identity<U>(U $value): U { return $value; }
}
function keepWidthNamed<T>(T $value): T { return $value; }
$box = new WidthNamedBox<u8>(new u8(17));
echo $box->get()->value, " ", keepWidthNamed<u8>(new u8(18))->value, " ",
    $box->identity<u8>(new u8(19))->value, "\n";
$inferred = new WidthNamedBox(new u8(20));
echo $inferred->get()->value, " ", keepWidthNamed(new u8(21))->value, " ",
    $box->identity(new u8(22))->value, "\n";
"#,
    );
    assert_eq!(output, "17 18 19\n20 21 22\n");
}

/// Verifies carriers recurse through compound bindings while nested buffers retain storage widths.
#[test]
fn test_generic_native_storage_carriers_compound_bindings() {
    let output = compile_and_run(
        r#"<?php
final class CarrierHolder<T> {
    public function __construct(private T $value) {}
    public function get(): T { return $this->value; }
}
final class CarrierRows<T> {
    private CarrierHolder<array<T>> $rows;
    private CarrierHolder<?T> $optional;
    private CarrierHolder<array<string, T>> $map;
    public function __construct(T $seed) {
        $this->rows = new CarrierHolder<array<T>>([$seed]);
        $this->optional = new CarrierHolder<?T>($seed);
        $this->map = new CarrierHolder<array<string, T>>(["value" => $seed]);
    }
    public function first(): T { return $this->rows->get()[0]; }
    public function optional(): ?T { return $this->optional->get(); }
    public function mapped(): T { return $this->map->get()["value"]; }
}
final class CarrierStorage<T> {
    private CarrierHolder<buffer<T>> $owner;
    public function __construct(T $seed) {
        $data = buffer_new<T>(2);
        $data[1] = $seed;
        $this->owner = new CarrierHolder<buffer<T>>($data);
    }
    public function get(): T { return $this->owner->get()[1]; }
}
$rows = new CarrierRows<u32>(4294967295);
$floats = new CarrierRows<f32>(1.1);
echo $rows->first(), " ", $rows->optional(), " ", $rows->mapped(), "\n";
echo $floats->first(), " ", $floats->optional(), " ", $floats->mapped(), "\n";
echo (new CarrierStorage<u8>(257))->get(), "\n";
"#,
    );
    assert_eq!(output, "4294967295 4294967295 4294967295\n1.1 1.1 1.1\n1\n");
}

/// Verifies written calls use caller namespace visibility and preserve captured type arguments.
#[test]
fn test_generic_native_storage_carriers_namespace_visibility() {
    let output = compile_and_run(
        r#"<?php
namespace {
    final class u8 { public function __construct(public int $value) {} }
    function nativeFirst<T>(buffer<T> $data): int { return $data[0]; }
}
namespace WidthScope {
    use \u8 as ByteObject;
    final class ScopedHolder<T> {
        public function __construct(private T $value) {}
        public function get(): T { return $this->value; }
        public static function keep(T $value): T { return $value; }
    }
    final class ScopedMethods {
        public function identity<T>(T $value): T { return $value; }
    }
    function scopedCarry<T>(T $value): T { return $value; }
    function scopedRepeat<T>(T $value): T { return scopedCarry<T>($value); }
    $bytes = buffer_new<u8>(1);
    $bytes[0] = 257;
    $methods = new ScopedMethods();
    echo \nativeFirst<u8>($bytes), " ", scopedCarry<u8>(257), " ", scopedRepeat<u8>(258), " ",
        $methods->identity<u8>(259), " ", ScopedHolder<u8>::keep(260), "\n";
    $scalar = new ScopedHolder<u8>(261);
    $object = new ScopedHolder<ByteObject>(new ByteObject(17));
    echo $scalar->get(), " ", $object->get()->value, " ",
        scopedCarry<ByteObject>(new ByteObject(18))->value, " ",
        scopedRepeat<ByteObject>(new ByteObject(19))->value, " ",
        $methods->identity<ByteObject>(new ByteObject(20))->value, "\n";
    echo $object instanceof ScopedHolder<ByteObject> ? "class" : "wrong", " ",
        $object instanceof ScopedHolder<u8> ? "wrong" : "distinct", "\n";
}
"#,
    );
    assert_eq!(output, "1 257 258 259 260\n261 17 18 19 20\nclass distinct\n");
}

/// Verifies scalar bounds use PHP carriers through class, function, method and deferred construction.
#[test]
fn test_generic_native_storage_carriers_scalar_bounds() {
    let output = compile_and_run(
        r#"<?php
final class BoundedWidth<T : int> {
    private buffer<T> $data;
    public function __construct(T $seed) { $this->data = buffer_new<T>(1); $this->data[0] = $seed; }
    public function get(): T { return $this->data[0]; }
    public function dependent<U : T>(U $value): U { return $value; }
}
final class BoundedFloat<T : float> {
    public function __construct(private T $value) {}
    public function get(): T { return $this->value; }
}
final class BoundedWidthMethods {
    public function keep<T : int>(T $value): T { return $value; }
}
function boundedCarry<T : int>(T $value): T { return $value; }
function boundedMake<T : int>(T $value): BoundedWidth<T> { return new BoundedWidth<T>($value); }
$words = boundedMake<u32>(4294967295);
$methods = new BoundedWidthMethods();
echo $words->get(), " ", boundedCarry<u8>(257), " ", $methods->keep<u8>(258), " ",
    $words->dependent(259), " ", (new BoundedFloat<f32>(1.1))->get(), "\n";
"#,
    );
    assert_eq!(output, "4294967295 257 258 259 1.1\n");
}

/// Verifies explicit root qualifiers survive source encoding while canonical class identities stay stable.
#[test]
fn test_generic_native_storage_carriers_rooted_call_arguments() {
    let output = compile_and_run(
        r#"<?php
namespace { class Thing { public function __construct(public int $v) {} } }
namespace Lib {
    class Item { public function __construct(public int $v) {} }
    final class Wrapper<T> { public function __construct(public T $value) {} }
}
namespace App {
    function keep<T>(T $value): T { return $value; }
    final class M { public function keep<T>(T $value): T { return $value; } }
    echo keep<\Thing>(new \Thing(1))->v, " ", keep<\Lib\Item>(new \Lib\Item(2))->v, " ",
        (new M())->keep<\Thing>(new \Thing(3))->v, " ",
        (new M())->keep<\Lib\Item>(new \Lib\Item(4))->v, "\n";
    $box = keep<\Lib\Wrapper<\Thing>>(new \Lib\Wrapper<\Thing>(new \Thing(5)));
    $array = keep<array<\Thing>>([new \Thing(6)]);
    echo $box->value->v, " ", $array[0]->v, " ", get_class($box), "\n";
}
"#,
    );
    assert_eq!(output, "1 2 3 4\n5 6 Lib\\Wrapper<Thing>\n");
}
