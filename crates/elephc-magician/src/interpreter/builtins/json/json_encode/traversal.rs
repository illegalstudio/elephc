//! Purpose:
//! Encodes eval arrays and objects with an explicit depth-first work stack.
//!
//! Called from:
//! - The parent `json_encode` result wrapper.
//!
//! Key details:
//! - Container depth errors remain post-order so callbacks run in PHP order.
//! - Leave and release actions run while unwinding any JSON or runtime failure.
//! - Canonical payload identities detect active-path recursion across copied cells.

use std::collections::HashSet;

use super::errors::eval_json_encode_check_depth;
use super::objects::{EvalJsonObjectEncoding, eval_json_object_encoding};
use super::ownership::release_encoded_value;
use super::*;

/// Encodes one value without consuming native stack per nested container.
pub(super) fn eval_json_encode_append(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    depth_limit: usize,
    depth: usize,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> EvalJsonEncodeResult {
    let mut traversal = EvalJsonTraversal {
        context,
        values,
        flags,
        depth_limit,
        error,
        output,
        active: HashSet::new(),
        work: Vec::new(),
    };
    let failure = traversal.encode_value(value, depth).err();
    traversal.run(failure)
}

/// Executes traversal actions and always drains ownership and recursion guards.
struct EvalJsonTraversal<'a, V: RuntimeValueOps> {
    context: &'a mut ElephcEvalContext,
    values: &'a mut V,
    flags: i64,
    depth_limit: usize,
    error: &'a mut Option<EvalJsonEncodeError>,
    output: &'a mut Vec<u8>,
    active: HashSet<usize>,
    work: Vec<EvalJsonAction>,
}

#[derive(Clone, Copy)]
enum EvalJsonAction {
    Encode { value: RuntimeCellHandle, depth: usize },
    Next { container: EvalJsonContainer, position: usize },
    Close(EvalJsonContainer),
    Release(RuntimeCellHandle),
    Leave(usize),
}

#[derive(Clone, Copy)]
struct EvalJsonContainer {
    value: RuntimeCellHandle,
    depth: usize,
    len: usize,
    kind: EvalJsonContainerKind,
}

#[derive(Clone, Copy)]
enum EvalJsonContainerKind {
    IndexedArray { force_object: bool },
    AssocArray,
    ObjectProperties,
    NativeEnumProperties,
}

