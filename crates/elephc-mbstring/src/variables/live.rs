//! Purpose:
//! Selects one source encoding and converts live PHP variables through a host adapter.
//!
//! Called from:
//! - Native and eval mb_convert_variables reference adapters.
//!
//! Key details:
//! - Both passes visit values, including object properties and nested references, but not keys.
//! - The conversion pass writes in traversal order and can leave earlier writes visible on failure.
//! - A host separates arrays before writes and applies PHP's distinct root and nested reference rules.

use std::collections::HashSet;
use std::hash::Hash;

use crate::detect;
use crate::encoding::{self, Encoding, Substitute};

/// A container identity used for recursion detection during one traversal path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Container<Identity> {
    Array(Identity),
    Object(Identity),
}

/// A dereferenced value in the host's live PHP storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveValue<Identity> {
    String(Vec<u8>),
    Container(Container<Identity>),
    Other,
}

/// Host actions needed by the two PHP mb_convert_variables traversal phases.
pub trait LiveHost {
    type Handle: Clone;
    type Identity: Copy + Eq + Hash;
    type Error;

    /// Reads one value after following any reference cell, without changing ownership.
    fn inspect(&mut self, handle: &Self::Handle) -> Result<LiveValue<Self::Identity>, Self::Error>;

    /// Returns the next value slot in array insertion or object property order.
    fn child(&mut self, container: Container<Self::Identity>, cursor: &mut usize)
        -> Result<Option<Self::Handle>, Self::Error>;

    /// Prepares an array for COW mutation and returns its current identity.
    /// Objects and reference cells retain their original identity.
    fn prepare_write(&mut self, handle: &Self::Handle, container: Container<Self::Identity>)
        -> Result<Container<Self::Identity>, Self::Error>;

    /// Replaces a string in the original slot. Root arguments are dereferenced first;
    /// nested reference slots are replaced instead of writing through their references.
    fn write_string(&mut self, handle: &Self::Handle, bytes: Vec<u8>) -> Result<(), Self::Error>;
}

/// A PHP warning condition or an exception/fatal returned by the host.
#[derive(Debug, PartialEq, Eq)]
pub enum LiveFailure<Error> {
    Recursive,
    Undetectable,
    Host(Error),
}

/// Converts all root variables with one detected source and returns its canonical name.
/// Earlier writes remain visible if a later host operation or recursive value fails.
pub fn convert_live<H: LiveHost>(
    host: &mut H,
    roots: &[H::Handle],
    to: Encoding,
    candidates: &[Encoding],
    strict: bool,
    order_significant: bool,
    substitute: Substitute,
) -> (Result<Encoding, LiveFailure<H::Error>>, u64) {
    let source = match candidates {
        [] => return (Err(LiveFailure::Undetectable), 0),
        [only] => *only,
        many => {
            let strings = match collect_strings(host, roots) {
                Ok(strings) => strings,
                Err(error) => return (Err(error), 0),
            };
            let borrowed = strings.iter().map(Vec::as_slice).collect::<Vec<_>>();
            match detect::guess_many(&borrowed, many, strict, order_significant) {
                Some(source) => source,
                None => return (Err(LiveFailure::Undetectable), 0),
            }
        },
    };
    encoding::errors::measure(|| (|| {
        for root in roots {
            walk_convert(host, root.clone(), source, to, substitute)?;
        }
        Ok(source)
    })())
}

/// Collects owned copies of every string across the whole argument list.
fn collect_strings<H: LiveHost>(
    host: &mut H,
    roots: &[H::Handle],
) -> Result<Vec<Vec<u8>>, LiveFailure<H::Error>> {
    let mut strings = Vec::new();
    let mut active = HashSet::new();
    for root in roots {
        let mut pending = vec![Visit::Value(root.clone())];
        while let Some(step) = pending.pop() {
            match step {
                Visit::Value(handle) => match host.inspect(&handle).map_err(LiveFailure::Host)? {
                    LiveValue::String(bytes) => strings.push(bytes),
                    LiveValue::Container(container) => {
                        if !active.insert(container) { return Err(LiveFailure::Recursive); }
                        pending.push(Visit::Children(container, 0));
                    },
                    LiveValue::Other => {},
                },
                Visit::Children(container, mut cursor) => {
                    match host.child(container, &mut cursor).map_err(LiveFailure::Host)? {
                        Some(child) => {
                            pending.push(Visit::Children(container, cursor));
                            pending.push(Visit::Value(child));
                        },
                        None => { active.remove(&container); },
                    }
                },
                Visit::WrittenChildren(_, _, _) => unreachable!("source collection never prepares writes"),
            }
        }
    }
    Ok(strings)
}

