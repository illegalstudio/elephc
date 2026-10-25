<?php

namespace Showcases\Doom\Data;

// A repeatable switch showing its pressed texture until $countdown runs out.
packed class Button {
    public int $active;
    public int $sidedef;
    public int $part;
    public int $texture;
    public int $countdown;
}
