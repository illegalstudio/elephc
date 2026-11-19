//! Purpose:
//! Checks `Time\Duration` construction and argument diagnostics.
//!
//! Called from:
//! - The error integration test binary with the same prelude injection as compilation.
//!
//! Key details:
//! - php-src reports direct construction and wrong argument types at run time; elephc's
//!   static checker reports the same invalid programs while compiling. The `--EXPECT--`
//!   fixtures that require the runtime wording stay out of the positive corpus, and these
//!   assertions pin the compile-time boundary instead.
//! - The private constructor is what makes `new Time\Duration()` invalid; factories are
//!   the only supported construction path.

use super::*;

/// Direct construction is rejected because the constructor is private.
#[test]
fn test_error_time_duration_constructor_is_private() {
    expect_error("<?php new Time\\Duration();", "Cannot access private constructor");
    expect_error("<?php new Time\\Duration(1);", "Cannot access private constructor");
}

/// The factory surface keeps php-src's arity: no zero-argument factory and no extra operands.
#[test]
fn test_error_time_duration_factory_arities() {
    expect_error("<?php Time\\Duration::fromSeconds();", "argument");
    expect_error("<?php Time\\Duration::fromSeconds(1, 2, 3);", "argument");
    expect_error("<?php Time\\Duration::fromNanoseconds();", "argument");
    expect_error("<?php Time\\Duration::fromMicroseconds();", "argument");
    expect_error("<?php Time\\Duration::fromMilliseconds();", "argument");
    expect_error("<?php Time\\Duration::fromMinutes();", "argument");
    expect_error("<?php Time\\Duration::fromHours();", "argument");
    expect_error("<?php Time\\Duration::fromIso8601DurationString();", "argument");
}

/// Methods require a `Time\Duration` operand, matching php-src's `Z_PARAM_DATE_TIME_DURATION`.
#[test]
fn test_error_time_duration_operand_types() {
    expect_error(
        "<?php Time\\Duration::compare(1, 2);",
        "parameter $a expects Time\\Duration, got int",
    );
    expect_error(
        "<?php $d = Time\\Duration::fromSeconds(1); $d->add(1);",
        "parameter $duration expects Time\\Duration, got int",
    );
    expect_error(
        "<?php $d = Time\\Duration::fromSeconds(1); $d->sub('x');",
        "parameter $duration expects Time\\Duration, got string",
    );
    expect_error(
        "<?php Time\\Duration::fromSeconds('x');",
        "parameter $seconds expects int",
    );
}
