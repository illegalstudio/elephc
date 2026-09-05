"""Data model for the builtins registry.

A *registry* is the single source of truth for everything we render into
docs/php/builtins/*.md and docs/internals/builtins/*.md. It is generated
from the Elephc source tree by `extract.py` and consumed by `render.py`.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple


# Areas used to group builtins in the sidebar. Order matters: this is the
# order they appear in the generated navigation.
AREAS: list[str] = [
    "String",
    "Array",
    "Math",
    "Type",
    "JSON",
    "Regex",
    "Date",
    "Hash",
    "IO",
    "Filesystem",
    "Streams",
    "Network",
    "Process",
    "SPL",
    "Class",
    "Pointer",
    "Buffer",
    "Misc",
    "Database",
    "Web",
    "Image",
]


# Default documentation areas derived from the backend-neutral registry Area.
REGISTRY_AREA_DEFAULTS: Dict[str, Tuple[str, str]] = {
    "string": ("String", "String"),
    "array": ("Array", "Array"),
    "math": ("Math", "Math"),
    "io": ("IO", "IO"),
    "system": ("Misc", "System"),
    "types": ("Type", "Type"),
    "callables": ("Misc", "Callables"),
    "spl": ("SPL", "SPL"),
    "pointers": ("Pointer", "Pointer"),
    # `Area::Curl` covers BOTH halves of ext/curl, and they render differently:
    #
    #  - the forty-three `internal: true` `__elephc_curl_*` entry points, which the
    #    elephc-PHP wrappers in `src/curl_prelude.rs` call — these contribute only to
    #    docs/internals/builtins, never to a user-facing builtin page;
    #  - the thirty-four PHP-visible `curl_*` contracts, which are
    #    `BuiltinKind::PreludeProvided`: `aot.kind == "prelude"` (implemented by the
    #    injected curl prelude, so NOT eval-only) and `eval.kind == "registry"`
    #    (Magician's own `eval_builtin!` homes). They render exactly like the four
    #    `hash_*` prelude contracts do.
    #
    # Both share the "Network" docs area with the eval interpreter's `network_env`
    # family rather than opening a page of its own; the narrative contract (options,
    # constants, TLS/CA behaviour) stays in the hand-written docs/php/curl.md.
    #
    # The PHP-visible half is published by `elephc-builtin-contract`'s `curl` feature
    # and bound by Magician's, so THE DOCS ARE GENERATED WITH THE ROOT `curl` FEATURE
    # ON — that is the single canonical configuration for the committed registry, the
    # generated pages and the CI drift gate alike. `extract.run_gen_builtins` refuses
    # to run against a default-feature exporter for exactly this reason.
    "curl": ("Network", "Network"),
    # Prelude-provided and name-resolver-rewritten surfaces (contracts seeded from the
    # built prelude declarations and PHP's own date/calendar signatures).
    "date": ("Date", "Date"),
    "calendar": ("Date", "Calendar"),
    "mysqli": ("Database", "mysqli"),
    "pdo": ("Database", "PDO"),
    "web": ("Web", "Web"),
    "image": ("Image", "Image"),
    "opcache": ("Misc", "OPcache"),
}


# Per-builtin area overrides for names that have no dedicated dispatch arm in
# builtins.rs. Use this when the function name itself carries strong area
# semantics (e.g. every `sin*`/`cos*`/`tan*` is Math).
AREA_BY_NAME: Dict[str, Tuple[str, str]] = {
    'acos': ('Math', 'Math'),
    'asin': ('Math', 'Math'),
    'atan': ('Math', 'Math'),
    'basename': ('Filesystem', 'Filesystem'),
    'boolval': ('Type', 'Casts'),
    'chdir': ('Filesystem', 'Filesystem'),
    'checkdate': ('Date', 'Date'),
    'chgrp': ('Filesystem', 'Filesystem'),
    'chmod': ('Filesystem', 'Filesystem'),
    'chop': ('String', 'String'),
    'chown': ('Filesystem', 'Filesystem'),
    'chr': ('String', 'String'),
    'class_alias': ('Class', 'Class'),
    'class_exists': ('Class', 'Class'),
    'class_implements': ('Class', 'Class'),
    'class_parents': ('Class', 'Class'),
    'class_uses': ('Class', 'Class'),
    'clearstatcache': ('Filesystem', 'Filesystem'),
    'constant': ('Misc', 'Constants'),
    'copy': ('Filesystem', 'Filesystem'),
    'cos': ('Math', 'Math'),
    'cosh': ('Math', 'Math'),
    'count': ('Array', 'Array'),
    'crc32': ('String', 'String'),
    'date': ('Date', 'Date'),
    'date_default_timezone_get': ('Date', 'Date'),
    'date_default_timezone_set': ('Date', 'Date'),
    'define': ('Misc', 'Constants'),
    'defined': ('Misc', 'Constants'),
    'die': ('Process', 'Process'),
    'dirname': ('Filesystem', 'Filesystem'),
    'disk_free_space': ('Filesystem', 'Filesystem'),
    'disk_total_space': ('Filesystem', 'Filesystem'),
    'empty': ('Misc', 'Variable'),
    'enum_exists': ('Class', 'Class'),
    'exec': ('Process', 'Process'),
    'exit': ('Process', 'Process'),
    'exp': ('Math', 'Math'),
    'file_exists': ('Filesystem', 'Filesystem'),
    'fileatime': ('Filesystem', 'Filesystem'),
    'filectime': ('Filesystem', 'Filesystem'),
    'filegroup': ('Filesystem', 'Filesystem'),
    'fileinode': ('Filesystem', 'Filesystem'),
    'filemtime': ('Filesystem', 'Filesystem'),
    'fileowner': ('Filesystem', 'Filesystem'),
    'fileperms': ('Filesystem', 'Filesystem'),
    'filesize': ('Filesystem', 'Filesystem'),
    'filetype': ('Filesystem', 'Filesystem'),
    'floatval': ('Type', 'Casts'),
    'fnmatch': ('Filesystem', 'Filesystem'),
    'fsockopen': ('Streams', 'Streams'),
    'function_exists': ('Class', 'Class'),
    'get_class': ('Class', 'Class'),
    'get_declared_classes': ('Class', 'Class'),
    'get_declared_interfaces': ('Class', 'Class'),
    'get_declared_traits': ('Class', 'Class'),
    'get_parent_class': ('Class', 'Class'),
    'getcwd': ('Filesystem', 'Filesystem'),
    'getdate': ('Date', 'Date'),
    'getenv': ('Filesystem', 'Filesystem'),
    'gettype': ('Type', 'Type'),
    'glob': ('Filesystem', 'Filesystem'),
    'gmdate': ('Date', 'Date'),
    'gmmktime': ('Date', 'Date'),
    'hrtime': ('Date', 'Date'),
    'interface_exists': ('Class', 'Class'),
    'intval': ('Type', 'Casts'),
    'is_a': ('Class', 'Class'),
    'is_array': ('Type', 'Type'),
    'is_bool': ('Type', 'Type'),
    'is_callable': ('Type', 'Type'),
    'is_dir': ('Filesystem', 'Filesystem'),
    'is_executable': ('Filesystem', 'Filesystem'),
    'is_file': ('Filesystem', 'Filesystem'),
    'is_float': ('Type', 'Type'),
    'is_int': ('Type', 'Type'),
    'is_integer': ('Type', 'Type'),
    'is_iterable': ('Type', 'Type'),
    'is_link': ('Filesystem', 'Filesystem'),
    'is_long': ('Type', 'Type'),
    'is_null': ('Type', 'Type'),
    'is_numeric': ('Type', 'Type'),
    'is_object': ('Type', 'Type'),
    'is_readable': ('Filesystem', 'Filesystem'),
    'is_scalar': ('Type', 'Type'),
    'is_string': ('Type', 'Type'),
    'is_subclass_of': ('Class', 'Class'),
    'is_writable': ('Filesystem', 'Filesystem'),
    'is_writeable': ('Filesystem', 'Filesystem'),
    'isset': ('Misc', 'Variable'),
    'lcfirst': ('String', 'String'),
    'lchgrp': ('Filesystem', 'Filesystem'),
    'lchown': ('Filesystem', 'Filesystem'),
    'link': ('Filesystem', 'Filesystem'),
    'linkinfo': ('Filesystem', 'Filesystem'),
    'localtime': ('Date', 'Date'),
    'log10': ('Math', 'Math'),
    'log2': ('Math', 'Math'),
    'lstat': ('Filesystem', 'Filesystem'),
    'ltrim': ('String', 'String'),
    'md5': ('String', 'String'),
    'microtime': ('Date', 'Date'),
    'mkdir': ('Filesystem', 'Filesystem'),
    'mktime': ('Date', 'Date'),
    'mt_rand': ('Math', 'Math'),
    'ord': ('String', 'String'),
    'passthru': ('Process', 'Process'),
    'pathinfo': ('Filesystem', 'Filesystem'),
    'pclose': ('Process', 'Process'),
    'pfsockopen': ('Streams', 'Streams'),
    'php_uname': ('Misc', 'Info'),
    'phpversion': ('Misc', 'Info'),
    'pi': ('Math', 'Math'),
    'popen': ('Process', 'Process'),
    'print_r': ('Misc', 'Variable'),
    'putenv': ('Filesystem', 'Filesystem'),
    'rand': ('Math', 'Math'),
    'readfile': ('Filesystem', 'Filesystem'),
    'readline': ('Process', 'Process'),
    'readlink': ('Filesystem', 'Filesystem'),
    'realpath': ('Filesystem', 'Filesystem'),
    'realpath_cache_get': ('Filesystem', 'Filesystem'),
    'realpath_cache_size': ('Filesystem', 'Filesystem'),
    'rename': ('Filesystem', 'Filesystem'),
    'rmdir': ('Filesystem', 'Filesystem'),
    'rtrim': ('String', 'String'),
    'scandir': ('Filesystem', 'Filesystem'),
    'settype': ('Type', 'Casts'),
    'sha1': ('String', 'String'),
    'shell_exec': ('Process', 'Process'),
    'sin': ('Math', 'Math'),
    'sinh': ('Math', 'Math'),
    'sleep': ('Process', 'Process'),
    'stat': ('Filesystem', 'Filesystem'),
    'str_contains': ('String', 'String'),
    'str_ends_with': ('String', 'String'),
    'str_ireplace': ('String', 'String'),
    'str_pad': ('String', 'String'),
    'str_repeat': ('String', 'String'),
    'str_split': ('String', 'String'),
    'str_starts_with': ('String', 'String'),
    'strcasecmp': ('String', 'String'),
    'stream_bucket_append': ('Streams', 'Streams'),
    'stream_bucket_prepend': ('Streams', 'Streams'),
    'stream_filter_append': ('Streams', 'Streams'),
    'stream_filter_prepend': ('Streams', 'Streams'),
    'strlen': ('String', 'String'),
    'strrev': ('String', 'String'),
    'strtolower': ('String', 'String'),
    'strtotime': ('Date', 'Date'),
    'strtoupper': ('String', 'String'),
    'strval': ('Type', 'Casts'),
    'symlink': ('Filesystem', 'Filesystem'),
    'sys_get_temp_dir': ('Filesystem', 'Filesystem'),
    'system': ('Process', 'Process'),
    'tan': ('Math', 'Math'),
    'tanh': ('Math', 'Math'),
    'tempnam': ('Filesystem', 'Filesystem'),
    'time': ('Date', 'Date'),
    'tmpfile': ('Filesystem', 'Filesystem'),
    'touch': ('Filesystem', 'Filesystem'),
    'trait_exists': ('Class', 'Class'),
    'trim': ('String', 'String'),
    'ucfirst': ('String', 'String'),
    'ucwords': ('String', 'String'),
    'umask': ('Filesystem', 'Filesystem'),
    'unlink': ('Filesystem', 'Filesystem'),
    'unset': ('Misc', 'Variable'),
    'usleep': ('Process', 'Process'),
    'var_dump': ('Misc', 'Variable'),
}


# Registry areas deliberately stay coarse. These names retain the established
# user-facing documentation category where one registry area spans multiple
# PHP extension families.
REGISTRY_AREA_OVERRIDES: Dict[str, Tuple[str, str]] = {
    "buffer_free": ("Buffer", "Buffer"),
    "buffer_len": ("Buffer", "Buffer"),
    "call_user_func": ("Array", "Array"),
    "call_user_func_array": ("Array", "Array"),
    "class_attribute_args": ("Class", "Attributes"),
    "class_attribute_names": ("Class", "Attributes"),
    "class_get_attributes": ("Class", "Attributes"),
    "ctype_alnum": ("Type", "Ctype"),
    "ctype_alpha": ("Type", "Ctype"),
    "ctype_digit": ("Type", "Ctype"),
    "ctype_space": ("Type", "Ctype"),
    "get_called_class": ("Class", "Class"),
    "get_class_methods": ("Class", "Class"),
    "get_class_vars": ("Class", "Class"),
    "get_object_vars": ("Class", "Class"),
    "is_finite": ("Math", "Math"),
    "is_infinite": ("Math", "Math"),
    "is_nan": ("Math", "Math"),
    "json_decode": ("JSON", "JSON"),
    "json_encode": ("JSON", "JSON"),
    "json_last_error": ("JSON", "JSON"),
    "json_last_error_msg": ("JSON", "JSON"),
    "json_validate": ("JSON", "JSON"),
    "mb_ereg_match": ("Regex", "Regex"),
    "method_exists": ("Class", "Class"),
    "preg_match": ("Regex", "Regex"),
    "preg_match_all": ("Regex", "Regex"),
    "preg_replace": ("Regex", "Regex"),
    "preg_replace_callback": ("Regex", "Regex"),
    "preg_split": ("Regex", "Regex"),
    "property_exists": ("Class", "Class"),
}


# Per-builtin parameter type overrides. The key is the lowercase canonical
# name; the value is a list of types, one per positional parameter, where
# the value `None` means "leave the parsed `mixed` alone".
#
# These are the types the user can rely on. Builtins that accept any value
# stay at `mixed` (so e.g. `strtolower($x)` keeps `mixed $x`, because the
# argument is converted internally regardless of its static type).
# Each entry is either:
#   - a `str` type (e.g. `"string"`) — the param is named `value` (default), or
#   - a 2-tuple `(type, name)` — the param is named explicitly, or
#   - `None` — leave whatever the parser produced alone (skip refinement).
ParamSpec = object  # str | Tuple[str, str] | None — kept loose for legibility
PARAM_TYPES: Dict[str, List[Optional[ParamSpec]]] = {
    '__elephc_gmmktime_raw': ['int', 'int', 'int', 'int', 'int', 'int'],
    '__elephc_mktime_raw': ['int', 'int', 'int', 'int', 'int', 'int'],
    '__elephc_strtotime_raw': ['string', 'int'],
    'abs': ['int'],
    'acos': ['float'],
    'addslashes': ['string'],
    'array_chunk': ['array', 'int', 'bool'],
    'array_column': ['array', 'string'],
    'array_combine': ['array', 'array'],
    'array_count_values': ['array'],
    'array_diff': ['array'],
    'array_diff_assoc': ['array'],
    'array_diff_key': ['array'],
    'array_fill': ['int', 'int', 'mixed'],
    'array_fill_keys': ['array', 'mixed'],
    'array_filter': ['array', 'callable', 'int'],
    'array_flip': ['array'],
    'array_intersect': ['array'],
    'array_intersect_assoc': ['array'],
    'array_intersect_key': ['array'],
    'array_key_exists': ['string', 'array'],
    'array_key_first': ['array'],
    'array_key_last': ['array'],
    'array_keys': ['array'],
    'array_map': ['callable', 'array'],
    'array_multisort': ['array', 'int'],
    'array_pad': ['array', 'int', 'mixed'],
    'array_pop': ['array'],
    'array_product': ['array'],
    'array_push': ['array'],
    'array_rand': ['array'],
    'array_reduce': ['array', 'callable', 'mixed'],
    'array_replace': ['array', 'array'],
    'array_replace_recursive': ['array', 'array'],
    'array_reverse': ['array', 'bool'],
    'array_search': ['mixed', 'array', 'bool'],
    'array_shift': ['array'],
    'array_slice': ['array', 'int', 'int', 'bool'],
    'array_splice': ['array', 'int', 'int', 'array'],
    'array_sum': ['array'],
    'array_udiff': ['array', 'array', 'callable'],
    'array_uintersect': ['array', 'array', 'callable'],
    'array_unique': ['array'],
    'array_unshift': ['array'],
    'array_values': ['array'],
    'array_walk': ['array', 'callable'],
    'array_walk_recursive': ['array', 'callable'],
    'arsort': ['array'],
    'asin': ['float'],
    'asort': ['array'],
    'atan': ['float'],
    'atan2': ['float', 'float'],
    'base64_decode': ['string', 'bool'],
    'base64_encode': ['string'],
    'basename': ['string', 'string'],
    'bin2hex': ['string'],
    'boolval': ['mixed'],
    'buffer_free': ['buffer'],
    'buffer_len': ['buffer'],
    'buffer_new': ['int'],
    'call_user_func': ['callable'],
    'call_user_func_array': ['callable', 'array'],
    'ceil': ['float'],
    'chdir': ['string'],
    'checkdate': ['int', 'int', 'int'],
    'chgrp': ['string', 'mixed'],
    'chmod': ['string', 'int'],
    'chop': ['string', 'string'],
    'chown': ['string', 'mixed'],
    'chr': ['int'],
    'clamp': ['int', 'int', 'int'],
    'class_alias': ['string', 'string', 'bool'],
    'class_exists': ['string', 'bool'],
    'class_implements': [('mixed', 'object_or_class'), 'bool'],
    'class_parents': [('mixed', 'object_or_class'), 'bool'],
    'class_uses': [('mixed', 'object_or_class'), 'bool'],
    'clearstatcache': ['bool', 'string'],
    'closedir': ['resource'],
    'constant': ['string'],
    'copy': ['string', 'string'],
    'cos': ['float'],
    'cosh': ['float'],
    'count': ['array', 'int'],
    'crc32': ['string'],
    # ext/curl. The shared contract types every handle as `Mixed` (the catalog has
    # no object vocabulary); the precise class names below are transcribed from the
    # injected prelude's own PHP declarations in `src/curl_prelude.rs`, which are
    # what compiled code really enforces. Where the prelude deliberately declares
    # `mixed` (`curl_multi_add_handle()`'s `$handle`, every `$value`) it stays mixed.
    'curl_close': ['CurlHandle'],
    'curl_copy_handle': ['CurlHandle'],
    'curl_errno': ['CurlHandle'],
    'curl_error': ['CurlHandle'],
    'curl_escape': ['CurlHandle', 'string'],
    'curl_exec': ['CurlHandle'],
    'curl_getinfo': ['CurlHandle', 'int'],
    'curl_multi_add_handle': ['CurlMultiHandle', 'mixed'],
    'curl_multi_close': ['CurlMultiHandle'],
    'curl_multi_errno': ['CurlMultiHandle'],
    'curl_multi_exec': ['CurlMultiHandle', 'int'],
    'curl_multi_get_handles': ['CurlMultiHandle'],
    'curl_multi_info_read': ['CurlMultiHandle', 'int'],
    'curl_multi_remove_handle': ['CurlMultiHandle', 'mixed'],
    'curl_multi_select': ['CurlMultiHandle', 'float'],
    'curl_multi_setopt': ['CurlMultiHandle', 'int', 'mixed'],
    'curl_pause': ['CurlHandle', 'int'],
    'curl_reset': ['CurlHandle'],
    'curl_setopt': ['CurlHandle', 'int', 'mixed'],
    'curl_setopt_array': ['CurlHandle', 'array'],
    'curl_share_close': ['CurlShareHandle'],
    'curl_share_errno': ['CurlShareHandle'],
    'curl_share_init_persistent': ['array'],
    'curl_share_setopt': ['CurlShareHandle', 'int', 'mixed'],
    'curl_unescape': ['CurlHandle', 'string'],
    'curl_upkeep': ['CurlHandle'],
    'ctype_alnum': ['string'],
    'ctype_alpha': ['string'],
    'ctype_digit': ['string'],
    'ctype_space': ['string'],
    'current': ['array'],
    'date': ['string', 'int'],
    'date_default_timezone_set': ['string'],
    'define': ['string', 'mixed'],
    'defined': ['string'],
    'deg2rad': ['float'],
    'die': ['int'],
    'dirname': ['string', 'int'],
    'disk_free_space': ['string'],
    'disk_total_space': ['string'],
    'empty': ['mixed'],
    'end': ['array'],
    'enum_exists': ['string', 'bool'],
    'exec': ['string'],
    'exit': ['int'],
    'exp': ['float'],
    'explode': ['string', 'string', 'int'],
    'fclose': ['resource'],
    'fdatasync': ['resource'],
    'fdiv': ['float', 'float'],
    'feof': ['resource'],
    'fflush': ['resource'],
    'fgetc': ['resource'],
    'fgetcsv': ['resource', 'int', 'string'],
    'fgets': ['resource'],
    'file': ['string', ('int', 'flags')],
    'file_exists': ['string'],
    'file_get_contents': ['string', ('bool', 'use_include_path'), ('mixed', 'context'), ('int', 'offset'), ('int', 'length')],
    'file_put_contents': ['string', 'mixed'],
    'fileatime': ['string'],
    'filectime': ['string'],
    'filegroup': ['string'],
    'fileinode': ['string'],
    'filemtime': ['string'],
    'fileowner': ['string'],
    'fileperms': ['string'],
    'filesize': ['string'],
    'filetype': ['string'],
    'floatval': ['mixed'],
    'flock': ['resource', 'int', 'bool'],
    'floor': ['float'],
    'fmod': ['float', 'float'],
    'fnmatch': ['string', 'string', 'int'],
    'fopen': ['string', 'string', ('bool', 'use_include_path'), ('mixed', 'context')],
    'fpassthru': ['resource'],
    'fprintf': ['resource', 'string'],
    'fputcsv': ['resource', 'array', 'string', 'string'],
    'fread': ['resource', 'int'],
    'fscanf': ['resource', 'string'],
    'fseek': ['resource', 'int', 'int'],
    'fsockopen': ['string', 'int', 'int', 'string', 'float'],
    'fstat': ['resource'],
    'fsync': ['resource'],
    'ftell': ['resource'],
    'ftruncate': ['resource', 'int'],
    'function_exists': ['string'],
    'fwrite': ['resource', 'string'],
    'get_class': ['object'],
    'get_loaded_extensions': ['bool'],
    'get_parent_class': [('mixed', 'object_or_class')],
    'get_resource_id': ['resource'],
    'get_resource_type': ['resource'],
    'getdate': ['int'],
    'getenv': ['string'],
    'gethostbyaddr': ['string'],
    'gethostbyname': ['string'],
    'getprotobyname': ['string'],
    'getprotobynumber': ['int'],
    'getservbyname': ['string', 'string'],
    'getservbyport': ['int', 'string'],
    'gettype': ['mixed'],
    'glob': ['string'],
    'gmdate': ['string', 'int'],
    'gmmktime': ['int', 'int', 'int', 'int', 'int', 'int'],
    'gzcompress': ['string', 'int'],
    'gzdeflate': ['string', 'int'],
    'gzinflate': ['string', 'int'],
    'gzuncompress': ['string', 'int'],
    'hash': ['string', 'string', 'bool'],
    'hash_copy': ['HashContext'],
    'hash_equals': ['string', 'string'],
    'hash_file': ['string', 'string', 'bool'],
    'hash_final': ['HashContext', 'bool'],
    'hash_hmac': ['string', 'string', 'string', 'bool'],
    'hash_init': ['string'],
    'hash_update': ['HashContext', 'string'],
    'hex2bin': ['string'],
    'hrtime': ['bool'],
    'html_entity_decode': ['string'],
    'htmlentities': ['string', 'int', 'string'],
    'htmlspecialchars': ['string', 'int', 'string'],
    'hypot': ['float', 'float'],
    'implode': ['string', 'array'],
    'in_array': ['mixed', 'array', 'bool'],
    'inet_ntop': ['string'],
    'inet_pton': ['string'],
    'intdiv': ['int', 'int'],
    'interface_exists': ['string', 'bool'],
    'intval': ['mixed', ('int', 'base')],
    'ip2long': ['string'],
    'is_a': ['object', 'string', 'bool'],
    'is_array': ['mixed'],
    'is_bool': ['mixed'],
    'is_callable': ['mixed'],
    'is_dir': ['string'],
    'is_executable': ['string'],
    'is_file': ['string'],
    'is_finite': ['float'],
    'is_float': ['mixed'],
    'is_infinite': ['float'],
    'is_int': ['mixed'],
    'is_integer': ['mixed'],
    'is_iterable': ['mixed'],
    'is_link': ['string'],
    'is_long': ['mixed'],
    'is_nan': ['float'],
    'is_null': ['mixed'],
    'is_numeric': ['mixed'],
    'is_object': ['mixed'],
    'is_readable': ['string'],
    'is_resource': ['mixed'],
    'is_scalar': ['mixed'],
    'is_string': ['mixed'],
    'is_subclass_of': ['mixed', 'string', 'bool'],
    'is_writable': ['string'],
    'is_writeable': ['string'],
    'isset': ['mixed'],
    'iterator_apply': ['traversable', 'callable', 'array'],
    'iterator_count': ['traversable'],
    'iterator_to_array': ['traversable', 'bool'],
    'json_decode': ['string', 'bool', 'int', 'int'],
    'json_encode': ['mixed', 'int', 'int'],
    'json_validate': ['string', 'int', 'int'],
    'key': ['array'],
    'krsort': ['array'],
    'ksort': ['array'],
    'lcfirst': ['string'],
    'lchgrp': ['string', 'mixed'],
    'lchown': ['string', 'mixed'],
    'link': ['string', 'string'],
    'linkinfo': ['string'],
    'localtime': ['int', 'bool'],
    'log': ['float', 'float'],
    'log10': ['float'],
    'log2': ['float'],
    'long2ip': ['int'],
    'lstat': ['string'],
    'ltrim': ['string', 'string'],
    'max': ['mixed'],
    'md5': ['string', 'bool'],
    'microtime': ['bool'],
    'min': ['mixed'],
    'mkdir': ['string'],
    'mktime': ['int', 'int', 'int', 'int', 'int', 'int'],
    'mt_rand': ['int', 'int'],
    'natcasesort': ['array'],
    'natsort': ['array'],
    'next': ['array'],
    'nl2br': ['string'],
    'number_format': ['float', 'int', 'string', 'string'],
    'opendir': ['string'],
    'ord': ['string'],
    'passthru': ['string'],
    'pathinfo': ['string', 'int'],
    'pclose': ['resource'],
    'pfsockopen': ['string', 'int', 'int', 'string', 'float'],
    'php_uname': ['string'],
    'phpversion': ['string'],
    'popen': ['string', 'string'],
    'pow': ['float', 'float'],
    'preg_match': ['string', 'string', 'array'],
    'preg_match_all': ['string', 'string'],
    'preg_replace': ['string', 'string', 'string'],
    'preg_replace_callback': ['string', 'callable', 'string'],
    'preg_split': ['string', 'string', 'int', 'int'],
    'prev': ['array'],
    'print_r': ['mixed', ('bool', 'return')],
    'printf': ['string'],
    'ptr': ['mixed'],
    'ptr_get': ['pointer'],
    'ptr_is_null': ['pointer'],
    'ptr_offset': ['pointer', 'int'],
    'ptr_read16': ['pointer'],
    'ptr_read32': ['pointer'],
    'ptr_read8': ['pointer'],
    'ptr_read_string': ['pointer', 'int'],
    'ptr_set': ['pointer', 'mixed'],
    'ptr_sizeof': ['string'],
    'ptr_write16': ['pointer', 'int'],
    'ptr_write32': ['pointer', 'int'],
    'ptr_write8': ['pointer', 'int'],
    'ptr_write_string': ['pointer', 'string'],
    'putenv': ['string'],
    'rad2deg': ['float'],
    'rand': ['int', 'int'],
    'random_int': ['int', 'int'],
    'range': ['mixed', 'mixed', 'int'],
    'rawurldecode': ['string'],
    'rawurlencode': ['string'],
    'readdir': ['resource'],
    'readfile': ['string'],
    'readline': ['string'],
    'readlink': ['string'],
    'realpath': ['string'],
    'rename': ['string', 'string'],
    'reset': ['array'],
    'rewind': ['resource'],
    'rewinddir': ['resource'],
    'rmdir': ['string'],
    'round': ['float', 'int'],
    'rsort': ['array'],
    'rtrim': ['string', 'string'],
    'scandir': ['string'],
    'settype': ['mixed', 'string'],
    'sha1': ['string', 'bool'],
    'shell_exec': ['string'],
    'shuffle': ['array'],
    'sin': ['float'],
    'sinh': ['float'],
    'sleep': ['int'],
    'sort': ['array'],
    'spl_autoload': ['string', 'string'],
    'spl_autoload_call': ['string'],
    'spl_autoload_extensions': ['string'],
    'spl_autoload_register': ['callable', 'bool', 'bool'],
    'spl_autoload_unregister': ['callable'],
    'spl_object_hash': ['object'],
    'spl_object_id': ['object'],
    'sprintf': ['string'],
    'sqrt': ['float'],
    'sscanf': ['string', 'string'],
    'stat': ['string'],
    'str_contains': ['string', 'string'],
    'str_ends_with': ['string', 'string'],
    'str_ireplace': ['mixed', 'mixed', 'mixed', 'int'],
    'str_pad': ['string', 'int', 'string', 'int'],
    'str_repeat': ['string', 'int'],
    'str_replace': ['string', 'string', 'string', 'int'],
    'str_split': ['string', 'int'],
    'str_starts_with': ['string', 'string'],
    'str_word_count': ['string', 'int', 'string'],
    'strcasecmp': ['string', 'string'],
    'strcmp': ['string', 'string'],
    'stream_bucket_append': ['mixed', 'mixed'],
    'stream_bucket_make_writeable': ['mixed'],
    'stream_bucket_new': ['resource', 'string'],
    'stream_bucket_prepend': ['mixed', 'mixed'],
    'stream_context_create': ['array', 'array'],
    'stream_context_get_default': ['array'],
    'stream_context_get_options': ['resource'],
    'stream_context_get_params': ['resource'],
    'stream_context_set_default': ['array'],
    'stream_context_set_option': ['resource', 'string', 'string', 'mixed'],
    'stream_context_set_params': ['resource', 'array'],
    'stream_copy_to_stream': ['resource', 'resource', 'int', 'int'],
    'stream_filter_append': ['resource', 'string', 'int', 'mixed'],
    'stream_filter_prepend': ['resource', 'string', 'int', 'mixed'],
    'stream_filter_register': ['string', 'string'],
    'stream_filter_remove': ['resource'],
    'stream_get_contents': ['resource', 'int', 'int'],
    'stream_get_line': ['resource', 'int', 'string'],
    'stream_get_meta_data': ['resource'],
    'stream_is_local': ['resource'],
    'stream_isatty': ['resource'],
    'stream_resolve_include_path': ['string'],
    'stream_select': ['array', 'array', 'array', 'int', 'int'],
    'stream_set_blocking': ['resource', 'bool'],
    'stream_set_chunk_size': ['resource', 'int'],
    'stream_set_read_buffer': ['resource', 'int'],
    'stream_set_timeout': ['resource', 'int', 'int'],
    'stream_set_write_buffer': ['resource', 'int'],
    'stream_socket_accept': ['resource', 'float', 'string'],
    'stream_socket_client': ['string'],
    'stream_socket_enable_crypto': ['resource', 'bool', 'int', 'resource'],
    'stream_socket_get_name': ['resource', 'bool'],
    'stream_socket_pair': ['int', 'int', 'int'],
    'stream_socket_recvfrom': ['resource', 'int', 'int', 'string'],
    'stream_socket_sendto': ['resource', 'string', 'int', 'string'],
    'stream_socket_server': ['string'],
    'stream_socket_shutdown': ['resource', 'int'],
    'stream_supports_lock': ['resource'],
    'stream_wrapper_register': ['string', 'string', 'int'],
    'stream_wrapper_restore': ['string'],
    'stream_wrapper_unregister': ['string'],
    'stripslashes': ['string'],
    'strlen': ['string'],
    'strncasecmp': ['string', 'string', 'int'],
    'strncmp': ['string', 'string', 'int'],
    'strpos': ['string', 'string', 'int'],
    'strrev': ['string'],
    'strrpos': ['string', 'string', 'int'],
    'strstr': ['string', 'string', 'bool'],
    'strtolower': ['string'],
    'strtotime': ['string', 'int'],
    'strtoupper': ['string'],
    'strtr': ['string', 'array|string', 'string'],
    'strval': ['mixed'],
    'substr': ['string', 'int', 'int'],
    'substr_count': ['string', 'string'],
    'substr_replace': ['string', 'string', 'int', 'int'],
    'symlink': ['string', 'string'],
    'system': ['string'],
    'tan': ['float'],
    'tanh': ['float'],
    'tempnam': ['string', 'string'],
    'touch': ['string', 'int', 'int'],
    'trait_exists': ['string', 'bool'],
    'trim': ['string', 'string'],
    'uasort': ['array', 'callable'],
    'ucfirst': ['string'],
    'ucwords': ['string', 'string'],
    'uksort': ['array', 'callable'],
    'umask': ['int'],
    'unlink': ['string'],
    'unset': ['mixed'],
    'urldecode': ['string'],
    'urlencode': ['string'],
    'usleep': ['int'],
    'usort': ['array', 'callable'],
    'var_dump': ['mixed'],
    'vfprintf': ['resource', 'string', 'array'],
    'vprintf': ['string', 'array'],
    'vsprintf': ['string', 'array'],
    'wordwrap': ['string', 'int', 'string', 'bool'],
    'zval_free': ['pointer'],
    'zval_pack': ['mixed'],
    'zval_type': ['pointer'],
    'zval_unpack': ['pointer'],
}


@dataclass
class Parameter:
    """A single positional parameter of a builtin signature."""

    name: str
    php_type: str  # displayed type, e.g. "string", "int", "mixed", "array"
    by_ref: bool = False
    default: Optional[str] = None  # rendered as PHP literal, e.g. "true", "0", "''"
    optional: bool = False


@dataclass
class BuiltinSig:
    """Resolved call signature for a builtin (parameters + return type)."""

    params: List[Parameter] = field(default_factory=list)
    variadic: Optional[str] = None  # name of the variadic parameter, if any
    return_type: str = "mixed"  # PHP-style rendered type
    return_is_intersect: bool = False  # whether the return is conditional (e.g. abs preserves type)


@dataclass
class LoweringInfo:
    """Where the builtin is implemented in the compiler source."""

    sig_file: Optional[str] = None
    sig_line: Optional[int] = None
    sig_arm: Optional[str] = None  # e.g. "\"strlen\" => Some(fixed(&[\"string\"]))"
    checker_file: Optional[str] = None
    checker_line: Optional[int] = None
    codegen_file: Optional[str] = None
    codegen_line: Optional[int] = None
    codegen_function: Optional[str] = None  # e.g. "lower_strlen"
    runtime_helpers: List[str] = field(default_factory=list)
    notes: List[str] = field(default_factory=list)  # pulled from the /// doc comment of the lowering fn


@dataclass
class Builtin:
    """A single PHP builtin known to the Elephc compiler."""

    name: str
    area: str
    sub_area: str
    canonical_name: str  # always lowercase
    in_catalog: bool  # present in SUPPORTED_BUILTIN_FUNCTIONS
    is_internal: bool  # compiler-only helper with no user-facing page
    sig: BuiltinSig = field(default_factory=BuiltinSig)
    lowering: LoweringInfo = field(default_factory=LoweringInfo)
    description: str = ""  # filled by hand, or left empty for the stub
    examples: List[str] = field(default_factory=list)  # raw ```php ... ``` blocks
    see_also: List[str] = field(default_factory=list)
    notes: List[str] = field(default_factory=list)
    # Eval-interpreter (elephc-magician) support block from the gen_builtins
    # exporter: {supported, kind, area, hooks, params, variadic, home_file}.
    eval_support: Optional[dict] = None
    # Compiler support route and effective AOT signature from the shared contract.
    aot_support: Optional[dict] = None
    # True when only the eval interpreter exposes this builtin (no AOT support).
    eval_only: bool = False
    # PHP module (php-src extension) owning the name per the shared contract, e.g.
    # "standard", "curl", or "elephc" for elephc-only surfaces.
    module: str = "elephc"
    # First PHP minor that ships the name ("8.5"), or None when every supported
    # profile has it.
    since: Optional[str] = None
    # True for elephc extensions with no PHP equivalent (ptr_*, buffer_*,
    # class_attribute_*); `--strict-php` hides them from user programs.
    is_extension: bool = False
    # Backend-neutral compiler semantics exported directly by `gen_builtins`.
    semantics: Optional[dict] = None


def slug(name: str) -> str:
    """Filename-safe slug for a builtin name."""
    return name.replace("\\", "-").replace("::", "-")


# Hand-curated return-type overrides. These win over anything parsed from
# `check_builtin()` arms (which sometimes narrow to the wrong type, e.g.
# `array_shift` returns `mixed`, not the array element type).
RETURN_TYPE_OVERRIDES: Dict[str, str] = {
    # getenv() returns the value string for a set name and the exact `false`
    # singleton when the name is absent; the neutral contract records the
    # union-covering `Mixed` because TypeSpec has no union variant.
    "getenv": "string|false",
    # phpversion() is `string`, phpversion($extension) is `string|false`; the
    # registry records the union-covering `Mixed` for the whole arity, so the
    # documented return type is recovered here (reference PHP's own signature).
    "phpversion": "string|false",
    # array_shift / array_pop return the shifted element which is `mixed`
    # in PHP's loose type system.
    "array_shift": "mixed",
    "array_pop": "mixed",
    "current": "mixed",
    "end": "mixed",
    "next": "mixed",
    "prev": "mixed",
    "reset": "mixed",
    # String functions with a concrete string return type.
    "addslashes": "string",
    "bin2hex": "string",
    "chr": "string",
    "lcfirst": "string",
    "nl2br": "string",
    "stripslashes": "string",
    "strrev": "string",
    "strtolower": "string",
    "strtoupper": "string",
    "ucfirst": "string",
    "ucwords": "string",
    "str_repeat": "string",
    # Array functions with a concrete array return type.
    "array_combine": "array",
    "array_diff_key": "array",
    "array_fill_keys": "array",
    "array_intersect_key": "array",
    "array_merge": "array",
    "array_reverse": "array",
    "array_splice": "array",
    "array_unique": "array",
    "getdate": "array",
    "localtime": "array",
    "realpath_cache_get": "array",
    "stream_context_get_options": "array",
    "stream_context_get_params": "array",
    "stream_get_meta_data": "array",
    "iterator_to_array": "array",
    # Sorting/array mutating functions that return true on success.
    "arsort": "bool",
    "asort": "bool",
    "krsort": "bool",
    "ksort": "bool",
    "rsort": "bool",
    "shuffle": "bool",
    "sort": "bool",
    "natcasesort": "bool",
    "natsort": "bool",
    "uasort": "bool",
    "uksort": "bool",
    "usort": "bool",
    # `$format`/`$mode` select the result shape at run time in reference PHP; the registry
    # records the coarse `Mixed` that covers both shapes for the whole arity.
    "str_word_count": "array|int",
    "count_chars": "array|string",
    # Other concrete return types confirmed by PHP reflection.
    "class_alias": "bool",
    "define": "bool",
    "touch": "bool",
    "fseek": "int",
    "vprintf": "int",
    "vsprintf": "string",
    "iterator_apply": "int",
    # The AOT curl prelude returns PHP 8 handle objects and php-src's own unions;
    # transcribed from `src/curl_prelude.rs`'s declarations. `curl_version()` and
    # `curl_multi_info_read()` are deliberately left `mixed`: the prelude declares no
    # return type for either (see the comment above `curl_version()` for why a
    # declared `array|false` would reinterpret the payload), and inventing one here
    # would document a contract the compiler does not enforce.
    "curl_copy_handle": "CurlHandle",
    "curl_exec": "string|bool",
    "curl_init": "CurlHandle",
    "curl_multi_get_handles": "array",
    "curl_multi_getcontent": "?string",
    "curl_multi_init": "CurlMultiHandle",
    "curl_share_init": "CurlShareHandle",
    "curl_share_init_persistent": "CurlSharePersistentHandle",
    # The AOT hash prelude exposes PHP 8-style HashContext objects.
    "hash_init": "HashContext",
    "hash_update": "bool",
    "hash_final": "string",
    "hash_copy": "HashContext",
    "zval_pack": "pointer",
    "zval_unpack": "mixed",
    "zval_type": "int",
    "zval_free": "void",
}


RUNTIME_HELPER_OVERRIDES: Dict[str, List[str]] = {
    "mb_ereg_match": ["__rt_mb_ereg_match"],
}


# Hand-curated one-line descriptions for the user-facing pages. When a
# builtin has no override here, the renderer falls back to the first line of
# the lowering function's `///` doc comment, if available.
DESCRIPTION_OVERRIDES: Dict[str, str] = {
    "class_exists": "Checks whether the given class has been defined.",
    "define": "Defines a named constant at runtime.",
    "defined": "Checks whether a given named constant exists.",
    "empty": "Determines whether a variable is considered empty.",
    "isset": "Determines whether a variable is set and is not null.",
    "unset": "Unsets the given variables.",
    "print_r": "Prints human-readable information about a variable.",
    "var_dump": "Dumps information about a variable, including its type and value.",
    "phpversion": "Returns the targeted PHP language version, or one extension's version.",
    "php_uname": "Returns information about the operating system PHP is running on.",
    # These four functions are supplied by the compiler-injected hash prelude.
    # Availability comes from the shared contract; the text below explains the
    # intentional AOT object versus eval resource representation difference.
    "hash_init": (
        "Opens an incremental hashing context, returning a HashContext object. "
        "Provided by the compiler-injected hash prelude in compiled code; the eval "
        "interpreter still returns a resource."
    ),
    "hash_update": (
        "Feeds data into an incremental hashing context. Provided by the "
        "compiler-injected hash prelude in compiled code."
    ),
    "hash_final": (
        "Finalizes an incremental hashing context and returns the digest (hex, or raw "
        "bytes when $binary). Provided by the compiler-injected hash prelude in "
        "compiled code."
    ),
    "hash_copy": (
        "Clones an incremental hashing context into an independent HashContext object. "
        "Provided by the compiler-injected hash prelude in compiled code."
    ),
}


# Notes for compiler-internal helpers. These appear on the internals pages for
# the __elephc_* builtins that have no user-facing reference.
INTERNAL_NOTES: Dict[str, List[str]] = {
    "__elephc_mktime_raw": [
        "Internal helper used by the mktime() builtin.",
        "Bypasses timezone handling and calls the runtime mktime helper directly.",
    ],
    "__elephc_gmmktime_raw": [
        "Internal helper used by the gmmktime() builtin.",
        "Bypasses timezone handling and calls the runtime gmmktime helper directly.",
    ],
    "__elephc_strtotime_raw": [
        "Internal helper used by the strtotime() builtin.",
        "Provides a raw timestamp parsing path for the runtime strtotime helper.",
    ],
    "__elephc_phar_list_entries": [
        "Internal helper used by the built-in Phar / PharData support to enumerate archive entries.",
        "Calls the native PHAR listing bridge and returns the entries as an array.",
    ],
    "__elephc_phar_set_compression": [
        "Internal helper used by the built-in Phar / PharData support to change archive compression.",
        "Calls the native PHAR compression-control bridge and returns whether the update succeeded.",
    ],
}
