---
title: "parse_str()"
description: "Parses a URL-encoded query string into variables."
sidebar:
  order: 926
---

## parse_str()

```php
function parse_str(string $string, mixed $result): void
```

Parses a URL-encoded query string into variables.

**Parameters**:
- `$string` (`string`)
- `$result` (`mixed`), passed by reference

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected parse_str prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `parse_str` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/parse_str.md).
