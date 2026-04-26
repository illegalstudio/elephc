use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::names::static_property_symbol;
use crate::parser::ast::StaticReceiver;
use crate::types::PhpType;

#[derive(Clone)]
struct StaticPropertyBranch {
    class_id: u64,
    declaring_class: String,
}

pub(super) fn emit_static_property_access(
    receiver: &StaticReceiver,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let Some((class_name, declaring_class, prop_ty)) =
        resolve_static_property(receiver, property, ctx, emitter)
    else {
        return PhpType::Int;
    };

    emitter.comment(&format!("{}::${}", class_name, property));
    let branches = dynamic_static_property_branches(receiver, property, &declaring_class, ctx);
    if branches.is_empty() {
        let symbol = static_property_symbol(&declaring_class, property);
        abi::emit_load_symbol_to_result(emitter, &symbol, &prop_ty);
    } else if emit_called_class_id_into(emitter, ctx, class_id_work_reg(emitter)) {
        emit_dynamic_load_static_property_result(
            property,
            class_id_work_reg(emitter),
            &declaring_class,
            &branches,
            &prop_ty,
            emitter,
            ctx,
        );
    } else {
        emitter.comment("WARNING: missing forwarded called class id");
        let symbol = static_property_symbol(&declaring_class, property);
        abi::emit_load_symbol_to_result(emitter, &symbol, &prop_ty);
    }
    prop_ty
}

fn emit_dynamic_load_static_property_result(
    property: &str,
    class_id_reg: &str,
    fallback_declaring_class: &str,
    branches: &[StaticPropertyBranch],
    prop_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let done = ctx.next_label("static_prop_load_done");
    let mut labels = Vec::new();
    for branch in branches {
        let label = ctx.next_label("static_prop_load_branch");
        emit_branch_if_class_id_matches(emitter, class_id_reg, branch.class_id, &label);
        labels.push((label, branch));
    }
    let fallback_symbol = static_property_symbol(fallback_declaring_class, property);
    abi::emit_load_symbol_to_result(emitter, &fallback_symbol, prop_ty);
    emit_jump(emitter, &done);
    for (label, branch) in labels {
        emitter.label(&label);
        let symbol = static_property_symbol(&branch.declaring_class, property);
        abi::emit_load_symbol_to_result(emitter, &symbol, prop_ty);
        emit_jump(emitter, &done);
    }
    emitter.label(&done);
}

fn emit_called_class_id_into(emitter: &mut Emitter, ctx: &Context, dest: &str) -> bool {
    if let Some(var) = ctx.variables.get("__elephc_called_class_id") {
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), var.stack_offset); // load the forwarded called-class id from the current static method frame
    } else if let Some(var) = ctx.variables.get("this") {
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), var.stack_offset); // load $this so its runtime class id can drive late static storage
        abi::emit_load_from_address(
            emitter,
            abi::int_result_reg(emitter),
            abi::int_result_reg(emitter),
            0,
        );
    } else {
        return false;
    }
    emitter.instruction(&format!("mov {}, {}", dest, abi::int_result_reg(emitter))); // copy the called class id into a scratch register for branch dispatch
    true
}

fn emit_branch_if_class_id_matches(
    emitter: &mut Emitter,
    class_id_reg: &str,
    class_id: u64,
    label: &str,
) {
    let compare_reg = class_id_compare_reg(emitter);
    abi::emit_load_int_immediate(emitter, compare_reg, class_id as i64);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, {}", class_id_reg, compare_reg)); // compare the runtime called class id to a redeclared static property owner
            emitter.instruction(&format!("b.eq {}", label));                   // read this static property slot when the called class id matches
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", class_id_reg, compare_reg)); // compare the runtime called class id to a redeclared static property owner
            emitter.instruction(&format!("je {}", label));                     // read this static property slot when the called class id matches
        }
    }
}

fn emit_jump(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("b {}", label));                       // jump to the end of the static property dispatch chain
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("jmp {}", label));                     // jump to the end of the static property dispatch chain
        }
    }
}

fn class_id_work_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x13",
        Arch::X86_64 => "r13",
    }
}

fn class_id_compare_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x14",
        Arch::X86_64 => "r14",
    }
}

fn dynamic_static_property_branches(
    receiver: &StaticReceiver,
    property: &str,
    fallback_declaring_class: &str,
    ctx: &Context,
) -> Vec<StaticPropertyBranch> {
    if !matches!(receiver, StaticReceiver::Static) {
        return Vec::new();
    }
    let Some(base_class) = ctx.current_class.as_deref() else {
        return Vec::new();
    };
    let mut branches = Vec::new();
    for (class_name, class_info) in &ctx.classes {
        if !is_same_or_descendant(class_name, base_class, ctx) {
            continue;
        }
        let Some(declaring_class) = class_info.static_property_declaring_classes.get(property) else {
            continue;
        };
        if declaring_class == fallback_declaring_class {
            continue;
        }
        branches.push(StaticPropertyBranch {
            class_id: class_info.class_id,
            declaring_class: declaring_class.clone(),
        });
    }
    branches.sort_by_key(|branch| branch.class_id);
    branches.dedup_by_key(|branch| branch.class_id);
    branches
}

fn is_same_or_descendant(class_name: &str, ancestor: &str, ctx: &Context) -> bool {
    let mut cursor = Some(class_name);
    while let Some(name) = cursor {
        if name == ancestor {
            return true;
        }
        cursor = ctx
            .classes
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

pub(crate) fn resolve_static_property(
    receiver: &StaticReceiver,
    property: &str,
    ctx: &Context,
    emitter: &mut Emitter,
) -> Option<(String, String, PhpType)> {
    let class_name = match receiver {
        StaticReceiver::Named(class_name) => class_name.as_str().to_string(),
        StaticReceiver::Self_ | StaticReceiver::Static => match &ctx.current_class {
            Some(class_name) => class_name.clone(),
            None => {
                emitter.comment("WARNING: self::/static:: used outside class scope");
                return None;
            }
        },
        StaticReceiver::Parent => {
            let current_class = match &ctx.current_class {
                Some(class_name) => class_name.clone(),
                None => {
                    emitter.comment("WARNING: parent:: used outside class scope");
                    return None;
                }
            };
            match ctx.classes.get(&current_class).and_then(|info| info.parent.clone()) {
                Some(parent_name) => parent_name,
                None => {
                    emitter.comment(&format!("WARNING: class {} has no parent", current_class));
                    return None;
                }
            }
        }
    };

    let class_info = match ctx.classes.get(&class_name) {
        Some(class_info) => class_info,
        None => {
            emitter.comment(&format!("WARNING: undefined class {}", class_name));
            return None;
        }
    };
    let prop_ty = match class_info
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
    {
        Some(prop_ty) => prop_ty,
        None => {
            emitter.comment(&format!(
                "WARNING: undefined static property {}::${}",
                class_name, property
            ));
            return None;
        }
    };
    let declaring_class = class_info
        .static_property_declaring_classes
        .get(property)
        .cloned()
        .unwrap_or_else(|| class_name.clone());
    Some((class_name, declaring_class, prop_ty))
}
