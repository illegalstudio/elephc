//! Purpose:
//! Groups the PDO Tier-D callback adapters (`__rt_pdo_*`) emitted into the runtime
//! `.text` section. These are the shared, stateless codegen adapters that re-enter
//! compiled-PHP callables on behalf of the `elephc-pdo` bridge: collation
//! comparators, scalar user functions, and aggregate step/finalize callbacks.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`, gated by
//!   `RuntimeFeatures::pdo_udf` so the family is emitted only when a PDO callback
//!   registration is reachable.
//!
//! Key details:
//! - Each adapter is a single `.globl __rt_pdo_*` symbol whose address is taken by
//!   the `__elephc_pdo_adapter_addr` builtin and handed to the bridge; the bridge
//!   stores and calls it but never references a `__rt_*` symbol directly.

mod pdo_call_agg_final;
mod pdo_call_agg_step;
mod pdo_call_collation;
mod pdo_call_scalar;

pub(crate) use pdo_call_agg_final::emit_pdo_call_agg_final;
pub(crate) use pdo_call_agg_step::emit_pdo_call_agg_step;
pub(crate) use pdo_call_collation::emit_pdo_call_collation;
pub(crate) use pdo_call_scalar::emit_pdo_call_scalar;
