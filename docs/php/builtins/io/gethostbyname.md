---
title: "gethostbyname()"
description: "Gets the IPv4 address corresponding to the given Internet host name."
sidebar:
  order: 182
---

## gethostbyname()

```php
function gethostbyname(string $hostname): string
```

Gets the IPv4 address corresponding to the given Internet host name.

**Parameters**:
- `$hostname` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/gethostbyname.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/gethostbyname.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gethostbyname` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gethostbyname.md).

