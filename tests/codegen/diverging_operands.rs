//! Purpose:
//! End-to-end coverage for expressions whose operand always diverges. A call on a receiver known
//! to be null (`unset($r); $r->m()`) throws php's `Error`, yet the cast, `@`, operator, argument
//! list or interpolation around it is still lowered after the operand's block ended. Also pins
//! the line of the Error an uninitialized typed property read raises.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Expected outputs are php 8.5's (`php -n`); see [`checked`] for the two ways each is run.

use super::local_definedness::checked;

/// A call on a receiver that is known to be null always throws; the expressions around it (`@`, casts, unary and binary operators, ternary, `match`, argument lists, interpolation, `print`/`echo` lists, array literals) must still lower, throw php's Error and leave `@` restored for the next warning (`elephc-2kc4`).
#[test]
fn test_error_suppressed_call_on_an_unset_receiver_throws() {
    let source = r####"<?php
class Reg {
    public $p = 1;
    public function same($v) { return $v; }
    public static function st($a, $b = 0) { return $a; }
}
function f($a, $b = 0) { return $a; }
$r = new Reg;
$n = $argc;
unset($r);
try { echo @$r->same($n); } catch (Error $e) { echo "1 ", $e->getMessage(), "\n"; }
try { @$r->same($n); } catch (Error $e) { echo "2 ", $e->getMessage(), "\n"; }
try { $x = @$r->same($n); var_dump($x); } catch (Error $e) { echo "3 ", $e->getMessage(), "\n"; }
try { echo f(1, @$r->same($n)); } catch (Error $e) { echo "4 ", $e->getMessage(), "\n"; }
try { echo (int)$r->same($n); } catch (Error $e) { echo "5 ", $e->getMessage(), "\n"; }
try { var_dump((string)@$r->same($n)); } catch (Error $e) { echo "6 ", $e->getMessage(), "\n"; }
try { echo -$r->same($n); } catch (Error $e) { echo "7 ", $e->getMessage(), "\n"; }
try { var_dump(!$r->same($n)); } catch (Error $e) { echo "8 ", $e->getMessage(), "\n"; }
try { echo 1 + @$r->same($n); } catch (Error $e) { echo "9 ", $e->getMessage(), "\n"; }
try { echo $n ? @$r->same($n) : 'b'; } catch (Error $e) { echo "10 ", $e->getMessage(), "\n"; }
try { echo $r->same($n) ?: 'b'; } catch (Error $e) { echo "11 ", $e->getMessage(), "\n"; }
try { echo match ($r->same($n)) { 1 => 'a', default => 'b' }; } catch (Error $e) { echo "12 ", $e->getMessage(), "\n"; }
try { echo match (true) { default => @$r->same($n) }; } catch (Error $e) { echo "13 ", $e->getMessage(), "\n"; }
try { echo f($r->same($n), $n); } catch (Error $e) { echo "14 ", $e->getMessage(), "\n"; }
try { echo Reg::st(1, @$r->same($n)); } catch (Error $e) { echo "15 ", $e->getMessage(), "\n"; }
try { echo "v={$r->same($n)}\n"; } catch (Error $e) { echo "16 ", $e->getMessage(), "\n"; }
try { echo "a" . @$r->same($n) . "b"; } catch (Error $e) { echo "17 ", $e->getMessage(), "\n"; }
try { print @$r->same($n); } catch (Error $e) { echo "18 ", $e->getMessage(), "\n"; }
try { echo 'a', @$r->same($n), 'b'; } catch (Error $e) { echo "19 ", $e->getMessage(), "\n"; }
try { var_dump([1, @$r->same($n)]); } catch (Error $e) { echo "20 ", $e->getMessage(), "\n"; }
try { $a = []; $a[] = @$r->same($n); } catch (Error $e) { echo "21 ", $e->getMessage(), "\n"; }
try { echo @@$r->same($n); } catch (Error $e) { echo "22 ", $e->getMessage(), "\n"; }
try { echo @$r->same($n)->p; } catch (Error $e) { echo "23 ", $e->getMessage(), "\n"; }
try { var_dump(isset($r->same($n)->p)); } catch (Error $e) { echo "24 ", $e->getMessage(), "\n"; }
try { var_dump(@$r->same($n) ?? 'd'); } catch (Error $e) { echo "25 ", $e->getMessage(), "\n"; }
try { var_dump((array)@$r->same($n), (bool)@$r->same($n)); } catch (Error $e) { echo "26 ", $e->getMessage(), "\n"; }
echo @$undefinedAfter, "end\n";
echo $stillWarns;"####;
    checked(
        source,
        r####"1 Call to a member function same() on null
2 Call to a member function same() on null
3 Call to a member function same() on null
4 Call to a member function same() on null
Warning: Undefined variable $r on line 15
5 Call to a member function same() on null
6 Call to a member function same() on null
Warning: Undefined variable $r on line 17
7 Call to a member function same() on null
Warning: Undefined variable $r on line 18
8 Call to a member function same() on null
9 Call to a member function same() on null
10 Call to a member function same() on null
Warning: Undefined variable $r on line 21
11 Call to a member function same() on null
Warning: Undefined variable $r on line 22
12 Call to a member function same() on null
13 Call to a member function same() on null
Warning: Undefined variable $r on line 24
14 Call to a member function same() on null
15 Call to a member function same() on null
Warning: Undefined variable $r on line 26
16 Call to a member function same() on null
17 Call to a member function same() on null
18 Call to a member function same() on null
a19 Call to a member function same() on null
20 Call to a member function same() on null
21 Call to a member function same() on null
22 Call to a member function same() on null
23 Call to a member function same() on null
Warning: Undefined variable $r on line 34
24 Call to a member function same() on null
25 Call to a member function same() on null
26 Call to a member function same() on null
end
Warning: Undefined variable $stillWarns on line 38"####,
        r####"1 Call to a member function same() on null
2 Call to a member function same() on null
3 Call to a member function same() on null
4 Call to a member function same() on null
5 Call to a member function same() on null
6 Call to a member function same() on null
7 Call to a member function same() on null
8 Call to a member function same() on null
9 Call to a member function same() on null
10 Call to a member function same() on null
11 Call to a member function same() on null
12 Call to a member function same() on null
13 Call to a member function same() on null
14 Call to a member function same() on null
15 Call to a member function same() on null
16 Call to a member function same() on null
17 Call to a member function same() on null
18 Call to a member function same() on null
a19 Call to a member function same() on null
20 Call to a member function same() on null
21 Call to a member function same() on null
22 Call to a member function same() on null
23 Call to a member function same() on null
24 Call to a member function same() on null
25 Call to a member function same() on null
26 Call to a member function same() on null
end
"####,
    );
}

