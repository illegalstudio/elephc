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
eval('print_r("print-r=ok\n");');
eval('var_dump("var-dump-ok");');
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
$type_checks = eval('return (is_int(1) ? "i" : "?") . (is_string("x") ? "s" : "?") . (is_array([1]) ? "a" : "?") . (is_iterable([1]) ? "t" : "?") . (is_numeric("42") ? "n" : "?") . (is_resource($memory) ? "r" : "?");');
$casts = eval('return strval(intval("42")) . ":" . strval(floatval("3.5")) . ":" . (boolval("0") ? "true" : "false");');
$type_name = eval('return gettype(["ok"]);');
$absolute = eval('return abs(-7) . ":" . gettype(abs(-2.5));');
$root = eval('return sqrt(81) . ":" . gettype(sqrt(16));');
$float_binary = eval('return fdiv(10, 4) . ":" . round(fmod(10.5, 3.2), 1);');
$rounding = eval('return floor(3.7) . ":" . ceil(3.2);');
$builtin_power = eval('return pow(2, 5) . ":" . gettype(pow(2, 3));');
$rounded = eval('return round(3.14159, 2) . ":" . round(2.5);');
$formatted_number = eval('return number_format(1234567.89, 2, ",", ".");');
$formatted_text = eval('return sprintf("%s:%04d:%s", "item", 7, vsprintf("%.1f", [3]));');
$runtime_constants = eval('return PHP_OS . ":" . DIRECTORY_SEPARATOR . ":" . (PHP_INT_MAX > 0 ? "int" : "bad") . ":" . (defined("PHP_EOL") ? "eol" : "bad");');
$minmax = eval('return min(3, 1, 2) . ":" . max(1.5, 2.5) . ":" . clamp(15, 0, 10);');
$random_range = eval('$r = rand(1, 3); $secure = random_int(4, 4); return (($r >= 1 && $r <= 3) ? "bounded" : "bad") . ":" . ($secure === 4 ? "secure" : "bad");');
$circle = eval('return round(pi(), 2);');
$extended_math = eval('return round(sin(pi() / 2), 0) . ":" . log(8, 2) . ":" . hypot(3, 4) . ":" . intdiv(7, 2);');
$float_predicates = eval('return (is_nan(fdiv(0, 0)) ? "nan" : "bad") . ":" . (is_infinite(fdiv(1, 0)) ? "inf" : "bad") . ":" . (is_finite(3.14) ? "finite" : "bad");');
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
$array_map = eval('function eval_example_double($value) { return $value * 2; } $mapped = array_map("eval_example_double", ["a" => 2, "b" => 3]); return $mapped["a"] . ":" . $mapped["b"];');
$array_reduce = eval('function eval_example_sum($carry, $item) { return $carry + $item; } return array_reduce([1, 2, 3], "eval_example_sum", 10);');
$array_filter = eval('$items = array_filter([0, 1, "", "ok"]); return count($items) . ":" . $items[1] . ":" . $items[3];');
$array_filter_callback = eval('function eval_example_keep_pair($value, $key) { return $key === "b" || $value === 3; } $items = array_filter(["a" => 1, "b" => 2, "c" => 3], "eval_example_keep_pair", ARRAY_FILTER_USE_BOTH); return count($items) . ":" . $items["b"] . ":" . $items["c"];');
$named_builtins = eval('return strlen(string: "eval") . ":" . (str_contains(...["haystack" => "dynamic eval", "needle" => "eval"]) ? "yes" : "no");');
$array_projection = eval('$vals = array_values(["a" => 10, "b" => 20]); $keys = array_keys(["a" => 10, "b" => 20]); return $keys[0] . ":" . $vals[1];');
$mixed_literal = eval('return [2 => "two", "tail"][3] . ":" . (["2" => "two", "next"][3]);');
$append_items = eval('$items = []; $items[] = "left"; $items[] = "right"; return $items[0] . ":" . $items[1] . ":" . count($items);');
$append_assoc = eval('$items = ["name" => "Ada"]; $items[] = "Grace"; return $items[0];');
$array_key_probe = eval('$m = ["name" => null]; return (array_key_exists("name", $m) ? "present" : "missing") . ":" . (array_key_exists("age", $m) ? "bad" : "absent");');
$array_search = eval('return (in_array("b", ["a", "b"]) ? "in" : "missing") . ":" . array_search("Grace", ["name" => "Grace"]);');
$array_fill = eval('$filled = array_fill(2, 2, "x"); $map = array_fill_keys(["a", "b"], 7); return $filled[2] . $filled[3] . ":" . $map["b"];');
$array_column = eval('$rows = [["name" => "Ada"], ["name" => "Lin"]]; $names = array_column($rows, "name"); return count($names) . ":" . $names[0] . ":" . $names[1];');
$array_shapes = eval('$padded = array_pad([1, 2], 4, 0); $chunks = array_chunk([1, 2, 3], 2); return $padded[3] . ":" . count($chunks);');
$array_slice = eval('$slice = array_slice([10, 20, 30], 1); return count($slice) . ":" . $slice[0];');
$array_merge = eval('$merged = array_merge([1, 2], [3]); return count($merged) . ":" . $merged[2];');
$array_sets = eval('$diff = array_diff(["a" => 1, "b" => 2, "c" => "2", "d" => 3], [2]); $inter = array_intersect(["a" => 1, "b" => 2, "c" => "2"], ["2"]); return count($diff) . ":" . count($inter) . ":" . $inter["b"];');
$array_key_sets = eval('$diff = array_diff_key(["a" => 1, "b" => 2], ["a" => 0]); $inter = array_intersect_key(["x" => 7, "y" => 8], ["y" => 0]); return count($diff) . ":" . $diff["b"] . ":" . $inter["y"];');
$range = eval('$up = range(1, 3); $down = range(3, 1); return count($up) . ":" . $up[2] . ":" . $down[2];');
$array_rand = eval('$items = ["a" => 1, "b" => 2]; $key = array_rand($items); return array_key_exists($key, $items) ? "valid" : "bad";');
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
$digest = eval('return md5("abc") . ":" . substr(hash("sha256", "abc"), 0, 8) . ":" . substr(hash_hmac("sha256", "data", "key"), 0, 8);');
$file_digest = eval('file_put_contents("eval-hash-file.txt", "abc"); $digest = hash_file("sha256", "eval-hash-file.txt"); unlink("eval-hash-file.txt"); return substr($digest, 0, 8);');
$system_info = eval('return (time() > 1000000000 ? "time" : "bad") . ":" . phpversion() . ":" . (strlen(php_uname("s")) > 0 ? "uname" : "bad") . ":" . sys_get_temp_dir() . ":" . (strlen(getcwd()) > 0 ? "cwd" : "bad");');
$date_sample = eval('$ts = mktime(0, 0, 0, 1, 2, 2024); return date("Y-m-d", $ts);');
$strtotime_sample = eval('$ts = strtotime("2024-01-02 03:04:05"); return date("Y-m-d H:i:s", $ts);');
$micro_time = eval('return microtime(true) > 1000000000 ? "ok" : "bad";');
$realpath_cache = eval('return count(realpath_cache_get()) . ":" . realpath_cache_size();');
$environment = eval('putenv("ELEPHC_EVAL_EXAMPLE=ok"); $value = getenv("ELEPHC_EVAL_EXAMPLE"); putenv("ELEPHC_EVAL_EXAMPLE"); return $value . ":" . (getenv("ELEPHC_EVAL_EXAMPLE") === "" ? "cleared" : "left");');
$sleeping = eval('usleep(0); return sleep(0) . ":awake";');
$host_lookup = eval('return (strlen(gethostname()) > 0 ? "host" : "empty") . ":" . gethostbyname("127.0.0.1") . ":" . gethostbyname("not a host") . ":" . (strlen(gethostbyaddr("127.0.0.1")) > 0 ? "reverse" : "empty") . ":" . (gethostbyaddr("not-an-ip-address") === false ? "bad-ip" : "bad");');
$protocol_services = eval('return getprotobyname("tcp") . ":" . getprotobynumber(17) . ":" . getservbyname("http", "tcp") . ":" . getservbyport(443, "tcp");');
$ip_conversion = eval('$packed = inet_pton("1.2.3.4"); return long2ip(ip2long("192.168.1.1")) . ":" . bin2hex($packed) . ":" . inet_ntop($packed);');
$stream_introspection = eval('$wrappers = stream_get_wrappers(); $transports = stream_get_transports(); $filters = stream_get_filters(); return count($wrappers) . ":" . $wrappers[0] . ":" . count($transports) . ":" . (in_array("string.rot13", $filters) ? "rot13" : "missing");');
$spl_classes = eval('$names = spl_classes(); return count($names) . ":" . (in_array("Exception", $names) ? "exception" : "missing");');
$path_components = eval('return basename("/var/log/syslog.log", ".log") . ":" . dirname("/usr/local/bin/tool", 2);');
$resolved_path = eval('return realpath(".") !== false ? "resolved" : "missing";');
$path_info = eval('$info = pathinfo("/var/log/syslog.log"); $match = fnmatch("*.LOG", "system.log", FNM_CASEFOLD); return $info["basename"] . ":" . pathinfo("archive.tar.gz", PATHINFO_EXTENSION) . ":" . ($match ? "match" : "bad");');
$filesystem = eval('file_put_contents("eval-example.txt", "hello"); $read = file_get_contents("eval-example.txt"); $size = filesize("eval-example.txt"); $ok = file_exists("eval-example.txt") && is_file("eval-example.txt") && is_readable("eval-example.txt") && is_writable("eval-example.txt") && unlink("eval-example.txt"); return $read . ":" . $size . ":" . ($ok ? "ok" : "bad");');
$disk_space = eval('return (disk_free_space(".") > 0 ? "free" : "bad") . ":" . (disk_total_space(".") >= disk_free_space(".") ? "ordered" : "bad");');
$file_stats = eval('file_put_contents("eval-stat.txt", "hello"); $type = filetype("eval-stat.txt"); $info = stat("eval-stat.txt"); $meta = filemtime("eval-stat.txt") > 0 && fileinode("eval-stat.txt") > 0 && fileperms("eval-stat.txt") > 0 && $info["size"] === 5 && $info[7] === $info["size"]; unlink("eval-stat.txt"); return $type . ":" . ($meta ? "meta" : "bad");');
$path_ops = eval('mkdir("eval-ops-dir"); file_put_contents("eval-ops-src.txt", "hello"); copy("eval-ops-src.txt", "eval-ops-copy.txt"); rename("eval-ops-copy.txt", "eval-ops-moved.txt"); symlink("eval-ops-src.txt", "eval-ops-link.txt"); $ok = is_dir("eval-ops-dir") && file_exists("eval-ops-moved.txt") && readlink("eval-ops-link.txt") === "eval-ops-src.txt" && linkinfo("eval-ops-link.txt") >= 0; unlink("eval-ops-link.txt"); unlink("eval-ops-moved.txt"); unlink("eval-ops-src.txt"); rmdir("eval-ops-dir"); return $ok ? "ok" : "bad";');
$file_listing = eval('file_put_contents("eval-lines.txt", "one\ntwo"); file_put_contents("eval-empty.txt", ""); mkdir("eval-list-dir"); file_put_contents("eval-list-dir/a.txt", "a"); $lines = file("eval-lines.txt"); $scan = scandir("eval-list-dir"); $glob = glob("eval-list-dir/*.txt"); $bytes = readfile("eval-empty.txt"); $ok = count($lines) === 2 && $lines[0] === "one\n" && $bytes === 0 && in_array("a.txt", $scan) && count($glob) === 1; unlink("eval-list-dir/a.txt"); rmdir("eval-list-dir"); unlink("eval-lines.txt"); unlink("eval-empty.txt"); return $ok ? "ok" : "bad";');
$file_modify = eval('touch("eval-touch.txt", 1000000000); file_put_contents("eval-mod.txt", "x"); $tmp = tempnam(".", "evm"); $previous = umask(18); $probe = umask(); umask($previous); $ok = filemtime("eval-touch.txt") === 1000000000 && chmod("eval-mod.txt", 384) && file_exists($tmp) && $probe === 18; unlink($tmp); unlink("eval-touch.txt"); unlink("eval-mod.txt"); return $ok ? "ok" : "bad";');
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
echo "printf-format=" . $formatted_text . "\n";
echo "runtime-constants=" . $runtime_constants . "\n";
echo "minmax=" . $minmax . "\n";
echo "random-range=" . $random_range . "\n";
echo "pi=" . $circle . "\n";
echo "extended-math=" . $extended_math . "\n";
echo "float-predicates=" . $float_predicates . "\n";
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
echo "array-map=" . $array_map . "\n";
echo "array-reduce=" . $array_reduce . "\n";
echo "array-filter=" . $array_filter . "\n";
echo "array-filter-callback=" . $array_filter_callback . "\n";
echo "named-builtins=" . $named_builtins . "\n";
echo "array-projection=" . $array_projection . "\n";
echo "mixed-literal=" . $mixed_literal . "\n";
echo "append-items=" . $append_items . "\n";
echo "append-assoc=" . $append_assoc . "\n";
echo "array-key-exists=" . $array_key_probe . "\n";
echo "array-search=" . $array_search . "\n";
echo "array-fill=" . $array_fill . "\n";
echo "array-column=" . $array_column . "\n";
echo "array-shapes=" . $array_shapes . "\n";
echo "array-slice=" . $array_slice . "\n";
echo "array-merge=" . $array_merge . "\n";
echo "array-sets=" . $array_sets . "\n";
echo "array-key-sets=" . $array_key_sets . "\n";
echo "range=" . $range . "\n";
echo "array-rand=" . $array_rand . "\n";
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
echo "digest=" . $digest . "\n";
echo "hash-file=" . $file_digest . "\n";
echo "system-info=" . $system_info . "\n";
echo "date-sample=" . $date_sample . "\n";
echo "strtotime-sample=" . $strtotime_sample . "\n";
echo "microtime=" . $micro_time . "\n";
echo "realpath-cache=" . $realpath_cache . "\n";
echo "environment=" . $environment . "\n";
echo "sleep=" . $sleeping . "\n";
echo "host-lookup=" . $host_lookup . "\n";
echo "protocol-services=" . $protocol_services . "\n";
echo "ip-conversion=" . $ip_conversion . "\n";
echo "stream-introspection=" . $stream_introspection . "\n";
echo "spl-classes=" . $spl_classes . "\n";
echo "path-components=" . $path_components . "\n";
echo "realpath=" . $resolved_path . "\n";
echo "pathinfo=" . $path_info . "\n";
echo "filesystem=" . $filesystem . "\n";
echo "disk-space=" . $disk_space . "\n";
echo "file-stats=" . $file_stats . "\n";
echo "path-ops=" . $path_ops . "\n";
echo "file-listing=" . $file_listing . "\n";
echo "file-modify=" . $file_modify . "\n";
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
