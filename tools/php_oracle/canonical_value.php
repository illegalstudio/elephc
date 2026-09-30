<?php

declare(strict_types=1);

/**
 * Encode arbitrary PHP values without losing binary strings, float bits, or array order.
 *
 * @return array<string, mixed>
 */
function elephc_oracle_canonical_value(mixed $value): array
{
    if ($value === null) {
        return ['type' => 'null'];
    }
    if (is_bool($value)) {
        return ['type' => 'bool', 'value' => $value];
    }
    if (is_int($value)) {
        return ['type' => 'int', 'decimal' => (string) $value];
    }
    if (is_float($value)) {
        return ['type' => 'float', 'ieee754_be_hex' => bin2hex(pack('E', $value))];
    }
    if (is_string($value)) {
        return [
            'type' => 'string',
            'base64' => base64_encode($value),
            'length' => strlen($value),
            'sha256' => hash('sha256', $value),
        ];
    }
    if (is_array($value)) {
        $entries = [];
        foreach ($value as $key => $entry) {
            $entries[] = [
                'key' => is_int($key)
                    ? ['type' => 'int', 'decimal' => (string) $key]
                    : elephc_oracle_canonical_value($key),
                'value' => elephc_oracle_canonical_value($entry),
            ];
        }
        return ['type' => 'array', 'entries' => $entries];
    }
    if (is_resource($value)) {
        return ['type' => 'resource', 'resource_type' => get_resource_type($value)];
    }
    if (is_object($value)) {
        $serialized = serialize($value);
        return [
            'type' => 'object',
            'class' => get_class($value),
            'serialized' => [
                'base64' => base64_encode($serialized),
                'length' => strlen($serialized),
                'sha256' => hash('sha256', $serialized),
            ],
        ];
    }

    return ['type' => get_debug_type($value)];
}
