<?php
function compiled_add($left, $right) { return $left + $right; }

function eval_arg_summary() {
    return eval('global $argc, $argv; return ($argc > 0 ? "argc" : "no-argc") . ":" . (count($argv) > 0 ? "argv" : "no-argv");');
}

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

    public function echoLabelSpreadThroughEval(): void {
        echo "eval-this-method-spread=" . eval('return $this->label(...[5, "?"]);') . "\n";
    }
}

class EvalAotBox {
    public int $value = 0;

    public function __construct(int $value) {
        $this->value = $value;
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
eval('function eval_example_counter() { static $n = 0; $n++; return $n; }');
$dynamic_call = eval('return plus_one(4);');
$dynamic_named = eval('function named_pair($left, $right) { return $left . ":" . $right; } return named_pair(right: "R", left: "L");');
$dynamic_spread = eval('function spread_pair($left, $right) { return $left . ":" . $right; } return spread_pair(...["L", "R"]);');
$static_first = eval('return eval_example_counter();');
$static_second = eval('return eval_example_counter();');
$dynamic_cuf = eval('return call_user_func("plus_one", 6);');
$dynamic_cufa = eval('return call_user_func_array("plus_one", [8]);');
$eval_native_call = eval('return compiled_add(2, 8);');
$eval_native_named = eval('return compiled_add(right: 8, left: 2);');
$eval_native_spread = eval('return compiled_add(...[2, 8]);');
$eval_native_cufa_named = eval('return call_user_func_array("compiled_add", ["right" => 8, "left" => 2]);');
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
$memory = fopen("php://memory", "r+");
$type_checks = eval('return (is_int(1) ? "i" : "?") . (is_string("x") ? "s" : "?") . (is_array([1]) ? "a" : "?") . (is_numeric("42") ? "n" : "?") . (is_resource($memory) ? "r" : "?");');
$casts = eval('return strval(intval("42")) . ":" . strval(floatval("3.5")) . ":" . (boolval("0") ? "true" : "false");');
$type_name = eval('return gettype(["ok"]);');
$absolute = eval('return abs(-7) . ":" . gettype(abs(-2.5));');
$root = eval('return sqrt(81) . ":" . gettype(sqrt(16));');
$float_binary = eval('return fdiv(10, 4) . ":" . round(fmod(10.5, 3.2), 1);');
$rounding = eval('return floor(3.7) . ":" . ceil(3.2);');
$builtin_power = eval('return pow(2, 5) . ":" . gettype(pow(2, 3));');
$rounded = eval('return round(3.14159, 2) . ":" . round(2.5);');
$formatted_number = eval('return number_format(1234567.89, 2, ",", ".");');
$minmax = eval('return min(3, 1, 2) . ":" . max(1.5, 2.5);');
$circle = eval('return round(pi(), 2);');
$case = eval('return strtoupper("eval") . ":" . strtolower("LOUD") . ":" . ucfirst("eval") . ":" . lcfirst("LOUD");');
$word_case = eval('return ucwords("hello eval");');
$reversed = eval('return strrev("eval");');
$contains = eval('return str_contains("dynamic eval", "eval") ? "contains" : "missing";');
$positions = eval('return strpos("banana", "na") . ":" . strrpos("banana", "na");');
$substring_from = eval('return strstr("user@example.com", "@");');
$ordinal = eval('return ord("A") . ":" . ord("");');
$boundaries = eval('return (str_starts_with("dynamic eval", "dynamic") ? "starts" : "missing") . ":" . (str_ends_with("dynamic eval", "eval") ? "ends" : "missing");');
$trimmed = eval('return trim("  boxed  ") . ":" . ltrim("0007", "0") . ":" . chop("tail...", ".");');
$aggregates = eval('return array_sum([1, 2, 3]) . ":" . array_product([2, 3, 4]);');
$named_builtins = eval('return strlen(string: "eval") . ":" . (str_contains(...["haystack" => "dynamic eval", "needle" => "eval"]) ? "yes" : "no");');
$array_projection = eval('$vals = array_values(["a" => 10, "b" => 20]); $keys = array_keys(["a" => 10, "b" => 20]); return $keys[0] . ":" . $vals[1];');
$mixed_literal = eval('return [2 => "two", "tail"][3] . ":" . (["2" => "two", "next"][3]);');
$append_items = eval('$items = []; $items[] = "left"; $items[] = "right"; return $items[0] . ":" . $items[1] . ":" . count($items);');
$append_assoc = eval('$items = ["name" => "Ada"]; $items[] = "Grace"; return $items[0];');
$array_key_probe = eval('$m = ["name" => null]; return (array_key_exists("name", $m) ? "present" : "missing") . ":" . (array_key_exists("age", $m) ? "bad" : "absent");');
$array_search = eval('return (in_array("b", ["a", "b"]) ? "in" : "missing") . ":" . array_search("Grace", ["name" => "Grace"]);');
$string_compare = eval('return (strcmp("abc", "abd") < 0 ? "lt" : "bad") . ":" . (strcasecmp("Hello", "hello") === 0 ? "ci" : "bad") . ":" . (hash_equals("abc", "abc") ? "hash" : "bad");');
$ctype_checks = eval('return (ctype_alpha("abc") ? "alpha" : "bad") . ":" . (ctype_digit("123") ? "digit" : "bad") . ":" . (ctype_space(" \t\n") ? "space" : "bad");');
$slashes = eval('return addslashes("A\"B") . ":" . stripslashes(addslashes("A\"B"));');
$chr = eval('return chr(65) . ":" . bin2hex(chr(256));');
$repeated = eval('return str_repeat("ha", 3);');
$substring = eval('return substr("abcdef", 2) . ":" . substr("abcdef", 1, -1);');
$substring_replaced = eval('return substr_replace("hello world", "PHP", 6, 5);');
$padded = eval('return str_pad("hi", 5, ".");');
$wrapped = eval('return wordwrap("hello dynamic world", 7, "|");');
$linebreaks = eval('return bin2hex(nl2br("a\nb", false));');
$split_joined = eval('$parts = explode(",", "red,green,blue"); return implode("|", $parts);');
$string_chunks = eval('$chunks = str_split("eval", 2); return $chunks[0] . ":" . $chunks[1];');
$replaced = eval('return str_replace("green", "lime", "red green blue");');
$html_escaped = eval('return htmlspecialchars("<b>bold</b>");');
$url_codec = eval('return urlencode("a b&=") . ":" . rawurldecode("a%20b%26%3D");');
$checksum = eval('return crc32("hello");');
$hash_algos = eval('$algos = hash_algos(); return count($algos) . ":" . (in_array("sha256", $algos) ? "sha256" : "missing");');
$system_info = eval('return (time() > 1000000000 ? "time" : "bad") . ":" . phpversion() . ":" . sys_get_temp_dir() . ":" . (strlen(getcwd()) > 0 ? "cwd" : "bad");');
$hexed = eval('return bin2hex("Az");');
$unhexed = eval('return hex2bin("417a");');
$base64 = eval('return base64_encode("Hello");');
$base64_decoded = eval('return base64_decode("SGVsbG8=");');
$eval_class_probe = eval('return class_exists("EvalAotBox") ? "yes" : "no";');
eval('class EvalDynamicEmptyClass {}');
$eval_dynamic_class_probe = eval('return class_exists("evaldynamicemptyclass") ? "yes" : "no";');
$eval_dynamic_class_native_probe = class_exists("EvalDynamicEmptyClass") ? "yes" : "no";
$eval_dynamic_const_probe = eval('define("EvalDynamicConst", "yes"); return EvalDynamicConst;');
$eval_dynamic_const_native_probe = defined("EvalDynamicConst") ? "yes" : "no";
$eval_dynamic_const_native_fetch = EvalDynamicConst;
$eval_dynamic_new = eval('$box = new EvalAotBox(21); return $box->value;');
eval('function native_add($left, $right) { return $left + $right; }');
eval('function native_double($value) { return $value * 2; }');

echo "x=" . $x . "\n";
echo "created=" . $created . "\n";
echo "name=" . $profile["name"] . "\n";
echo "source=" . $meta["source"] . "\n";
echo "meta-count=" . $meta_count . "\n";
echo "dynamic-call=" . $dynamic_call . "\n";
echo "dynamic-named=" . $dynamic_named . "\n";
echo "dynamic-spread=" . $dynamic_spread . "\n";
echo "static-counter=" . $static_first . ":" . $static_second . "\n";
echo "dynamic-cuf=" . $dynamic_cuf . "\n";
echo "dynamic-cufa=" . $dynamic_cufa . "\n";
echo "eval-native-call=" . $eval_native_call . "\n";
echo "eval-native-named=" . $eval_native_named . "\n";
echo "eval-native-spread=" . $eval_native_spread . "\n";
echo "eval-native-cufa-named=" . $eval_native_cufa_named . "\n";
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
echo "float-binary=" . $float_binary . "\n";
echo "rounding=" . $rounding . "\n";
echo "builtin-power=" . $builtin_power . "\n";
echo "rounded=" . $rounded . "\n";
echo "number-format=" . $formatted_number . "\n";
echo "minmax=" . $minmax . "\n";
echo "pi=" . $circle . "\n";
echo "case=" . $case . "\n";
echo "ucwords=" . $word_case . "\n";
echo "reversed=" . $reversed . "\n";
echo "contains=" . $contains . "\n";
echo "positions=" . $positions . "\n";
echo "strstr=" . $substring_from . "\n";
echo "ordinal=" . $ordinal . "\n";
echo "boundaries=" . $boundaries . "\n";
echo "trimmed=" . $trimmed . "\n";
echo "aggregates=" . $aggregates . "\n";
echo "named-builtins=" . $named_builtins . "\n";
echo "array-projection=" . $array_projection . "\n";
echo "mixed-literal=" . $mixed_literal . "\n";
echo "append-items=" . $append_items . "\n";
echo "append-assoc=" . $append_assoc . "\n";
echo "array-key-exists=" . $array_key_probe . "\n";
echo "array-search=" . $array_search . "\n";
echo "string-compare=" . $string_compare . "\n";
echo "ctype=" . $ctype_checks . "\n";
echo "slashes=" . $slashes . "\n";
echo "chr=" . $chr . "\n";
echo "str-repeat=" . $repeated . "\n";
echo "substr=" . $substring . "\n";
echo "substr-replace=" . $substring_replaced . "\n";
echo "str-pad=" . $padded . "\n";
echo "wordwrap=" . $wrapped . "\n";
echo "nl2br-hex=" . $linebreaks . "\n";
echo "explode-implode=" . $split_joined . "\n";
echo "str-split=" . $string_chunks . "\n";
echo "str-replace=" . $replaced . "\n";
echo "htmlspecialchars=" . $html_escaped . "\n";
echo "url-codec=" . $url_codec . "\n";
echo "crc32=" . $checksum . "\n";
echo "hash-algos=" . $hash_algos . "\n";
echo "system-info=" . $system_info . "\n";
echo "bin2hex=" . $hexed . "\n";
echo "hex2bin=" . $unhexed . "\n";
echo "base64=" . $base64 . "\n";
echo "base64-decode=" . $base64_decoded . "\n";
echo "eval-class-exists=" . $eval_class_probe . "\n";
echo "eval-dynamic-class-exists=" . $eval_dynamic_class_probe . "\n";
echo "native-class-exists-eval-class=" . $eval_dynamic_class_native_probe . "\n";
echo "eval-dynamic-const-exists=" . $eval_dynamic_const_probe . "\n";
echo "native-defined-eval-const=" . $eval_dynamic_const_native_probe . "\n";
echo "native-fetch-eval-const=" . $eval_dynamic_const_native_fetch . "\n";
echo "eval-dynamic-new=" . $eval_dynamic_new . "\n";
$counter = new EvalCounter();
$counter->bump();
echo "eval-this-property=" . $counter->value . "\n";
$counter->echoReadThroughEval();
$counter->echoAddThroughEval();
$counter->echoLabelThroughEval();
$counter->echoLabelSpreadThroughEval();
echo "native-dynamic-call=" . native_add(40, 2) . "\n";
echo "call-user-func=" . call_user_func('native_double', 6) . "\n";
echo "function-exists=" . (function_exists('native_double') ? "yes" : "no") . "\n";
echo "arg-globals=" . eval_arg_summary() . "\n";
echo "result=" . $result . "\n";