impl<V: RuntimeValueOps> EvalJsonTraversal<'_, V> {
    /// Runs scheduled work and lets later cleanup failures supersede earlier failures.
    fn run(
        mut self,
        mut failure: Option<EvalJsonEncodeFailure>,
    ) -> EvalJsonEncodeResult {
        while let Some(action) = self.work.pop() {
            match action {
                EvalJsonAction::Release(value) => {
                    if !value.is_borrowed() {
                        if let Err(status) = eval_release_value(self.context, self.values, value) {
                            failure = Some(EvalJsonEncodeFailure::Runtime(status));
                        }
                    }
                }
                EvalJsonAction::Leave(identity) => {
                    self.active.remove(&identity);
                }
                _ if failure.is_some() => {}
                _ => {
                    if let Err(current) = self.step(action) {
                        failure = Some(current);
                    }
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }

    /// Executes one non-cleanup traversal action.
    fn step(&mut self, action: EvalJsonAction) -> EvalJsonEncodeResult {
        match action {
            EvalJsonAction::Encode { value, depth } => self.encode_value(value, depth),
            EvalJsonAction::Next { container, position } => {
                self.encode_container_element(container, position)
            }
            EvalJsonAction::Close(container) => self.close_container(container),
            EvalJsonAction::Release(_) | EvalJsonAction::Leave(_) => unreachable!(),
        }
    }

    /// Encodes a scalar immediately or schedules one collection or object.
    fn encode_value(&mut self, value: RuntimeCellHandle, depth: usize) -> EvalJsonEncodeResult {
        match self.values.type_tag(value)? {
            EVAL_TAG_INT => self.output.extend_from_slice(&self.values.string_bytes(value)?),
            EVAL_TAG_FLOAT => eval_json_encode_append_float(
                value, self.values, self.flags, self.error, self.output,
            )?,
            EVAL_TAG_STRING => eval_json_encode_append_string(
                &self.values.string_bytes(value)?,
                self.flags,
                EvalJsonStringPosition::Value,
                self.error,
                self.output,
            )?,
            EVAL_TAG_BOOL => self.output.extend_from_slice(
                if self.values.truthy(value)? { b"true" } else { b"false" },
            ),
            EVAL_TAG_ARRAY => self.start_container(
                value,
                depth,
                EvalJsonContainerKind::IndexedArray {
                    force_object: self.flags & EVAL_JSON_FORCE_OBJECT != 0,
                },
                false,
            )?,
            EVAL_TAG_ASSOC => self.start_container(
                value, depth, EvalJsonContainerKind::AssocArray, false,
            )?,
            EVAL_TAG_OBJECT => self.encode_object(value, depth)?,
            EVAL_TAG_NULL | EVAL_TAG_RESOURCE => self.output.extend_from_slice(b"null"),
            _ => return Err(EvalStatus::UnsupportedConstruct.into()),
        }
        Ok(())
    }

    /// Selects an object's PHP representation and schedules owned results safely.
    fn encode_object(&mut self, value: RuntimeCellHandle, depth: usize) -> EvalJsonEncodeResult {
        match eval_json_object_encoding(value, self.context, self.values)? {
            EvalJsonObjectEncoding::Properties => self.start_container(
                value, depth, EvalJsonContainerKind::ObjectProperties, false,
            ),
            EvalJsonObjectEncoding::BackedEnum(backing) => {
                self.work.push(EvalJsonAction::Release(backing));
                self.work.push(EvalJsonAction::Encode { value: backing, depth });
                Ok(())
            }
            EvalJsonObjectEncoding::UnitEnum => {
                self.output.push(b'0');
                eval_json_encode_record_error(
                    self.flags,
                    self.error,
                    EvalJsonEncodeError {
                        code: EVAL_JSON_ERROR_NON_BACKED_ENUM,
                        message: "Non-backed enums have no default serialization",
                    },
                )
            }
            EvalJsonObjectEncoding::SerializableEnum => {
                self.encode_serializable_enum(value, depth)
            }
        }
    }

    /// Invokes one enum serializer and schedules its owned result at the same depth.
    fn encode_serializable_enum(
        &mut self,
        value: RuntimeCellHandle,
        depth: usize,
    ) -> EvalJsonEncodeResult {
        let identity = self.values.object_identity(value)? as usize;
        if self.active.contains(&identity) {
            self.output.extend_from_slice(b"null");
            return eval_json_encode_record_error(
                self.flags,
                self.error,
                EvalJsonEncodeError { code: 6, message: "Recursion detected" },
            );
        }
        self.active.insert(identity);
        self.work.push(EvalJsonAction::Leave(identity));

        let result = eval_method_call_result(
            value, "jsonSerialize", Vec::new(), self.context, self.values,
        )?;
        self.work.push(EvalJsonAction::Release(result));
        let returns_receiver = self.values.type_tag(result)? == EVAL_TAG_OBJECT
            && self.values.object_identity(result)? as usize == identity;
        if returns_receiver {
            // Native enum iterators do not expose their synthetic name/value properties.
            let native_receiver = dynamic_object_is_a(
                result, "UnitEnum", false, self.context, self.values,
            )?
            .is_none();
            let kind = if native_receiver {
                EvalJsonContainerKind::NativeEnumProperties
            } else {
                EvalJsonContainerKind::ObjectProperties
            };
            self.start_container(result, depth, kind, true)
        } else {
            self.work.push(EvalJsonAction::Encode { value: result, depth });
            Ok(())
        }
    }

    /// Enters one container, emits its opening token and schedules post-order work.
    fn start_container(
        &mut self,
        value: RuntimeCellHandle,
        depth: usize,
        kind: EvalJsonContainerKind,
        guard_active: bool,
    ) -> EvalJsonEncodeResult {
        if !guard_active {
            let identity = match kind {
                EvalJsonContainerKind::IndexedArray { .. } | EvalJsonContainerKind::AssocArray => {
                    self.values.raw_value_word(value)? as usize
                }
                EvalJsonContainerKind::ObjectProperties
                | EvalJsonContainerKind::NativeEnumProperties => {
                    self.values.object_identity(value)? as usize
                }
            };
            if self.active.contains(&identity) {
                self.output.extend_from_slice(b"null");
                eval_json_encode_record_error(
                    self.flags,
                    self.error,
                    EvalJsonEncodeError { code: 6, message: "Recursion detected" },
                )?;
                return Ok(());
            }
            self.active.insert(identity);
            self.work.push(EvalJsonAction::Leave(identity));
        }

        let len = match kind {
            EvalJsonContainerKind::IndexedArray { .. } | EvalJsonContainerKind::AssocArray => {
                self.values.array_len(value)?
            }
            EvalJsonContainerKind::ObjectProperties => {
                self.values.object_property_len(value)?
            }
            EvalJsonContainerKind::NativeEnumProperties => {
                if self.values.property_is_initialized(value, "value")? { 2 } else { 1 }
            }
        };
        let open = match kind {
            EvalJsonContainerKind::IndexedArray { force_object: false } => b'[',
            _ => b'{',
        };
        self.output.push(open);
        if self.pretty() && len > 0 {
            self.output.push(b'\n');
        }
        let container = EvalJsonContainer { value, depth, len, kind };
        self.work.push(EvalJsonAction::Close(container));
        self.work.push(EvalJsonAction::Next { container, position: 0 });
        Ok(())
    }

    /// Emits one collection entry and schedules its child before the following entry.
    fn encode_container_element(
        &mut self,
        container: EvalJsonContainer,
        position: usize,
    ) -> EvalJsonEncodeResult {
        if position >= container.len {
            return Ok(());
        }
        if position > 0 {
            self.output.push(b',');
            if self.pretty() {
                self.output.push(b'\n');
            }
        }
        if self.pretty() {
            eval_json_encode_pretty_indent(self.output, container.depth + 1);
        }

        let element = match container.kind {
            EvalJsonContainerKind::IndexedArray { force_object } => {
                if force_object {
                    let key = self.values.array_iter_key(container.value, position)?;
                    self.append_owned_key(key)?;
                }
                self.values.array_iter_value(container.value, position)?
            }
            EvalJsonContainerKind::AssocArray => {
                let key = self.values.array_iter_key(container.value, position)?;
                self.append_owned_key(key)?;
                self.values.array_iter_value(container.value, position)?
            }
            EvalJsonContainerKind::ObjectProperties => {
                let key = self.values.object_property_iter_key(container.value, position)?;
                let key_bytes = self.read_owned_key(key)?;
                self.append_key(&key_bytes)?;
                let property = std::str::from_utf8(&key_bytes)
                    .map_err(|_| EvalStatus::RuntimeFatal)?;
                self.values.property_get(container.value, property)?
            }
            EvalJsonContainerKind::NativeEnumProperties => {
                let name = if position == 0 { "name" } else { "value" };
                self.output.push(b'"');
                self.output.extend_from_slice(name.as_bytes());
                self.output.push(b'"');
                eval_json_encode_append_colon(self.flags, self.output);
                eval_property_get_result(container.value, name, self.context, self.values)?
            }
        };

        self.work.push(EvalJsonAction::Next {
            container,
            position: position + 1,
        });
        self.work.push(EvalJsonAction::Release(element));
        self.work.push(EvalJsonAction::Encode {
            value: element,
            depth: container.depth + 1,
        });
        Ok(())
    }

    /// Reads and releases one temporary key before appending its JSON representation.
    fn append_owned_key(&mut self, key: RuntimeCellHandle) -> EvalJsonEncodeResult {
        let key_bytes = self.read_owned_key(key)?;
        self.append_key(&key_bytes)
    }

    /// Copies one temporary key and releases its runtime cell on every result.
    fn read_owned_key(&mut self, key: RuntimeCellHandle) -> EvalJsonEncodeResult<Vec<u8>> {
        let bytes = self.values.string_bytes(key).map_err(EvalJsonEncodeFailure::Runtime);
        release_encoded_value(key, bytes, self.context, self.values)
    }

    /// Appends one object key and its colon with numeric conversion disabled.
    fn append_key(&mut self, key_bytes: &[u8]) -> EvalJsonEncodeResult {
        eval_json_encode_append_string(
            key_bytes,
            self.flags & !EVAL_JSON_NUMERIC_CHECK,
            EvalJsonStringPosition::Key,
            self.error,
            self.output,
        )?;
        eval_json_encode_append_colon(self.flags, self.output);
        Ok(())
    }

    /// Emits a container's closing token and applies PHP's post-order depth check.
    fn close_container(&mut self, container: EvalJsonContainer) -> EvalJsonEncodeResult {
        if self.pretty() && container.len > 0 {
            self.output.push(b'\n');
            eval_json_encode_pretty_indent(self.output, container.depth);
        }
        let close = match container.kind {
            EvalJsonContainerKind::IndexedArray { force_object: false } => b']',
            _ => b'}',
        };
        self.output.push(close);
        eval_json_encode_check_depth(
            self.depth_limit,
            container.depth,
            self.flags,
            self.error,
        )
    }

    /// Reports whether PHP pretty-print whitespace is active.
    fn pretty(&self) -> bool {
        self.flags & EVAL_JSON_PRETTY_PRINT != 0
    }
}
