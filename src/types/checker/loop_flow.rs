//! Purpose:
//! Keeps loop and switch control exits separate from fall-through local read views.
//!
//! Called from:
//! - Statement checking for loops, switches, break, continue, and finally blocks.
//!
//! Key details:
//! - Target depths include switches; each function-like scope owns its own stack.
//! - Edge snapshots retain physical storage even when a read has a narrower guard view.

use crate::types::TypeEnv;
use std::collections::HashSet;

use super::inference::{ConditionalFlow, FlowSnapshot};
use super::Checker;

/// Accumulated states for one loop or switch target at its PHP break/continue depth.
pub(super) struct LoopFlowFrame {
    depth: usize,
    flow: ConditionalFlow,
    breaks: Vec<FlowSnapshot>,
    continues: Vec<FlowSnapshot>,
    normal: Option<FlowSnapshot>,
    rebound: HashSet<String>,
    is_loop: bool,
    backedge_can_exit: bool,
}

impl Checker {
    /// Opens a loop target before checking its body and update expressions.
    pub(super) fn begin_loop_flow(&mut self, flow: ConditionalFlow, entry_is_exit: bool, backedge_can_exit: bool) {
        self.begin_control_target(flow, true, entry_is_exit, backedge_can_exit);
    }

    /// Opens a switch target whose level-one continue has switch-break semantics.
    pub(super) fn begin_switch_flow(&mut self, flow: ConditionalFlow, entry_is_exit: bool) {
        self.begin_control_target(flow, false, entry_is_exit, false);
    }

    /// Shares PHP target depths while retaining each construct's exit semantics.
    fn begin_control_target(&mut self, mut flow: ConditionalFlow, is_loop: bool, entry_is_exit: bool, backedge_can_exit: bool) {
        if !entry_is_exit { self.exclude_target_entry_exit(&mut flow); }
        self.break_continue_depth += 1;
        self.loop_flow_frames.push(LoopFlowFrame { depth: self.break_continue_depth,
            flow, breaks: Vec::new(), continues: Vec::new(), normal: None, rebound: HashSet::new(), is_loop, backedge_can_exit });
    }

    /// Records a validated loop exit before enclosing guards restore their local views.
    pub(super) fn record_loop_control_exit(&mut self, levels: usize, continuing: bool, env: &TypeEnv) {
        let Some(depth) = self.break_continue_depth.checked_sub(levels).map(|depth| depth + 1) else { return; };
        let Some(index) = self.loop_flow_frames.iter().rposition(|frame| frame.depth == depth) else { return; };
        let edge = self.snapshot_flow_exit(env);
        if continuing && self.loop_flow_frames[index].is_loop { self.loop_flow_frames[index].continues.push(edge); }
        else { self.loop_flow_frames[index].breaks.push(edge); }
    }

    /// Saves the ordinary body back edge only when the body can reach its end.
    pub(super) fn record_loop_body_end(&mut self, env: &TypeEnv, falls_through: bool) {
        let edge = falls_through.then(|| self.snapshot_flow_exit(env));
        self.loop_flow_frames.last_mut().expect("active loop").normal = edge;
    }

    /// Captures the state before another iterator callback, including continue paths.
    pub(super) fn loop_backedge_snapshot(&self) -> Option<FlowSnapshot> {
        let frame = self.loop_flow_frames.last().expect("active loop");
        let edges: Vec<_> = frame.normal.iter().chain(&frame.continues).cloned().collect();
        self.join_flow_snapshots(&edges)
    }

    /// Joins ordinary and continue paths before a for-update or do-while condition.
    pub(super) fn loop_backedge_env(&mut self) -> Option<TypeEnv> {
        let frame = self.loop_flow_frames.last().expect("active loop");
        let edges: Vec<_> = frame.normal.iter().chain(&frame.continues).cloned().collect();
        let joined = self.join_flow_snapshots(&edges)?;
        self.restore_tail_flow_storage(&joined);
        Some(joined.views)
    }

