<?php

const SNMP_VERSION_1 = 0;
const SNMP_VERSION_2C = 1;
const SNMP_VERSION_3 = 3;
const SNMP_VALUE_INTEGER = 2;
const SNMP_VALUE_OCTET_STRING = 4;
const SNMP_VALUE_NULL = 5;
const SNMP_VALUE_OBJECT_ID = 6;
const SNMP_VALUE_IPADDRESS = 64;
const SNMP_VALUE_COUNTER = 65;
const SNMP_VALUE_GAUGE = 66;
const SNMP_VALUE_TIMETICKS = 67;
const SNMP_VALUE_OPAQUE = 68;
const SNMP_VALUE_COUNTER64 = 70;
const SNMP_VALUE_NOSUCHOBJECT = 128;
const SNMP_VALUE_NOSUCHINSTANCE = 129;
const SNMP_VALUE_ENDOFMIBVIEW = 130;

class SNMPException extends \Exception
{
}

extern "elephc_snmp" {
    function elephc_snmp_open(int $version, string $host, int $hostLength, string $community, int $communityLength, int $timeoutMs, int $retries): int;
    function elephc_snmp_close(int $session): void;
    function elephc_snmp_request(int $session, string $oid, int $oidLength, int $pdu): int;
    function elephc_snmp_response_count(int $response): int;
    function elephc_snmp_response_error_status(int $response): int;
    function elephc_snmp_response_error_index(int $response): int;
    function elephc_snmp_response_oid(int $response, int $index): ptr;
    function elephc_snmp_response_type(int $response, int $index): int;
    function elephc_snmp_response_value(int $response, int $index): ptr;
    function elephc_snmp_response_free(int $response): void;
    function elephc_snmp_string_len(): int;
}

class SNMP
{
    public const int VERSION_1 = 0;
    public const int VERSION_2C = 1;
    public const int VERSION_3 = 3;

    private int $__session = 0;
    private int $__version = 0;

    public function __construct(int $version, string $hostname, string $community, int $timeout = -1, int $retries = -1)
    {
        if ($version !== self::VERSION_1 && $version !== self::VERSION_2C) {
            throw new \SNMPException("SNMPv3 is not implemented");
        }
        $this->__version = $version;
        $this->__session = __elephc_snmp_open($version, $hostname, $community, $timeout, $retries);
        if ($this->__session < 0) {
            throw new \SNMPException("Unable to open SNMP session");
        }
    }

    public function get(array|string $object_id)
    {
        return __elephc_snmp_session_get($this->__session, $object_id, 0xA0);
    }

    public function getnext(array|string $object_id)
    {
        return __elephc_snmp_session_get($this->__session, $object_id, 0xA1);
    }

    public function walk(array|string $object_id, int $max_repetitions = 0, int $non_repeaters = 0): array|false
    {
        return __elephc_snmp_session_walk($this->__session, $object_id);
    }

    public function set(array|string $object_id, array|string $type, array|string $value): bool
    {
        return false;
    }

    public function close(): bool
    {
        if ($this->__session !== 0) {
            \elephc_snmp_close($this->__session);
            $this->__session = 0;
        }
        return true;
    }

    public function __destruct()
    {
        $this->close();
    }
}

function snmp_get(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1)
{
    return __elephc_snmp_get(SNMP_VERSION_1, $hostname, $community, $object_id, 0xA0, $timeout, $retries);
}

function snmp2_get(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1)
{
    return __elephc_snmp_get(SNMP_VERSION_2C, $hostname, $community, $object_id, 0xA0, $timeout, $retries);
}

function snmp_getnext(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1)
{
    return __elephc_snmp_get(SNMP_VERSION_1, $hostname, $community, $object_id, 0xA1, $timeout, $retries);
}

function snmp2_getnext(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1)
{
    return __elephc_snmp_get(SNMP_VERSION_2C, $hostname, $community, $object_id, 0xA1, $timeout, $retries);
}

function snmp_walk(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1): array|false
{
    return __elephc_snmp_walk(SNMP_VERSION_1, $hostname, $community, $object_id, $timeout, $retries);
}

function snmp2_walk(string $hostname, string $community, array|string $object_id, int $timeout = -1, int $retries = -1): array|false
{
    return __elephc_snmp_walk(SNMP_VERSION_2C, $hostname, $community, $object_id, $timeout, $retries);
}

function __elephc_snmp_text(ptr $pointer): string|false
{
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    return \__elephc_ptr_read_string($pointer, \elephc_snmp_string_len());
}

function __elephc_snmp_open(int $version, string $hostname, string $community, int $timeout, int $retries): int
{
    $timeout_us = $timeout < 0 ? 1000000 : $timeout;
    $timeout_ms = \intdiv($timeout_us, 1000);
    if ($timeout_ms < 1) {
        $timeout_ms = 1;
    }
    return \elephc_snmp_open($version, $hostname, \strlen($hostname), $community, \strlen($community), $timeout_ms, $retries);
}

