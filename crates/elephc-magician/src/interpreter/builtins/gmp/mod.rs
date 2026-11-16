//! Purpose:
//! Eval homes for PHP's `ext/gmp` (`gmp_*`) surface. None of them re-implements GMP:
//! every home forwards to the compiled GMP prelude helper the host program registered,
//! so eval code runs the same bridge-backed arithmetic as compiled code.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::{EvalDirectHook, EvalValuesHook}::Gmp`.
//! - The declarative eval builtin registry joined from the shared GMP contracts.
//!
//! Key details:
//! - The public `gmp_*` builtins are AOT registry contracts, not prelude functions. The
//!   compiled prelude exposes one hidden `__elephc_gmp_*` helper per builtin, and the
//!   host's generated eval registration is the only bridge between the two.
//! - A host that did not inject the GMP prelude (`--with-gmp`, or a compiled GMP use)
//!   has no helper registered, so a call raises PHP's catchable
//!   `Error: Call to undefined function gmp_*()` instead of evaluating a partial surface.
//! - The internal helper's leading `bool $strict` is compiler-owned. Eval fragments are
//!   evaluated in PHP's weak mode, so the forwarder always passes `false`; php-src then
//!   applies the same coercions and diagnostics as a compiled call.

use super::super::*;

mod dispatch;
mod gmp_abs;
mod gmp_add;
mod gmp_and;
mod gmp_binomial;
mod gmp_clrbit;
mod gmp_cmp;
mod gmp_com;
mod gmp_div;
mod gmp_div_q;
mod gmp_div_qr;
mod gmp_div_r;
mod gmp_divexact;
mod gmp_export;
mod gmp_fact;
mod gmp_gcd;
mod gmp_gcdext;
mod gmp_hamdist;
mod gmp_import;
mod gmp_init;
mod gmp_intval;
mod gmp_invert;
mod gmp_jacobi;
mod gmp_kronecker;
mod gmp_lcm;
mod gmp_legendre;
mod gmp_mod;
mod gmp_mul;
mod gmp_neg;
mod gmp_nextprime;
mod gmp_or;
mod gmp_perfect_power;
mod gmp_perfect_square;
mod gmp_popcount;
mod gmp_pow;
mod gmp_powm;
mod gmp_prob_prime;
mod gmp_random_bits;
mod gmp_random_range;
mod gmp_random_seed;
mod gmp_root;
mod gmp_rootrem;
mod gmp_scan0;
mod gmp_scan1;
mod gmp_setbit;
mod gmp_sign;
mod gmp_sqrt;
mod gmp_sqrtrem;
mod gmp_strval;
mod gmp_sub;
mod gmp_testbit;
mod gmp_xor;

/// Returns whether the host program registered the compiled GMP prelude in this eval context.
///
/// The public builtins exist in the shared catalog regardless, but an eval fragment can only
/// reach them when `--with-gmp` (or a compiled GMP use) injected the prelude and its hidden
/// helpers into this context.
pub(in crate::interpreter) fn eval_gmp_bridge_linked(context: &ElephcEvalContext) -> bool {
    context.native_function("__elephc_gmp_strval").is_some()
}

/// Returns whether `name` is the `gmp` extension loaded into this eval context.
pub(in crate::interpreter) fn eval_gmp_bridge_extension(
    name: &str,
    context: &ElephcEvalContext,
) -> bool {
    name.eq_ignore_ascii_case("gmp") && eval_gmp_bridge_linked(context)
}

pub(in crate::interpreter) use dispatch::*;
