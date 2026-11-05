//! Purpose:
//! The PHP form of the FFI prelude, kept under `cfg(test)` as the transcription source
//! and parse-parity oracle. Production uses the AST builders generated from this text.
//!
//! Called from:
//! - `cargo test -p elephc` through the `synthetic_class::transcribe` migration driver
//!   (`ELEPHC_TRANSCRIBE_WHICH=file ELEPHC_TRANSCRIBE_IN=...`).
//!
//! Key details:
//! - No production path tokenizes or parses this string; see the user-facing rule that
//!   preludes are AST-built.
//! - The bodies deliberately stay thin wrappers over the `elephc_ffi_*` bridge so a
//!   transcription change is mechanical.

/// The complete FFI prelude source, without a `<?php` header.
pub(super) const SRC: &str = r###"<?php
extern "elephc_ffi" {
    function elephc_ffi_cdef(string $code, ?string $library): int;
    function elephc_ffi_load(string $path): int;
    function elephc_ffi_scope(string $name): int;
    function elephc_ffi_error_kind(): int;
    function elephc_ffi_error_ptr(): ptr;
    function elephc_ffi_error_len(): int;
    function elephc_ffi_clear_error(): void;
    function elephc_ffi_type(int $context, string $name): int;
    function elephc_ffi_type_name(int $type): ptr;
    function elephc_ffi_type_name_len(int $type): int;
    function elephc_ffi_type_kind(int $type): int;
    function elephc_ffi_type_size(int $type): int;
    function elephc_ffi_type_align(int $type): int;
    function elephc_ffi_type_attributes(int $type): int;
    function elephc_ffi_type_enum_kind(int $type): int;
    function elephc_ffi_type_array_element(int $type): int;
    function elephc_ffi_type_array_length(int $type): int;
    function elephc_ffi_type_pointer_to(int $type): int;
    function elephc_ffi_type_pointer_pointee(int $type): int;
    function elephc_ffi_array_type(int $element, int $length): int;
    function elephc_ffi_type_struct_field_count(int $type): int;
    function elephc_ffi_type_struct_field_name(int $type, int $index): ptr;
    function elephc_ffi_type_struct_field_name_len(int $type, int $index): int;
    function elephc_ffi_type_struct_field_offset(int $type, string $name): int;
    function elephc_ffi_type_struct_field_type(int $type, string $name): int;
    function elephc_ffi_type_func_return(int $type): int;
    function elephc_ffi_type_func_param_count(int $type): int;
    function elephc_ffi_type_func_param(int $type, int $index): int;
    function elephc_ffi_type_func_abi(int $type): int;
    function elephc_ffi_new(int $context, string $name, bool $owned): int;
    function elephc_ffi_new_type(int $context, int $type, bool $owned): int;
    function elephc_ffi_new_any(int $context, string $name, int $type, bool $owned): int;
    function elephc_ffi_set_persistent(int $value): bool;
    function elephc_ffi_cast(int $type, int $value): int;
    function elephc_ffi_cast_name(int $context, string $name, int $value): int;
    function elephc_ffi_cast_any(int $context, string $name, int $type, int $value): int;
    function elephc_ffi_typeof(int $value): int;
    function elephc_ffi_cdata_type_name(int $value): ptr;
    function elephc_ffi_cdata_type_name_len(int $value): int;
    function elephc_ffi_sizeof_cdata(int $value): int;
    function elephc_ffi_alignof_cdata(int $value): int;
    function elephc_ffi_sizeof_type(int $type): int;
    function elephc_ffi_alignof_type(int $type): int;
    function elephc_ffi_addr(int $value): int;
    function elephc_ffi_addr_owned(int $value): int;
    function elephc_ffi_cdata_is_callable(int $value): bool;
    function elephc_ffi_free(int $value): bool;
    function elephc_ffi_release_cdata(int $value): void;
    function elephc_ffi_cdata_clone(int $value): int;
    function elephc_ffi_is_null(int $value): bool;
    function elephc_ffi_is_freed(int $value): bool;
    function elephc_ffi_pointer_value(int $value): u64;
    function elephc_ffi_memcpy(int $to, int $from, int $size): bool;
    function elephc_ffi_memcpy_string(int $to, string $from, int $from_len, int $size): bool;
    function elephc_ffi_memcmp(int $left, int $right, int $size): int;
    function elephc_ffi_memcmp_string(int $left, string $right, int $right_len, int $size): int;
    function elephc_ffi_memset(int $ptr, int $value, int $size): bool;
    function elephc_ffi_string(int $ptr, int $size, bool $has_size): ptr;
    function elephc_ffi_string_len(): int;
    function elephc_ffi_field_kind(int $value, string $name): int;
    function elephc_ffi_field_int(int $value, string $name): int;
    function elephc_ffi_field_float(int $value, string $name): float;
    function elephc_ffi_field_bool(int $value, string $name): bool;
    function elephc_ffi_field_char(int $value, string $name): int;
    function elephc_ffi_field_cdata(int $value, string $name): int;
    function elephc_ffi_field_pointer_type(int $value, string $name): int;
    function elephc_ffi_field_is_null(int $value, string $name): bool;
    function elephc_ffi_field_addr(int $value, string $name): int;
    function elephc_ffi_field_type(int $value, string $name): int;
    function elephc_ffi_field_size(int $value, string $name): int;
    function elephc_ffi_field_write_int(int $value, string $name, int $input): bool;
    function elephc_ffi_field_write_float(int $value, string $name, float $input): bool;
    function elephc_ffi_field_write_bool(int $value, string $name, bool $input): bool;
    function elephc_ffi_field_write_char(int $value, string $name, string $input, int $len): bool;
    function elephc_ffi_field_write_cdata(int $value, string $name, int $input): bool;
    function elephc_ffi_field_write_pointer(int $value, string $name, int $input): bool;
    function elephc_ffi_field_write_null(int $value, string $name): bool;
    function elephc_ffi_field_write_string(int $value, string $name, string $input, int $len): bool;
    function elephc_ffi_field_count(int $value): int;
    function elephc_ffi_field_name_at(int $value, int $index): ptr;
    function elephc_ffi_field_name_at_len(int $value, int $index): int;
    function elephc_ffi_offset_kind(int $value, int $index): int;
    function elephc_ffi_offset_int(int $value, int $index): int;
    function elephc_ffi_offset_float(int $value, int $index): float;
    function elephc_ffi_offset_bool(int $value, int $index): bool;
    function elephc_ffi_offset_char(int $value, int $index): int;
    function elephc_ffi_offset_cdata(int $value, int $index): int;
    function elephc_ffi_offset_pointer_type(int $value, int $index): int;
    function elephc_ffi_offset_is_null(int $value, int $index): bool;
    function elephc_ffi_offset_addr(int $value, int $index): int;
    function elephc_ffi_offset_type(int $value, int $index): int;
    function elephc_ffi_offset_size(int $value, int $index): int;
    function elephc_ffi_offset_write_int(int $value, int $index, int $input): bool;
    function elephc_ffi_offset_write_float(int $value, int $index, float $input): bool;
    function elephc_ffi_offset_write_bool(int $value, int $index, bool $input): bool;
    function elephc_ffi_offset_write_char(int $value, int $index, string $input, int $len): bool;
    function elephc_ffi_offset_write_cdata(int $value, int $index, int $input): bool;
    function elephc_ffi_offset_write_pointer(int $value, int $index, int $input): bool;
    function elephc_ffi_offset_write_null(int $value, int $index): bool;
    function elephc_ffi_offset_write_string(int $value, int $index, string $input, int $len): bool;
    function elephc_ffi_array_length(int $value): int;
    function elephc_ffi_iter_cdata(int $value, int $index): int;
    function elephc_ffi_free_offset(int $value, int $index): bool;
    function elephc_ffi_free_field(int $value, string $name): bool;
    function elephc_ffi_offset_string(int $value, int $index, int $size, bool $has_size): ptr;
    function elephc_ffi_field_string(int $value, string $name, int $size, bool $has_size): ptr;
    function elephc_ffi_memset_offset(int $value, int $index, int $byte, int $size): bool;
    function elephc_ffi_memset_field(int $value, string $name, int $byte, int $size): bool;
    function elephc_ffi_memcpy_offset(int $value, int $index, int $source, int $size): bool;
    function elephc_ffi_memcpy_field(int $value, string $name, int $source, int $size): bool;
    function elephc_ffi_global_kind(int $context, string $name): int;
    function elephc_ffi_global_int(int $context, string $name): int;
    function elephc_ffi_global_float(int $context, string $name): float;
    function elephc_ffi_global_bool(int $context, string $name): bool;
    function elephc_ffi_global_char(int $context, string $name): int;
    function elephc_ffi_global_cdata(int $context, string $name): int;
    function elephc_ffi_global_string(int $context, string $name): ptr;
    function elephc_ffi_global_write_int(int $context, string $name, int $input): bool;
    function elephc_ffi_global_write_float(int $context, string $name, float $input): bool;
    function elephc_ffi_global_write_bool(int $context, string $name, bool $input): bool;
    function elephc_ffi_global_write_cdata(int $context, string $name, int $input): bool;
    function elephc_ffi_global_write_pointer(int $context, string $name, int $input): bool;
    function elephc_ffi_global_write_null(int $context, string $name): bool;
    function elephc_ffi_cdata_address(int $value): int;
    function elephc_ffi_offset_address(int $value, int $index): int;
    function elephc_ffi_global_address(int $context, string $name): int;
    function elephc_ffi_global_pointer_type(int $context, string $name): int;
    function elephc_ffi_cdata_pointer(int $value): int;
    function elephc_ffi_global_write_closure(int $context, string $name): int;
    function elephc_ffi_field_write_closure(int $value, string $name): int;
    function elephc_ffi_offset_write_closure(int $value, int $index): int;
    function elephc_ffi_cdata_write_closure(int $value): int;
    function elephc_ffi_set_dispatcher_int(callable $dispatcher): void;
    function elephc_ffi_set_dispatcher_float(callable $dispatcher): void;
    function elephc_ffi_set_dispatcher_void(callable $dispatcher): void;
    function elephc_ffi_set_dispatcher_aggregate(callable $dispatcher): void;
    function elephc_ffi_stage_aggregate(int $value): int;
    function elephc_ffi_callback_arg_count(int $call): int;
    function elephc_ffi_callback_arg_kind(int $call, int $index): int;
    function elephc_ffi_callback_arg_int(int $call, int $index): int;
    function elephc_ffi_callback_arg_float(int $call, int $index): float;
    function elephc_ffi_callback_arg_string(int $call, int $index): ptr;
    function elephc_ffi_callback_arg_string_len(int $call, int $index): int;
    function elephc_ffi_callback_arg_cdata(int $call, int $index): int;
    function elephc_ffi_callback_name(int $call): ptr;
    function elephc_ffi_callback_name_len(int $call): int;
    function elephc_ffi_callback_active_name(int $call): ptr;
    function elephc_ffi_callback_active_name_len(int $call): int;
    function elephc_ffi_callback_abort(): void;
    function elephc_ffi_function_cdata(int $context, string $name): int;
    function elephc_ffi_call_begin(int $context, string $name): int;
    function elephc_ffi_call_begin_cdata(int $value): int;
    function elephc_ffi_call_arg_int(int $call, int $index, int $value): bool;
    function elephc_ffi_call_arg_float(int $call, int $index, float $value): bool;
    function elephc_ffi_call_arg_string(int $call, int $index, string $value, int $len): bool;
    function elephc_ffi_call_arg_cdata(int $call, int $index, int $value): bool;
    function elephc_ffi_call_param_is_function_pointer(int $call, int $index): bool;
    function elephc_ffi_call_arg_callback(int $call, int $index): int;
    function elephc_ffi_call_arg_pointer(int $call, int $index, int $value): bool;
    function elephc_ffi_callback_code(int $id): int;
    function elephc_ffi_call_arg_null(int $call, int $index): bool;
    function elephc_ffi_call_arg_bool(int $call, int $index, bool $value): bool;
    function elephc_ffi_call_invoke(int $call): int;
    function elephc_ffi_call_result_int(int $call): int;
    function elephc_ffi_call_result_float(int $call): float;
    function elephc_ffi_call_result_bool(int $call): bool;
    function elephc_ffi_call_result_cdata(int $call): int;
    function elephc_ffi_call_result_string(int $call): ptr;
    function elephc_ffi_call_result_string_len(int $call): int;
    function elephc_ffi_call_release(int $call): void;
    function elephc_ffi_binary_kind(int $left, int $left_kind, int $right, int $right_kind, int $op): int;
    function elephc_ffi_binary_int(int $left, int $left_kind, int $right, int $right_kind, int $op): int;
    function elephc_ffi_binary_cdata(int $left, int $left_kind, int $right, int $right_kind, int $op): int;
    function elephc_ffi_binary_compare(int $left, int $left_kind, int $right, int $right_kind): int;
    function elephc_ffi_last_place_type(): int;
    function elephc_ffi_last_place_addr(): int;
    function elephc_ffi_last_place_size(): int;
    function elephc_ffi_last_place_align(): int;
    function elephc_ffi_last_place_string(int $size, bool $has_size): ptr;
    function elephc_ffi_last_place_memset(int $byte, int $size): bool;
    function elephc_ffi_last_place_free(): bool;
    function elephc_ffi_last_place_memcmp(int $other, int $size): int;
    function elephc_ffi_last_place_memcpy(int $destination, int $size): bool;
    function elephc_ffi_last_place_cast(int $context, string $name, int $type): int;
}

