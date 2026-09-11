<?php
// Capture the mbstring API and encoding catalog from the project's PHP baseline.
// Run: php scripts/mbstring/capture_surface.php > scripts/mbstring/php_surface.json

if (!extension_loaded('mbstring')) {
    fwrite(STDERR, "The mbstring extension is required.\n");
    exit(1);
}

$extension = new ReflectionExtension('mbstring');
$functions = [];
foreach ($extension->getFunctions() as $name => $function) {
    $parameters = [];
    foreach ($function->getParameters() as $parameter) {
        $hasDefault = $parameter->isDefaultValueAvailable();
        $parameters[] = [
            'name' => $parameter->getName(),
            'type' => (string) $parameter->getType(),
            'reference' => $parameter->isPassedByReference(),
            'variadic' => $parameter->isVariadic(),
            'optional' => $parameter->isOptional(),
            'has_default' => $hasDefault,
            'default' => $hasDefault ? $parameter->getDefaultValue() : null,
        ];
    }
    $functions[$name] = [
        'parameters' => $parameters,
        'returns' => (string) $function->getReturnType(),
    ];
}
ksort($functions);
$encodings = [];
foreach (mb_list_encodings() as $encoding) {
    // The legacy transfer encodings are catalogued even though PHP deprecates them.
    try { @mb_ord('A', $encoding); $ord = true; } catch (ValueError) { $ord = false; }
    try { @mb_chr(65, $encoding); $chr = true; } catch (ValueError) { $chr = false; }
    if ($ord !== $chr) { throw new RuntimeException("Different ord/chr support for $encoding"); }
    $encodings[$encoding] = [
        'aliases' => @mb_encoding_aliases($encoding),
        'mime' => @mb_preferred_mime_name($encoding),
        'supports_ord_chr' => $ord,
        'supports_detection' => @mb_detect_encoding('', [$encoding]) !== false,
    ];
}
echo json_encode([
    'php_version' => PHP_VERSION,
    'functions' => $functions,
    'constants' => $extension->getConstants(),
    'encoding_order' => array_keys($encodings),
    'encodings' => $encodings,
], JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR), "\n";