/// Writes one root in PHP traversal order while rejecting active array or object cycles.
fn walk_convert<H: LiveHost>(
    host: &mut H,
    root: H::Handle,
    from: Encoding,
    to: Encoding,
    substitute: Substitute,
) -> Result<(), LiveFailure<H::Error>> {
    let mut active = HashSet::new();
    let mut pending = vec![Visit::Value(root)];
    while let Some(step) = pending.pop() {
        match step {
            Visit::Value(handle) => match host.inspect(&handle).map_err(LiveFailure::Host)? {
                LiveValue::String(bytes) => {
                    let converted = to.encode_conversion(&bytes, from, substitute);
                    host.write_string(&handle, converted).map_err(LiveFailure::Host)?;
                },
                LiveValue::Container(original) => {
                    if active.contains(&original) { return Err(LiveFailure::Recursive); }
                    let current = host.prepare_write(&handle, original).map_err(LiveFailure::Host)?;
                    if !active.insert(current) { return Err(LiveFailure::Recursive); }
                    if current != original { active.insert(original); }
                    pending.push(Visit::WrittenChildren(current, original, 0));
                },
                LiveValue::Other => {},
            },
            Visit::WrittenChildren(container, original, mut cursor) => {
                match host.child(container, &mut cursor).map_err(LiveFailure::Host)? {
                    Some(child) => {
                        pending.push(Visit::WrittenChildren(container, original, cursor));
                        pending.push(Visit::Value(child));
                    },
                    None => {
                        active.remove(&container);
                        active.remove(&original);
                    },
                }
            },
            Visit::Children(_, _) => unreachable!("conversion only uses written containers"),
        }
    }
    Ok(())
}

/// Tracks value and container positions without keeping a host borrow across callbacks.
enum Visit<Handle, Identity> {
    Value(Handle),
    Children(Container<Identity>, usize),
    WrittenChildren(Container<Identity>, Container<Identity>, usize),
}

#[cfg(test)]
mod tests {
    use super::*;
    use elephc_builtin_contract::mbstring_abi::array::Key;

    impl Container<usize> {
        /// Returns the mock heap node behind either container kind.
        fn identity(self) -> usize {
            match self { Self::Array(id) | Self::Object(id) => id }
        }
    }

    /// An owned slot or a reference to another slot in the test heap.
    #[derive(Clone)]
    enum Cell {
        String(Vec<u8>),
        Container(Container<usize>),
        Reference(usize),
    }

    /// Roots arrive dereferenced, while children retain their containing slot identity.
    #[derive(Clone)]
    enum Handle { Root(usize), Child(usize) }

    impl Handle {
        /// Returns the underlying cell index without resolving a reference.
        fn index(&self) -> usize {
            match self { Self::Root(index) | Self::Child(index) => *index }
        }
    }

    /// A PHP array or object with insertion-ordered value slots and unchanged keys.
    struct Node {
        entries: Vec<(Key, usize)>,
    }

    /// Models stable host identities and write-through reference cells.
    struct Heap {
        cells: Vec<Cell>,
        nodes: Vec<Node>,
        cow_shared: HashSet<usize>,
    }

    impl Heap {
        /// Follows a test reference to its writable value slot.
        fn slot(&self, mut handle: usize) -> usize {
            while let Cell::Reference(next) = self.cells[handle] { handle = next; }
            handle
        }

        /// Reads converted bytes from a named test slot.
        fn bytes(&self, handle: usize) -> &[u8] {
            match &self.cells[self.slot(handle)] {
                Cell::String(bytes) => bytes,
                _ => panic!("expected a string slot"),
            }
        }
    }

    impl LiveHost for Heap {
        type Handle = Handle;
        type Identity = usize;
        type Error = ();

