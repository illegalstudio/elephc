---
title: "parse_str()"
description: "Parses a query string into an array of variables."
sidebar:
  order: 1019
---

## parse_str()

```php
function parse_str(string $string, mixed $result): void
```

Parses a query string into an array of variables.

**Parameters**:
- `$string` (`string`)
- `$result` (`mixed`), passed by reference

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected parse_str prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/parse_str.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/parse_str.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `parse_str` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/parse_str.md).
