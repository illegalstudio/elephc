<?php
// Verifies one value against a DECLARED scalar return contract, like php-src's
// `zend_verify_scalar_type_hint()`, and returns the value the caller must see.
// `$mask` bits: 1 int, 2 float, 4 string, 8 bool, 16 false, 32 true, 64 null.
// An exact member always passes and strict mode only widens int to float; coercive mode tries
// int, then float, then string, then bool, in php-src's preference order.
function __elephc_return_coerce(mixed $value, int $mask, bool $strict, string $prefix): mixed
{
    if (is_int($value)) {
        if (($mask & 1) !== 0) {
            return $value;
        }
        if (($mask & 2) !== 0) {
            return (float) $value;
        }
        if (!$strict) {
            if (($mask & 4) !== 0) {
                return (string) $value;
            }
            if (($mask & 8) !== 0) {
                return $value !== 0;
            }
        }
    } elseif (is_float($value)) {
        if (($mask & 2) !== 0) {
            return $value;
        }
        if (!$strict) {
            if (($mask & 1) !== 0 && __elephc_return_float_fits_int($value)) {
                return (int) $value;
            }
            if (($mask & 4) !== 0) {
                return (string) $value;
            }
            if (($mask & 8) !== 0) {
                return (bool) $value;
            }
        }
    } elseif (is_string($value)) {
        if (($mask & 4) !== 0) {
            return $value;
        }
        if (!$strict) {
            if (($mask & 3) !== 0 && is_numeric($value)) {
                // `+ 0` is php-src's `is_numeric_string()` split: int for an integer string that
                // fits, float for everything else.
                $number = $value + 0;
                if (is_int($number)) {
                    if (($mask & 1) !== 0) {
                        return $number;
                    }
                    return (float) $number;
                }
                if (($mask & 2) !== 0) {
                    // `(float)` keeps the spelling's sign: `"-0.0" + 0` is `+0.0` in IEEE.
                    return (float) $value;
                }
                if (__elephc_return_float_fits_int($number)) {
                    return (int) $number;
                }
            }
            if (($mask & 8) !== 0) {
                return $value !== "" && $value !== "0";
            }
        }
    } elseif (is_bool($value)) {
        if (($mask & 8) !== 0 || ($value && ($mask & 32) !== 0) || (!$value && ($mask & 16) !== 0)) {
            return $value;
        }
        if (!$strict) {
            if (($mask & 1) !== 0) {
                return $value ? 1 : 0;
            }
            if (($mask & 2) !== 0) {
                return $value ? 1.0 : 0.0;
            }
            if (($mask & 4) !== 0) {
                return $value ? "1" : "";
            }
        }
    } elseif ($value === null) {
        if (($mask & 64) !== 0) {
            return null;
        }
    }
    if (is_object($value)) {
        // php-src `zend_parse_arg_str_weak()`: coercive mode converts an object with
        // `__toString()` when the declaration accepts a string.
        if (!$strict && ($mask & 4) !== 0 && $value instanceof Stringable) {
            // A throwing `__toString()` surfaces as the return `TypeError`, with the original
            // exception chained as its previous (php-src's `zend_verify_return_error`).
            try {
                return (string) $value;
            } catch (Throwable $error) {
                throw new TypeError($prefix . get_class($value) . " returned", 0, $error);
            }
        }
        throw new TypeError($prefix . get_class($value) . " returned");
    }
    throw new TypeError($prefix . __elephc_return_value_name($value) . " returned");
}

// php-src `ZEND_DOUBLE_FITS_LONG()`: ordered (not NaN) and inside [-2^63, 2^63).
function __elephc_return_float_fits_int(float $value): bool
{
    return $value >= -9.2233720368547758E18 && $value < 9.2233720368547758E18;
}

// php-src `zend_zval_value_name()` for a non-object: a boolean prints as its value.
function __elephc_return_value_name(mixed $value): string
{
    if (is_bool($value)) {
        return $value ? "true" : "false";
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
    if ($value === null) {
        return "null";
    }
    if (is_array($value)) {
        return "array";
    }
    return "resource";
}
