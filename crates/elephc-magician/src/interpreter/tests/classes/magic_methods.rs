//! Purpose:
//! Interpreter tests for eval magic property, string-conversion, debug, and
//! state-restoration methods.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Dispatch, visibility, existing dynamic properties, and PHP magic-method
//!   contracts are covered separately.

use super::super::super::*;
use super::super::support::*;

/// Verifies undefined eval property reads and writes dispatch through `__get` and `__set`.
#[test]
fn execute_program_dispatches_eval_magic_get_and_set() {
    let program = parse_fragment(
        br#"class EvalMagicPropertyBox {
    public string $events = "";
    public function __get($name) {
        $this->events = $this->events . "get:" . $name . ";";
        return "value:" . $name;
    }
    public function __set($name, $value) {
        $this->events = $this->events . "set:" . $name . "=" . $value . ";";
    }
}
$box = new EvalMagicPropertyBox();
echo $box->missing; echo ":";
$box->other = "B";
$box->events = $box->events . "public;";
return $box->events;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "value:missing:");
    assert_eq!(
        values.get(result),
        FakeValue::String("get:missing;set:other=B;public;".to_string())
    );
}

/// Verifies eval invokes non-public magic methods that PHP accepts with warnings.
#[test]
fn execute_program_dispatches_non_public_eval_magic_methods() {
    let program = parse_fragment(
        br#"class EvalNonPublicMagicBox {
    public string $events = "";
    protected function __get(string $name) {
        $this->events = $this->events . "get:" . $name . ";";
        return "value:" . $name;
    }
    protected function __set(string $name, $value): void {
        $this->events = $this->events . "set:" . $name . "=" . $value . ";";
    }
    private function __isset(string $name): bool {
        $this->events = $this->events . "isset:" . $name . ";";
        return true;
    }
    private function __unset(string $name): void {
        $this->events = $this->events . "unset:" . $name . ";";
    }
    private function __call(string $name, array $args) {
        return $name . ":" . $args[0] . ":" . $args["name"];
    }
    private static function __callStatic(string $name, array $args) {
        return $name . ":" . $args[0] . ":" . $args["name"];
    }
    private function __invoke(string $left = "I", string $right = "J") {
        return "invoke:" . $left . $right;
    }
}
$box = new EvalNonPublicMagicBox();
echo is_callable($box) ? "callable:" : "bad:";
echo $box->missing; echo ":";
$box->other = "B";
echo isset($box->probe) ? "isset:" : "bad:";
unset($box->gone);
echo $box->run("A", name: "B"); echo ":";
echo EvalNonPublicMagicBox::staticRun("C", name: "D"); echo ":";
echo $box(right: "F", left: "E");
return $box->events;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "callable:value:missing:isset:run:A:B:staticRun:C:D:invoke:EF"
    );
    assert_eq!(
        values.get(result),
        FakeValue::String("get:missing;set:other=B;isset:probe;unset:gone;".to_string())
    );
}

/// Verifies inaccessible eval properties dispatch through magic property methods.
#[test]
fn execute_program_dispatches_inaccessible_eval_properties_to_magic_methods() {
    let program = parse_fragment(
        br#"class EvalMagicPrivatePropertyBox {
    private string $secret = "raw";
    public string $events = "";
    public function readOwn() { return $this->secret; }
    public function __get($name) {
        $this->events = $this->events . "get:" . $name . ";";
        return "read:" . $name;
    }
    public function __set($name, $value) {
        $this->events = $this->events . "set:" . $name . "=" . $value . ";";
    }
}
$box = new EvalMagicPrivatePropertyBox();
echo $box->readOwn(); echo ":";
echo $box->secret; echo ":";
$box->secret = "new";
return $box->events;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "raw:read:secret:");
    assert_eq!(
        values.get(result),
        FakeValue::String("get:secret;set:secret=new;".to_string())
    );
}

/// Verifies dynamic properties created without `__set` are read directly even when `__get` exists.
#[test]
fn execute_program_reads_existing_dynamic_property_before_magic_get() {
    let program = parse_fragment(
        br#"class EvalMagicExistingDynamicBox {
    public function __get($name) {
        return "magic:" . $name;
    }
}
$box = new EvalMagicExistingDynamicBox();
$box->known = "plain";
echo $box->known; echo ":";
return $box->missing;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "plain:");
    assert_eq!(
        values.get(result),
        FakeValue::String("magic:missing".to_string())
    );
}

