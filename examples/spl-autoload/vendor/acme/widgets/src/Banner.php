<?php
namespace Acme\Widgets;

class Banner {
    public function __construct(private string $label) {}

    public function render(): string {
        return "[" . $this->label . "]";
    }
}