function __elephc_ffi_type_handle(mixed $type): int {
    if ($type instanceof \FFI\CType) {
        return $type->__elephc_raw();
    }
    return 0;
}

function __elephc_ffi_type_name(mixed $type): string {
    if (is_string($type)) {
        return $type;
    }
    return "";
}

function __elephc_ffi_field_value(int $handle, string $name, int $kind): mixed {
    if ($kind === 5) {
        return \FFI\CData::__elephc_wrap(elephc_ffi_field_cdata($handle, $name));
    }
    if ($kind === 6) {
        return null;
    }
    if ($kind === 4) {
        return chr(elephc_ffi_field_char($handle, $name));
    }
    if ($kind === 3) {
        return elephc_ffi_field_bool($handle, $name);
    }
    if ($kind === 2) {
        return elephc_ffi_field_float($handle, $name);
    }
    return elephc_ffi_field_int($handle, $name);
}

function __elephc_ffi_offset_value(int $handle, int $index, int $kind): mixed {
    if ($kind === 5) {
        return \FFI\CData::__elephc_wrap(elephc_ffi_offset_cdata($handle, $index));
    }
    if ($kind === 6) {
        return null;
    }
    if ($kind === 4) {
        return chr(elephc_ffi_offset_char($handle, $index));
    }
    if ($kind === 3) {
        return elephc_ffi_offset_bool($handle, $index);
    }
    if ($kind === 2) {
        return elephc_ffi_offset_float($handle, $index);
    }
    return elephc_ffi_offset_int($handle, $index);
}

function __elephc_ffi_add_arg(int $call, int $index, mixed $arg): void {
    if (elephc_ffi_call_param_is_function_pointer($call, $index)) {
        if (is_null($arg)) {
            elephc_ffi_call_arg_null($call, $index);
            return;
        }
        if ($arg instanceof \FFI\CData) {
            elephc_ffi_call_arg_cdata($call, $index, $arg->__elephc_raw());
            return;
        }
        if (is_callable($arg)) {
            \FFI::__elephc_ensure_dispatchers();
            $id = elephc_ffi_call_arg_callback($call, $index);
            if ($id === 0) {
                \FFI::__elephc_throw();
            }
            \FFI::__elephc_register_callback_closure($id, $arg);
            elephc_ffi_call_arg_pointer($call, $index, elephc_ffi_callback_code($id));
            return;
        }
        if (is_string($arg)) {
            throw new \FFI\Exception('Attempt to assign an invalid callback, function "' . $arg . '" not found or invalid function name');
        }
        throw new \FFI\Exception("Attempt to assign an invalid callback, no array or string given");
    }
    if (is_null($arg)) {
        elephc_ffi_call_arg_null($call, $index);
        return;
    }
    if (is_bool($arg)) {
        elephc_ffi_call_arg_bool($call, $index, $arg);
        return;
    }
    if (is_int($arg)) {
        elephc_ffi_call_arg_int($call, $index, $arg);
        return;
    }
    if (is_float($arg)) {
        elephc_ffi_call_arg_float($call, $index, $arg);
        return;
    }
    if (is_string($arg)) {
        elephc_ffi_call_arg_string($call, $index, $arg, strlen($arg));
        return;
    }
    if ($arg instanceof \FFI\CData) {
        elephc_ffi_call_arg_cdata($call, $index, $arg->__elephc_raw());
        return;
    }
    if ($arg instanceof \Closure) {
        throw new \FFI\Exception("Passing incompatible argument for a C parameter, found PHP 'Closure'");
    }
    if (is_object($arg)) {
        elephc_ffi_call_arg_cdata($call, $index, $arg->__elephc_raw());
        return;
    }
    throw new \FFI\Exception("Unsupported FFI argument type: " . gettype($arg));
}

