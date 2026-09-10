<?php

declare(strict_types=1);

/**
 * Extracts php-src's semantic `@readonly` virtual-property declarations.
 *
 * Reflection intentionally reports these handler-backed properties as virtual
 * but not language-level readonly, so the stub remains the authoritative source
 * for whether a native property write operation exists.
 */
function snapshot_stub_readonly_properties(string $sourceRoot): array
{
    $lock = json_decode(
        file_get_contents(__DIR__ . '/source-lock.json'),
        true,
        flags: JSON_THROW_ON_ERROR,
    );
    $readonly = [];
    foreach ($lock['php']['stubs'] as $relativePath => $metadata) {
        $path = rtrim($sourceRoot, DIRECTORY_SEPARATOR)
            . DIRECTORY_SEPARATOR
            . $relativePath;
        if (!is_file($path) || hash_file('sha256', $path) !== $metadata['sha256']) {
            throw new RuntimeException("stub does not match source lock: {$relativePath}");
        }

        $namespace = '';
        $class = null;
        $docComment = '';
        $collectingComment = false;
        foreach (file($path, FILE_IGNORE_NEW_LINES) as $line) {
            if (preg_match(
                '/^\s*namespace(?:\s+([A-Za-z_\\\\][A-Za-z0-9_\\\\]*))?\s*$/',
                $line,
                $match,
            )) {
                $namespace = isset($match[1]) ? $match[1] . '\\' : '';
            }
            if (preg_match(
                '/^\s*(?:(?:abstract|final|readonly)\s+)*'
                    . '(?:class|interface)\s+([A-Za-z_][A-Za-z0-9_]*)/',
                $line,
                $match,
            )) {
                $class = $namespace . $match[1];
                $docComment = '';
            }

            if (str_contains($line, '/**')) {
                $collectingComment = true;
                $docComment = $line;
            } elseif ($collectingComment) {
                $docComment .= "\n" . $line;
            }
            if ($collectingComment && str_contains($line, '*/')) {
                $collectingComment = false;
            }

            if ($class !== null && preg_match(
                '/^\s*public\s+(?:readonly\s+)?[^$;]+\$([A-Za-z_][A-Za-z0-9_]*)\s*;/',
                $line,
                $match,
            )) {
                if (str_contains($docComment, '@readonly')) {
                    $readonly[strtolower($class) . '::$' . $match[1]] = true;
                }
                $docComment = '';
            }
        }
    }
    if (count($readonly) !== 135) {
        throw new RuntimeException(sprintf(
            'expected 135 semantic readonly properties from pinned stubs, got %d',
            count($readonly),
        ));
    }
    return $readonly;
}

/**
 * Converts Reflection values into deterministic JSON-compatible records.
 */
function snapshot_value(mixed $value): mixed
{
    if ($value instanceof BackedEnum) {
        return [
            'kind' => 'backed-enum',
            'class' => $value::class,
            'case' => $value->name,
            'value' => $value->value,
        ];
    }
    if ($value instanceof UnitEnum) {
        return [
            'kind' => 'unit-enum',
            'class' => $value::class,
            'case' => $value->name,
        ];
    }
    if (is_object($value)) {
        return ['kind' => 'object', 'class' => $value::class];
    }
    if (is_resource($value)) {
        return ['kind' => 'resource', 'type' => get_resource_type($value)];
    }
    if (is_float($value) && !is_finite($value)) {
        return [
            'kind' => 'float',
            'value' => is_nan($value) ? 'NAN' : ($value > 0 ? 'INF' : '-INF'),
        ];
    }
    return $value;
}

/**
 * Serializes Reflection attributes without depending on object IDs.
 */
function snapshot_attributes(Reflector $reflector): array
{
    $result = [];
    foreach ($reflector->getAttributes() as $attribute) {
        $arguments = [];
        foreach ($attribute->getArguments() as $key => $value) {
            $arguments[$key] = snapshot_value($value);
        }
        $result[] = ['name' => $attribute->getName(), 'arguments' => $arguments];
    }
    return $result;
}

/**
 * Serializes a Reflection type exactly as PHP renders it.
 */
function snapshot_type(?ReflectionType $type): ?string
{
    return $type === null ? null : (string) $type;
}

/**
 * Serializes a function or method parameter.
 */
