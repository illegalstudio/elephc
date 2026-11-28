<?php
function referencesAfterUnset(): array {
    $value = 'old';
    $old = function(mixed $candidate) use (&$value): mixed {
        if ($candidate !== null) { $value = $candidate; }
        return $value;
    };
    unset($value);
    $fresh = function(mixed $candidate) use (&$value): mixed {
        if ($candidate !== null) { $value = $candidate; }
        return $value;
    };
    echo gettype($value), '|', $old(null), '|', gettype($fresh(null)), '|';
    $fresh('new');
    echo $old(null), ':', $fresh(null), '|';
    try { $old(3); } catch (TypeError $error) { echo 'T'; }
    echo gettype($old(null)), '|';
    $fresh(4);
    echo gettype($fresh(null)), '|';
    return [$old, $fresh];
}
$closures = referencesAfterUnset();
echo $closures[0](null), ':', $closures[1](null), '|';
unset($closures);

function freshAliasSources(): void {
    $alias = &$missing;
    echo gettype($missing), '|';
    $alias = 'alias';
    echo $missing, '|';
    unset($missing);
    $next = &$missing;
    echo gettype($missing), ':', $alias, '|';
    $next = 9;
    echo $missing, '|';
}
freshAliasSources();
$items = ['last', 'first'];
usort($items, function(string $left, string $right) use (&$state): int {
    $state = 'sorted';
    return strcmp($left, $right);
});
echo $state, '|';
unset($items, $state);
$self = &$self;
echo gettype($self), '|';
$self = 'self';
echo $self, '|';
unset($self);
$probe = 'before';
unset($probe);
$peek = function() use (&$probe): mixed { return $probe; };
echo gettype($probe), ':', gettype($peek()), '|';
$probe = 'seen';
echo $peek(), '|';
unset($probe, $peek);

$null = null;
$write = function(mixed $candidate) use (&$null): void { $null = $candidate; };
try { $write('value'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
echo gettype($null), '|';
