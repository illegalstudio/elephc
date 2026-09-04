<?php

declare(strict_types=1);

/**
 * Render a reflection type without losing union, intersection, or nullable syntax.
 */
function stream_manifest_type(?ReflectionType $type): ?string
{
    return $type === null ? null : (string) $type;
}

/**
 * Encode a PHP value with an explicit type so constants such as false and zero differ.
 *
 * @return array{type: string, value: mixed}
 */
function stream_manifest_typed_value(mixed $value): array
{
    if (is_float($value)) {
        return [
            'type' => 'float',
            'ieee754_be_hex' => bin2hex(pack('E', $value)),
        ];
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
                'key' => stream_manifest_typed_value($key),
                'value' => stream_manifest_typed_value($entry),
            ];
        }
        return ['type' => 'array', 'entries' => $entries];
    }

    return ['type' => get_debug_type($value), 'value' => $value];
}

/**
 * Describe one reflected parameter, including named-argument and reference semantics.
 *
 * @return array<string, mixed>
 */
function stream_manifest_parameter(ReflectionParameter $parameter): array
{
    $defaultAvailable = $parameter->isDefaultValueAvailable();
    $defaultState = $defaultAvailable
        ? 'available'
        : ($parameter->isOptional() && !$parameter->isVariadic()
            ? 'unknown_optional'
            : 'none');
    $default = null;
    $defaultConstant = null;
    if ($defaultAvailable) {
        $default = stream_manifest_typed_value($parameter->getDefaultValue());
        if ($parameter->isDefaultValueConstant()) {
            $defaultConstant = $parameter->getDefaultValueConstantName();
        }
    }

    return [
        'position' => $parameter->getPosition(),
        'name' => $parameter->getName(),
        'type' => stream_manifest_type($parameter->getType()),
        'allows_null' => $parameter->allowsNull(),
        'by_reference' => $parameter->isPassedByReference(),
        'can_be_passed_by_value' => $parameter->canBePassedByValue(),
        'variadic' => $parameter->isVariadic(),
        'optional' => $parameter->isOptional(),
        'default_state' => $defaultState,
        'default_available' => $defaultAvailable,
        'default' => $default,
        'default_constant' => $defaultConstant,
    ];
}

/**
 * Describe one internal function selected by the source-reachability manifest.
 *
 * @return array<string, mixed>
 */
function stream_manifest_function(string $name, ?string $aliasOf): array
{
    $reflection = new ReflectionFunction($name);

    return [
        'name' => $reflection->getName(),
        'canonical_name' => strtolower($reflection->getName()),
        'alias_of' => $aliasOf,
        'extension' => $reflection->getExtensionName(),
        'deprecated' => $reflection->isDeprecated(),
        'returns_reference' => $reflection->returnsReference(),
        'return_type' => stream_manifest_type($reflection->getReturnType()),
        'tentative_return_type' => method_exists($reflection, 'getTentativeReturnType')
            ? stream_manifest_type($reflection->getTentativeReturnType())
            : null,
        'required_parameter_count' => $reflection->getNumberOfRequiredParameters(),
        'parameter_count' => $reflection->getNumberOfParameters(),
        'parameters' => array_map(
            stream_manifest_parameter(...),
            $reflection->getParameters(),
        ),
    ];
}

/**
 * Describe one reflected class constant.
 *
 * @return array<string, mixed>
 */
function stream_manifest_class_constant(ReflectionClassConstant $constant): array
{
    return [
        'name' => $constant->getName(),
        'declaring_class' => $constant->getDeclaringClass()->getName(),
        'visibility' => $constant->isPublic()
            ? 'public'
            : ($constant->isProtected() ? 'protected' : 'private'),
        'final' => $constant->isFinal(),
        'deprecated' => $constant->isDeprecated(),
        'value' => stream_manifest_typed_value($constant->getValue()),
    ];
}

/**
 * Describe one reflected property, including visibility and declared default.
 *
 * @return array<string, mixed>
 */
