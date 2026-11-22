<?php
// Loading untrusted JSON into typed maps. Every entry is validated before it is
// stored, so the local builder and the property it replaces keep the declared
// `array<string, bool>` storage: the values stay raw booleans end to end, and a
// bad document leaves the previous preferences untouched.

final class Preferences
{
    /** Feature switches by name. */
    private array<string, bool> $flags = [];
    /** Display colors by role, as ARGB words. */
    private array<string, int> $colors = ['text' => 0xFF29_2233, 'accent' => 0xFF75_4AE0];

    public function load(string $json): void
    {
        $data = json_decode($json, true, 512, JSON_THROW_ON_ERROR);
        if (!is_array($data)) {
            throw new UnexpectedValueException('Expected a JSON object');
        }

        array<string, bool> $flags = [];
        foreach ($data as $name => $value) {
            if (!is_string($name) || !is_bool($value)) {
                throw new UnexpectedValueException('Expected boolean preferences');
            }
            // `$value` is a bool here, so the map keeps its declared storage.
            $flags[$name] = $value;
        }

        $this->flags = $flags;
    }

    /** Returns a copy with some colors replaced; this object keeps its own palette. */
    public function withColors(array<string, int> $overrides): Preferences
    {
        $colors = $this->colors;
        foreach ($overrides as $role => $color) {
            if (!isset($colors[$role])) {
                throw new InvalidArgumentException('Unknown color role: ' . $role);
            }
            $colors[$role] = $color;
        }

        $copy = clone $this;
        $copy->colors = $colors;
        return $copy;
    }

    public function enabled(string $name): bool
    {
        return $this->flags[$name] ?? false;
    }

    public function describe(): string
    {
        $on = [];
        foreach ($this->flags as $name => $enabled) {
            if ($enabled) {
                $on[] = $name;
            }
        }
        $colors = [];
        foreach ($this->colors as $role => $color) {
            $colors[] = $role . '=' . dechex($color);
        }
        return 'on=[' . implode(',', $on) . '] colors=' . implode(',', $colors);
    }
}

$preferences = new Preferences();
$preferences->load('{"activity":true,"compact":false,"sidebar":true}');
echo $preferences->describe(), "\n";

foreach (['{"activity":"yes"}', '[true]', 'not json'] as $document) {
    try {
        $preferences->load($document);
    } catch (UnexpectedValueException $error) {
        echo 'rejected: ', $error->getMessage(), "\n";
    } catch (JsonException $error) {
        echo "rejected: malformed JSON\n";
    }
}
echo 'compact is ', $preferences->enabled('compact') ? 'on' : 'off', "\n";

$dark = $preferences->withColors(['text' => 0xFFF5_EEFB]);
echo $dark->describe(), "\n", $preferences->describe(), "\n";