function snapshot_parameter(ReflectionParameter $parameter): array
{
    $default = null;
    if ($parameter->isDefaultValueAvailable()) {
        $default = $parameter->isDefaultValueConstant()
            ? ['kind' => 'constant', 'name' => $parameter->getDefaultValueConstantName()]
            : ['kind' => 'value', 'value' => snapshot_value($parameter->getDefaultValue())];
    }

    return [
        'name' => $parameter->getName(),
        'position' => $parameter->getPosition(),
        'type' => snapshot_type($parameter->getType()),
        'optional' => $parameter->isOptional(),
        'variadic' => $parameter->isVariadic(),
        'by_reference' => $parameter->isPassedByReference(),
        'can_be_passed_by_value' => $parameter->canBePassedByValue(),
        'allows_null' => $parameter->allowsNull(),
        'default' => $default,
        'attributes' => snapshot_attributes($parameter),
    ];
}

/**
 * Serializes shared ReflectionFunctionAbstract metadata.
 */
function snapshot_callable(ReflectionFunctionAbstract $function): array
{
    $parameters = [];
    foreach ($function->getParameters() as $parameter) {
        $parameters[] = snapshot_parameter($parameter);
    }

    return [
        'name' => $function->getName(),
        'internal' => $function->isInternal(),
        'deprecated' => $function->isDeprecated(),
        'returns_reference' => $function->returnsReference(),
        'required_parameters' => $function->getNumberOfRequiredParameters(),
        'parameters' => $parameters,
        'return_type' => snapshot_type($function->getReturnType()),
        'tentative_return_type' => $function->hasTentativeReturnType()
            ? snapshot_type($function->getTentativeReturnType())
            : null,
        'attributes' => snapshot_attributes($function),
    ];
}

/**
 * Serializes a declared method.
 */
function snapshot_method(ReflectionMethod $method): array
{
    return snapshot_callable($method) + [
        'declaring_class' => $method->getDeclaringClass()->getName(),
        'public' => $method->isPublic(),
        'protected' => $method->isProtected(),
        'private' => $method->isPrivate(),
        'static' => $method->isStatic(),
        'abstract' => $method->isAbstract(),
        'final' => $method->isFinal(),
        'constructor' => $method->isConstructor(),
        'destructor' => $method->isDestructor(),
    ];
}

/**
 * Serializes a declared property, including virtual-property metadata.
 */
function snapshot_property(ReflectionProperty $property): array
{
    $semanticKey = strtolower($property->getDeclaringClass()->getName())
        . '::$'
        . $property->getName();
    $hooks = [];
    if (method_exists($property, 'getHooks')) {
        foreach ($property->getHooks() as $name => $hook) {
            $hooks[$name] = snapshot_method($hook);
        }
    }

    return [
        'name' => $property->getName(),
        'declaring_class' => $property->getDeclaringClass()->getName(),
        'type' => snapshot_type($property->getType()),
        'public' => $property->isPublic(),
        'protected' => $property->isProtected(),
        'private' => $property->isPrivate(),
        'static' => $property->isStatic(),
        'readonly' => $property->isReadOnly(),
        'writable' => !$property->isReadOnly()
            && !isset($GLOBALS['stubReadonlyProperties'][$semanticKey]),
        'virtual' => method_exists($property, 'isVirtual') && $property->isVirtual(),
        'deprecated' => method_exists($property, 'isDeprecated') && $property->isDeprecated(),
        'has_default' => $property->hasDefaultValue(),
        'default' => $property->hasDefaultValue()
            ? snapshot_value($property->getDefaultValue())
            : null,
        'hooks' => $hooks,
        'attributes' => snapshot_attributes($property),
    ];
}

/**
 * Serializes a directly declared class or enum constant.
 */
function snapshot_class_constant(ReflectionClassConstant $constant): array
{
    return [
        'name' => $constant->getName(),
        'declaring_class' => $constant->getDeclaringClass()->getName(),
        'value' => snapshot_value($constant->getValue()),
        'public' => $constant->isPublic(),
        'protected' => $constant->isProtected(),
        'private' => $constant->isPrivate(),
        'final' => $constant->isFinal(),
        'deprecated' => method_exists($constant, 'isDeprecated') && $constant->isDeprecated(),
        'type' => method_exists($constant, 'getType')
            ? snapshot_type($constant->getType())
            : null,
        'attributes' => snapshot_attributes($constant),
    ];
}

