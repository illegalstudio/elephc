<?php
function compiled_add($left, $right) { return $left + $right; }

class EvalCounter {
    public int $value = 1;

    public function bump(): void {
        eval('$this->value = $this->value + 1;');
    }

    public function read(): int {
        return $this->value;
    }

    public function add(int $amount): int {
        return $this->value + $amount;
    }

    public function label(int $amount, string $suffix): string {
        return ($this->value + $amount) . $suffix;
    }

    public function echoReadThroughEval(): void {
        echo "eval-this-method=" . eval('return $this->read();') . "\n";
    }

    public function echoAddThroughEval(): void {
        echo "eval-this-method-arg=" . eval('return $this->add(5);') . "\n";
    }

    public function echoLabelThroughEval(): void {
        echo "eval-this-method-two-args=" . eval('return $this->label(5, "!");') . "\n";
    }
}

$x = 1;
$profile = ["name" => "Ada"];
$result = eval('$x = $x + 2; $created = "dynamic"; return $x + 4;');
eval('$profile["name"] = "Grace";');
eval('if ($x >= 3) { echo "x>=3\n"; }');
eval('if ($x < 0) { echo "negative\n"; } elseif ($x == 3) { echo "x==3\n"; }');
eval('if ("10" !== 10) { echo "strict-ok\n"; }');
$ternary = eval('return $x >= 3 ? "ternary-yes" : "ternary-no";');
eval('do { echo "do-once\n"; } while (false);');
eval('if (true) echo "single-if\n";');
eval('foreach ([1, 2] as $n) { echo "n=" . $n . "\n"; }');
eval('foreach (["a" => 1, "b" => 2] as $key => $value) { echo "pair=" . $key . ":" . $value . "\n"; }');
eval('switch (2) { case 1: echo "switch-one\n"; break; case 2: echo "switch-two\n"; break; }');
eval('echo "echo-list=", "ok\n";');
eval('if (isset($profile["name"])) { echo "isset-name\n"; }');
eval('if (empty($profile["missing"])) { echo "empty-missing\n"; }');
$meta = eval('return ["source" => "eval"];');
$meta_count = eval('return count($meta);');
eval('function plus_one($value) { return $value + 1; }');
$dynamic_call = eval('return plus_one(4);');
$dynamic_cuf = eval('return call_user_func("plus_one", 6);');
$dynamic_cufa = eval('return call_user_func_array("plus_one", [8]);');
$eval_native_call = eval('return compiled_add(2, 8);');
$logic = eval('return true || missing_eval_rhs();');
$keyword_logic = eval('return true xor false;');
$not_false = eval('return !false;');
$coalesced = eval('return $missing ?? "coalesced";');
$compound = eval('$n = 2; $n += 3; $label = "n="; $label .= $n; return $label;');
$incdec = eval('$i = 0; $i++; ++$i; return $i;');
$negative = eval('return -5 + +2;');
$quotient = eval('return 9 / 2;');
$modulo = eval('$n = 20; $n /= 2; return $n % 6;');
$power = eval('return 2 ** 3;');
$bitwise = eval('return (5 & 3) | (1 << 2);');
$spaceship = eval('return 3 <=> 2;');
$magic_line = eval("
return __LINE__;
");
eval('function EvalMagicName() { return __FUNCTION__; }');
$magic_function = eval('return evalmagicname();');
eval('function EvalMagicMethodName() { return __METHOD__; }');
$magic_method = eval('return evalmagicmethodname();');
$magic_file_has_path = eval('return strlen(__FILE__) > strlen(__DIR__);');
$magic_dir_has_path = eval('return strlen(__DIR__) > 0;');
$magic_scope = eval('return "[" . __CLASS__ . "|" . __NAMESPACE__ . "|" . __TRAIT__ . "]";');
$type_checks = eval('return (is_int(1) ? "i" : "?") . (is_string("x") ? "s" : "?") . (is_array([1]) ? "a" : "?");');
$casts = eval('return strval(intval("42")) . ":" . strval(floatval("3.5")) . ":" . (boolval("0") ? "true" : "false");');
$type_name = eval('return gettype(["ok"]);');
$absolute = eval('return abs(-7) . ":" . gettype(abs(-2.5));');
$root = eval('return sqrt(81) . ":" . gettype(sqrt(16));');
$rounding = eval('return floor(3.7) . ":" . ceil(3.2);');
$builtin_power = eval('return pow(2, 5) . ":" . gettype(pow(2, 3));');
eval('function native_add($left, $right) { return $left + $right; }');
eval('function native_double($value) { return $value * 2; }');

echo "x=" . $x . "\n";
echo "created=" . $created . "\n";
echo "name=" . $profile["name"] . "\n";
echo "source=" . $meta["source"] . "\n";
echo "meta-count=" . $meta_count . "\n";
echo "dynamic-call=" . $dynamic_call . "\n";
echo "dynamic-cuf=" . $dynamic_cuf . "\n";
echo "dynamic-cufa=" . $dynamic_cufa . "\n";
echo "eval-native-call=" . $eval_native_call . "\n";
echo "logic=" . $logic . "\n";
echo "keyword-logic=" . $keyword_logic . "\n";
echo "not-false=" . $not_false . "\n";
echo "coalesce=" . $coalesced . "\n";
echo "ternary=" . $ternary . "\n";
echo "compound=" . $compound . "\n";
echo "incdec=" . $incdec . "\n";
echo "negative=" . $negative . "\n";
echo "quotient=" . $quotient . "\n";
echo "modulo=" . $modulo . "\n";
echo "power=" . $power . "\n";
echo "bitwise=" . $bitwise . "\n";
echo "spaceship=" . $spaceship . "\n";
echo "magic-line=" . $magic_line . "\n";
echo "magic-function=" . $magic_function . "\n";
echo "magic-method=" . $magic_method . "\n";
echo "magic-file=" . $magic_file_has_path . "\n";
echo "magic-dir=" . $magic_dir_has_path . "\n";
echo "magic-scope=" . $magic_scope . "\n";
echo "type-checks=" . $type_checks . "\n";
echo "casts=" . $casts . "\n";
echo "type-name=" . $type_name . "\n";
echo "absolute=" . $absolute . "\n";
echo "root=" . $root . "\n";
echo "rounding=" . $rounding . "\n";
echo "builtin-power=" . $builtin_power . "\n";
$counter = new EvalCounter();
$counter->bump();
echo "eval-this-property=" . $counter->value . "\n";
$counter->echoReadThroughEval();
$counter->echoAddThroughEval();
$counter->echoLabelThroughEval();
echo "native-dynamic-call=" . native_add(40, 2) . "\n";
echo "call-user-func=" . call_user_func('native_double', 6) . "\n";
echo "function-exists=" . (function_exists('native_double') ? "yes" : "no") . "\n";
echo "result=" . $result . "\n";
