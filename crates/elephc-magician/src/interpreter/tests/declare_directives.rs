//! Purpose:
//! Interpreter tests for every `declare(…)` form other than `strict_types`, all of which the
//! parser refused outright.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::declare_directives`.
//!
//! Key details:
//! - php refuses almost nothing here. An unknown directive is a WARNING and the script runs on;
//!   `encoding` is accepted and warns that it is ignored; `ticks` is a real directive. Refusing
//!   them made files php parses unparseable, which is the whole defect.
//! - `ticks` does nothing on its own and `register_tick_function()` does nothing on its own. It
//!   takes both, which is why the tests assert the pair rather than either half.
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse declare fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute declare fragment");
    values.output.clone()
}

/// Runs one fragment and returns what it echoed plus the warnings it raised.
fn out_with_warnings(fragment: &[u8]) -> (String, Vec<String>) {
    let program = parse_fragment(fragment).expect("parse declare fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute declare fragment");
    (values.output.clone(), values.warnings.clone())
}

/// Verifies the handler runs after every statement of a file-scoped `declare(ticks=1)`.
///
/// `php -n` 8.5.6 prints `tttt|3`. The four ticks are the statements from the registration
/// onwards -- `register_tick_function()` itself ticks, and `unregister_tick_function()` does not,
/// because by the time the tick would fire there is no handler left to run.
#[test]
fn a_file_scoped_ticks_directive_runs_the_handler_after_every_statement() {
    assert_eq!(
        out(
            br#"declare(ticks=1);
function h() { echo "t"; }
register_tick_function('h');
$a = 1;
$b = 2;
$c = $a + $b;
unregister_tick_function('h');
echo "|", $c;"#
        ),
        "tttt|3",
    );
}

/// Verifies a block body scopes the directive to itself.
///
/// `php -n` 8.5.6 prints `tt|done`. The two statements before and after the block do not tick,
/// which is the difference between a scoped directive and a flag that is merely turned on.
#[test]
fn a_block_body_scopes_ticking_to_the_block() {
    assert_eq!(
        out(
            br#"function h() { echo "t"; }
register_tick_function('h');
$before = 1;
declare(ticks=1) {
    $a = 1;
    $b = 2;
}
$after = 3;
echo "|done";"#
        ),
        "tt|done",
    );
}

/// Verifies the alternative `:` … `enddeclare;` body scopes it the same way.
///
/// `php -n` 8.5.6 prints `ttt|done`.
#[test]
fn an_enddeclare_body_scopes_ticking_the_same_way() {
    assert_eq!(
        out(
            br#"function h() { echo "t"; }
register_tick_function('h');
declare(ticks=1):
    $a = 1;
    $b = 2;
    $c = 3;
enddeclare;
$after = 9;
echo "|done";"#
        ),
        "ttt|done",
    );
}

/// Verifies the interval counts rather than merely switching ticking on.
///
/// `php -n` 8.5.6 prints `tt|done` for `declare(ticks=3)` over six statements: the counter resets
/// on each fire, so six statements fire twice rather than four times or once.
#[test]
fn an_interval_above_one_fires_every_nth_statement() {
    assert_eq!(
        out(
            br#"function h() { echo "t"; }
register_tick_function('h');
declare(ticks=3) {
    $a = 1;
    $b = 2;
    $c = 3;
    $d = 4;
    $e = 5;
    $f = 6;
}
echo "|done";"#
        ),
        "tt|done",
    );
}

/// Verifies a directive with no registered handler runs nothing at all.
///
/// `php -n` 8.5.6 prints `done`. Half the feature is not the feature: the directive alone must be
/// inert, or every ticked scope would pay for a hook nobody asked for.
#[test]
fn the_directive_alone_runs_nothing() {
    assert_eq!(
        out(
            br#"declare(ticks=1);
$a = 1;
$b = 2;
echo "done";"#
        ),
        "done",
    );
}

/// Verifies `encoding` is accepted, warned about, and otherwise ignored.
///
/// `php -n` 8.5.6 says `declare(encoding=...) ignored because Zend multibyte feature is turned
/// off by settings` and runs the script. The directive used to be refused at parse time, so the
/// file did not run at all.
#[test]
fn an_encoding_directive_is_accepted_and_ignored_with_phps_warning() {
    let (output, warnings) = out_with_warnings(
        br#"declare(encoding='UTF-8');
echo "ran";"#,
    );
    assert_eq!(output, "ran");
    assert_eq!(
        warnings,
        vec![
            "declare(encoding=...) ignored because Zend multibyte feature is turned off by \
             settings"
                .to_string(),
        ]
    );
}

/// Verifies an unknown directive warns and the script continues.
///
/// `php -n` 8.5.6 says `Unsupported declare 'foo'` and then prints `ran`. Refusing the file was
/// strictly worse than php's own answer, which is to name the directive and carry on.
#[test]
fn an_unknown_directive_warns_and_the_script_continues() {
    let (output, warnings) = out_with_warnings(
        br#"declare(foo=1);
echo "ran";"#,
    );
    assert_eq!(output, "ran");
    assert_eq!(warnings, vec!["Unsupported declare 'foo'".to_string()]);
}
