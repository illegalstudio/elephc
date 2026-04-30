mod compound;
mod locals;
mod postfix;
mod simple;

pub(super) use locals::{
    looks_like_typed_assign,
    parse_global,
    parse_incdec_stmt,
    parse_list_unpack,
    parse_static_var,
    parse_typed_assign,
};
pub(crate) use postfix::{
    can_replay_assignment_target,
};
pub(super) use postfix::{
    try_parse_postfix_assignment,
    try_parse_scoped_property_assignment,
};
pub(super) use simple::parse_variable_stmt;
