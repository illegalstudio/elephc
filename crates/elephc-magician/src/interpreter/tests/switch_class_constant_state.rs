//! Purpose:
//! Interpreter tests for a `switch`-driven state machine whose case labels and/or assigned
//! values are class constants, reduced from Symfony's `Dotenv::doParse()`.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - `php -n` 8.5.6 alternates in all three shapes below. Before the fix, elephc got stuck: a
//!   class-constant fetch handed out the class's cached constant CELL itself rather than an
//!   independent value, so assigning it into a local (`$state = self::STATE_B;`) bound the local
//!   directly to that shared cell. The next reassignment of the local then released the shared
//!   cell as though the local had owned it, corrupting the constant for every later read.

use super::super::*;
use super::support::*;

const MACHINE_CLASS: &str = r#"class SwitchStateMachine {
    public const STATE_A = 0;
    public const STATE_B = 1;

    private int $n = 0;

    private function step(): int {
        return ++$this->n;
    }

    // Exactly Dotenv's shape: class constants for both the case labels and the assigned
    // values, plus a method call in each case body.
    public function constantsAndCalls(int $rounds): string {
        $state = self::STATE_A;
        $out = '';
        for ($i = 0; $i < $rounds; ++$i) {
            switch ($state) {
                case self::STATE_A:
                    $out .= 'A' . $this->step();
                    $state = self::STATE_B;
                    break;
                case self::STATE_B:
                    $out .= 'B' . $this->step();
                    $state = self::STATE_A;
                    break;
            }
        }
        return $out;
    }

    // Case labels are literals; only the assigned value is a class constant.
    public function literalCases(int $rounds): string {
        $state = 0;
        $out = '';
        for ($i = 0; $i < $rounds; ++$i) {
            switch ($state) {
                case 0:
                    $out .= 'a';
                    $state = self::STATE_B;
                    break;
                case 1:
                    $out .= 'b';
                    $state = self::STATE_A;
                    break;
            }
        }
        return $out;
    }

    // Same as constantsAndCalls, with no method call in the body.
    public function constantsNoCalls(int $rounds): string {
        $state = self::STATE_A;
        $out = '';
        for ($i = 0; $i < $rounds; ++$i) {
            switch ($state) {
                case self::STATE_A:
                    $out .= 'A';
                    $state = self::STATE_B;
                    break;
                case self::STATE_B:
                    $out .= 'B';
                    $state = self::STATE_A;
                    break;
            }
        }
        return $out;
    }
}
"#;

/// Verifies a `switch` state machine whose case labels AND assigned values are class constants,
/// with a method call in each body, alternates exactly as `php -n` 8.5.6 does: `A1B2A3B4A5B6`.
#[test]
fn execute_program_switch_state_machine_constants_and_calls_alternates() {
    let source = format!(
        "{MACHINE_CLASS}$m = new SwitchStateMachine();\nreturn $m->constantsAndCalls(6);"
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.get(result),
        FakeValue::String("A1B2A3B4A5B6".to_string())
    );
}

/// Verifies a `switch` whose case LABELS are literals but the assigned values are class
/// constants alternates fully rather than sticking after the first transition: `ababab`.
#[test]
fn execute_program_switch_state_machine_literal_cases_alternates() {
    let source =
        format!("{MACHINE_CLASS}$m = new SwitchStateMachine();\nreturn $m->literalCases(6);");
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("ababab".to_string()));
}

/// Verifies a `switch` state machine whose case labels AND assigned values are class constants,
/// with no method call in the body, alternates rather than sticking in the first state: `ABABAB`.
#[test]
fn execute_program_switch_state_machine_constants_no_calls_alternates() {
    let source =
        format!("{MACHINE_CLASS}$m = new SwitchStateMachine();\nreturn $m->constantsNoCalls(6);");
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("ABABAB".to_string()));
}
