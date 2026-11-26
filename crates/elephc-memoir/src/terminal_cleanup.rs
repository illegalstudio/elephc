//! Purpose:
//! Orders terminal owner retirement and cycle collection as one restartable fixed point.
//!
//! Called from:
//! - CLI main shutdown and persistent web application shutdown emission.
//!
//! Key details:
//! - Roots are drained before cycle collection on every pass.
//! - The consumer's rescan branches to the supplied restart label when cleanup republished a root.

/// Emits one complete terminal fixed-point pass through consumer-supplied codegen callbacks.
pub fn emit_fixed_point_pass<S>(
    state: &mut S,
    restart: &str,
    mut begin_pass: impl FnMut(&mut S, &str),
    mut drain_roots: impl FnMut(&mut S),
    mut collect_cycles: impl FnMut(&mut S),
    mut rescan_roots: impl FnMut(&mut S, &str),
) {
    begin_pass(state, restart);
    drain_roots(state);
    collect_cycles(state);
    rescan_roots(state, restart);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The coordinator keeps root retirement, collection, and rescan in the required order.
    #[test]
    fn terminal_fixed_point_orders_every_phase_before_rescan() {
        let mut phases = Vec::new();
        emit_fixed_point_pass(
            &mut phases,
            "restart",
            |phases, label| phases.push(format!("begin:{label}")),
            |phases| phases.push("drain".into()),
            |phases| phases.push("collect".into()),
            |phases, label| phases.push(format!("rescan:{label}")),
        );
        assert_eq!(
            phases,
            ["begin:restart", "drain", "collect", "rescan:restart"]
        );
    }
}