function __elephc_ffi_operand_value(mixed $value): int {
    if ($value instanceof \FFI\CData) {
        return $value->__elephc_raw();
    }
    return (int) $value;
}

function __elephc_ffi_operand_kind(mixed $value): int {
    if ($value instanceof \FFI\CData) {
        return 1;
    }
    return 0;
}

function __elephc_ffi_operand_type(mixed $value): string {
    if ($value instanceof \FFI\CData) {
        return "FFI\\CData";
    }
    if ($value === null) {
        return "null";
    }
    if (is_bool($value)) {
        return "bool";
    }
    if (is_int($value)) {
        return "int";
    }
    if (is_float($value)) {
        return "float";
    }
    if (is_string($value)) {
        return "string";
    }
    if (is_array($value)) {
        return "array";
    }
    if (is_object($value)) {
        return get_class($value);
    }
    return "resource";
}

function __elephc_ffi_unsupported_operands(mixed $left, mixed $right, string $operator, bool $object_first = false): void {
    $left_type = __elephc_ffi_operand_type($left);
    $right_type = __elephc_ffi_operand_type($right);
    if ($object_first && !($left instanceof \FFI\CData) && $right instanceof \FFI\CData) {
        throw new \TypeError("Unsupported operand types: " . $right_type . " " . $operator . " " . $left_type);
    }
    throw new \TypeError("Unsupported operand types: " . $left_type . " " . $operator . " " . $right_type);
}

function __elephc_ffi_finish_call(int $call): mixed {
    $kind = elephc_ffi_call_invoke($call);
    if ($kind === 7) {
        \FFI::__elephc_throw();
    }
    if ($kind === 1) {
        $result = elephc_ffi_call_result_int($call);
    } elseif ($kind === 2) {
        $result = elephc_ffi_call_result_float($call);
    } elseif ($kind === 3) {
        $result = elephc_ffi_call_result_bool($call);
    } elseif ($kind === 4) {
        $result = \FFI\CData::__elephc_wrap(elephc_ffi_call_result_cdata($call));
    } elseif ($kind === 5) {
        $ptr = elephc_ffi_call_result_string($call);
        $len = elephc_ffi_call_result_string_len($call);
        $result = ptr_read_string($ptr, $len);
    } elseif ($kind === 6) {
        $result = chr(elephc_ffi_call_result_int($call));
    } else {
        $result = null;
    }
    elephc_ffi_call_release($call);
    return $result;
}

function __elephc_ffi_callback_dispatch_int(int $id, int $call): int {
    return \FFI::__elephc_callback_invoke_int($id, $call);
}

function __elephc_ffi_callback_dispatch_float(int $id, int $call): float {
    return \FFI::__elephc_callback_invoke_float($id, $call);
}

function __elephc_ffi_callback_dispatch_void(int $id, int $call): void {
    \FFI::__elephc_callback_invoke_void($id, $call);
}

function __elephc_ffi_callback_dispatch_aggregate(int $id, int $call): int {
    return \FFI::__elephc_callback_invoke_aggregate($id, $call);
}

final class FFI implements \IteratorAggregate {
    public const __BIGGEST_ALIGNMENT__ = 16;

    private int $__elephc_context;
    private static array $__elephc_closures = [];
    private static array $__elephc_callback_closures = [];
    private static bool $__elephc_dispatchers_ready = false;
    private static bool $__elephc_enabled = true;

    private static function __elephc_validate_api(): void {
        if (!self::$__elephc_enabled) {
            throw new \FFI\Exception('FFI API is restricted by "ffi.enable" configuration directive');
        }
    }

    public static function __elephc_register_closure(int $address, mixed $closure): void {
        $closures = self::$__elephc_closures;
        $closures[$address] = $closure;
        self::$__elephc_closures = $closures;
    }

    public static function __elephc_has_closure(int $address): bool {
        $closures = self::$__elephc_closures;
        return isset($closures[$address]);
    }

    public static function __elephc_closure(int $address): mixed {
        $closures = self::$__elephc_closures;
        return $closures[$address];
    }

    public static function __elephc_register_callback_closure(int $id, mixed $closure): void {
        $closures = self::$__elephc_callback_closures;
        $closures[$id] = $closure;
        self::$__elephc_callback_closures = $closures;
    }

    public static function __elephc_invoke_array(\FFI\CData $callback, array $args): mixed {
        return $callback->__elephc_invoke_array($args);
    }

    public static function __elephc_set_iter_by_ref(\FFI\CData $data, bool $by_ref): void {
        $data->__elephc_set_iter_by_ref($by_ref);
    }

    public static function __elephc_ensure_dispatchers(): void {
        if (self::$__elephc_dispatchers_ready) {
            return;
        }
        self::$__elephc_dispatchers_ready = true;
        elephc_ffi_set_dispatcher_int("__elephc_ffi_callback_dispatch_int");
        elephc_ffi_set_dispatcher_float("__elephc_ffi_callback_dispatch_float");
        elephc_ffi_set_dispatcher_void("__elephc_ffi_callback_dispatch_void");
        elephc_ffi_set_dispatcher_aggregate("__elephc_ffi_callback_dispatch_aggregate");
    }

    public static function __elephc_callback_invoke(int $id, int $call): mixed {
        $closures = self::$__elephc_callback_closures;
        $closure = $closures[$id] ?? null;
        if (!is_callable($closure)) {
            throw new \FFI\Exception("Cannot call callback");
        }
        $args = [];
        $count = elephc_ffi_callback_arg_count($call);
        for ($index = 0; $index < $count; $index++) {
            $kind = elephc_ffi_callback_arg_kind($call, $index);
            if ($kind === 1) {
                $args[] = elephc_ffi_callback_arg_int($call, $index);
            } elseif ($kind === 2) {
                $args[] = elephc_ffi_callback_arg_float($call, $index);
            } elseif ($kind === 3) {
                $args[] = elephc_ffi_callback_arg_int($call, $index) !== 0;
            } elseif ($kind === 4) {
                $args[] = chr(elephc_ffi_callback_arg_int($call, $index));
            } elseif ($kind === 5) {
                $string = elephc_ffi_callback_arg_string($call, $index);
                $string_len = elephc_ffi_callback_arg_string_len($call, $index);
                $args[] = ptr_read_string($string, $string_len);
            } elseif ($kind === 6) {
                $args[] = \FFI\CData::__elephc_wrap(elephc_ffi_callback_arg_cdata($call, $index));
            } else {
                $args[] = null;
            }
        }
        return $closure(...$args);
    }

    public static function __elephc_callback_invoke_int(int $id, int $call): int {
        try {
            $result = self::__elephc_callback_invoke($id, $call);
        } catch (\Throwable $error) {
            self::__elephc_callback_fatal($call, $error);
            return 0;
        }
        if ($result instanceof \FFI\CData) {
            return elephc_ffi_cdata_pointer($result->__elephc_raw());
        }
        if (is_string($result) && strlen($result) === 1) {
            return ord($result);
        }
        return (int) $result;
    }

    public static function __elephc_callback_invoke_float(int $id, int $call): float {
        try {
            $result = self::__elephc_callback_invoke($id, $call);
        } catch (\Throwable $error) {
            self::__elephc_callback_fatal($call, $error);
            return 0.0;
        }
        return (float) $result;
    }

    public static function __elephc_callback_invoke_void(int $id, int $call): void {
        try {
            self::__elephc_callback_invoke($id, $call);
        } catch (\Throwable $error) {
            self::__elephc_callback_fatal($call, $error);
        }
    }

    public static function __elephc_callback_invoke_aggregate(int $id, int $call): int {
        try {
            $result = self::__elephc_callback_invoke($id, $call);
        } catch (\Throwable $error) {
            self::__elephc_callback_fatal($call, $error);
            return 0;
        }
        if (!($result instanceof \FFI\CData)) {
            self::__elephc_callback_fatal($call, new \TypeError("Cannot call callback"));
            return 0;
        }
        return elephc_ffi_stage_aggregate($result->__elephc_raw());
    }

    private static function __elephc_callback_fatal(int $call, \Throwable $error): void {
        $name_len = elephc_ffi_callback_active_name_len($call);
        if ($name_len > 0) {
            $name_ptr = elephc_ffi_callback_active_name($call);
        } else {
            $name_ptr = elephc_ffi_callback_name($call);
            $name_len = elephc_ffi_callback_name_len($call);
        }
        $name = ptr_read_string($name_ptr, $name_len);
        $file = $error->getFile();
        $line = $error->getLine();
        echo "Warning: Uncaught " . get_class($error) . ": " . $error->getMessage() . " in " . $file . ":" . $line . "\n";
        echo "Stack trace:\n";
        echo "#0 " . $file . "(" . $line . "): {closure}()\n";
        echo "#1 " . $file . "(0): FFI->" . $name . "()\n";
        echo "#2 {main}\n";
        echo "  thrown in " . $file . " on line " . $line . "\n";
        echo "\n";
        echo "Fatal error: Throwing from FFI callbacks is not allowed in " . $file . " on line " . $line . "\n";
        elephc_ffi_callback_abort();
    }