/// The Error of an uninitialized typed property read names the line of the read, also when the read is an argument of a call (`elephc-u9kr`: `getLine()` answered 0).
#[test]
fn test_uninitialized_typed_property_error_carries_the_line_of_the_read() {
    let source = r####"<?php
class Reg { public function same($v) { return $v; } }
class P { public int $u; public ?string $s; }
function f($x) { return $x; }
$r = new Reg;
$p = new P;
try { echo $p->u; } catch (Error $e) { echo $e->getMessage(), " @", $e->getLine(), "\n"; }
try { echo $r->same($p->u); } catch (Error $e) { echo $e->getMessage(), " @", $e->getLine(), "\n"; }
try { echo strlen($p->s); } catch (Error $e) { echo $e->getMessage(), " @", $e->getLine(), "\n"; }
try {
    echo $r->same(
        $p->u
    );
} catch (Error $e) { echo $e->getMessage(), " @", $e->getLine(), "\n"; }
try { echo f($p->u); } catch (Error $e) { echo $e->getMessage(), " @", $e->getLine(), "\n"; }
function read(P $q) { return $q->u + 1; }
try { echo read($p); } catch (Error $e) { echo $e->getMessage(), " @", $e->getLine(), "\n"; }"####;
    checked(
        source,
        r####"Typed property P::$u must not be accessed before initialization @7
Typed property P::$u must not be accessed before initialization @8
Typed property P::$s must not be accessed before initialization @9
Typed property P::$u must not be accessed before initialization @12
Typed property P::$u must not be accessed before initialization @15
Typed property P::$u must not be accessed before initialization @16"####,
        r####"Typed property P::$u must not be accessed before initialization @7
Typed property P::$u must not be accessed before initialization @8
Typed property P::$s must not be accessed before initialization @9
Typed property P::$u must not be accessed before initialization @12
Typed property P::$u must not be accessed before initialization @15
Typed property P::$u must not be accessed before initialization @16
"####,
    );
}
