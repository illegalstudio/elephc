//! Purpose:
//! Implements fake runtime instance-method dispatch used by interpreter tests.
//!
//! Called from:
//! - `FakeOps`'s `RuntimeValueOps::method_call()` implementation.
//!
//! Key details:
//! - The match models only registered test doubles and their observable effects.

use super::*;

impl FakeOps {

    /// Calls one fake object method by name.
    pub(in crate::interpreter::tests::support) fn runtime_method_call(
        &mut self,
        object: RuntimeCellHandle,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus> {
        let method = method.to_ascii_lowercase();
        match (self.get(object), method.as_str()) {
            (FakeValue::Iterator { .. }, "rewind") if args.is_empty() => {
                let id = object.as_ptr() as usize;
                let Some(FakeValue::Iterator { position, .. }) = self.values.get_mut(&id) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                *position = 0;
                self.null()
            }
            (FakeValue::Iterator { len, position }, "valid") if args.is_empty() => {
                self.bool_value(position < len)
            }
            (FakeValue::Iterator { .. }, "next") if args.is_empty() => {
                let id = object.as_ptr() as usize;
                let Some(FakeValue::Iterator { position, .. }) = self.values.get_mut(&id) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                *position += 1;
                self.null()
            }
            (FakeValue::Object(_), "answer") if args.is_empty() => self.int(42),
            (FakeValue::Object(properties), "__tostring") if args.is_empty() => {
                let class_name = self.object_classes.get(&(object.as_ptr() as usize)).cloned();
                self.reflection_type_to_string(class_name.as_deref(), &properties)
            }
            (FakeValue::Object(properties), "getname") if args.is_empty() => {
                Self::object_property(&properties, "__name").map_or_else(|| self.string(""), Ok)
            }
            (FakeValue::Object(_), "getdoccomment" | "getextensionname") if args.is_empty() => {
                self.bool_value(false)
            }
            (FakeValue::Object(_), "getextension") if args.is_empty() => self.null(),
            (FakeValue::Object(properties), "getshortname") if args.is_empty() => {
                Self::object_property(&properties, "__short_name")
                    .map_or_else(|| self.string(""), Ok)
            }
            (FakeValue::Object(properties), "getnamespacename") if args.is_empty() => {
                Self::object_property(&properties, "__namespace_name")
                    .map_or_else(|| self.string(""), Ok)
            }
            (FakeValue::Object(properties), "innamespace") if args.is_empty() => {
                Self::object_property(&properties, "__in_namespace")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isfinal") if args.is_empty() => {
                Self::object_property(&properties, "__is_final")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isabstract") if args.is_empty() => {
                Self::object_property(&properties, "__is_abstract")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isinterface") if args.is_empty() => {
                Self::object_property(&properties, "__is_interface")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "istrait") if args.is_empty() => {
                Self::object_property(&properties, "__is_trait")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isenum") if args.is_empty() => {
                Self::object_property(&properties, "__is_enum")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isreadonly") if args.is_empty() => {
                Self::object_property(&properties, "__is_readonly")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "ispromoted") if args.is_empty() => {
                Self::object_property(&properties, "__is_promoted")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isvirtual") if args.is_empty() => {
                Self::object_property(&properties, "__is_virtual")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isanonymous") if args.is_empty() => {
                Self::object_property(&properties, "__is_anonymous")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isinstantiable") if args.is_empty() => {
                Self::object_property(&properties, "__is_instantiable")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "iscloneable") if args.is_empty() => {
                Self::object_property(&properties, "__is_cloneable")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isiterable" | "isiterateable") if args.is_empty() => {
                Self::object_property(&properties, "__is_iterable")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isinternal") if args.is_empty() => {
                Self::object_property(&properties, "__is_internal")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isuserdefined") if args.is_empty() => {
                Self::object_property(&properties, "__is_user_defined")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isdeprecated") if args.is_empty() => {
                Self::object_property(&properties, "__is_deprecated")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "getparentclass") if args.is_empty() => {
                Self::object_property(&properties, "__parent_class")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "getconstructor") if args.is_empty() => {
                Self::object_property(&properties, "__constructor").map_or_else(|| self.null(), Ok)
            }
            (FakeValue::Object(properties), "getdeclaringclass") if args.is_empty() => {
                Self::object_property(&properties, "__declaring_class")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "getdeclaringfunction") if args.is_empty() => {
                Self::object_property(&properties, "__declaring_function")
                    .map_or_else(|| self.null(), Ok)
            }
            (FakeValue::Object(properties), "getmodifiers") if args.is_empty() => {
                Self::object_property(&properties, "__modifiers").map_or_else(|| self.int(0), Ok)
            }
            (FakeValue::Object(properties), "isprotectedset") if args.is_empty() => {
                self.reflection_modifier_mask(&properties, 2048)
            }
            (FakeValue::Object(properties), "isprivateset") if args.is_empty() => {
                self.reflection_modifier_mask(&properties, 4096)
            }
            (FakeValue::Object(properties), "getvalue") if args.is_empty() => {
                Self::object_property(&properties, "__value").map_or_else(|| self.null(), Ok)
            }
            (FakeValue::Object(properties), "getbackingvalue") if args.is_empty() => {
                Self::object_property(&properties, "__backing_value")
                    .map_or_else(|| self.null(), Ok)
            }
            (FakeValue::Object(properties), "isenumcase") if args.is_empty() => {
                Self::object_property(&properties, "__is_enum_case")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isstatic") if args.is_empty() => {
                Self::object_property(&properties, "__is_static")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "ispublic") if args.is_empty() => {
                Self::object_property(&properties, "__is_public")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isprotected") if args.is_empty() => {
                Self::object_property(&properties, "__is_protected")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isprivate") if args.is_empty() => {
                Self::object_property(&properties, "__is_private")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "hasmethod") if args.len() == 1 => {
                self.object_string_array_contains(&properties, "__method_names", args[0], true)
            }
            (FakeValue::Object(properties), "hasproperty") if args.len() == 1 => {
                self.object_string_array_contains(&properties, "__property_names", args[0], false)
            }
            (FakeValue::Object(properties), "hasconstant") if args.len() == 1 => {
                self.object_string_array_contains(&properties, "__constant_names", args[0], false)
            }
            (FakeValue::Object(properties), "getconstant") if args.len() == 1 => {
                let Some(constants) = Self::object_property(&properties, "__constants") else {
                    return self.bool_value(false);
                };
                let exists = self.runtime_array_key_exists(args[0], constants)?;
                if matches!(self.get(exists), FakeValue::Bool(true)) {
                    self.runtime_array_get(constants, args[0])
                } else {
                    self.bool_value(false)
                }
            }
            (FakeValue::Object(properties), "implementsinterface") if args.len() == 1 => {
                let direct = self.object_string_array_contains(
                    &properties,
                    "__interface_names",
                    args[0],
                    true,
                )?;
                if matches!(self.get(direct), FakeValue::Bool(true)) {
                    return Ok(direct);
                }
                let Some(is_interface) = Self::object_property(&properties, "__is_interface")
                else {
                    return Ok(direct);
                };
                if !matches!(self.get(is_interface), FakeValue::Bool(true)) {
                    return Ok(direct);
                }
                let Some(reflected_name) = Self::object_property(&properties, "__name") else {
                    return Ok(direct);
                };
                let FakeValue::String(reflected_name) = self.get(reflected_name) else {
                    return Ok(direct);
                };
                let FakeValue::String(interface_name) = self.get(args[0]) else {
                    return Ok(direct);
                };
                self.bool_value(reflected_name.eq_ignore_ascii_case(&interface_name))
            }
            (FakeValue::Object(properties), "getinterfacenames") if args.is_empty() => {
                Self::object_property(&properties, "__interface_names")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getinterfaces") if args.is_empty() => {
                self.object_relation_reflection_classes(&properties, "__interface_names")
            }
            (FakeValue::Object(properties), "gettraitnames") if args.is_empty() => {
                Self::object_property(&properties, "__trait_names")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "gettraits") if args.is_empty() => {
                self.object_relation_reflection_classes(&properties, "__trait_names")
            }
            (FakeValue::Object(properties), "getmethods") if args.is_empty() => {
                Self::object_property(&properties, "__methods")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getmethod") if args.len() == 1 => {
                self.object_named_member(&properties, "__methods", args[0], true)
            }
            (FakeValue::Object(properties), "getproperties") if args.is_empty() => {
                Self::object_property(&properties, "__properties")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getproperty") if args.len() == 1 => {
                self.object_named_member(&properties, "__properties", args[0], false)
            }
            (FakeValue::Object(properties), "getconstants") if args.is_empty() => {
                Self::object_property(&properties, "__constants")
                    .map_or_else(|| self.runtime_assoc_new(0), Ok)
            }
            (FakeValue::Object(properties), "getreflectionconstants") if args.is_empty() => {
                Self::object_property(&properties, "__reflection_constants")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getreflectionconstant") if args.len() == 1 => self
                .object_named_member_or_false(
                    &properties,
                    "__reflection_constants",
                    args[0],
                    false,
                ),
            (FakeValue::Object(properties), "getarguments") if args.is_empty() => {
                Self::object_property(&properties, "__args")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getparameters") if args.is_empty() => {
                Self::object_property(&properties, "__parameters")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getnumberofparameters") if args.is_empty() => {
                match Self::object_property(&properties, "__parameters") {
                    Some(parameters) => {
                        let len = self.array_len(parameters)?;
                        self.int(len as i64)
                    }
                    None => self.int(0),
                }
            }
            (FakeValue::Object(properties), "getnumberofrequiredparameters") if args.is_empty() => {
                Self::object_property(&properties, "__required_parameter_count")
                    .map_or_else(|| self.int(0), Ok)
            }
            (FakeValue::Object(properties), "getposition") if args.is_empty() => {
                Self::object_property(&properties, "__position").map_or_else(|| self.int(0), Ok)
            }
            (FakeValue::Object(properties), "gettype") if args.is_empty() => {
                Self::object_property(&properties, "__type").map_or_else(|| self.null(), Ok)
            }
            (FakeValue::Object(properties), "getsettabletype") if args.is_empty() => {
                Self::object_property(&properties, "__settable_type")
                    .map_or_else(|| self.null(), Ok)
            }
            (FakeValue::Object(properties), "getclass") if args.is_empty() => {
                Self::object_property(&properties, "__class").map_or_else(|| self.null(), Ok)
            }
            (FakeValue::Object(properties), "isdynamic") if args.is_empty() => {
                Self::object_property(&properties, "__is_dynamic")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "gettypes") if args.is_empty() => {
                Self::object_property(&properties, "__types")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getdefaultvalue") if args.is_empty() => {
                Self::object_property(&properties, "__default_value")
                    .map_or_else(|| self.null(), Ok)
            }
            (FakeValue::Object(properties), "getdefaultvalueconstantname") if args.is_empty() => {
                Self::object_property(&properties, "__default_value_constant_name")
                    .map_or_else(|| self.null(), Ok)
            }
            (FakeValue::Object(properties), "isconstructor") if args.is_empty() => {
                let Some(name) = Self::object_property(&properties, "__name") else {
                    return self.bool_value(false);
                };
                let FakeValue::String(name) = self.get(name) else {
                    return self.bool_value(false);
                };
                self.bool_value(name.eq_ignore_ascii_case("__construct"))
            }
            (FakeValue::Object(properties), "isdestructor") if args.is_empty() => {
                let Some(name) = Self::object_property(&properties, "__name") else {
                    return self.bool_value(false);
                };
                let FakeValue::String(name) = self.get(name) else {
                    return self.bool_value(false);
                };
                self.bool_value(name.eq_ignore_ascii_case("__destruct"))
            }
            (FakeValue::Object(properties), "isoptional") if args.is_empty() => {
                Self::object_property(&properties, "__is_optional")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isvariadic") if args.is_empty() => {
                Self::object_property(&properties, "__is_variadic")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "ispassedbyreference") if args.is_empty() => {
                Self::object_property(&properties, "__is_passed_by_reference")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "canbepassedbyvalue") if args.is_empty() => {
                let Some(by_ref) = Self::object_property(&properties, "__is_passed_by_reference")
                else {
                    return self.bool_value(true);
                };
                self.bool_value(!matches!(self.get(by_ref), FakeValue::Bool(true)))
            }
            (FakeValue::Object(properties), "hastype") if args.is_empty() => {
                if let Some(has_type) = Self::object_property(&properties, "__has_type") {
                    return Ok(has_type);
                }
                match Self::object_property(&properties, "__type") {
                    Some(value) => self.bool_value(!matches!(self.get(value), FakeValue::Null)),
                    None => self.bool_value(false),
                }
            }
            (FakeValue::Object(properties), "isdefaultvalueavailable" | "hasdefaultvalue")
                if args.is_empty() =>
            {
                Self::object_property(&properties, "__has_default_value")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isdefaultvalueconstant") if args.is_empty() => {
                Self::object_property(&properties, "__is_default_value_constant")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isdefault") if args.is_empty() => {
                match Self::object_property(&properties, "__is_dynamic") {
                    Some(is_dynamic) => {
                        self.bool_value(!matches!(self.get(is_dynamic), FakeValue::Bool(true)))
                    }
                    None => self.bool_value(true),
                }
            }
            (FakeValue::Object(properties), "allowsnull") if args.is_empty() => {
                Self::object_property(&properties, "__allows_null")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isarray") if args.is_empty() => {
                Self::object_property(&properties, "__is_array_type")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "iscallable") if args.is_empty() => {
                Self::object_property(&properties, "__is_callable_type")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(properties), "isbuiltin") if args.is_empty() => {
                Self::object_property(&properties, "__is_builtin")
                    .map_or_else(|| self.bool_value(false), Ok)
            }
            (FakeValue::Object(_), "newinstance") if args.is_empty() => self.null(),
            (FakeValue::Object(properties), "getattributes") if args.is_empty() => {
                Self::object_property(&properties, "__attrs")
                    .map_or_else(|| self.runtime_array_new(0), Ok)
            }
            (FakeValue::Object(properties), "getmessage") if args.is_empty() => {
                Self::object_property(&properties, "message").map_or_else(|| self.string(""), Ok)
            }
            (FakeValue::Object(properties), "getcode") if args.is_empty() => {
                Self::object_property(&properties, "code").map_or_else(|| self.int(0), Ok)
            }
            (FakeValue::Object(properties), "read_x") => {
                if !args.is_empty() {
                    return Err(EvalStatus::UnsupportedConstruct);
                }
                Self::object_property(&properties, "x").map_or_else(|| self.null(), Ok)
            }
            (FakeValue::Object(properties), "add_x") => {
                let [arg] = args.as_slice() else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let x = Self::object_property(&properties, "x").ok_or(EvalStatus::RuntimeFatal)?;
                let FakeValue::Int(x) = self.get(x) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::Int(arg) = self.get(*arg) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                self.int(x + arg)
            }
            (FakeValue::Object(properties), "add2_x") => {
                let [left, right] = args.as_slice() else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let x = Self::object_property(&properties, "x").ok_or(EvalStatus::RuntimeFatal)?;
                let FakeValue::Int(x) = self.get(x) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::Int(left) = self.get(*left) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                let FakeValue::Int(right) = self.get(*right) else {
                    return Err(EvalStatus::UnsupportedConstruct);
                };
                self.int(x + left + right)
            }
            _ => Err(EvalStatus::UnsupportedConstruct),
        }
    }

}