        /// Reads the underlying test value after reference resolution.
        fn inspect(&mut self, handle: &Handle) -> Result<LiveValue<usize>, ()> {
            Ok(match &self.cells[self.slot(handle.index())] {
                Cell::String(bytes) => LiveValue::String(bytes.clone()),
                Cell::Container(container) => LiveValue::Container(*container),
                Cell::Reference(_) => unreachable!("slot() resolves references"),
            })
        }

        /// Returns the next ordered value slot without exposing its key to conversion.
        fn child(&mut self, container: Container<usize>, cursor: &mut usize)
            -> Result<Option<Handle>, ()> {
            let child = self.nodes[container.identity()].entries.get(*cursor)
                .map(|(_, value)| Handle::Child(*value));
            *cursor += usize::from(child.is_some());
            Ok(child)
        }

        /// Separates a marked shared array while preserving reference slots.
        fn prepare_write(&mut self, handle: &Handle, container: Container<usize>)
            -> Result<Container<usize>, ()> {
            let Container::Array(original) = container else { return Ok(container); };
            if !self.cow_shared.remove(&original) { return Ok(container); }
            let entries = self.nodes[original].entries.clone().into_iter().map(|(key, slot)| {
                let copy = self.cells[slot].clone();
                let new_slot = self.cells.len();
                self.cells.push(copy);
                (key, new_slot)
            }).collect();
            let next = self.nodes.len();
            self.nodes.push(Node { entries });
            let slot = self.slot(handle.index());
            self.cells[slot] = Cell::Container(Container::Array(next));
            Ok(Container::Array(next))
        }

        /// Replaces a nested reference slot, while top-level arguments are dereferenced.
        fn write_string(&mut self, handle: &Handle, bytes: Vec<u8>) -> Result<(), ()> {
            let slot = match handle {
                Handle::Root(index) => self.slot(*index),
                Handle::Child(index) => *index,
            };
            self.cells[slot] = Cell::String(bytes);
            Ok(())
        }
    }

    /// Object properties and nested references share detection with array values while keys stay raw.
    #[test]
    fn object_and_reference_values_share_source_without_changing_keys() {
        let mut heap = Heap {
            cells: vec![
                Cell::Container(Container::Array(0)),
                Cell::String(vec![b'C', b'a', b'f', 0xe9]),
                Cell::Reference(1),
                Cell::Container(Container::Object(1)),
                Cell::String(vec![b'C', b'r', 0xe8, b'm', b'e']),
                Cell::Reference(4),
            ],
            nodes: vec![
                Node { entries: vec![(Key::String(vec![0xe9]), 2), (Key::Int(1), 3)] },
                Node { entries: vec![(Key::String(b"private\0name".to_vec()), 5)] },
            ],
            cow_shared: HashSet::new(),
        };
        let utf8 = Encoding::lookup(b"UTF-8").unwrap();
        let latin1 = Encoding::lookup(b"ISO-8859-1").unwrap();
        let (result, illegal) = convert_live(&mut heap, &[Handle::Root(0)], utf8, &[utf8, latin1], true, true,
            Substitute::default());
        assert_eq!(result, Ok(latin1));
        assert_eq!(illegal, 0);
        assert_eq!(heap.bytes(1), b"Caf\xe9");
        assert_eq!(heap.bytes(2), "Café".as_bytes());
        assert_eq!(heap.bytes(4), b"Cr\xe8me");
        assert_eq!(heap.bytes(5), "Crème".as_bytes());
        assert_eq!(heap.nodes[0].entries[0].0, Key::String(vec![0xe9]));
        assert_eq!(heap.nodes[1].entries[0].0, Key::String(b"private\0name".to_vec()));
    }

    /// A recursive object leaves earlier string writes observable before reporting failure.
    #[test]
    fn recursive_object_fails_after_prior_live_writes() {
        let mut heap = Heap {
            cells: vec![
                Cell::Container(Container::Object(0)),
                Cell::String(vec![0xe9]),
                Cell::Reference(0),
            ],
            nodes: vec![Node { entries: vec![(Key::Int(0), 1), (Key::Int(1), 2)] }],
            cow_shared: HashSet::new(),
        };
        let utf8 = Encoding::lookup(b"UTF-8").unwrap();
        let latin1 = Encoding::lookup(b"ISO-8859-1").unwrap();
        let (result, _) = convert_live(&mut heap, &[Handle::Root(0)], utf8, &[latin1], false, true,
            Substitute::default());
        assert_eq!(result, Err(LiveFailure::Recursive));
        assert_eq!(heap.bytes(1), "é".as_bytes());
    }

