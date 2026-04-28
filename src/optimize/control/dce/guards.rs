mod eval;
mod record;

pub(super) use eval::{
    guard_literal_truthy,
    has_excluded_guard,
    known_condition_value,
    known_exact_guard,
    scalar_guard_value,
};
pub(super) use record::{
    clear_guards_for_name,
    extend_guards,
    extend_guards_for_switch_case,
    extend_guards_for_switch_case_no_match,
    extend_guards_for_switch_case_no_match_subject,
};
