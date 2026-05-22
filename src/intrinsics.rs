//! Purpose:
//! Defines compiler-recognized intrinsic method calls for runtime-managed core objects.
//! Centralizes the small set of method calls that must bypass normal PHP method bodies.
//!
//! Called from:
//! - `crate::codegen::expr::objects::dispatch`
//!
//! Key details:
//! - Intrinsics preserve PHP-facing class/method signatures while routing codegen directly to runtime helpers.

use crate::names::php_symbol_key;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntrinsicCallForm {
    Instance,
    Static,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntrinsicCallKind {
    FiberStart,
    FiberResume,
    FiberThrow,
    FiberGetReturn,
    FiberIsStarted,
    FiberIsRunning,
    FiberIsSuspended,
    FiberIsTerminated,
    FiberSuspend,
    FiberGetCurrent,
    GeneratorCurrent,
    GeneratorKey,
    GeneratorNext,
    GeneratorValid,
    GeneratorRewind,
    GeneratorSend,
    GeneratorThrow,
    GeneratorGetReturn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IntrinsicCall {
    kind: IntrinsicCallKind,
    form: IntrinsicCallForm,
}

#[derive(Debug, Clone, Copy)]
struct IntrinsicSpec {
    kind: IntrinsicCallKind,
    form: IntrinsicCallForm,
    class_key: &'static str,
    class_name: &'static str,
    method_key: &'static str,
    runtime_helper: Option<&'static str>,
}

const INTRINSICS: &[IntrinsicSpec] = &[
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberStart,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "start",
        runtime_helper: Some("__rt_fiber_start"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberResume,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "resume",
        runtime_helper: Some("__rt_fiber_resume"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberThrow,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "throw",
        runtime_helper: Some("__rt_fiber_throw"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberGetReturn,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "getreturn",
        runtime_helper: Some("__rt_fiber_get_return"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberIsStarted,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "isstarted",
        runtime_helper: Some("__rt_fiber_state_eq"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberIsRunning,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "isrunning",
        runtime_helper: Some("__rt_fiber_state_eq"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberIsSuspended,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "issuspended",
        runtime_helper: Some("__rt_fiber_state_eq"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberIsTerminated,
        form: IntrinsicCallForm::Instance,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "isterminated",
        runtime_helper: Some("__rt_fiber_state_eq"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberSuspend,
        form: IntrinsicCallForm::Static,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "suspend",
        runtime_helper: Some("__rt_fiber_suspend"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::FiberGetCurrent,
        form: IntrinsicCallForm::Static,
        class_key: "fiber",
        class_name: "Fiber",
        method_key: "getcurrent",
        runtime_helper: Some("__rt_fiber_get_current"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorCurrent,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "current",
        runtime_helper: Some("__rt_gen_current"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorKey,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "key",
        runtime_helper: Some("__rt_gen_key"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorNext,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "next",
        runtime_helper: Some("__rt_gen_next"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorValid,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "valid",
        runtime_helper: Some("__rt_gen_valid"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorRewind,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "rewind",
        runtime_helper: Some("__rt_gen_rewind"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorSend,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "send",
        runtime_helper: Some("__rt_gen_send"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorThrow,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "throw",
        runtime_helper: Some("__rt_gen_throw"),
    },
    IntrinsicSpec {
        kind: IntrinsicCallKind::GeneratorGetReturn,
        form: IntrinsicCallForm::Instance,
        class_key: "generator",
        class_name: "Generator",
        method_key: "getreturn",
        runtime_helper: Some("__rt_gen_get_return"),
    },
];

impl IntrinsicCall {
    pub fn instance_method(class_name: &str, method: &str) -> Option<Self> {
        resolve(IntrinsicCallForm::Instance, class_name, method)
    }

    pub fn static_method(class_name: &str, method: &str) -> Option<Self> {
        resolve(IntrinsicCallForm::Static, class_name, method)
    }

    pub fn kind(self) -> IntrinsicCallKind {
        self.kind
    }

    pub fn form(self) -> IntrinsicCallForm {
        self.form
    }

    pub fn class_name(self) -> &'static str {
        spec_for(self.kind).class_name
    }

    pub fn method_key(self) -> &'static str {
        spec_for(self.kind).method_key
    }

    pub fn runtime_helper(self) -> Option<&'static str> {
        spec_for(self.kind).runtime_helper
    }
}

fn resolve(form: IntrinsicCallForm, class_name: &str, method: &str) -> Option<IntrinsicCall> {
    let class_key = php_symbol_key(class_name);
    let method_key = php_symbol_key(method);
    INTRINSICS
        .iter()
        .find(|spec| {
            spec.form == form && spec.class_key == class_key && spec.method_key == method_key
        })
        .map(|spec| IntrinsicCall {
            kind: spec.kind,
            form: spec.form,
        })
}

fn spec_for(kind: IntrinsicCallKind) -> &'static IntrinsicSpec {
    INTRINSICS
        .iter()
        .find(|spec| spec.kind == kind)
        .expect("intrinsic registry is missing a declared call kind")
}

#[cfg(test)]
mod tests {
    use super::{IntrinsicCall, IntrinsicCallForm, IntrinsicCallKind};

    #[test]
    fn resolves_generator_methods_case_insensitively() {
        let call = IntrinsicCall::instance_method("Generator", "getReturn")
            .expect("Generator::getReturn should be intrinsic");

        assert_eq!(call.kind(), IntrinsicCallKind::GeneratorGetReturn);
        assert_eq!(call.form(), IntrinsicCallForm::Instance);
        assert_eq!(call.runtime_helper(), Some("__rt_gen_get_return"));
    }

    #[test]
    fn separates_static_and_instance_fiber_methods() {
        assert!(IntrinsicCall::instance_method("Fiber", "suspend").is_none());
        assert!(IntrinsicCall::static_method("Fiber", "suspend").is_some());
        assert!(IntrinsicCall::instance_method("Fiber", "start").is_some());
        assert!(IntrinsicCall::static_method("Fiber", "start").is_none());
    }

    #[test]
    fn ignores_user_classes_with_matching_method_names() {
        assert!(IntrinsicCall::instance_method("UserFiber", "start").is_none());
        assert!(IntrinsicCall::instance_method("Box", "current").is_none());
    }
}
