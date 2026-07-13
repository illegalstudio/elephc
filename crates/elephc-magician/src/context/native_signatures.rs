//! Purpose:
//! Registers generated instance, static, and constructor callable signatures.
//!
//! Called from:
//! - FFI registration and native argument binding.
//!
//! Key details:
//! - Parameter names, types, defaults, by-ref flags, variadics, and return types share one shape.

use super::*;

impl ElephcEvalContext {
    /// Defines native AOT instance-method signature metadata for eval named-argument binding.
    pub fn define_native_method_signature(
        &mut self,
        class_name: &str,
        method_name: &str,
        signature: NativeCallableSignature,
    ) -> bool {
        self.native_methods
            .insert(native_method_key(class_name, method_name), signature)
            .is_none()
    }

    /// Defines native AOT static-method signature metadata for eval named-argument binding.
    pub fn define_native_static_method_signature(
        &mut self,
        class_name: &str,
        method_name: &str,
        signature: NativeCallableSignature,
    ) -> bool {
        self.native_static_methods
            .insert(native_method_key(class_name, method_name), signature)
            .is_none()
    }

    /// Defines native AOT constructor signature metadata for eval named-argument binding.
    pub fn define_native_constructor_signature(
        &mut self,
        class_name: &str,
        signature: NativeCallableSignature,
    ) -> bool {
        self.native_constructors
            .insert(normalize_class_name(class_name), signature)
            .is_none()
    }