    private function __construct(int $context) {
        $this->__elephc_context = $context;
    }

    public function getIterator(): \Iterator {
        return new \ArrayIterator([]);
    }

    public static function __elephc_wrap(int $context): self {
        return new self($context);
    }

    public static function __elephc_throw(): void {
        $kind = elephc_ffi_error_kind();
        $len = elephc_ffi_error_len();
        $message = ptr_read_string(elephc_ffi_error_ptr(), $len);
        if ($kind === 1) {
            throw new \FFI\ParserException($message);
        }
        if ($kind === 3) {
            throw new \TypeError($message);
        }
        throw new \FFI\Exception($message);
    }

    public static function cdef(mixed $code = null, mixed $library = null): FFI {
        self::__elephc_validate_api();
        $code = (string) ($code ?? "");
        $library = $library === null ? null : (string) $library;
        $context = elephc_ffi_cdef($code, $library);
        if ($context === 0) {
            $kind = elephc_ffi_error_kind();
            $len = elephc_ffi_error_len();
            $message = ptr_read_string(elephc_ffi_error_ptr(), $len);
            if ($kind === 1) {
                throw new \FFI\ParserException($message);
            }
            if ($kind === 3) {
                throw new \TypeError($message);
            }
            throw new \FFI\Exception($message);
        }
        return self::__elephc_wrap($context);
    }

    public static function load(string $filename): FFI {
        self::__elephc_validate_api();
        $context = elephc_ffi_load($filename);
        if ($context === 0) {
            self::__elephc_throw();
        }
        return self::__elephc_wrap($context);
    }

    public static function scope(string $name): FFI {
        self::__elephc_validate_api();
        $context = elephc_ffi_scope($name);
        if ($context === 0) {
            self::__elephc_throw();
        }
        return self::__elephc_wrap($context);
    }

    public static function new(FFI\CType|string $type, bool $owned = true, bool $persistent = false): FFI\CData {
        self::__elephc_validate_api();
        $handle = __elephc_ffi_type_handle($type);
        $name = __elephc_ffi_type_name($type);
        $data = elephc_ffi_new_any(0, $name, $handle, $owned);
        if ($data === 0) {
            self::__elephc_throw();
        }
        if ($persistent && !elephc_ffi_set_persistent($data)) {
            self::__elephc_throw();
        }
        return \FFI\CData::__elephc_wrap($data);
    }

    public static function free(mixed $ptr): void {
        self::__elephc_validate_api();
        if ($ptr instanceof \FFI\CData) {
            $ok = elephc_ffi_free($ptr->__elephc_raw());
        } else {
            $ok = elephc_ffi_last_place_free();
        }
        if (!$ok) {
            self::__elephc_throw();
        }
    }

    public static function __elephc_is_function_pointer_type(int $pointee_type): bool {
        return elephc_ffi_type_kind($pointee_type) === 16;
    }

    private static function __elephc_type_label(mixed $value): string {
        $label = gettype($value);
        if ($label === "NULL") {
            return "null";
        }
        if ($label === "object") {
            return get_class($value);
        }
        return $label;
    }

    private static function __elephc_cdata_raw(mixed $value, string $method, string $argument, string $name): int {
        if ($value instanceof \FFI\CData) {
            return $value->__elephc_raw();
        }
        throw new \TypeError("FFI::" . $method . "(): Argument #" . $argument . " ($" . $name . ") must be of type FFI\\CData, " . self::__elephc_type_label($value) . " given");
    }

    public static function cast(mixed $type, mixed $ptr): FFI\CData {
        self::__elephc_validate_api();
        $handle = __elephc_ffi_type_handle($type);
        $name = __elephc_ffi_type_name($type);
        if ($ptr instanceof \FFI\CData) {
            $data = elephc_ffi_cast_any(0, $name, $handle, $ptr->__elephc_raw());
        } else {
            $data = elephc_ffi_last_place_cast(0, $name, $handle);
        }
        if ($data === 0) {
            self::__elephc_throw();
        }
        return \FFI\CData::__elephc_wrap($data);
    }

    public static function type(string $type): FFI\CType {
        self::__elephc_validate_api();
        $handle = elephc_ffi_type(0, $type);
        if ($handle === 0) {
            self::__elephc_throw();
        }
        return \FFI\CType::__elephc_wrap($handle);
    }

    public static function typeof(mixed $ptr): FFI\CType {
        self::__elephc_validate_api();
        if ($ptr instanceof \FFI\CData) {
            $handle = elephc_ffi_typeof($ptr->__elephc_raw());
        } else {
            $handle = elephc_ffi_last_place_type();
        }
        if ($handle === 0) {
            throw new \TypeError('FFI::typeof(): Argument #1 ($ptr) must be of type FFI\CData, ' . self::__elephc_type_label($ptr) . " given");
        }
        return \FFI\CType::__elephc_wrap($handle);
    }

    public static function arrayType(FFI\CType $type, array $dimensions): FFI\CType {
        self::__elephc_validate_api();
        $handle = $type->__elephc_raw();
        foreach ($dimensions as $dimension) {
            $handle = elephc_ffi_array_type($handle, (int) $dimension);
        }
        return \FFI\CType::__elephc_wrap($handle);
    }

    public static function addr(mixed $ptr): FFI\CData {
        self::__elephc_validate_api();
        if ($ptr instanceof \FFI\CData) {
            $data = elephc_ffi_addr($ptr->__elephc_raw());
            if ($data === 0) {
                self::__elephc_throw();
            }
        } else {
            $data = elephc_ffi_last_place_addr();
            if ($data === 0) {
                throw new \TypeError('FFI::addr(): Argument #1 ($ptr) must be of type FFI\CData, ' . self::__elephc_type_label($ptr) . " given");
            }
        }
        return \FFI\CData::__elephc_wrap($data);
    }

    public static function __elephc_addr_temp(mixed $ptr): \FFI\CData {
        if ($ptr instanceof \FFI\CData) {
            $data = elephc_ffi_addr_owned($ptr->__elephc_raw());
            if ($data === 0) {
                self::__elephc_throw();
            }
            return \FFI\CData::__elephc_wrap($data);
        }
        return self::addr($ptr);
    }

    public static function __elephc_is_callable(mixed $value): bool {
        if ($value instanceof \FFI\CData) {
            return elephc_ffi_cdata_is_callable($value->__elephc_raw());
        }
        return false;
    }

    public static function sizeof(mixed $ptr): int {
        self::__elephc_validate_api();
        if ($ptr instanceof \FFI\CType) {
            return elephc_ffi_sizeof_type($ptr->__elephc_raw());
        }
        if ($ptr instanceof \FFI\CData) {
            return elephc_ffi_sizeof_cdata($ptr->__elephc_raw());
        }
        $size = elephc_ffi_last_place_size();
        if ($size === 0) {
            throw new \TypeError('FFI::sizeof(): Argument #1 ($ptr) must be of type FFI\CData|FFI\CType, ' . self::__elephc_type_label($ptr) . " given");
        }
        return $size;
    }

    public static function alignof(mixed $ptr): int {
        self::__elephc_validate_api();
        if ($ptr instanceof \FFI\CType) {
            return elephc_ffi_alignof_type($ptr->__elephc_raw());
        }
        if ($ptr instanceof \FFI\CData) {
            return elephc_ffi_alignof_cdata($ptr->__elephc_raw());
        }
        $align = elephc_ffi_last_place_align();
        if ($align === 0) {
            throw new \TypeError('FFI::alignof(): Argument #1 ($ptr) must be of type FFI\CData|FFI\CType, ' . self::__elephc_type_label($ptr) . " given");
        }
        return $align;
    }

    public static function memcpy(mixed $to, mixed $from, int $size): void {
        self::__elephc_validate_api();
        if (!($to instanceof \FFI\CData)) {
            throw new \TypeError('FFI::memcpy(): Argument #1 ($to) must be of type FFI\CData, ' . self::__elephc_type_label($to) . " given");
        }
        if (is_string($from)) {
            $ok = elephc_ffi_memcpy_string(self::__elephc_cdata_raw($to, "memcpy", "1", "to"), $from, strlen($from), $size);
        } elseif ($from instanceof \FFI\CData) {
            $ok = elephc_ffi_memcpy(self::__elephc_cdata_raw($to, "memcpy", "1", "to"), $from->__elephc_raw(), $size);
        } else {
            $ok = elephc_ffi_last_place_memcpy(self::__elephc_cdata_raw($to, "memcpy", "1", "to"), $size);
        }
        if (!$ok) {
            self::__elephc_throw();
        }
    }

