//! Purpose:
//! Implements modern `Dom\TokenList` behavior for an element's `class` attribute.
//! Preserves ordered-set semantics and cached `classList` wrapper identity.
//!
//! Called from:
//! - `super::routes` for token-list methods.
//! - `super::routes::properties` for `classList`, `length`, and `value`.
//!
//! Key details:
//! - The current unnamespaced class attribute is reparsed before every operation.
//! - Mutations validate every token first and serialize unique tokens with one space.
//! - Token-list handles retain and follow the authoritative document graph.

use crate::context::Context;
use crate::objects::{
    DocumentFamily, NativeObject, TokenListObject, HANDLE_TOKEN_LIST,
};
use crate::request::Request;

use super::{
    receiver_pointer_and_graph, require_no_values, token_list, DispatchResult,
};

/// Returns the cached `Dom\TokenList` associated with one modern element.
pub(super) fn class_list(
    context: &mut Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let (element, graph, is_document) =
        receiver_pointer_and_graph(context, request.header.receiver)?;
    if is_document
        || graph.family() == DocumentFamily::Legacy
        || crate::native::node_type(element) != 1
    {
        return Err(());
    }
    if let Some(handle) = context.token_list_handles.get(&element).copied() {
        let valid = context
            .native_objects
            .get(handle, HANDLE_TOKEN_LIST)
            .ok()
            .and_then(NativeObject::token_list)
            .is_some_and(|token_list| token_list.element() == element);
        if valid {
            return Ok(DispatchResult::bridge_handle(handle));
        }
        context.token_list_handles.remove(&element);
    }
    let handle = context.native_objects.insert(
        HANDLE_TOKEN_LIST,
        NativeObject::TokenList(TokenListObject::new(element, graph)),
    );
    context.token_list_handles.insert(element, handle);
    Ok(DispatchResult::bridge_handle(handle))
}

/// Returns the current number of unique ordered class tokens.
pub(super) fn length(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let tokens = current_tokens(token_list_element(context, request)?);
    Ok(DispatchResult::integer(tokens.len() as i64))
}

/// Returns the current raw class-attribute value without canonicalization.
pub(super) fn value(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    require_no_values(request)?;
    let element = token_list_element(context, request)?;
    Ok(DispatchResult::bytes(
        crate::native::element_get_attribute_ns(element, None, b"class")
            .unwrap_or_default(),
    ))
}

/// Replaces the raw class-attribute value and defers ordered-set parsing.
pub(super) fn set_value(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let value = request.byte_string(0)?;
    if value.contains(&0) {
        return Ok(DispatchResult::value_error(
            b"Value must not contain any null bytes",
        ));
    }
    let element = token_list_element(context, request)?;
    crate::native::element_set_attribute(element, b"class", value).ok_or(())?;
    Ok(DispatchResult::null())
}

/// Returns one token by zero-based position or null when the index is out of bounds.
pub(super) fn item(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let index = request.integer(0)?;
    let element = token_list_element(context, request)?;
    let Some(token) = usize::try_from(index)
        .ok()
        .and_then(|index| current_tokens(element).get(index).cloned())
    else {
        return Ok(DispatchResult::null());
    };
    Ok(DispatchResult::bytes(token))
}

/// Tests exact membership without applying the mutation-token validity rules.
pub(super) fn contains(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let candidate = request.byte_string(0)?;
    if candidate.contains(&0) {
        return Ok(null_byte_error("contains", 1, Some("token")));
    }
    let element = token_list_element(context, request)?;
    Ok(DispatchResult::boolean(
        current_tokens(element)
            .iter()
            .any(|token| token == candidate),
    ))
}

/// Appends every absent token after validating the complete variadic input.
pub(super) fn add(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let additions = validate_variadic_tokens(request, "add")?;
    if let Some(error) = additions.error {
        return Ok(error);
    }
    let element = token_list_element(context, request)?;
    let mut tokens = current_tokens(element);
    for token in additions.tokens {
        if !tokens.contains(&token) {
            tokens.push(token);
        }
    }
    update_tokens(element, &tokens)?;
    Ok(DispatchResult::null())
}

/// Removes every supplied token after validating the complete variadic input.
pub(super) fn remove(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    let removals = validate_variadic_tokens(request, "remove")?;
    if let Some(error) = removals.error {
        return Ok(error);
    }
    let element = token_list_element(context, request)?;
    let mut tokens = current_tokens(element);
    tokens.retain(|token| !removals.tokens.contains(token));
    update_tokens(element, &tokens)?;
    Ok(DispatchResult::null())
}

/// Toggles one token according to PHP's omitted-or-null force semantics.
pub(super) fn toggle(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if !(1..=2).contains(&request.values.len()) {
        return Err(());
    }
    let candidate = request.byte_string(0)?;
    if let Some(error) =
        validate_token(candidate, "toggle", 1, Some("token"))
    {
        return Ok(error);
    }
    let force = if request.values.len() == 2 {
        request.optional_boolean(1)?
    } else {
        None
    };
    let element = token_list_element(context, request)?;
    let mut tokens = current_tokens(element);
    if let Some(index) = tokens.iter().position(|token| token == candidate) {
        if force == Some(true) {
            return Ok(DispatchResult::boolean(true));
        }
        tokens.remove(index);
        update_tokens(element, &tokens)?;
        return Ok(DispatchResult::boolean(false));
    }
    if force == Some(false) {
        return Ok(DispatchResult::boolean(false));
    }
    tokens.push(candidate.to_vec());
    update_tokens(element, &tokens)?;
    Ok(DispatchResult::boolean(true))
}

