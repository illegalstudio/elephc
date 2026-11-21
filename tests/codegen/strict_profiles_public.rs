//! Purpose:
//! Verifies the complete strict umbrella and independent profiles through the public CLI.
//!
//! Called from:
//! - Focused strict-profile acceptance and the sharded codegen suite.
//!
//! Key details:
//! - Fixtures own their generated binaries; exact string keys and override order are observable.

use crate::support::*;

/// Every catalog flag and the umbrella compile and execute the documented complete example.
#[test]
fn test_strict_profiles_public_catalog_and_umbrella_example() {
    let source=include_str!("../../examples/strict-mode/main.php");
    for feature in elephc_semantics::FEATURES {
        let flag=format!("--{}",feature.flag);
        assert_eq!(StrictFixture::new(source).run(&[&flag]),"5\n7\n2\n","{flag}");
    }
    for flags in [&["--strict"][..],&["--strict","--no-ir-opt"][..]] {
        assert_eq!(StrictFixture::new(source).run(flags),"5\n7\n2\n");
    }
}

/// Explicit exceptions override the umbrella in either order and PHP-only remains a separate syntax policy.
#[test]
fn test_strict_profiles_public_overrides_and_php_only() {
    for flags in [["--strict","--no-strict-strings"],["--no-strict-strings","--strict"]] {
        assert_eq!(StrictFixture::new("<?php echo 'value=' . 1;").run(&flags),"value=1");
    }
    assert_eq!(StrictFixture::new("<?php echo 'ok';").run(&["--strict","--php-only"]),"ok");
    StrictFixture::new("<?php int $value=1;echo $value;")
        .assert_error(&["--strict","--php-only"],"--php-only");
    StrictFixture::new("<?php echo 'ok';").assert_error(&["--strict-php"],"--strict-php");
}
