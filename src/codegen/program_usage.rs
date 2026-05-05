mod required_classes;
mod variables;

pub(super) use required_classes::{collect_required_class_names, program_has_dynamic_instanceof};
pub(super) use variables::program_uses_variable;