    /// Records one parameter name for registered native AOT instance-method metadata.
    pub fn define_native_method_param(
        &mut self,
        class_name: &str,
        method_name: &str,
        index: usize,
        param_name: impl Into<String>,
    ) -> bool {
        self.native_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| signature.set_param_name(index, param_name))
    }

    /// Records one parameter type for registered native AOT instance-method metadata.
    pub fn define_native_method_param_type(
        &mut self,
        class_name: &str,
        method_name: &str,
        index: usize,
        param_type: EvalParameterType,
    ) -> bool {
        self.native_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| signature.set_param_type(index, param_type))
    }

    /// Records one parameter default for registered native AOT instance-method metadata.
    pub fn define_native_method_param_default(
        &mut self,
        class_name: &str,
        method_name: &str,
        index: usize,
        default: NativeCallableDefault,
    ) -> bool {
        self.native_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| signature.set_param_default(index, default))
    }

    /// Records whether one native AOT instance-method parameter is by-reference.
    pub fn define_native_method_param_by_ref(
        &mut self,
        class_name: &str,
        method_name: &str,
        index: usize,
        by_ref: bool,
    ) -> bool {
        self.native_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| signature.set_param_by_ref(index, by_ref))
    }

    /// Records which native AOT instance-method parameter is variadic.
    pub fn define_native_method_variadic_param(
        &mut self,
        class_name: &str,
        method_name: &str,
        index: usize,
    ) -> bool {
        self.native_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| signature.set_variadic_index(index))
    }

    /// Records whether eval may dispatch one native AOT instance method.
    pub fn define_native_method_bridge_supported(
        &mut self,
        class_name: &str,
        method_name: &str,
        supported: bool,
    ) -> bool {
        self.native_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| {
                signature.set_bridge_supported(supported);
                true
            })
    }

    /// Records one return type for registered native AOT instance-method metadata.
    pub fn define_native_method_return_type(
        &mut self,
        class_name: &str,
        method_name: &str,
        return_type: EvalParameterType,
    ) -> bool {
        self.native_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| {
                signature.set_return_type(return_type);
                true
            })
    }

    /// Records one parameter name for registered native AOT static-method metadata.
    pub fn define_native_static_method_param(
        &mut self,
        class_name: &str,
        method_name: &str,
        index: usize,
        param_name: impl Into<String>,
    ) -> bool {
        self.native_static_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| signature.set_param_name(index, param_name))
    }

    /// Records one parameter type for registered native AOT static-method metadata.
    pub fn define_native_static_method_param_type(
        &mut self,
        class_name: &str,
        method_name: &str,
        index: usize,
        param_type: EvalParameterType,
    ) -> bool {
        self.native_static_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| signature.set_param_type(index, param_type))
    }

    /// Records one parameter default for registered native AOT static-method metadata.
    pub fn define_native_static_method_param_default(
        &mut self,
        class_name: &str,
        method_name: &str,
        index: usize,
        default: NativeCallableDefault,
    ) -> bool {
        self.native_static_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| signature.set_param_default(index, default))
    }

    /// Records whether one native AOT static-method parameter is by-reference.
    pub fn define_native_static_method_param_by_ref(
        &mut self,
        class_name: &str,
        method_name: &str,
        index: usize,
        by_ref: bool,
    ) -> bool {
        self.native_static_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| signature.set_param_by_ref(index, by_ref))
    }

    /// Records which native AOT static-method parameter is variadic.
    pub fn define_native_static_method_variadic_param(
        &mut self,
        class_name: &str,
        method_name: &str,
        index: usize,
    ) -> bool {
        self.native_static_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| signature.set_variadic_index(index))
    }

    /// Records whether eval may dispatch one native AOT static method.
    pub fn define_native_static_method_bridge_supported(
        &mut self,
        class_name: &str,
        method_name: &str,
        supported: bool,
    ) -> bool {
        self.native_static_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| {
                signature.set_bridge_supported(supported);
                true
            })
    }

    /// Records one return type for registered native AOT static-method metadata.
    pub fn define_native_static_method_return_type(
        &mut self,
        class_name: &str,
        method_name: &str,
        return_type: EvalParameterType,
    ) -> bool {
        self.native_static_methods
            .get_mut(&native_method_key(class_name, method_name))
            .is_some_and(|signature| {
                signature.set_return_type(return_type);
                true
            })
    }

    /// Records one parameter name for registered native AOT constructor metadata.
    pub fn define_native_constructor_param(
        &mut self,
        class_name: &str,
        index: usize,
        param_name: impl Into<String>,
    ) -> bool {
        self.native_constructors
            .get_mut(&normalize_class_name(class_name))
            .is_some_and(|signature| signature.set_param_name(index, param_name))
    }

    /// Records one parameter type for registered native AOT constructor metadata.
    pub fn define_native_constructor_param_type(
        &mut self,
        class_name: &str,
        index: usize,
        param_type: EvalParameterType,
    ) -> bool {
        self.native_constructors
            .get_mut(&normalize_class_name(class_name))
            .is_some_and(|signature| signature.set_param_type(index, param_type))
    }

    /// Records one parameter default for registered native AOT constructor metadata.
    pub fn define_native_constructor_param_default(
        &mut self,
        class_name: &str,
        index: usize,
        default: NativeCallableDefault,
    ) -> bool {
        self.native_constructors
            .get_mut(&normalize_class_name(class_name))
            .is_some_and(|signature| signature.set_param_default(index, default))
    }

    /// Records whether one native AOT constructor parameter is by-reference.
    pub fn define_native_constructor_param_by_ref(
        &mut self,
        class_name: &str,
        index: usize,
        by_ref: bool,
    ) -> bool {
        self.native_constructors
            .get_mut(&normalize_class_name(class_name))
            .is_some_and(|signature| signature.set_param_by_ref(index, by_ref))
    }

    /// Records which native AOT constructor parameter is variadic.
    pub fn define_native_constructor_variadic_param(
        &mut self,
        class_name: &str,
        index: usize,
    ) -> bool {
        self.native_constructors
            .get_mut(&normalize_class_name(class_name))
            .is_some_and(|signature| signature.set_variadic_index(index))
    }

    /// Records whether eval may dispatch one native AOT constructor.
    pub fn define_native_constructor_bridge_supported(
        &mut self,
        class_name: &str,
        supported: bool,
    ) -> bool {
        self.native_constructors
            .get_mut(&normalize_class_name(class_name))
            .is_some_and(|signature| {
                signature.set_bridge_supported(supported);
                true
            })
    }

    /// Returns native AOT instance-method signature metadata by PHP class and method name.
    pub fn native_method_signature(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Option<NativeCallableSignature> {
        self.native_methods
            .get(&native_method_key(class_name, method_name))
            .cloned()
    }

    /// Returns native AOT static-method signature metadata by PHP class and method name.
    pub fn native_static_method_signature(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Option<NativeCallableSignature> {
        self.native_static_methods
            .get(&native_method_key(class_name, method_name))
            .cloned()
    }

    /// Returns native AOT constructor signature metadata by PHP class name.
    pub fn native_constructor_signature(
        &self,
        class_name: &str,
    ) -> Option<NativeCallableSignature> {
        self.native_constructors
            .get(&normalize_class_name(class_name))
            .cloned()
    }
}
