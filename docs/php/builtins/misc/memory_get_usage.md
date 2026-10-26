---
title: "memory_get_usage()"
description: "Returns live native heap bytes, including block headers and allocator rounding. With real_usage=true returns the fixed heap arena capacity, including unused space. Excludes concat scratch, libc, and Rust bridge allocations."
sidebar:
  order: 641
---

## memory_get_usage()

```php
function memory_get_usage(bool $real_usage = false): int
```

Returns live native heap bytes, including block headers and allocator rounding. With real_usage=true returns the fixed heap arena capacity, including unused space. Excludes concat scratch, libc, and Rust bridge allocations.

**Parameters**:
- `$real_usage` (`bool`), default `false`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/memory_get_usage.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/memory_get_usage.rs)).

**Examples**:

```php
$before = memory_get_usage();
$values = array_fill(0, 10000, 1);
echo memory_get_usage() - $before;
```

## Internals

For how `memory_get_usage` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/memory_get_usage.md).