    /// Retains each arm's physical writes while publishing only an actual final fallthrough.
    pub(super) fn record_switch_case_end(&mut self, env: &TypeEnv, reachable: bool) {
        let edge = self.snapshot_flow_exit(env);
        let mut frame = self.loop_flow_frames.pop().expect("active switch");
        self.observe_flow_snapshot(&mut frame.flow, &edge, false);
        if reachable { frame.breaks.push(edge); }
        self.loop_flow_frames.push(frame);
    }

    /// Replaces body back edges with the state after the update or trailing condition.
    pub(super) fn record_loop_tail_end(&mut self, env: &TypeEnv, reachable: bool) {
        let edge = reachable.then(|| self.snapshot_flow_exit(env));
        let frame = self.loop_flow_frames.last_mut().expect("active loop");
        frame.continues.clear();
        frame.normal = edge;
    }

    /// Closes the target before restoring a while condition's temporary guard scope.
    pub(super) fn pop_loop_flow(&mut self) -> LoopFlowFrame {
        self.break_continue_depth -= 1;
        self.loop_flow_frames.pop().expect("active loop")
    }

    /// Publishes loop entry and all reachable exits, including zero-iteration alternatives.
    pub(super) fn publish_loop_flow(&mut self, mut frame: LoopFlowFrame, env: &mut TypeEnv) {
        for edge in frame.normal.iter().chain(&frame.continues).chain(&frame.breaks) {
            self.observe_flow_snapshot(&mut frame.flow, edge, frame.backedge_can_exit);
        }
        if !frame.backedge_can_exit {
            for edge in &frame.breaks { self.observe_flow_snapshot(&mut frame.flow, edge, true); }
        }
        self.forget_conditional_bindings(&mut frame.flow, &frame.rebound);
        self.finish_target_flow(frame.flow, env, frame.is_loop);
    }

    /// Prevents a loop join from reviving metadata belonging to an explicitly ended binding.
    pub(super) fn record_loop_binding_end(&mut self, name: &str) {
        for frame in &mut self.loop_flow_frames { frame.rebound.insert(name.to_string()); }
    }

    /// Marks exits made inside a try/catch so its finally block can update those edges.
    pub(super) fn pending_loop_exit_counts(&self) -> Vec<(usize, usize)> {
        self.loop_flow_frames.iter().map(|frame| (frame.breaks.len(), frame.continues.len())).collect()
    }

    /// Includes paths already leaving the try in the environment its finally body observes.
    pub(super) fn join_pending_finally_inputs(&mut self, counts: &[(usize, usize)], function_exits: &[FlowSnapshot], falls_through: bool, env: &mut TypeEnv) {
        let mut edges = Vec::new();
        if falls_through { edges.push(self.snapshot_flow_exit(env)); }
        edges.extend_from_slice(function_exits);
        for (frame, (breaks, continues)) in self.loop_flow_frames.iter().zip(counts.iter().copied()) {
            edges.extend(frame.breaks[breaks..].iter().chain(&frame.continues[continues..]).cloned());
        }
        if edges.is_empty() { edges.push(self.snapshot_flow_exit(env)); }
        if let Some(joined) = self.join_flow_snapshots(&edges) {
            self.restore_tail_flow_storage(&joined);
            *env = joined.views;
        }
    }

    /// Carries finally effects to pending exits; a diverging finally overrides those jumps.
    pub(super) fn apply_finally_to_loop_exits(&mut self, counts: &[(usize, usize)], env: &TypeEnv, diverges: bool) {
        let after = self.snapshot_flow_exit(env);
        for (index, (breaks, continues)) in counts.iter().copied().enumerate() {
            if index >= self.loop_flow_frames.len() { break; }
            if diverges {
                self.loop_flow_frames[index].breaks.truncate(breaks);
                self.loop_flow_frames[index].continues.truncate(continues);
                continue;
            }
            let mut edges = self.loop_flow_frames[index].breaks.split_off(breaks);
            let mut next = self.loop_flow_frames[index].continues.split_off(continues);
            for edge in edges.iter_mut().chain(&mut next) {
                self.join_finally_flow_snapshot(edge, &after);
            }
            self.loop_flow_frames[index].breaks.extend(edges);
            self.loop_flow_frames[index].continues.extend(next);
        }
    }
}