function stream_manifest_property(ReflectionProperty $property): array
{
    $name = $property->getName();
    $defaultAvailable = $property->hasDefaultValue();

    return [
        'name' => $name,
        'declaring_class' => $property->getDeclaringClass()->getName(),
        'visibility' => $property->isPublic()
            ? 'public'
            : ($property->isProtected() ? 'protected' : 'private'),
        'set_visibility' => $property->isPrivateSet()
            ? 'private'
            : ($property->isProtectedSet() ? 'protected' : 'public'),
        'static' => $property->isStatic(),
        'readonly' => $property->isReadOnly(),
        'final' => $property->isFinal(),
        'virtual' => $property->isVirtual(),
        'has_hooks' => $property->hasHooks(),
        'type' => stream_manifest_type($property->getType()),
        'settable_type' => stream_manifest_type($property->getSettableType()),
        'default_available' => $defaultAvailable,
        'default' => $defaultAvailable
            ? stream_manifest_typed_value($property->getDefaultValue())
            : null,
    ];
}

/**
 * Describe one reflected method with its complete caller-visible signature.
 *
 * @param ?string $aliasOf Canonical php-src alias target, when applicable.
 * @return array<string, mixed>
 */
function stream_manifest_method(ReflectionMethod $method, ?string $aliasOf): array
{
    return [
        'name' => $method->getName(),
        'canonical_name' => strtolower($method->getName()),
        'alias_of' => $aliasOf,
        'declaring_class' => $method->getDeclaringClass()->getName(),
        'visibility' => $method->isPublic()
            ? 'public'
            : ($method->isProtected() ? 'protected' : 'private'),
        'static' => $method->isStatic(),
        'final' => $method->isFinal(),
        'abstract' => $method->isAbstract(),
        'deprecated' => $method->isDeprecated(),
        'returns_reference' => $method->returnsReference(),
        'return_type' => stream_manifest_type($method->getReturnType()),
        'tentative_return_type' => stream_manifest_type($method->getTentativeReturnType()),
        'required_parameter_count' => $method->getNumberOfRequiredParameters(),
        'parameter_count' => $method->getNumberOfParameters(),
        'parameters' => array_map(
            stream_manifest_parameter(...),
            $method->getParameters(),
        ),
    ];
}

/**
 * Describe one stream-facing class selected by the frozen source manifest.
 *
 * @param array<string, string> $methodAliases Lowercase method to canonical alias target.
 * @return array<string, mixed>
 */
function stream_manifest_class(string $name, array $methodAliases): array
{
    $reflection = new ReflectionClass($name);
    $interfaces = $reflection->getInterfaceNames();
    $traits = $reflection->getTraitNames();
    sort($interfaces, SORT_STRING);
    sort($traits, SORT_STRING);

    $constants = array_map(
        stream_manifest_class_constant(...),
        $reflection->getReflectionConstants(),
    );
    usort($constants, static fn (array $left, array $right): int => $left['name'] <=> $right['name']);

    $properties = array_map(
        stream_manifest_property(...),
        $reflection->getProperties(),
    );
    usort($properties, static fn (array $left, array $right): int => $left['name'] <=> $right['name']);

    $methods = array_map(
        static fn (ReflectionMethod $method): array => stream_manifest_method(
            $method,
            $methodAliases[strtolower($method->getName())] ?? null,
        ),
        $reflection->getMethods(),
    );
    usort(
        $methods,
        static fn (array $left, array $right): int => $left['canonical_name'] <=> $right['canonical_name'],
    );

    return [
        'name' => $reflection->getName(),
        'canonical_name' => strtolower($reflection->getName()),
        'extension' => $reflection->getExtensionName(),
        'parent' => ($parent = $reflection->getParentClass()) ? $parent->getName() : null,
        'interfaces' => $interfaces,
        'traits' => $traits,
        'final' => $reflection->isFinal(),
        'abstract' => $reflection->isAbstract(),
        'readonly' => $reflection->isReadOnly(),
        'internal' => $reflection->isInternal(),
        'instantiable' => $reflection->isInstantiable(),
        'interface' => $reflection->isInterface(),
        'trait' => $reflection->isTrait(),
        'enum' => $reflection->isEnum(),
        'anonymous' => $reflection->isAnonymous(),
        'constants' => $constants,
        'properties' => $properties,
        'methods' => $methods,
    ];
}