    public static function memcmp(mixed $ptr1, mixed $ptr2, int $size): int {
        self::__elephc_validate_api();
        if (is_string($ptr2)) {
            $left = is_string($ptr1) ? 0 : self::__elephc_cdata_raw($ptr1, "memcmp", "1", "ptr1");
            if ($left === 0) {
                throw new \FFI\Exception("FFI::memcmp() does not accept two strings");
            }
            return elephc_ffi_memcmp_string($left, $ptr2, strlen($ptr2), $size);
        }
        $left = is_string($ptr1) ? 0 : self::__elephc_cdata_raw($ptr1, "memcmp", "1", "ptr1");
        if ($left === 0) {
            throw new \FFI\Exception("FFI::memcmp() does not accept a string as the first argument");
        }
        if ($ptr2 instanceof \FFI\CData) {
            return elephc_ffi_memcmp($left, $ptr2->__elephc_raw(), $size);
        }
        return elephc_ffi_last_place_memcmp($left, $size);
    }

    public static function memset(mixed $ptr, int $value, int $size): void {
        self::__elephc_validate_api();
        $target = $ptr instanceof \FFI\CData ? $ptr->__elephc_raw() : 0;
        $ok = $target !== 0
            ? elephc_ffi_memset($target, $value, $size)
            : elephc_ffi_last_place_memset($value, $size);
        if (!$ok) {
            self::__elephc_throw();
        }
    }

    public static function string(mixed $ptr, ?int $size = null): string {
        self::__elephc_validate_api();
        if (is_string($ptr)) {
            return $ptr;
        }
        $has_size = $size !== null;
        $actual = $has_size ? (int) $size : 0;
        $start = $ptr instanceof \FFI\CData
            ? elephc_ffi_string($ptr->__elephc_raw(), $actual, $has_size)
            : elephc_ffi_last_place_string($actual, $has_size);
        return ptr_read_string($start, elephc_ffi_string_len());
    }

    public static function isNull(mixed $ptr): bool {
        self::__elephc_validate_api();
        if (!($ptr instanceof \FFI\CData)) {
            throw new \TypeError('FFI::isNull(): Argument #1 ($ptr) must be of type FFI\CData, ' . self::__elephc_type_label($ptr) . " given");
        }
        return elephc_ffi_is_null(self::__elephc_cdata_raw($ptr, "isNull", "1", "ptr"));
    }

    public function __elephc_global_cdata(string $name): \FFI\CData {
        $kind = elephc_ffi_global_kind($this->__elephc_context, $name);
        if ($kind === 0) {
            $function = elephc_ffi_function_cdata($this->__elephc_context, $name);
            if ($function !== 0) {
                return \FFI\CData::__elephc_wrap($function);
            }
            self::__elephc_throw();
        }
        return \FFI\CData::__elephc_wrap(elephc_ffi_global_cdata($this->__elephc_context, $name));
    }

    public function __elephc_instance_new(mixed $type, bool $owned = true, bool $persistent = false): \FFI\CData {
        $handle = __elephc_ffi_type_handle($type);
        $type_name = __elephc_ffi_type_name($type);
        $data = elephc_ffi_new_any($this->__elephc_context, $type_name, $handle, $owned);
        if ($data === 0) {
            self::__elephc_throw();
        }
        if ($persistent && !elephc_ffi_set_persistent($data)) {
            self::__elephc_throw();
        }
        return \FFI\CData::__elephc_wrap($data);
    }

    public function __call(string $name, array $args): mixed {
        if ($name === "new") {
            return $this->__elephc_instance_new($args[0] ?? "", (bool) ($args[1] ?? true), (bool) ($args[2] ?? false));
        }
        if ($name === "type") {
            $handle = elephc_ffi_type($this->__elephc_context, (string) ($args[0] ?? ""));
            if ($handle === 0) {
                self::__elephc_throw();
            }
            return \FFI\CType::__elephc_wrap($handle);
        }
        if ($name === "cast") {
            $type = $args[0] ?? "";
            $ptr = $args[1] ?? null;
            if (!($ptr instanceof \FFI\CData)) {
                throw new \TypeError("FFI::cast(): Argument #2 ($ptr) must be of type FFI\\CData, " . self::__elephc_type_label($ptr) . " given");
            }
            $handle = __elephc_ffi_type_handle($type);
            $type_name = __elephc_ffi_type_name($type);
            $raw = self::__elephc_cdata_raw($ptr, "cast", "2", "ptr");
            $data = elephc_ffi_cast_any($this->__elephc_context, $type_name, $handle, $raw);
            if ($data === 0) {
                self::__elephc_throw();
            }
            return \FFI\CData::__elephc_wrap($data);
        }
        if ($name === "arrayType") {
            $handle = __elephc_ffi_type_handle($args[0] ?? null);
            foreach (($args[1] ?? []) as $dimension) {
                $handle = elephc_ffi_array_type($handle, (int) $dimension);
            }
            return \FFI\CType::__elephc_wrap($handle);
        }
        if ($name === "sizeof") {
            return self::sizeof($args[0] ?? null);
        }
        if ($name === "alignof") {
            return self::alignof($args[0] ?? null);
        }
        if ($name === "addr") {
            return self::addr($args[0] ?? null);
        }
        if ($name === "typeof") {
            return self::typeof($args[0] ?? null);
        }
        if ($name === "string") {
            $size = $args[1] ?? null;
            return self::string($args[0] ?? null, $size === null ? null : (int) $size);
        }
        if ($name === "isNull") {
            return self::isNull($args[0] ?? null);
        }
        if ($name === "memset") {
            return self::memset($args[0] ?? null, (int) ($args[1] ?? 0), (int) ($args[2] ?? 0));
        }
        if ($name === "memcpy") {
            return self::memcpy($args[0] ?? null, $args[1] ?? null, (int) ($args[2] ?? 0));
        }
        if ($name === "memcmp") {
            return self::memcmp($args[0] ?? null, $args[1] ?? null, (int) ($args[2] ?? 0));
        }
        if ($name === "free") {
            return self::free($args[0] ?? null);
        }
        $call = elephc_ffi_call_begin($this->__elephc_context, $name);
        if ($call === 0) {
            self::__elephc_throw();
        }
        $index = 0;
        foreach ($args as $arg) {
            __elephc_ffi_add_arg($call, $index, $arg);
            $index++;
        }
        return __elephc_ffi_finish_call($call);
    }

    public function __get(string $name): mixed {
        $kind = elephc_ffi_global_kind($this->__elephc_context, $name);
        if ($kind === 0) {
            $function = elephc_ffi_function_cdata($this->__elephc_context, $name);
            if ($function !== 0) {
                return \FFI\CData::__elephc_wrap($function);
            }
            self::__elephc_throw();
        }
        if ($kind === 5) {
            return \FFI\CData::__elephc_wrap(elephc_ffi_global_cdata($this->__elephc_context, $name));
        }
        if ($kind === 6) {
            return null;
        }
        if ($kind === 4) {
            return chr(elephc_ffi_global_char($this->__elephc_context, $name));
        }
        if ($kind === 3) {
            return elephc_ffi_global_bool($this->__elephc_context, $name);
        }
        if ($kind === 2) {
            return elephc_ffi_global_float($this->__elephc_context, $name);
        }
        return elephc_ffi_global_int($this->__elephc_context, $name);
    }

    public function __set(string $name, mixed $value): void {
        $pointer_type = elephc_ffi_global_pointer_type($this->__elephc_context, $name);
        if (self::__elephc_is_function_pointer_type($pointer_type)) {
            if ($value instanceof \Closure) {
                self::__elephc_ensure_dispatchers();
                $id = elephc_ffi_global_write_closure($this->__elephc_context, $name);
                if ($id === 0) {
                    self::__elephc_throw();
                }
                self::__elephc_register_callback_closure($id, $value);
                return;
            }
            if ($value instanceof \FFI\CData) {
                if (!elephc_ffi_global_write_cdata($this->__elephc_context, $name, $value->__elephc_raw())) {
                    self::__elephc_throw();
                }
                return;
            }
            if (is_null($value)) {
                if (!elephc_ffi_global_write_null($this->__elephc_context, $name)) {
                    self::__elephc_throw();
                }
                return;
            }
            throw new \FFI\Exception("Attempt to assign an invalid callback, no array or string given");
        }
        if ($value instanceof \Closure) {
            throw new \FFI\Exception("Attempt to assign an invalid callback, no array or string given");
        }
        $ok = false;
        if ($value instanceof \FFI\CData) {
            $ok = elephc_ffi_global_write_cdata($this->__elephc_context, $name, $value->__elephc_raw());
        } elseif (is_null($value)) {
            $ok = elephc_ffi_global_write_null($this->__elephc_context, $name);
        } elseif (is_bool($value)) {
            $ok = elephc_ffi_global_write_bool($this->__elephc_context, $name, $value);
        } elseif (is_float($value)) {
            $ok = elephc_ffi_global_write_float($this->__elephc_context, $name, $value);
        } elseif (is_int($value)) {
            $ok = elephc_ffi_global_write_int($this->__elephc_context, $name, $value);
        }
        if (!$ok) {
            self::__elephc_throw();
        }
    }
}