/// Verifies eval property probes and unsets dispatch through `__isset` and `__unset`.
#[test]
fn execute_program_dispatches_eval_magic_isset_empty_and_unset() {
    let program = parse_fragment(
        br#"class EvalMagicPropertyProbeBox {
    public string $events = "";
    public string $present = "ready";
    public $nullish = null;
    private string $secret = "raw";
    public function __isset($name) {
        $this->events = $this->events . "isset:" . $name . ";";
        return $name !== "no";
    }
    public function __get($name) {
        $this->events = $this->events . "get:" . $name . ";";
        return $name === "empty" ? "" : "value:" . $name;
    }
    public function __unset($name) {
        $this->events = $this->events . "unset:" . $name . ";";
    }
}
$box = new EvalMagicPropertyProbeBox();
echo isset($box->present) ? "P" : "p"; echo ":";
echo isset($box->nullish) ? "N" : "n"; echo ":";
echo isset($box->secret) ? "S" : "s"; echo ":";
echo isset($box->no) ? "bad" : "no"; echo ":";
echo empty($box->secret) ? "bad" : "filled"; echo ":";
echo empty($box->empty) ? "empty" : "bad"; echo ":";
unset($box->present);
unset($box->secret);
unset($box->missing);
echo isset($box->present) ? "bad" : "unset"; echo ":";
return $box->events;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "P:n:S:no:filled:empty:unset:");
    assert_eq!(
        values.get(result),
        FakeValue::String(
            "isset:secret;isset:no;isset:secret;get:secret;isset:empty;get:empty;unset:secret;unset:missing;"
                .to_string()
        )
    );
}

/// Verifies eval objects stringify through public `__toString()` in PHP string contexts.
#[test]
fn execute_program_dispatches_eval_magic_tostring_for_string_contexts() {
    let program = parse_fragment(
        br#"class EvalStringableBox {
    public string $name = "Ada";
    public function __toString() {
        return "box:" . $this->name;
    }
    public function accepts(string $value) {
        return "typed:" . $value;
    }
}
$box = new EvalStringableBox();
echo $box; echo ":";
print $box; echo ":";
echo "pre" . $box; echo ":";
echo strval($box); echo ":";
echo call_user_func("strval", $box); echo ":";
echo call_user_func_array("strval", [$box]); echo ":";
echo $box instanceof Stringable ? "S" : "s"; echo ":";
return $box->accepts($box);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "box:Ada:box:Ada:prebox:Ada:box:Ada:box:Ada:box:Ada:S:"
    );
    assert_eq!(
        values.get(result),
        FakeValue::String("typed:box:Ada".to_string())
    );
}

