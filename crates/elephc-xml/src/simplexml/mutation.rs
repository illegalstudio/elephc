//! Purpose:
//! SimpleXML tree writes and removal with native ownership retained by live views.
//!
//! Called from:
//! - SimpleXML C ABI mutation entry points.
//!
//! Key details:
//! - Assignments escape text; addChild preserves libxml2 entity semantics.

use super::*;

/// Negative ABI results distinguish PHP's user-visible add-operation warnings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i64)]
pub(super) enum AddError {
    ElementToAttributes = -10,
    MissingChildParent = -11,
    MissingAttributeParent = -12,
    AttributePrefix = -13,
    DuplicateAttribute = -14,
    InvalidName = -15,
}

impl View {
    /// Creates the sibling selected by an absent writable numeric offset.
    pub(super) fn append_sibling(&self) -> Option<Self> {
        let base = if matches!(self.scope, Scope::Children(_, None)) { self.identity } else { self.first()? };
        let node = unsafe { ffi::append_sibling(base) };
        if node.is_null() { return None; }
        let mut view = self.with_scope(Scope::Node(node));
        if unsafe { ffi::parent(node).is_null() } {
            view._clone_owner = Some(Rc::new(CloneOwner { node, _doc: self.doc.clone(), transferred: Cell::new(false) }));
        }
        Some(view)
    }
    /// Creates a missing named child and its missing ancestors for a chained write.
    pub(super) fn ensure_first(&self) -> Option<Node> {
        if let Some(node) = self.first() { return Some(node); }
        match &self.scope {
            Scope::Children(parent, Some(name)) => {
                let node = parent.ensure_first()?;
                parent.with_scope(Scope::Node(node)).add_literal(name, None, false)
            },
            Scope::Attributes(parent, Some(name)) => {
                let node = parent.ensure_first()?;
                parent.with_scope(Scope::Node(node)).add_literal(name, None, true)
            },
            Scope::Index(parent, 0) => parent.ensure_first(),
            _ => None,
        }
    }
    /// Adds an element or attribute under the first selected element.
    #[cfg(all(test, elephc_xml_native))]
    pub(super) fn add(&self, name: &str, value: Option<&str>, namespace: Option<&str>, attribute: bool) -> Option<Node> {
        self.add_checked(name.as_bytes(), value.map(str::as_bytes), namespace.map(str::as_bytes), attribute).ok()
    }
    /// Resolves non-element attribute receivers to their element parent and reports precise failures.
    pub(super) fn add_checked(&self, name: &[u8], value: Option<&[u8]>, namespace: Option<&[u8]>, attribute: bool) -> Result<Node, AddError> {
        self.add_with_name_mode(name, value, namespace, attribute, false)
    }
    /// Adds property/dimension names without interpreting their colon as a namespace prefix.
    fn add_literal(&self, name: &[u8], value: Option<&[u8]>, attribute: bool) -> Option<Node> {
        self.add_with_name_mode(name, value, None, attribute, true).ok()
    }
    /// Shares receiver validation while preserving the caller's literal or qualified name rules.
    fn add_with_name_mode(&self, name: &[u8], value: Option<&[u8]>, namespace: Option<&[u8]>, attribute: bool, literal: bool) -> Result<Node, AddError> {
        let missing = if attribute { AddError::MissingAttributeParent } else { AddError::MissingChildParent };
        if !attribute && matches!(self.scope, Scope::Attributes(..)) { return Err(AddError::ElementToAttributes); }
        let mut parent = self.first().ok_or(missing)?;
        if attribute && unsafe { ffi::node_type(parent) } != 1 {
            parent = unsafe { ffi::parent(parent) };
        }
        if parent.is_null() || unsafe { ffi::node_type(parent) } != 1 { return Err(missing); }
        if name.is_empty() { return Err(AddError::InvalidName); }
        if attribute {
            let uri = namespace.unwrap_or(b"");
            let qualified = name.iter().position(|byte| *byte == b':').map(|index| (&name[..index], &name[index + 1..])).filter(|(prefix, local)| !prefix.is_empty() && !local.is_empty());
            if !uri.is_empty() && qualified.is_none() { return Err(AddError::AttributePrefix); }
            let local = if literal { name } else { qualified.map(|(_, local)| local).unwrap_or(name) };
            let mut current = unsafe { ffi::attrs(parent) };
            while !current.is_null() {
                let current_uri = unsafe { ffi::ns(current, 0) };
                let same_namespace = match namespace {
                    None => current_uri.is_null(),
                    Some(uri) => !current_uri.is_null() && unsafe { bytes(current_uri).as_slice() == uri },
                };
                if same_namespace && unsafe { bytes(ffi::name(current)).as_slice() == local } {
                    return Err(AddError::DuplicateAttribute);
                }
                current = unsafe { ffi::next(current) };
            }
        }
        let name = cstring(name);
        let text = cstring(value.unwrap_or(b""));
        let uri = cstring(namespace.unwrap_or(b""));
        let raw = unsafe {
            if literal {
                ffi::add_literal(parent, name.as_ptr(), text.as_ptr(), i32::from(attribute), i32::from(value.is_some()))
            } else {
                ffi::add(parent, name.as_ptr(), text.as_ptr(), uri.as_ptr(),
                    i32::from(namespace.is_some()), i32::from(attribute), i32::from(value.is_some()))
            }
        };
        if raw.is_null() { Err(missing) } else { Ok(raw) }
    }
    /// Replaces direct content without freeing child wrappers that remain in user storage.
    pub(super) fn assign_node(&self, node: Node, value: impl AsRef<[u8]>) {
        self.assign_node_content(node, value, false);
    }
    /// Gives DOM nodeValue its entity-aware element/attribute rules without releasing live children.
    pub(super) fn assign_dom_node_value(&self, node: Node, value: impl AsRef<[u8]>) {
        self.assign_node_content(node, value, true);
    }
    /// Retires exposed child storage before either literal or entity-parsing replacement.
    fn assign_node_content(&self, node: Node, value: impl AsRef<[u8]>, entities: bool) {
        unsafe {
            if !matches!(ffi::node_type(node), 1 | 2) {
                ffi::content(node, cstring(value).as_ptr());
                return;
            }
            let mut cursor = ffi::child(node);
            while !cursor.is_null() {
                let next = ffi::next(cursor);
                self.detach(cursor);
                cursor = next;
            }
            let text = cstring(value);
            if entities { ffi::content(node, text.as_ptr()); }
            else { ffi::assign(node, text.as_ptr()); }
        }
    }
    /// Writes a named property/attribute or numeric offset, returning a PHP status code.
    pub(super) fn write(&self, kind: i64, key: impl AsRef<[u8]>, index: i64, value: impl AsRef<[u8]>) -> i64 {
        let key = key.as_ref();
        let value = value.as_ref();
        if kind == 4 && index < 0 { return 0; }
        if matches!(kind, 4 | 5) && matches!(self.scope, Scope::Children(_, Some(_)))
            && self.first().is_none() { return 0; }
        if kind == 5 {
            if matches!(self.scope, Scope::Node(_) | Scope::Index(..)) {
                return self.first().filter(|node| unsafe { ffi::node_type(*node) } == 1)
                    .map(|node| { self.assign_node(node, value); 1 }).unwrap_or(0);
            }
            if let Scope::Children(parent, Some(name)) = &self.scope {
                return parent.add_literal(name, None, false).map(|node| { self.assign_node(node, value); 1 }).unwrap_or(0);
            }
            return 0;
        }
        let target = self.select(kind, key, "", false, index.max(0) as usize);
        let nodes = target.nodes();
        if nodes.len() > 1 { return -2; }
        if let Some(node) = nodes.first() { self.assign_node(*node, value); return 1; }
        if kind == 3 || (kind == 2 && matches!(self.scope, Scope::Attributes(..))) {
            let owner = if matches!(self.scope, Scope::Attributes(..)) { parent_for_attributes(self) } else { Rc::new(self.clone()) };
            return i64::from(owner.ensure_first().and_then(|node| {
                owner.with_scope(Scope::Node(node)).add_literal(key, Some(value), true)
            }).is_some());
        }
        if kind == 2 {
            if let Some(node) = target.ensure_first() { target.assign_node(node, value); return 1; }
        }
        if kind == 4 || kind == 5 {
            if let Scope::Children(parent, Some(name)) = &self.scope {
                if let Some(node) = parent.add_literal(name, None, false) { self.assign_node(node, value); return 1; }
            }
        }
        0
    }
    /// Removes selected nodes while preserving all borrowed wrapper storage.
    pub(super) fn remove(&self, kind: i64, key: impl AsRef<[u8]>, index: i64) {
        if kind == 4 && index < 0 { return; }
        for node in self.select(kind, key, "", false, index.max(0) as usize).nodes() { self.detach(node); }
    }
}
