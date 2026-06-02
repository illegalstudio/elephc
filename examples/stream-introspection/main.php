<?php
// Resource and stream introspection: examining open file handles,
// what kind of resource they are, and which stream features they support.

$handle = fopen("notes.txt", "w");
fwrite($handle, "elephc stream introspection\n");

// is_resource() tells an open handle apart from a plain value.
echo "is_resource(handle): " . (is_resource($handle) ? "yes" : "no") . "\n";
echo "is_resource(42):     " . (is_resource(42) ? "yes" : "no") . "\n";

// Every handle has a kind and a numeric id.
echo "resource type: " . get_resource_type($handle) . "\n";
echo "resource id:   " . get_resource_id($handle) . "\n";

// Capability probes for the open stream.
echo "is a terminal: " . (stream_isatty($handle) ? "yes" : "no") . "\n";
echo "is local:      " . (stream_is_local($handle) ? "yes" : "no") . "\n";
echo "supports lock: " . (stream_supports_lock($handle) ? "yes" : "no") . "\n";

fclose($handle);
unlink("notes.txt");

// Registries: which wrappers, transports and filters are available.
echo "wrappers:\n";
foreach (stream_get_wrappers() as $wrapper) {
    echo "  - " . $wrapper . "\n";
}
echo "transports available: " . count(stream_get_transports()) . "\n";
echo "filters available:    " . count(stream_get_filters()) . "\n";
