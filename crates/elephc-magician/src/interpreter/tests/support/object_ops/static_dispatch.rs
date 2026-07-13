//! Purpose:
//! Implements fake runtime static-method dispatch for AOT bridge tests.
//!
//! Called from:
//! - `FakeOps`'s `RuntimeValueOps::static_method_call()` implementation.
//!
//! Key details:
//! - Unsupported class/method pairs fail with the existing eval status.

use super::*;

impl FakeOps {

    /// Calls one fake public static AOT method by class and method name.
    pub(in crate::interpreter::tests::support) fn runtime_static_method_call(
        &mut self,
        class_name: &str,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let method = method.to_ascii_lowercase();
        if !class_name.eq_ignore_ascii_case("KnownClass") {
            return Err(EvalStatus::UnsupportedConstruct);
        }
        match method.as_str() {
            "join" => {
                let [left, right] = args.as_slice() else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::String(left) = self.get(*left) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::String(right) = self.get(*right) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                self.string(&format!("{}{}", left, right))
            }
            "sum" => {
                let [left, right] = args.as_slice() else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::Int(left) = self.get(*left) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::Int(right) = self.get(*right) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                self.int(left + right)
            }
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }

}
