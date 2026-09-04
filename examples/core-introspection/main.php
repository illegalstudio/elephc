<?php

function inspect_request(string $label, mixed ...$details): void
{
    $trace = debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 1);
    $functions = get_defined_functions();

    echo "Frame: {$trace[0]['function']} at line {$trace[0]['line']}\n";
    echo "Label: {$label}, details: ", count($details), "\n";
    echo "Core strlen available: ", in_array('strlen', $functions['internal']) ? 'yes' : 'no', "\n";
    echo "Included files: ", count(get_included_files()), "\n";
    echo "GC enabled: ", gc_enabled() ? 'yes' : 'no', "\n";
}

inspect_request('core', 7, 'ready');
