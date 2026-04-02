<?php

extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_Quit(): void;
    function SDL_CreateWindow(string $title, int $x, int $y, int $w, int $h, int $flags): ptr;
    function SDL_DestroyWindow(ptr $window): void;
    function SDL_CreateRenderer(ptr $window, int $index, int $flags): ptr;
    function SDL_DestroyRenderer(ptr $renderer): void;
    function SDL_SetRenderDrawColor(ptr $renderer, int $r, int $g, int $b, int $a): int;
    function SDL_RenderClear(ptr $renderer): int;
    function SDL_RenderPresent(ptr $renderer): void;
    function SDL_PumpEvents(): void;
    function SDL_GetKeyboardState(ptr $numkeys): ptr;
    function SDL_GetTicks(): int;
    function SDL_Delay(int $ms): void;
    function SDL_GetError(): string;
}