namespace FFI {
    class Exception extends \Error {
    }

    final class ParserException extends Exception {
    }

    final class CType {
        public const TYPE_VOID = 0;
        public const TYPE_FLOAT = 1;
        public const TYPE_DOUBLE = 2;
        public const TYPE_LONGDOUBLE = 3;
        public const TYPE_UINT8 = 4;
        public const TYPE_SINT8 = 5;
        public const TYPE_UINT16 = 6;
        public const TYPE_SINT16 = 7;
        public const TYPE_UINT32 = 8;
        public const TYPE_SINT32 = 9;
        public const TYPE_UINT64 = 10;
        public const TYPE_SINT64 = 11;
        public const TYPE_ENUM = 12;
        public const TYPE_BOOL = 13;
        public const TYPE_CHAR = 14;
        public const TYPE_POINTER = 15;
        public const TYPE_FUNC = 16;
        public const TYPE_ARRAY = 17;
        public const TYPE_STRUCT = 18;
        public const ATTR_CONST = 1;
        public const ATTR_INCOMPLETE_TAG = 2;
        public const ATTR_VARIADIC = 4;
        public const ATTR_INCOMPLETE_ARRAY = 8;
        public const ATTR_VLA = 16;
        public const ATTR_UNION = 32;
        public const ATTR_PACKED = 64;
        public const ATTR_MS_STRUCT = 128;
        public const ATTR_GCC_STRUCT = 256;
        public const ABI_DEFAULT = 0;
        public const ABI_CDECL = 1;
        public const ABI_FASTCALL = 2;
        public const ABI_THISCALL = 3;
        public const ABI_STDCALL = 4;
        public const ABI_PASCAL = 5;
        public const ABI_REGISTER = 6;
        public const ABI_MS = 7;
        public const ABI_SYSV = 8;
        public const ABI_VECTORCALL = 9;

        private int $__elephc_handle;
        private string $__elephc_type_name;
        private int $__elephc_index;
        private static int $__elephc_freed_debug_calls = 0;

        private function __construct(int $handle, string $type_name) {
            $this->__elephc_handle = $handle;
            $this->__elephc_type_name = $type_name;
            $this->__elephc_index = 0;
        }

        public static function __elephc_wrap(int $handle): self {
            $ptr = elephc_ffi_type_name($handle);
            $len = elephc_ffi_type_name_len($handle);
            return new self($handle, "FFI\\CType:" . ptr_read_string($ptr, $len));
        }

        public function __elephc_raw(): int {
            return $this->__elephc_handle;
        }

        public function getName(): string {
            $ptr = elephc_ffi_type_name($this->__elephc_handle);
            return ptr_read_string($ptr, elephc_ffi_type_name_len($this->__elephc_handle));
        }

        public function getKind(): int {
            return elephc_ffi_type_kind($this->__elephc_handle);
        }

        public function getSize(): int {
            return elephc_ffi_type_size($this->__elephc_handle);
        }

        public function getAlignment(): int {
            return elephc_ffi_type_align($this->__elephc_handle);
        }

        public function getAttributes(): int {
            return elephc_ffi_type_attributes($this->__elephc_handle);
        }

        public function getEnumKind(): int {
            return elephc_ffi_type_enum_kind($this->__elephc_handle);
        }

        public function getArrayElementType(): CType {
            return self::__elephc_wrap(elephc_ffi_type_array_element($this->__elephc_handle));
        }

        public function getArrayLength(): int {
            return elephc_ffi_type_array_length($this->__elephc_handle);
        }

        public function getPointerType(): CType {
            return self::__elephc_wrap(elephc_ffi_type_pointer_pointee($this->__elephc_handle));
        }

        public function getStructFieldNames(): array {
            $names = [];
            $count = elephc_ffi_type_struct_field_count($this->__elephc_handle);
            for ($index = 0; $index < $count; $index++) {
                $ptr = elephc_ffi_type_struct_field_name($this->__elephc_handle, $index);
                $len = elephc_ffi_type_struct_field_name_len($this->__elephc_handle, $index);
                $names[] = ptr_read_string($ptr, $len);
            }
            return $names;
        }

        public function getStructFieldOffset(string $name): int {
            return elephc_ffi_type_struct_field_offset($this->__elephc_handle, $name);
        }

        public function getStructFieldType(string $name): CType {
            return self::__elephc_wrap(elephc_ffi_type_struct_field_type($this->__elephc_handle, $name));
        }

        public function getFuncABI(): int {
            return elephc_ffi_type_func_abi($this->__elephc_handle);
        }

        public function getFuncReturnType(): CType {
            return self::__elephc_wrap(elephc_ffi_type_func_return($this->__elephc_handle));
        }

        public function getFuncParameterCount(): int {
            return elephc_ffi_type_func_param_count($this->__elephc_handle);
        }

        public function getFuncParameterType(int $index): CType {
            return self::__elephc_wrap(elephc_ffi_type_func_param($this->__elephc_handle, $index));
        }

        public function __debugInfo(): array {
            return [];
        }
    }

    final class CData implements \ArrayAccess, \Countable, \Iterator {
        private int $__elephc_handle;
        private string $__elephc_type_name;
        private int $__elephc_index;
        private bool $__elephc_iter_by_ref;

        private function __construct(int $handle, string $type_name) {
            $this->__elephc_handle = $handle;
            $this->__elephc_type_name = $type_name;
            $this->__elephc_index = 0;
            $this->__elephc_iter_by_ref = false;
        }

        public function __elephc_set_iter_by_ref(bool $by_ref): void {
            $this->__elephc_iter_by_ref = $by_ref;
        }

        public static function __elephc_wrap(int $handle): self {
            $ptr = elephc_ffi_cdata_type_name($handle);
            $len = elephc_ffi_cdata_type_name_len($handle);
            return new self($handle, "FFI\\CData:" . ptr_read_string($ptr, $len));
        }

        public function __elephc_raw(): int {
            return $this->__elephc_handle;
        }

        public function __elephc_cdata(): CData {
            return self::__elephc_wrap(elephc_ffi_field_cdata($this->__elephc_handle, "cdata"));
        }

        private function __destruct() {
            if ($this->__elephc_handle !== 0) {
                elephc_ffi_release_cdata($this->__elephc_handle);
                $this->__elephc_handle = 0;
            }
        }

        public function __clone() {
            $this->__elephc_handle = elephc_ffi_cdata_clone($this->__elephc_handle);
        }

        public function __get(string $name): mixed {
            $kind = elephc_ffi_field_kind($this->__elephc_handle, $name);
            if ($kind === 0) {
                \FFI::__elephc_throw();
            }
            if ($name === "cdata") {
                // php-src returns the scalar payload for a scalar CData and a CData view for
                // aggregate/pointer storage.
                if ($kind === 5) {
                    return self::__elephc_wrap(elephc_ffi_field_cdata($this->__elephc_handle, $name));
                }
                return \__elephc_ffi_field_value($this->__elephc_handle, $name, $kind);
            }
            return \__elephc_ffi_field_value($this->__elephc_handle, $name, $kind);
        }