    /// Repeated roots write through one reference twice, as PHP does for repeated arguments.
    #[test]
    fn repeated_reference_is_converted_at_each_argument_position() {
        let mut heap = Heap {
            cells: vec![Cell::String(vec![0xe9]), Cell::Reference(0)],
            nodes: vec![],
            cow_shared: HashSet::new(),
        };
        let utf8 = Encoding::lookup(b"UTF-8").unwrap();
        let latin1 = Encoding::lookup(b"ISO-8859-1").unwrap();
        let (result, _) = convert_live(&mut heap, &[Handle::Root(1), Handle::Root(1)], utf8, &[latin1], false, true,
            Substitute::default());
        assert_eq!(result, Ok(latin1));
        assert_eq!(heap.bytes(0), "Ã©".as_bytes());
    }

    /// A by-value array alias retains its original bytes after the writable argument separates.
    #[test]
    fn array_write_separates_copy_on_write_alias() {
        let mut heap = Heap {
            cells: vec![
                Cell::Container(Container::Array(0)),
                Cell::Container(Container::Array(0)),
                Cell::String(vec![0xe9]),
            ],
            nodes: vec![Node { entries: vec![(Key::String(b"key".to_vec()), 2)] }],
            cow_shared: HashSet::from([0]),
        };
        let utf8 = Encoding::lookup(b"UTF-8").unwrap();
        let latin1 = Encoding::lookup(b"ISO-8859-1").unwrap();
        let (result, _) = convert_live(&mut heap, &[Handle::Root(0)], utf8, &[latin1], false, true,
            Substitute::default());
        assert_eq!(result, Ok(latin1));
        let Cell::Container(Container::Array(converted)) = heap.cells[0] else { panic!("new array"); };
        let Cell::Container(Container::Array(original)) = heap.cells[1] else { panic!("old array"); };
        assert_ne!(converted, original);
        assert_eq!(heap.bytes(heap.nodes[converted].entries[0].1), "é".as_bytes());
        assert_eq!(heap.bytes(heap.nodes[original].entries[0].1), &[0xe9]);
    }

    /// A referenced nested array keeps its live reference after its string element changes.
    #[test]
    fn nested_array_reference_keeps_its_target() {
        let mut heap = Heap {
            cells: vec![
                Cell::Container(Container::Array(0)),
                Cell::Reference(2),
                Cell::Container(Container::Array(1)),
                Cell::String(vec![0xe9]),
            ],
            nodes: vec![
                Node { entries: vec![(Key::Int(0), 1)] },
                Node { entries: vec![(Key::String(b"value".to_vec()), 3)] },
            ],
            cow_shared: HashSet::new(),
        };
        let utf8 = Encoding::lookup(b"UTF-8").unwrap();
        let latin1 = Encoding::lookup(b"ISO-8859-1").unwrap();
        let (result, _) = convert_live(&mut heap, &[Handle::Root(0)], utf8, &[latin1], false, true,
            Substitute::default());
        assert_eq!(result, Ok(latin1));
        assert!(matches!(heap.cells[1], Cell::Reference(2)));
        assert_eq!(heap.bytes(3), "é".as_bytes());
    }

    /// Two entries pointing at one object traverse that object twice, including its new bytes.
    #[test]
    fn repeated_object_handle_converts_twice() {
        let mut heap = Heap {
            cells: vec![
                Cell::Container(Container::Array(0)),
                Cell::Container(Container::Object(1)),
                Cell::String(vec![0xe9]),
            ],
            nodes: vec![
                Node { entries: vec![(Key::Int(0), 1), (Key::Int(1), 1)] },
                Node { entries: vec![(Key::String(b"value".to_vec()), 2)] },
            ],
            cow_shared: HashSet::new(),
        };
        let utf8 = Encoding::lookup(b"UTF-8").unwrap();
        let latin1 = Encoding::lookup(b"ISO-8859-1").unwrap();
        let (result, _) = convert_live(&mut heap, &[Handle::Root(0)], utf8, &[latin1], false, true,
            Substitute::default());
        assert_eq!(result, Ok(latin1));
        assert_eq!(heap.bytes(2), "Ã©".as_bytes());
    }
}
