<?php
// Minimal SDL2 window lifecycle.
// Run with:
// elephc -l SDL2 -L /opt/homebrew/lib examples/sdl_window/main.php

extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_Quit(): void;
    function SDL_CreateWindow(string $title, int $x, int $y, int $w, int $h, int $flags): ptr;
    function SDL_DestroyWindow(ptr $window): void;
    function SDL_GetError(): string;
    function SDL_Delay(int $ms): void;
}

$SDL_INIT_VIDEO = 32;

if (SDL_Init($SDL_INIT_VIDEO) != 0) {
    echo "SDL_Init failed: " . SDL_GetError() . "\n";
    exit(1);
}

$window = SDL_CreateWindow("elephc SDL2", 100, 100, 640, 360, 0);
if (ptr_is_null($window)) {
    echo "SDL_CreateWindow failed: " . SDL_GetError() . "\n";
    SDL_Quit();
    exit(1);
}

echo "window ok\n";
SDL_Delay(750);

SDL_DestroyWindow($window);
SDL_Quit();