        public function __set(string $name, mixed $value): void {
            if ($name === "cdata" && $value instanceof \Closure) {
                $cdata_type = elephc_ffi_typeof($this->__elephc_handle);
                $pointee_type = elephc_ffi_type_pointer_pointee($cdata_type);
                if (elephc_ffi_type_kind($cdata_type) === 15 && elephc_ffi_type_kind($pointee_type) === 16) {
                    \FFI::__elephc_ensure_dispatchers();
                    $id = elephc_ffi_cdata_write_closure($this->__elephc_handle);
                    if ($id === 0) {
                        \FFI::__elephc_throw();
                    }
                    \FFI::__elephc_register_callback_closure($id, $value);
                    return;
                }
            }
            $field_type = elephc_ffi_field_pointer_type($this->__elephc_handle, $name);
            if (\FFI::__elephc_is_function_pointer_type($field_type)) {
                if ($value instanceof \Closure) {
                    \FFI::__elephc_ensure_dispatchers();
                    $id = elephc_ffi_field_write_closure($this->__elephc_handle, $name);
                    if ($id === 0) {
                        \FFI::__elephc_throw();
                    }
                    \FFI::__elephc_register_callback_closure($id, $value);
                    return;
                }
                if ($value instanceof CData) {
                    if (!elephc_ffi_field_write_cdata($this->__elephc_handle, $name, $value->__elephc_raw())) {
                        \FFI::__elephc_throw();
                    }
                    return;
                }
                if (is_null($value)) {
                    if (!elephc_ffi_field_write_null($this->__elephc_handle, $name)) {
                        \FFI::__elephc_throw();
                    }
                    return;
                }
                throw new \FFI\Exception("Attempt to assign an invalid callback, no array or string given");
            }
            $kind = elephc_ffi_field_kind($this->__elephc_handle, $name);
            if ($kind === 0) {
                throw new \FFI\Exception("Attempt to assign field '" . $name . "' of non C struct/union");
            }
            $ok = false;
            if ($value instanceof CData) {
                $ok = elephc_ffi_field_write_cdata($this->__elephc_handle, $name, $value->__elephc_raw());
            } elseif (is_null($value)) {
                $ok = elephc_ffi_field_write_null($this->__elephc_handle, $name);
            } elseif (is_bool($value)) {
                $ok = elephc_ffi_field_write_bool($this->__elephc_handle, $name, $value);
            } elseif (is_float($value)) {
                $ok = elephc_ffi_field_write_float($this->__elephc_handle, $name, $value);
            } elseif (is_int($value)) {
                $type = elephc_ffi_field_type($this->__elephc_handle, $name);
                if (elephc_ffi_type_kind($type) === 15) {
                    $ok = elephc_ffi_field_write_pointer($this->__elephc_handle, $name, $value);
                } else {
                    $ok = elephc_ffi_field_write_int($this->__elephc_handle, $name, $value);
                }
            } elseif (is_string($value)) {
                $ok = elephc_ffi_field_write_string($this->__elephc_handle, $name, $value, strlen($value));
            }
            if (!$ok) {
                \FFI::__elephc_throw();
            }
        }

        public function offsetExists(mixed $_offset): bool {
            return true;
        }

        public function offsetGet(mixed $offset): mixed {
            $index = (int) $offset;
            $kind = elephc_ffi_offset_kind($this->__elephc_handle, $index);
            if ($kind === 0) {
                \FFI::__elephc_throw();
            }
            return \__elephc_ffi_offset_value($this->__elephc_handle, $index, $kind);
        }

        public function offsetSet(mixed $offset, mixed $value): void {
            if (is_null($offset)) {
                throw new \FFI\Exception("Cannot add next element to object of type FFI\\CData");
            }
            $index = (int) $offset;
            $offset_type = elephc_ffi_offset_pointer_type($this->__elephc_handle, $index);
            if (\FFI::__elephc_is_function_pointer_type($offset_type)) {
                if ($value instanceof \Closure) {
                    \FFI::__elephc_ensure_dispatchers();
                    $id = elephc_ffi_offset_write_closure($this->__elephc_handle, $index);
                    if ($id === 0) {
                        \FFI::__elephc_throw();
                    }
                    \FFI::__elephc_register_callback_closure($id, $value);
                    return;
                }
                if ($value instanceof CData) {
                    if (!elephc_ffi_offset_write_cdata($this->__elephc_handle, $index, $value->__elephc_raw())) {
                        \FFI::__elephc_throw();
                    }
                    return;
                }
                if (is_null($value)) {
                    if (!elephc_ffi_offset_write_null($this->__elephc_handle, $index)) {
                        \FFI::__elephc_throw();
                    }
                    return;
                }
                throw new \FFI\Exception("Attempt to assign an invalid callback, no array or string given");
            }
            if ($value instanceof \Closure) {
                throw new \FFI\Exception("Attempt to assign an invalid callback, no array or string given");
            }
            $kind = elephc_ffi_offset_kind($this->__elephc_handle, $index);
            if ($kind === 0) {
                \FFI::__elephc_throw();
            }
            $ok = false;
            if ($value instanceof CData) {
                $ok = elephc_ffi_offset_write_cdata($this->__elephc_handle, $index, $value->__elephc_raw());
            } elseif (is_null($value)) {
                $ok = elephc_ffi_offset_write_null($this->__elephc_handle, $index);
            } elseif (is_bool($value)) {
                $ok = elephc_ffi_offset_write_bool($this->__elephc_handle, $index, $value);
            } elseif (is_float($value)) {
                $ok = elephc_ffi_offset_write_float($this->__elephc_handle, $index, $value);
            } elseif (is_int($value)) {
                $type = elephc_ffi_offset_type($this->__elephc_handle, $index);
                if (elephc_ffi_type_kind($type) === 15) {
                    $ok = elephc_ffi_offset_write_pointer($this->__elephc_handle, $index, $value);
                } else {
                    $ok = elephc_ffi_offset_write_int($this->__elephc_handle, $index, $value);
                }
            } elseif (is_string($value)) {
                $ok = elephc_ffi_offset_write_string($this->__elephc_handle, $index, $value, strlen($value));
            }
            if (!$ok) {
                \FFI::__elephc_throw();
            }
        }

        public function offsetUnset(mixed $_offset): void {
            throw new \FFI\Exception("Cannot unset C array element");
        }

        public function count(): int {
            $type = elephc_ffi_typeof($this->__elephc_handle);
            if (elephc_ffi_type_kind($type) !== 17) {
                throw new \FFI\Exception("Attempt to count() on non C array");
            }
            return elephc_ffi_array_length($this->__elephc_handle);
        }

        public function current(): mixed {
            $type = elephc_ffi_typeof($this->__elephc_handle);
            if (elephc_ffi_type_kind($type) !== 17) {
                throw new \FFI\Exception("Attempt to iterate on non C array");
            }
            // php-src converts scalar elements to PHP scalars for a value loop, but keeps a
            // writable CData wrapper for `foreach ($array as &$value)`.
            if ($this->__elephc_iter_by_ref) {
                return self::__elephc_wrap(elephc_ffi_iter_cdata($this->__elephc_handle, $this->__elephc_index));
            }
            $kind = elephc_ffi_offset_kind($this->__elephc_handle, $this->__elephc_index);
            if ($kind === 0) {
                \FFI::__elephc_throw();
            }
            return \__elephc_ffi_offset_value($this->__elephc_handle, $this->__elephc_index, $kind);
        }

        public function key(): int {
            return $this->__elephc_index;
        }

        public function next(): void {
            $this->__elephc_index = $this->__elephc_index + 1;
        }

        public function rewind(): void {
            $this->__elephc_index = 0;
        }

        public function valid(): bool {
            $type = elephc_ffi_typeof($this->__elephc_handle);
            if (elephc_ffi_type_kind($type) !== 17) {
                throw new \FFI\Exception("Attempt to iterate on non C array");
            }
            return $this->__elephc_index < elephc_ffi_array_length($this->__elephc_handle);
        }

        public function __toString(): string {
            $kind = elephc_ffi_field_kind($this->__elephc_handle, "cdata");
            if ($kind === 1) {
                return (string) elephc_ffi_field_int($this->__elephc_handle, "cdata");
            }
            if ($kind === 2) {
                return (string) elephc_ffi_field_float($this->__elephc_handle, "cdata");
            }
            if ($kind === 3) {
                return elephc_ffi_field_bool($this->__elephc_handle, "cdata") ? "1" : "";
            }
            if ($kind === 4) {
                return chr(elephc_ffi_field_char($this->__elephc_handle, "cdata"));
            }
            if ($kind === 6) {
                return "";
            }
            throw new \Error("Object of class FFI\\CData could not be converted to string");
        }

        public function __debugInfo(): mixed {
            if (elephc_ffi_is_freed($this->__elephc_handle)) {
                return $this->__elephc_debug_freed();
            }
            $type = elephc_ffi_typeof($this->__elephc_handle);
            $kind = elephc_ffi_type_kind($type);
            if ($kind === 18) {
                return $this->__elephc_debug_struct();
            }
            if ($kind === 17) {
                return $this->__elephc_debug_array();
            }
            if ($kind === 15) {
                return $this->__elephc_debug_pointer();
            }
            if ($kind === 16) {
                return [];
            }
            return $this->__elephc_debug_scalar();
        }

        private function __elephc_debug_freed(): mixed {
            // php-src's internal get_debug_info handler throws; var_dump has already opened the
            // object block, so the header and empty body are visible before the exception.
            echo "object(" . $this->__elephc_type_name . ")#" . spl_object_id($this) . " (0) {\n}\n";
            throw new \FFI\Exception("Use after free()");
        }

