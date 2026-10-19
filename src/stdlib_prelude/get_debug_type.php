<?php
function get_debug_type(mixed $value) {
    if ($value === null) { return "null"; }
    if (is_bool($value)) { return "bool"; }
    if (is_int($value)) { return "int"; }
    if (is_float($value)) { return "float"; }
    if (is_string($value)) { return "string"; }
    if (is_array($value)) { return "array"; }
    if (is_object($value)) {
        $name = get_class($value);
        if (strpos($name, "@anonymous") === false) {
            return $name;
        }
        $parent = get_parent_class($value);
        if ($parent !== false && $parent !== "") {
            return $parent . "@anonymous";
        }
        foreach (get_declared_interfaces() as $iface) {
            if ($iface !== "" && is_a($value, $iface)) {
                return $iface . "@anonymous";
            }
        }
        return "class@anonymous";
    }
    if (is_resource($value)) {
        $t = get_resource_type($value);
        if ($t === "Unknown") { return "resource (closed)"; }
        return "resource (" . $t . ")";
    }
    return "mixed";
}
