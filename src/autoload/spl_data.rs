//! SPL data-structure classes synthesised at compile time.
//!
//! The four classes shipped here — `SplStack`, `SplQueue`,
//! `SplDoublyLinkedList`, `SplFixedArray` — are written as PHP source
//! strings, lexed/parsed into top-level `ClassDecl` statements, and
//! appended to the program in `Registry::build`. After that they go
//! through the regular resolver / name resolver / type checker /
//! codegen pipeline like any user-declared class.
//!
//! Each class is implemented in pure userland PHP backed by an
//! `array $items` private property. This is Strategy A from the SPL
//! plan: simpler than the IntrinsicCall / runtime-helper approach,
//! at the cost of `array_unshift` / `array_shift` being O(n) for
//! `SplDoublyLinkedList::shift` / `SplDoublyLinkedList::unshift`.
//! Phase 6's heap-backed structures will move to Strategy B.
//!
//! Mixed values: `$items` holds `mixed`, so users can push heterogeneous
//! types. Most operations (push, count, current, key, next, …) work
//! cleanly. Echoing a popped value back to stdout works for ints,
//! strings, bools, floats, and null. Object values round-trip
//! correctly only when the user has the static type at hand
//! (`$x = $stack->pop(); $x->method()` requires the user to cast or
//! check via instanceof first).

use crate::parser::ast::{Stmt, StmtKind};

const SPL_STACK_SOURCE: &str = "<?php
class SplStack implements Iterator, Countable {
    private array $items = [];
    private int $_pos = 0;

    public function push(mixed $value): void { $this->items[] = $value; }
    public function pop(): mixed {
        $idx = count($this->items) - 1;
        $val = $this->items[$idx];
        $this->items = array_slice($this->items, 0, $idx);
        return $val;
    }
    public function top(): mixed { return $this->items[count($this->items) - 1]; }
    public function count(): int { return count($this->items); }
    public function isEmpty(): bool { return count($this->items) === 0; }

    public function rewind(): void { $this->_pos = count($this->items) - 1; }
    public function valid(): bool { return $this->_pos >= 0; }
    public function current(): mixed { return $this->items[$this->_pos]; }
    public function key(): mixed { return $this->_pos; }
    public function next(): void { $this->_pos = $this->_pos - 1; }
}
";

const SPL_QUEUE_SOURCE: &str = "<?php
class SplQueue implements Iterator, Countable {
    private array $items = [];
    private int $_pos = 0;

    public function enqueue(mixed $value): void { $this->items[] = $value; }
    public function dequeue(): mixed {
        $val = $this->items[0];
        $this->items = array_slice($this->items, 1);
        return $val;
    }
    public function count(): int { return count($this->items); }
    public function isEmpty(): bool { return count($this->items) === 0; }

    public function rewind(): void { $this->_pos = 0; }
    public function valid(): bool { return $this->_pos < count($this->items); }
    public function current(): mixed { return $this->items[$this->_pos]; }
    public function key(): mixed { return $this->_pos; }
    public function next(): void { $this->_pos = $this->_pos + 1; }
}
";

const SPL_DOUBLY_LINKED_LIST_SOURCE: &str = "<?php
class SplDoublyLinkedList implements Iterator, Countable {
    private array $items = [];
    private int $_pos = 0;

    public function push(mixed $value): void { $this->items[] = $value; }
    public function pop(): mixed {
        $idx = count($this->items) - 1;
        $val = $this->items[$idx];
        $this->items = array_slice($this->items, 0, $idx);
        return $val;
    }
    public function unshift(mixed $value): void {
        $head = [$value];
        $this->items = array_merge($head, $this->items);
    }
    public function shift(): mixed {
        $val = $this->items[0];
        $this->items = array_slice($this->items, 1);
        return $val;
    }
    public function top(): mixed { return $this->items[count($this->items) - 1]; }
    public function bottom(): mixed { return $this->items[0]; }
    public function count(): int { return count($this->items); }
    public function isEmpty(): bool { return count($this->items) === 0; }

    public function rewind(): void { $this->_pos = 0; }
    public function valid(): bool { return $this->_pos >= 0 && $this->_pos < count($this->items); }
    public function current(): mixed { return $this->items[$this->_pos]; }
    public function key(): mixed { return $this->_pos; }
    public function next(): void { $this->_pos = $this->_pos + 1; }
    public function prev(): void { $this->_pos = $this->_pos - 1; }
}
";

const SPL_FIXED_ARRAY_SOURCE: &str = "<?php
class SplFixedArray implements Iterator, Countable {
    private array $items = [];
    private int $size = 0;
    private int $_pos = 0;

    public function __construct(int $size = 0) {
        $this->size = $size;
        $__fa_i = 0;
        while ($__fa_i < $size) {
            $this->items[] = null;
            $__fa_i = $__fa_i + 1;
        }
    }

    public function getSize(): int { return $this->size; }
    public function count(): int { return $this->size; }

    public function offsetExists(int $offset): bool { return $offset >= 0 && $offset < $this->size; }
    public function offsetGet(int $offset): mixed {
        if ($offset < 0 || $offset >= $this->size) {
            throw new RuntimeException(\"SplFixedArray index out of range\");
        }
        return $this->items[$offset];
    }
    public function offsetSet(int $offset, mixed $value): void {
        if ($offset < 0 || $offset >= $this->size) {
            throw new RuntimeException(\"SplFixedArray index out of range\");
        }
        $this->items[$offset] = $value;
    }
    public function offsetUnset(int $offset): void {
        if ($offset >= 0 && $offset < $this->size) {
            $this->items[$offset] = null;
        }
    }

    public function rewind(): void { $this->_pos = 0; }
    public function valid(): bool { return $this->_pos < $this->size; }
    public function current(): mixed { return $this->items[$this->_pos]; }
    public function key(): mixed { return $this->_pos; }
    public function next(): void { $this->_pos = $this->_pos + 1; }
}
";

/// Build the four SPL data-structure ClassDecl statements. They go
/// through the regular pipeline as if the user had declared them.
///
/// Each class is wrapped in a `NamespaceBlock { name: None, ... }` so
/// the synthesised classes always live in the root namespace, even
/// when the user's program has its own `namespace App;` declaration
/// preceding our injection point.
pub fn synthesised_class_decls() -> Vec<Stmt> {
    let mut out = Vec::new();
    for source in [
        SPL_STACK_SOURCE,
        SPL_QUEUE_SOURCE,
        SPL_DOUBLY_LINKED_LIST_SOURCE,
        SPL_FIXED_ARRAY_SOURCE,
    ] {
        let body = parse_synthetic_top_level(source);
        if !body.is_empty() {
            let span = body[0].span;
            out.push(Stmt::new(
                StmtKind::NamespaceBlock { name: None, body },
                span,
            ));
        }
    }
    out
}

/// Lex + parse a `<?php class Foo { ... }`-style source string and return
/// the resulting top-level statements. Any failure is treated as a
/// programming error in the synthesiser, since these strings are
/// compiler-controlled rather than user input.
fn parse_synthetic_top_level(source: &str) -> Vec<Stmt> {
    let tokens = crate::lexer::tokenize(source).expect("synthetic SPL source must lex");
    let parsed = crate::parser::parse(&tokens).expect("synthetic SPL source must parse");
    parsed
        .into_iter()
        .filter(|stmt| {
            matches!(
                stmt.kind,
                StmtKind::ClassDecl { .. }
                    | StmtKind::InterfaceDecl { .. }
                    | StmtKind::TraitDecl { .. }
                    | StmtKind::NamespaceBlock { .. }
            )
        })
        .collect()
}