        private function __elephc_debug_struct(): mixed {
            $fields = [];
            $count = elephc_ffi_field_count($this->__elephc_handle);
            for ($index = 0; $index < $count; $index++) {
                $ptr = elephc_ffi_field_name_at($this->__elephc_handle, $index);
                $len = elephc_ffi_field_name_at_len($this->__elephc_handle, $index);
                $name = ptr_read_string($ptr, $len);
                $field_kind = elephc_ffi_field_kind($this->__elephc_handle, $name);
                $value = \__elephc_ffi_field_value($this->__elephc_handle, $name, $field_kind);
                if ($field_kind === 5) {
                    $field_type = elephc_ffi_field_type($this->__elephc_handle, $name);
                    if (elephc_ffi_type_kind($field_type) === 15) {
                        $pointer = elephc_ffi_pointer_value($value->__elephc_raw());
                        if ($pointer < 4096) {
                            $value = sprintf("0x%x", $pointer);
                        }
                    }
                }
                $fields[$name] = $value;
            }
            return $fields;
        }

        private function __elephc_debug_array(): mixed {
            $items = [];
            $count = elephc_ffi_array_length($this->__elephc_handle);
            for ($index = 0; $index < $count; $index++) {
                $element_kind = elephc_ffi_offset_kind($this->__elephc_handle, $index);
                $items[(string) $index] = \__elephc_ffi_offset_value($this->__elephc_handle, $index, $element_kind);
            }
            return $items;
        }

        private function __elephc_debug_pointer(): mixed {
            if (elephc_ffi_is_null($this->__elephc_handle)) {
                return ["0" => null];
            }
            $element_kind = elephc_ffi_offset_kind($this->__elephc_handle, 0);
            $pointer = elephc_ffi_pointer_value($this->__elephc_handle);
            if ($element_kind === 4 && $pointer < 4096) {
                return ["0" => sprintf("0x%x", $pointer)];
            }
            if ($element_kind === 0) {
                return ["0" => self::__elephc_wrap(elephc_ffi_offset_cdata($this->__elephc_handle, 0))];
            }
            return ["0" => \__elephc_ffi_offset_value($this->__elephc_handle, 0, $element_kind)];
        }

        private function __elephc_debug_scalar(): mixed {
            $scalar_kind = elephc_ffi_field_kind($this->__elephc_handle, "cdata");
            return ["cdata" => \__elephc_ffi_field_value($this->__elephc_handle, "cdata", $scalar_kind)];
        }

        public function __serialize(): array {
            throw new \Exception("Serialization of 'FFI\\CData' is not allowed");
        }

        public function __unserialize(array $_data): void {
            throw new \Exception("Unserialization of 'FFI\\CData' is not allowed");
        }

        public static function __elephc_add(mixed $left, mixed $right): CData {
            $left_value = \__elephc_ffi_operand_value($left);
            $left_kind = \__elephc_ffi_operand_kind($left);
            $right_value = \__elephc_ffi_operand_value($right);
            $right_kind = \__elephc_ffi_operand_kind($right);
            $kind = elephc_ffi_binary_kind($left_value, $left_kind, $right_value, $right_kind, 1);
            if ($kind !== 2) {
                \__elephc_ffi_unsupported_operands($left, $right, "+");
            }
            $handle = elephc_ffi_binary_cdata($left_value, $left_kind, $right_value, $right_kind, 1);
            if ($handle === 0) {
                \FFI::__elephc_throw();
            }
            return self::__elephc_wrap($handle);
        }

        public static function __elephc_sub(mixed $left, mixed $right): mixed {
            $left_value = \__elephc_ffi_operand_value($left);
            $left_kind = \__elephc_ffi_operand_kind($left);
            $right_value = \__elephc_ffi_operand_value($right);
            $right_kind = \__elephc_ffi_operand_kind($right);
            $kind = elephc_ffi_binary_kind($left_value, $left_kind, $right_value, $right_kind, 2);
            if ($kind === 2) {
                $handle = elephc_ffi_binary_cdata($left_value, $left_kind, $right_value, $right_kind, 2);
                if ($handle === 0) {
                    \FFI::__elephc_throw();
                }
                return self::__elephc_wrap($handle);
            }
            if ($kind === 1) {
                return elephc_ffi_binary_int($left_value, $left_kind, $right_value, $right_kind, 2);
            }
            \__elephc_ffi_unsupported_operands($left, $right, "-");
            return null;
        }

        public static function __elephc_mul(mixed $left, mixed $right): mixed {
            \__elephc_ffi_unsupported_operands($left, $right, "*", true);
            return null;
        }

        public static function __elephc_div(mixed $left, mixed $right): mixed {
            \__elephc_ffi_unsupported_operands($left, $right, "/");
            return null;
        }

        public static function __elephc_mod(mixed $left, mixed $right): mixed {
            \__elephc_ffi_unsupported_operands($left, $right, "%");
            return null;
        }

        public static function __elephc_pow(mixed $left, mixed $right): mixed {
            \__elephc_ffi_unsupported_operands($left, $right, "**");
            return null;
        }

        public static function __elephc_bitand(mixed $left, mixed $right): mixed {
            \__elephc_ffi_unsupported_operands($left, $right, "&", true);
            return null;
        }

        public static function __elephc_bitor(mixed $left, mixed $right): mixed {
            \__elephc_ffi_unsupported_operands($left, $right, "|", true);
            return null;
        }

        public static function __elephc_bitxor(mixed $left, mixed $right): mixed {
            \__elephc_ffi_unsupported_operands($left, $right, "^", true);
            return null;
        }

        public static function __elephc_shl(mixed $left, mixed $right): mixed {
            \__elephc_ffi_unsupported_operands($left, $right, "<<");
            return null;
        }

        public static function __elephc_shr(mixed $left, mixed $right): mixed {
            \__elephc_ffi_unsupported_operands($left, $right, ">>");
            return null;
        }

        public static function __elephc_compare(mixed $left, mixed $right): int {
            $left_value = \__elephc_ffi_operand_value($left);
            $left_kind = \__elephc_ffi_operand_kind($left);
            $right_value = \__elephc_ffi_operand_value($right);
            $right_kind = \__elephc_ffi_operand_kind($right);
            elephc_ffi_clear_error();
            $result = elephc_ffi_binary_compare($left_value, $left_kind, $right_value, $right_kind);
            if (elephc_ffi_error_kind() !== 0) {
                \FFI::__elephc_throw();
            }
            return $result;
        }

        public function __elephc_bool(): bool {
            return true;
        }

        public function append(mixed $_value): void {
            throw new \FFI\Exception("Cannot add next element to object of type FFI\\CData");
        }

        public function __elephc_invoke_array(array $args): mixed {
            $call = elephc_ffi_call_begin_cdata($this->__elephc_handle);
            if ($call === 0) {
                \FFI::__elephc_throw();
            }
            $index = 0;
            foreach ($args as $arg) {
                \__elephc_ffi_add_arg($call, $index, $arg);
                $index++;
            }
            return \__elephc_ffi_finish_call($call);
        }

        public function __invoke(
            mixed $arg0 = "\0__elephc_ffi_unset__",
            mixed $arg1 = "\0__elephc_ffi_unset__",
            mixed $arg2 = "\0__elephc_ffi_unset__",
            mixed $arg3 = "\0__elephc_ffi_unset__",
            mixed $arg4 = "\0__elephc_ffi_unset__",
            mixed $arg5 = "\0__elephc_ffi_unset__",
            mixed ...$rest
        ): mixed {
            $args = [];
            if ($arg0 !== "\0__elephc_ffi_unset__") {
                $args[] = $arg0;
            }
            if ($arg1 !== "\0__elephc_ffi_unset__") {
                $args[] = $arg1;
            }
            if ($arg2 !== "\0__elephc_ffi_unset__") {
                $args[] = $arg2;
            }
            if ($arg3 !== "\0__elephc_ffi_unset__") {
                $args[] = $arg3;
            }
            if ($arg4 !== "\0__elephc_ffi_unset__") {
                $args[] = $arg4;
            }
            if ($arg5 !== "\0__elephc_ffi_unset__") {
                $args[] = $arg5;
            }
            foreach ($rest as $extra) {
                $args[] = $extra;
            }
            $address = elephc_ffi_cdata_address($this->__elephc_handle);
            if (\FFI::__elephc_has_closure($address)) {
                $closure = \FFI::__elephc_closure($address);
                return $closure(...$args);
            }
            $call = elephc_ffi_call_begin_cdata($this->__elephc_handle);
            if ($call === 0) {
                \FFI::__elephc_throw();
            }
            $index = 0;
            foreach ($args as $arg) {
                \__elephc_ffi_add_arg($call, $index, $arg);
                $index++;
            }
            return \__elephc_ffi_finish_call($call);
        }
    }

}
"###;
