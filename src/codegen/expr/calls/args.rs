use crate::codegen::emit::Emitter;
use crate::types::PhpType;

pub(super) fn push_arg_value(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Pointer(_) => {
            emitter.instruction("str x0, [sp, #-16]!");                         // push int/bool/array/callable/pointer arg onto stack
        }
        PhpType::Float => {
            emitter.instruction("str d0, [sp, #-16]!");                         // push float arg onto stack
        }
        PhpType::Str => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push string ptr+len arg onto stack
        }
        PhpType::Void => {}
    }
}

pub(super) fn build_arg_assignments(
    arg_types: &[PhpType],
    initial_int_reg_idx: usize,
) -> Vec<(PhpType, usize, bool)> {
    let mut assignments = Vec::new();
    let mut int_reg_idx = initial_int_reg_idx;
    let mut float_reg_idx = 0usize;
    for ty in arg_types {
        if ty.is_float_reg() {
            assignments.push((ty.clone(), float_reg_idx, true));
            float_reg_idx += 1;
        } else {
            assignments.push((ty.clone(), int_reg_idx, false));
            int_reg_idx += ty.register_count();
        }
    }
    assignments
}

pub(super) fn load_arg_assignments(
    emitter: &mut Emitter,
    assignments: &[(PhpType, usize, bool)],
    arg_count: usize,
) {
    for i in (0..arg_count).rev() {
        let (ty, start_reg, _is_float) = &assignments[i];
        match ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Pointer(_) => {
                emitter.instruction(&format!("ldr x{}, [sp], #16", start_reg)); // pop int-like arg into register
            }
            PhpType::Float => {
                emitter.instruction(&format!("ldr d{}, [sp], #16", start_reg)); // pop float arg into float register
            }
            PhpType::Str => {
                emitter.instruction(&format!(                                   // pop string ptr+len arg into consecutive registers
                    "ldp x{}, x{}, [sp], #16",
                    start_reg,
                    start_reg + 1
                ));
            }
            PhpType::Void => {}
        }
    }
}
