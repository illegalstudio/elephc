---
title: "simplexml_load_string()"
description: "Parses XML bytes into a SimpleXML element."
sidebar:
  order: 1013
---

## simplexml_load_string()

```php
function simplexml_load_string(string $data, ?string $class_name = 'SimpleXMLElement', int $options = 0, string $namespace_or_prefix = '', bool $is_prefix = false): SimpleXMLElement|false
```

Parses XML bytes into a SimpleXML element.

**Parameters**:
- `$data` (`string`)
- `$class_name` (`?string`), default `'SimpleXMLElement'`, optional
- `$options` (`int`), default `0`, optional
- `$namespace_or_prefix` (`string`), default `''`, optional
- `$is_prefix` (`bool`), default `false`, optional

**Returns**: `SimpleXMLElement|false`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/simplexml_load_string.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/simplexml_load_string.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `simplexml_load_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/simplexml_load_string.md).
