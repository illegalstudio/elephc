<?php

namespace Showcases\Doom\App;

use Showcases\Doom\SDL\Input;
use Showcases\Doom\SDL\SDL;

// The window around a Session: keyboard, the fixed 35 Hz tic clock, one frame per
// loop iteration presented through SDL, and the frame-rate statistics.
final class Game {
    public Config $config;
    public SDL $sdl;
    public Input $input;
    public Session $session;
    public GameState $state;
    public bool $vsyncToggleHeld;
    public bool $mapToggleHeld;
    public string $fpsText;
    public int $tic;
    public int $statStartTicks;
    public int $statFrames;
    public int $statRenderNanos;

    public function __construct(Config $config) {
        $this->config = $config;
        $this->sdl = new SDL();
        $this->input = new Input();
        $this->session = new Session($config);
        $this->state = GameState::Booting;
        $this->vsyncToggleHeld = false;
        $this->mapToggleHeld = false;
        $this->fpsText = "";
        $this->tic = 0;
        $this->statStartTicks = 0;
        $this->statFrames = 0;
        $this->statRenderNanos = 0;
    }

    public function run(): void {
        if (!$this->session->boot()) {
            return;
        }
        if (!$this->sdl->boot($this->config)) {
            echo "SDL boot failed: " . $this->sdl->lastError() . "\n";
            return;
        }

        $this->state = GameState::Running;
        echo "DOOM showcase running\n";
        echo "WASD/arrows move, Shift runs, Space/E uses, Tab shows the map, V toggles vsync, ESC quits\n";

        Session $session = $this->session;
        int $start = $this->sdl->ticks();
        $this->statStartTicks = $start;
        while ($this->state === GameState::Running) {
            $this->sdl->pumpEvents();
            $keys = $this->sdl->keyboardState();
            if ($this->input->shouldQuit($keys) !== 0) {
                $this->state = GameState::Stopped;
            }
            $this->handleVsyncToggle($keys);
            $this->handleMapToggle($keys);

            int $now = $this->sdl->ticks();
            if ($this->config->bootDurationMs > 0 && $now - $start >= $this->config->bootDurationMs) {
                $this->state = GameState::Stopped;
            }

            // fixed-rate game tics: run every tic that is due, at most 5 per frame
            int $elapsed = $now - $start;
            int $tickTime = $elapsed * $this->config->tickRate;
            int $dueTics = intdiv($tickTime, 1000);
            int $behind = $dueTics - $this->tic;
            if ($behind > 5) {
                int $skipTo = $dueTics - 5;
                $this->tic = $skipTo;
            }
            while ($this->tic < $dueTics) {
                $session->runTic($this->input, $keys);
                $this->tic++;
            }

            int $renderStart = hrtime(true);
            $session->render($this->fpsText);
            $session->renderer->screen->toRgb($session->palette, $session->player->palette());
            int $renderNanos = hrtime(true) - $renderStart;
            $this->sdl->present($session->renderer->screen);
            $this->reportStats($now, $renderNanos);
        }

        $this->sdl->shutdown();
    }

    // Once per second: frames shown and the average time spent drawing one frame.
    public function reportStats(int $now, int $renderNanos): void {
        $this->statFrames++;
        int $total = $this->statRenderNanos + $renderNanos;
        $this->statRenderNanos = $total;
        int $elapsed = $now - $this->statStartTicks;
        if ($elapsed < 1000) {
            return;
        }
        int $frameTime = $this->statFrames * 1000;
        int $fps = intdiv($frameTime, $elapsed);
        int $nanosPerFrame = intdiv($this->statRenderNanos, $this->statFrames);
        int $hundredths = intdiv($nanosPerFrame, 10000);
        int $whole = intdiv($hundredths, 100);
        int $fraction = $hundredths % 100;
        string $padding = "";
        if ($fraction < 10) {
            $padding = "0";
        }
        string $vsync = "off";
        if ($this->sdl->vsync) {
            $vsync = "on";
        }
        string $stats = $this->session->mapName . " | " . $fps . " fps | render " . $whole . "." . $padding . $fraction . " ms | vsync " . $vsync;
        $this->fpsText = $fps . " FPS " . $whole . "." . $padding . $fraction . " MS";
        $this->sdl->setTitle($this->config->windowTitle . " | " . $stats);
        echo $stats . "\n";
        $this->statStartTicks = $now;
        $this->statFrames = 0;
        $this->statRenderNanos = 0;
    }

    public function handleMapToggle(ptr $keys): void {
        if ($this->input->toggleMap($keys) === 0) {
            $this->mapToggleHeld = false;
            return;
        }
        if (!$this->mapToggleHeld) {
            $this->session->renderer->showAutomap = !$this->session->renderer->showAutomap;
        }
        $this->mapToggleHeld = true;
    }

    public function handleVsyncToggle(ptr $keys): void {
        if ($this->input->toggleVsync($keys) === 0) {
            $this->vsyncToggleHeld = false;
            return;
        }
        if (!$this->vsyncToggleHeld) {
            $this->sdl->setVsync(!$this->sdl->vsync);
        }
        $this->vsyncToggleHeld = true;
    }
}
