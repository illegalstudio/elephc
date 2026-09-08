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

class DisplaySettings {
    public string $theme = 'dark';
    public mixed $palette = ['accent' => ['blue', 'white']];
    public string $label { get => 'Theme: ' . $this->theme; }
    public function reset(): void { $this->theme = 'dark'; }
}

echo 'Stored defaults: ', implode(', ', array_keys(get_class_vars(DisplaySettings::class))), "\n";
echo 'Public methods: ', implode(', ', get_class_methods(DisplaySettings::class)), "\n";

// Introspection also accepts an object returned through the boxed eval boundary.
$source = 'return new DisplaySettings();' . ' // ' . $argc;
$settings = eval($source);
echo 'Runtime object methods: ', implode(', ', get_class_methods($settings)), "\n";
echo 'Default accent: ', $settings->palette['accent'][0], "\n";

// A customized copy of a nested default leaves the original settings unchanged.
$nativeSettings = new DisplaySettings();
$customPalette = $nativeSettings->palette;
$customPalette['accent'][0] = 'green';
echo 'Custom accent: ', $customPalette['accent'][0], "\n";
echo 'Original accent: ', $nativeSettings->palette['accent'][0], "\n";
