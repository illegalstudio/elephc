<?php
// SDL2 keyboard state polling without event unions.
// Run with:
// elephc -l SDL2 -L /opt/homebrew/lib examples/sdl_input/main.php

extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_Quit(): void;
    function SDL_CreateWindow(string $title, int $x, int $y, int $w, int $h, int $flags): ptr;
    function SDL_DestroyWindow(ptr $window): void;
    function SDL_PumpEvents(): void;
    function SDL_GetKeyboardState(ptr $numkeys): ptr;
    function SDL_Delay(int $ms): void;
    function SDL_GetError(): string;
}

$SDL_INIT_VIDEO = 32;
$SDL_SCANCODE_SPACE = 44;

if (SDL_Init($SDL_INIT_VIDEO) != 0) {
    echo "SDL_Init failed: " . SDL_GetError() . "\n";
    exit(1);
}

$window = SDL_CreateWindow("input demo", 100, 100, 400, 200, 0);
if (ptr_is_null($window)) {
    echo "SDL_CreateWindow failed: " . SDL_GetError() . "\n";
    SDL_Quit();
    exit(1);
}

echo "press SPACE during the next second\n";

for ($i = 0; $i < 10; $i++) {
    SDL_PumpEvents();
    $keys = SDL_GetKeyboardState(ptr_null());
    $space = ptr_read8(ptr_offset($keys, $SDL_SCANCODE_SPACE));
    echo $space ? "1" : "0";
    SDL_Delay(100);
}

echo "\n";

SDL_DestroyWindow($window);
SDL_Quit();