/// Verifies eval objects without `__toString()` fail in PHP string contexts.
#[test]
fn execute_program_rejects_eval_object_string_context_without_tostring() {
    let program = parse_fragment(
        br#"class EvalPlainStringContext {}
$box = new EvalPlainStringContext();
try {
    echo $box;
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Error:Object of class EvalPlainStringContext could not be converted to string"
    );
}

/// Verifies eval rejects magic methods whose staticness, arity, or fatal contracts are invalid.
#[test]
fn execute_program_rejects_invalid_eval_magic_method_contracts() {
    let cases: Vec<(&[u8], &str)> = vec![
        (
            br#"class EvalBadToString { private function __toString() { return "x"; } }"#.as_slice(),
            "private __toString",
        ),
        (
            br#"class EvalBadToStringReturn { public function __toString(): int { return 1; } }"#.as_slice(),
            "bad __toString return type",
        ),
        (
            br#"class EvalBadGetByRef { public function __get(&$name) { return "x"; } }"#.as_slice(),
            "by-ref __get",
        ),
        (
            br#"class EvalBadGetParamType { public function __get(int $name) { return "x"; } }"#.as_slice(),
            "bad __get parameter type",
        ),
        (
            br#"class EvalBadIssetReturn { public function __isset($name): string { return "yes"; } }"#.as_slice(),
            "bad __isset return type",
        ),
        (
            br#"class EvalBadUnsetReturn { public function __unset($name): int { return 1; } }"#.as_slice(),
            "bad __unset return type",
        ),
        (
            br#"class EvalBadSetReturn { public function __set($name, $value): int { return 1; } }"#.as_slice(),
            "bad __set return type",
        ),
        (
            br#"class EvalBadSetParamType { public function __set(int $name, $value): void {} }"#.as_slice(),
            "bad __set parameter type",
        ),
        (
            br#"class EvalBadCall { public function __call($name, ...$args) { return "x"; } }"#.as_slice(),
            "variadic __call",
        ),
        (
            br#"class EvalBadCallArgsType { public function __call(string $name, string $args) {} }"#.as_slice(),
            "bad __call args type",
        ),
        (
            br#"class EvalBadCallNameType { public function __call(int $name, array $args) {} }"#.as_slice(),
            "bad __call name type",
        ),
        (
            br#"class EvalBadCallStatic { public function __callStatic($name, $args) { return "x"; } }"#.as_slice(),
            "instance __callStatic",
        ),
        (
            br#"class EvalBadCallStaticArgsType { public static function __callStatic(string $name, string $args) {} }"#.as_slice(),
            "bad __callStatic args type",
        ),
        (
            br#"class EvalBadSleepReturn { public function __sleep(): string { return "x"; } }"#.as_slice(),
            "bad __sleep return type",
        ),
        (
            br#"class EvalBadSerializeStatic { public static function __serialize(): array { return []; } }"#.as_slice(),
            "static __serialize",
        ),
        (
            br#"class EvalBadWakeupArity { public function __wakeup($value): void {} }"#.as_slice(),
            "bad __wakeup arity",
        ),
        (
            br#"class EvalBadUnserializeArity { public function __unserialize(): void {} }"#.as_slice(),
            "bad __unserialize arity",
        ),
        (
            br#"class EvalBadUnserializeReturn { public function __unserialize(array $data): int { return 1; } }"#.as_slice(),
            "bad __unserialize return type",
        ),
        (
            br#"class EvalBadUnserializeParam { public function __unserialize(string $data): void {} }"#.as_slice(),
            "bad __unserialize parameter type",
        ),
        (
            br#"class EvalBadDebugInfoReturn { public function __debugInfo(): string { return "x"; } }"#.as_slice(),
            "bad __debugInfo return type",
        ),
        (
            br#"class EvalBadDebugInfoStatic { public static function __debugInfo(): array { return []; } }"#.as_slice(),
            "static __debugInfo",
        ),
        (
            br#"class EvalBadSetStateInstance { public function __set_state($data) {} }"#.as_slice(),
            "instance __set_state",
        ),
        (
            br#"class EvalBadSetStateArity { public static function __set_state($data, $extra) {} }"#.as_slice(),
            "bad __set_state arity",
        ),
        (
            br#"class EvalBadSetStateParam { public static function __set_state(string $data) {} }"#.as_slice(),
            "bad __set_state parameter type",
        ),
        (
            br#"class EvalBadClone { public static function __clone() {} }"#.as_slice(),
            "static __clone",
        ),
        (
            br#"class EvalBadCloneReturn { public function __clone(): int {} }"#.as_slice(),
            "bad __clone return type",
        ),
        (
            br#"class EvalBadDestruct { public static function __destruct() {} }"#.as_slice(),
            "static __destruct",
        ),
        (
            br#"class EvalBadConstructReturn { public function __construct(): void {} }"#.as_slice(),
            "bad __construct return type",
        ),
        (
            br#"class EvalBadDestructReturn { public function __destruct(): void {} }"#.as_slice(),
            "bad __destruct return type",
        ),
        (
            br#"trait EvalBadMagicTrait { public static function __isset($name) { return true; } }"#.as_slice(),
            "trait static __isset",
        ),
    ];

    for (source, label) in cases {
        let program = parse_fragment(source).expect(label);
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        execute_program(&program, &mut scope, &mut values).expect_err(label);
    }
}

/// Verifies eval accepts PHP-compatible debug and set-state magic method contracts.
#[test]
fn execute_program_accepts_debug_and_set_state_magic_contracts() {
    let program = parse_fragment(
        br#"class EvalGoodDebugInfoMagic {
    public function __debugInfo(): ?array { return null; }
}
class EvalGoodSetStateMagic {
    public static function __set_state($data) {}
}
return class_exists("EvalGoodDebugInfoMagic") && class_exists("EvalGoodSetStateMagic");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Bool(true));
}
