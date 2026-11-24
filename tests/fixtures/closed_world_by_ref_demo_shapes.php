<?php
// The shapes of the Symfony Demo's thirteen passing-mode queries: an unknown callable called with
// an array element or a property while SimpleXMLElement exists in the program.
$xml = new SimpleXMLElement('<r><c>old</c></r>');
echo $xml->c, "\n";

final class Holder {
    public array $elements = ['a' => 1, 'b' => 2];
    public array $positions = ['x' => 'a'];
    public ?string $prop = 'p';

    /** NamedObjectSet::modify shape: the callable sees an element of a property array. */
    public function modify(string $key, callable $modifier): void {
        $position = $this->positions[$key];
        echo $key, $position, $modifier($this->elements[$position]), "\n";
    }

    /** Table::render shape: a static closure over rows built in a loop. */
    public function render(): void {
        $isCell = static fn ($cell) => $cell instanceof Holder;
        $rows = [];
        foreach ([[1], [new Holder]] as $i => $row) {
            $rows[$i] = $row;
            echo $isCell($rows[$i][0]) ? 'cell' : 'plain', "\n";
        }
    }

    /** stream() / evaluate() shape: a callable property applied to a property. */
    public function apply(callable $callback): string {
        return (string) $callback($this->prop);
    }
}

/** DeepClone getHydrator / Strings::replace shape: a callable handed an element of a local array. */
function through(callable $callback, array $subject): mixed {
    return $callback($subject['k']);
}

/** addChoiceViewsGroupedByCallable shape: a callable handed list elements in a loop. */
function grouped(array $choices, callable $groupBy): array {
    $out = [];
    foreach ($choices as $index => $choice) {
        $out[$index] = $groupBy($choices[$index], $index);
    }
    return $out;
}

/** registerMappingFilesFromConfig shape: array_map over a closure with a nested element. */
function mapped(array $config): array {
    return array_map(static fn ($entry) => $entry['dir'] . '/' . $entry['name'], $config);
}

$holder = new Holder;
$holder->modify('x', fn ($value) => $value + 1);
$holder->modify('x', function (&$value) { $value += 10; return $value; });
$holder->render();
echo $holder->apply(fn ($p) => strtoupper($p)), "\n";
echo $holder->apply('ucfirst'), "\n";
echo through(fn ($v) => $v . '!', ['k' => 'a']), "\n";
echo through('strrev', ['k' => 'abc']), "\n";
echo json_encode(grouped(['p', 'q'], fn ($choice, $i) => $choice . $i)), "\n";
echo json_encode(mapped([['dir' => 'd', 'name' => 'n']])), "\n";

$rows = [['n' => 3], ['n' => 1], ['n' => 2]];
usort($rows, fn ($l, $r) => $l['n'] <=> $r['n']);
echo json_encode($rows), "\n";
uasort($rows, function ($l, $r) { return $r['n'] <=> $l['n']; });
echo json_encode($rows), "\n";
echo $xml->c, "\n";