/**
 * Serializes one exported class name and its canonical definition.
 */
function snapshot_class(string $exportedName, ReflectionClass $class): array
{
    $methods = [];
    foreach ($class->getMethods() as $method) {
        if ($method->getDeclaringClass()->getName() === $class->getName()) {
            $methods[] = snapshot_method($method);
        }
    }

    $properties = [];
    foreach ($class->getProperties() as $property) {
        if ($property->getDeclaringClass()->getName() === $class->getName()) {
            $properties[] = snapshot_property($property);
        }
    }

    $constants = [];
    foreach ($class->getReflectionConstants() as $constant) {
        if ($constant->getDeclaringClass()->getName() === $class->getName()) {
            $constants[] = snapshot_class_constant($constant);
        }
    }

    $interfaces = array_values(array_map(
        static fn (ReflectionClass $interface): string => $interface->getName(),
        $class->getInterfaces(),
    ));

    return [
        'exported_name' => $exportedName,
        'canonical_name' => $class->getName(),
        'extension' => $class->getExtensionName(),
        'internal' => $class->isInternal(),
        'interface' => $class->isInterface(),
        'trait' => $class->isTrait(),
        'enum' => $class->isEnum(),
        'abstract' => $class->isAbstract(),
        'final' => $class->isFinal(),
        'readonly' => method_exists($class, 'isReadOnly') && $class->isReadOnly(),
        'instantiable' => $class->isInstantiable(),
        'cloneable' => $class->isCloneable(),
        'parent' => ($parent = $class->getParentClass()) === false ? null : $parent->getName(),
        'interfaces' => $interfaces,
        'methods' => $methods,
        'properties' => $properties,
        'constants' => $constants,
        'attributes' => snapshot_attributes($class),
    ];
}

/**
 * Serializes one complete PHP extension surface.
 */
function snapshot_extension(string $name): array
{
    $extension = new ReflectionExtension($name);

    $classes = [];
    foreach ($extension->getClasses() as $exportedName => $class) {
        $classes[] = snapshot_class($exportedName, $class);
    }

    $functions = [];
    foreach ($extension->getFunctions() as $exportedName => $function) {
        $functions[] = ['exported_name' => $exportedName] + snapshot_callable($function);
    }

    $constants = [];
    foreach ($extension->getConstants() as $constantName => $value) {
        $constants[] = ['name' => $constantName, 'value' => snapshot_value($value)];
    }

    return [
        'name' => $extension->getName(),
        'version' => $extension->getVersion(),
        'classes' => $classes,
        'functions' => $functions,
        'constants' => $constants,
    ];
}

if (PHP_VERSION !== '8.5.8' || LIBXML_VERSION !== 21503 || LIBXML_DOTTED_VERSION !== '2.15.3') {
    fwrite(STDERR, sprintf(
        "expected PHP 8.5.8/libxml2 2.15.3, got PHP %s/libxml2 %s (%d)\n",
        PHP_VERSION,
        LIBXML_DOTTED_VERSION,
        LIBXML_VERSION,
    ));
    exit(1);
}

$outputPath = $argv[1] ?? null;
$sourceRoot = $argv[2] ?? null;
if ($outputPath === null || $sourceRoot === null) {
    fwrite(STDERR, "usage: snapshot_surface.php OUTPUT.json PHP_SOURCE_ROOT\n");
    exit(2);
}

$GLOBALS['stubReadonlyProperties'] = snapshot_stub_readonly_properties($sourceRoot);

$snapshot = [
    'schema' => 2,
    'php_version' => PHP_VERSION,
    'php_version_id' => PHP_VERSION_ID,
    'libxml_dotted_version' => LIBXML_DOTTED_VERSION,
    'libxml_version' => LIBXML_VERSION,
    'extensions' => [
        snapshot_extension('dom'),
        snapshot_extension('libxml'),
        snapshot_extension('simplexml'),
    ],
];

$json = json_encode(
    $snapshot,
    JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR,
);
if (file_put_contents($outputPath, $json . "\n") === false) {
    fwrite(STDERR, "failed to write {$outputPath}\n");
    exit(1);
}
