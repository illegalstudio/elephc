---
title: "parse_url()"
description: "Parses a URL and returns its components."
sidebar:
  order: 443
---

## parse_url()

```php
function parse_url(string $url, int $component = -1): mixed
```

Parses a URL and returns its components.

**Parameters**:
- `$url` (`string`)
- `$component` (`int`), default `-1`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/parse_url.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/parse_url.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `parse_url` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/parse_url.md).
