<?php

// An override may append trailing optional parameters to the method it replaces.
// Callers typed against the narrower parent or interface still reach the override,
// which then sees its own defaults for the parameters they did not pass.

interface EventDispatcher {
    public function dispatch(object $event);
}

interface NamedEventDispatcher extends EventDispatcher {
    public function dispatch(object $event, ?string $eventName = null): object;
}

final class Event {
    public array $log = [];
}

final class Dispatcher implements NamedEventDispatcher {
    public function dispatch(object $event, ?string $eventName = null): object {
        $event->log[] = $eventName ?? get_class($event);
        return $event;
    }
}

abstract class Text {
    public function __construct(public string $value) {}

    abstract public function folded(): static;
}

final class Unicode extends Text {
    public function folded(bool $compat = true): static {
        return new static($compat ? strtolower($this->value) : $this->value);
    }
}

function notify(EventDispatcher $dispatcher, Event $event): void {
    $dispatcher->dispatch($event);
}

function fold(Text $text): string {
    return $text->folded()->value;
}

$event = new Event();
$dispatcher = new Dispatcher();
notify($dispatcher, $event);
$dispatcher->dispatch($event, "user.created");
echo implode(", ", $event->log) . "\n";

echo fold(new Unicode("HeLLo")) . " " . (new Unicode("HeLLo"))->folded(false)->value . "\n";
