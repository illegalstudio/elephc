//! Purpose:
//! Verifies enum case names cannot also denote direct or trait-imported constants.
//!
//! Called from:
//! - The error integration suite through its shared diagnostic helpers.
//!
//! Key details:
//! - Direct, nested and grouped trait imports must identify the consuming trait,
//!   while own declarations keep PHP's class-constant redefinition diagnostic.

use super::*;

/// Rejects the direct case/constant collision before metadata construction.
#[test]
fn test_error_enum_trait_constant_case_collision_direct() {
    expect_error(
        r#"<?php trait T7 { const A = 1; } enum E7 { use T7; case A; }"#,
        "Cannot use trait T7, because T7::A conflicts with enum case E7::A",
    );
}

/// Rejects the nested case/constant collision before metadata construction.
#[test]
fn test_error_enum_trait_constant_case_collision_nested() {
    expect_error(
        r#"<?php trait BaseT { const A = 1; } trait MidT { use BaseT; } enum E7 { use MidT; case A; }"#,
        "Cannot use trait MidT, because MidT::A conflicts with enum case E7::A",
    );
}

/// Rejects the own case/constant collision before metadata construction.
#[test]
fn test_error_enum_trait_constant_case_collision_own() {
    expect_error(
        r#"<?php enum E7 { case A; const A = 1; }"#,
        "Cannot redefine class constant E7::A",
    );
}

/// Rejects the backed grouped case/constant collision before metadata construction.
#[test]
fn test_error_enum_trait_constant_case_collision_backed_grouped() {
    expect_error(
        r#"<?php trait T1 { const X = 1; } trait T2 { const A = 2; } enum E7: int { use T1, T2; case A = 1; }"#,
        "Cannot use trait T2, because T2::A conflicts with enum case E7::A",
    );
}