function __elephc_snmp_value(int $response, int $index)
{
    $type = \elephc_snmp_response_type($response, $index);
    $pointer = \elephc_snmp_response_value($response, $index);
    if (\__elephc_ptr_is_null($pointer)) {
        return false;
    }
    $text = \__elephc_ptr_read_string($pointer, \elephc_snmp_string_len());
    if ($type === SNMP_VALUE_OCTET_STRING || $type === SNMP_VALUE_OPAQUE) {
        $binary = @\hex2bin($text);
        return $binary === false ? $text : $binary;
    }
    if ($type === SNMP_VALUE_INTEGER || $type === SNMP_VALUE_COUNTER
        || $type === SNMP_VALUE_GAUGE || $type === SNMP_VALUE_TIMETICKS
        || $type === SNMP_VALUE_COUNTER64) {
        return (int) $text;
    }
    if ($type === SNMP_VALUE_OBJECT_ID || $type === SNMP_VALUE_IPADDRESS) {
        return $text;
    }
    return null;
}

function __elephc_snmp_session_get(int $session, array|string $object_id, int $pdu)
{
    if (\is_array($object_id)) {
        $values = [];
        foreach ($object_id as $oid) {
            $entry = __elephc_snmp_session_get($session, $oid, $pdu);
            if ($entry === false) {
                return false;
            }
            $values[] = $entry;
        }
        return $values;
    }
    $response = \elephc_snmp_request($session, $object_id, \strlen($object_id), $pdu);
    if ($response < 0) {
        return false;
    }
    if (\elephc_snmp_response_error_status($response) !== 0
        || \elephc_snmp_response_count($response) < 1) {
        \elephc_snmp_response_free($response);
        return false;
    }
    $value = __elephc_snmp_value($response, 0);
    \elephc_snmp_response_free($response);
    return $value;
}

function __elephc_snmp_get(int $version, string $hostname, string $community, array|string $object_id, int $pdu, int $timeout, int $retries)
{
    $session = __elephc_snmp_open($version, $hostname, $community, $timeout, $retries);
    if ($session < 0) {
        return false;
    }
    $value = __elephc_snmp_session_get($session, $object_id, $pdu);
    \elephc_snmp_close($session);
    return $value;
}

function __elephc_snmp_walk(int $version, string $hostname, string $community, array|string $object_id, int $timeout, int $retries): array|false
{
    $root = "." . \ltrim($object_id, ".");
    $session = __elephc_snmp_open($version, $hostname, $community, $timeout, $retries);
    if ($session < 0) {
        return false;
    }
    $result = [];
    $current = $root;
    for ($i = 0; $i < 4096; $i++) {
        $response = \elephc_snmp_request($session, $current, \strlen($current), 0xA1);
        if ($response < 0) {
            break;
        }
        if (\elephc_snmp_response_error_status($response) !== 0
            || \elephc_snmp_response_count($response) < 1) {
            \elephc_snmp_response_free($response);
            break;
        }
        $oid = __elephc_snmp_text(\elephc_snmp_response_oid($response, 0));
        if ($oid === false) {
            \elephc_snmp_response_free($response);
            break;
        }
        if (\strpos($oid, $root) !== 0 || \array_key_exists($oid, $result)) {
            \elephc_snmp_response_free($response);
            break;
        }
        $value = __elephc_snmp_value($response, 0);
        \elephc_snmp_response_free($response);
        $result[$oid] = $value;
        $current = $oid;
    }
    \elephc_snmp_close($session);
    return $result;
}

function __elephc_snmp_session_walk(int $session, array|string $object_id): array|false
{
    $root = "." . \ltrim($object_id, ".");
    $result = [];
    $current = $root;
    for ($i = 0; $i < 4096; $i++) {
        $response = \elephc_snmp_request($session, $current, \strlen($current), 0xA1);
        if ($response < 0) {
            break;
        }
        if (\elephc_snmp_response_error_status($response) !== 0
            || \elephc_snmp_response_count($response) < 1) {
            \elephc_snmp_response_free($response);
            break;
        }
        $oid = __elephc_snmp_text(\elephc_snmp_response_oid($response, 0));
        if ($oid === false) {
            \elephc_snmp_response_free($response);
            break;
        }
        if (\strpos($oid, $root) !== 0 || \array_key_exists($oid, $result)) {
            \elephc_snmp_response_free($response);
            break;
        }
        $value = __elephc_snmp_value($response, 0);
        \elephc_snmp_response_free($response);
        $result[$oid] = $value;
        $current = $oid;
    }
    return $result;
}