/**
 * Return stream-facing constants with values and types.
 *
 * @return array<string, array{type: string, value: mixed}>
 */
function stream_manifest_constants(): array
{
    $constants = [];
    foreach (get_defined_constants() as $name => $value) {
        if (!preg_match('/^(?:STREAM_|PSFS_|FILE_|LOCK_|SEEK_|GLOB_)/', $name)) {
            continue;
        }
        $constants[$name] = stream_manifest_typed_value($value);
    }
    ksort($constants, SORT_STRING);

    return $constants;
}

/**
 * Read an optional source-reachability input and select runtime-visible entries.
 *
 * @return array{functions: list<array<string, mixed>>, classes: list<array<string, mixed>>}
 */
function stream_manifest_reachable_surface(): array
{
    $path = getenv('ELEPHC_STREAM_REACHABILITY');
    if ($path === false || $path === '') {
        return ['functions' => [], 'classes' => []];
    }

    $source = json_decode((string) file_get_contents($path), true, flags: JSON_THROW_ON_ERROR);
    $functions = [];
    foreach ($source['functions'] as $entry) {
        if (!function_exists($entry['name'])) {
            throw new RuntimeException(
                sprintf('reachable function is not exposed by this build: %s', $entry['name']),
            );
        }
        $functions[] = stream_manifest_function($entry['name'], $entry['alias_of']);
    }
    usort(
        $functions,
        static fn (array $left, array $right): int => $left['canonical_name'] <=> $right['canonical_name'],
    );

    $classes = [];
    foreach ($source['classes'] as $entry) {
        if (!class_exists($entry['name'])) {
            throw new RuntimeException(
                sprintf('reachable class is not exposed by this build: %s', $entry['name']),
            );
        }
        $methodAliases = [];
        foreach ($entry['methods'] as $method) {
            if ($method['alias_of'] !== null) {
                $methodAliases[strtolower($method['name'])] = $method['alias_of'];
            }
        }
        $classes[] = stream_manifest_class($entry['name'], $methodAliases);
    }
    usort(
        $classes,
        static fn (array $left, array $right): int => $left['canonical_name'] <=> $right['canonical_name'],
    );

    return ['functions' => $functions, 'classes' => $classes];
}

$extensions = [];
foreach (get_loaded_extensions() as $extension) {
    $extensions[] = [
        'name' => $extension,
        'version' => phpversion($extension) ?: null,
    ];
}

$reachable = stream_manifest_reachable_surface();
$constants = stream_manifest_constants();
$cryptoMethods = [];
foreach ($constants as $name => $value) {
    if (str_starts_with($name, 'STREAM_CRYPTO_METHOD_')) {
        $cryptoMethods[] = ['name' => $name, ...$value];
    }
}

$result = [
    'runtime' => [
        'php_version' => PHP_VERSION,
        'php_version_id' => PHP_VERSION_ID,
        'php_binary' => PHP_BINARY,
        'php_sapi' => PHP_SAPI,
        'zend_version' => zend_version(),
        'os_family' => PHP_OS_FAMILY,
        'os' => PHP_OS,
        'uname_machine' => php_uname('m'),
        'integer_size' => PHP_INT_SIZE,
        'zts' => PHP_ZTS,
        'debug' => PHP_DEBUG,
        'loaded_ini' => php_ini_loaded_file() ?: null,
        'scanned_ini' => php_ini_scanned_files() ?: null,
        'locale' => setlocale(LC_ALL, '0'),
        'timezone' => date_default_timezone_get(),
        'extensions' => $extensions,
    ],
    'surface' => [
        'functions' => $reachable['functions'],
        'constants' => $constants,
        'classes' => $reachable['classes'],
        'wrappers' => stream_get_wrappers(),
        'transports' => stream_get_transports(),
        'filters' => stream_get_filters(),
        'crypto_methods' => $cryptoMethods,
    ],
];

echo json_encode(
    $result,
    JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR,
), "\n";
