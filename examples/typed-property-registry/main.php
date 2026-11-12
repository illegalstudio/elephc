<?php
// Filling typed array properties: an actor registry keyed by id, a draw list in
// insertion order, and a per-layer counter. Each element write updates the
// property's own storage in place, so registering N actors costs O(N), and a
// copy taken earlier keeps its own elements.

final class Actor
{
    public function __construct(public int $id, public string $name, public int $layer) {}
}

final class Scene
{
    /** Actors by id. */
    private array<int, Actor> $actors = [];
    /** Actor ids in the order they were registered. */
    private array<int> $drawOrder = [];
    /** Number of actors on each layer. */
    private array<int, int> $perLayer = [];

    public function register(Actor $actor): void
    {
        $this->actors[$actor->id] = $actor;
        $this->drawOrder[] = $actor->id;
        $this->perLayer[$actor->layer] = ($this->perLayer[$actor->layer] ?? 0) + 1;
    }

    public function rename(int $id, string $name): void
    {
        // Replacing an element keeps every other entry where it is.
        $this->actors[$id] = new Actor($id, $name, $this->actors[$id]->layer);
    }

    public function snapshot(): array<int, Actor>
    {
        return $this->actors;
    }

    public function describe(): string
    {
        $names = [];
        foreach ($this->drawOrder as $id) {
            $names[] = $this->actors[$id]->name;
        }
        return implode(", ", $names);
    }

    public function layerCount(int $layer): int
    {
        return $this->perLayer[$layer] ?? 0;
    }

    public function size(): int
    {
        return count($this->actors);
    }
}

$scene = new Scene();
$scene->register(new Actor(7, "hero", 1));
$scene->register(new Actor(3, "door", 0));
$scene->register(new Actor(9, "lamp", 0));

$before = $scene->snapshot();
$scene->rename(3, "gate");
$scene->register(new Actor(4, "crate", 0));

echo "scene: ", $scene->describe(), "\n";
echo "layer 0 holds ", $scene->layerCount(0), ", layer 1 holds ", $scene->layerCount(1), "\n";
echo "snapshot still has ", count($before), " actors and calls #3 ", $before[3]->name, "\n";

// A large registry stays cheap to fill.
for ($id = 100; $id < 5100; $id++) {
    $scene->register(new Actor((int) $id, "extra", 2));
}
echo "after the crowd: ", $scene->size(), " actors, ", $scene->layerCount(2), " on layer 2\n";