/// Replaces one existing token in place, removing it when the replacement exists.
pub(super) fn replace(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 2 {
        return Err(());
    }
    let candidate = request.byte_string(0)?;
    if let Some(error) =
        validate_token(candidate, "replace", 1, Some("token"))
    {
        return Ok(error);
    }
    let replacement = request.byte_string(1)?;
    if let Some(error) =
        validate_token(replacement, "replace", 2, Some("newToken"))
    {
        return Ok(error);
    }
    let element = token_list_element(context, request)?;
    let mut tokens = current_tokens(element);
    let Some(index) = tokens.iter().position(|token| token == candidate) else {
        return Ok(DispatchResult::boolean(false));
    };
    if candidate != replacement {
        if tokens.iter().any(|token| token == replacement) {
            tokens.remove(index);
        } else {
            tokens[index] = replacement.to_vec();
        }
        update_tokens(element, &tokens)?;
    } else {
        update_tokens(element, &tokens)?;
    }
    Ok(DispatchResult::boolean(true))
}

/// Rejects feature-token queries because `class` defines no supported-token set.
pub(super) fn supports(
    context: &Context,
    request: &Request,
) -> Result<DispatchResult, ()> {
    if request.values.len() != 1 {
        return Err(());
    }
    let candidate = request.byte_string(0)?;
    if candidate.contains(&0) {
        return Ok(null_byte_error("supports", 1, Some("token")));
    }
    token_list_element(context, request)?;
    Ok(DispatchResult::type_error(
        b"Attribute \"class\" does not define any supported tokens",
    ))
}

/// Returns the associated native element for one validated token-list handle.
fn token_list_element(
    context: &Context,
    request: &Request,
) -> Result<usize, ()> {
    Ok(token_list(context, request.header.receiver)?.element())
}

/// Parses the current class attribute into a unique insertion-ordered token set.
fn current_tokens(element: usize) -> Vec<Vec<u8>> {
    let Some(value) =
        crate::native::element_get_attribute_ns(element, None, b"class")
    else {
        return Vec::new();
    };
    let mut tokens = Vec::new();
    for token in value
        .split(|byte| is_ascii_whitespace(*byte))
        .filter(|token| !token.is_empty())
    {
        if !tokens.iter().any(|existing| existing == token) {
            tokens.push(token.to_vec());
        }
    }
    tokens
}

/// Serializes a token set into the associated unnamespaced class attribute.
fn update_tokens(element: usize, tokens: &[Vec<u8>]) -> Result<(), ()> {
    let has_attribute =
        crate::native::element_get_attribute_node_ns(element, None, b"class")
            .is_some();
    if !has_attribute && tokens.is_empty() {
        return Ok(());
    }
    let mut value = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if index != 0 {
            value.push(b' ');
        }
        value.extend_from_slice(token);
    }
    crate::native::element_set_attribute(element, b"class", &value)
        .ok_or(())
        .map(|_| ())
}

/// Holds either a fully validated variadic token sequence or one PHP error.
struct ValidatedTokens {
    tokens: Vec<Vec<u8>>,
    error: Option<DispatchResult>,
}

/// Validates all variadic mutation tokens before exposing any native side effect.
fn validate_variadic_tokens(
    request: &Request,
    method: &str,
) -> Result<ValidatedTokens, ()> {
    let mut tokens = Vec::with_capacity(request.values.len());
    for index in 0..request.values.len() {
        let token = request.byte_string(index)?;
        if let Some(error) = validate_token(token, method, index + 1, None) {
            return Ok(ValidatedTokens {
                tokens: Vec::new(),
                error: Some(error),
            });
        }
        tokens.push(token.to_vec());
    }
    Ok(ValidatedTokens {
        tokens,
        error: None,
    })
}

/// Returns PHP's first token-validation failure, if any.
fn validate_token(
    token: &[u8],
    method: &str,
    index: usize,
    parameter: Option<&str>,
) -> Option<DispatchResult> {
    if token.contains(&0) {
        return Some(null_byte_error(method, index, parameter));
    }
    if token.is_empty() {
        return Some(DispatchResult::dom_exception(
            12,
            b"The empty string is not a valid token",
        ));
    }
    if token.iter().any(|byte| is_ascii_whitespace(*byte)) {
        return Some(DispatchResult::dom_exception(
            5,
            b"The token must not contain any ASCII whitespace",
        ));
    }
    None
}

/// Builds one exact method-qualified PHP null-byte `ValueError`.
fn null_byte_error(
    method: &str,
    index: usize,
    parameter: Option<&str>,
) -> DispatchResult {
    let parameter = parameter
        .map(|parameter| format!(" (${parameter})"))
        .unwrap_or_default();
    DispatchResult::value_error(
        format!(
            "Dom\\TokenList::{method}(): Argument #{index}{parameter} must not contain any null bytes"
        )
        .as_bytes(),
    )
}

/// Reports whether one byte is HTML's ASCII whitespace.
fn is_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | 0x0C | b'\r' | b' ')
}
