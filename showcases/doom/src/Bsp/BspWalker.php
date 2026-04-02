<?php

namespace Showcases\Doom\Bsp;

use Showcases\Doom\Map\MapData;
use Showcases\Doom\Player\Camera;

class BspWalker {
    public function walk(MapData $map, Camera $camera) {
        if (!$map->isValid() || $map->subSectorCount <= 0) {
            return [];
        }

        if ($map->nodeCount <= 0) {
            $order = [];
            int $i = 0;
            while ($i < $map->subSectorCount) {
                $order[] = $i;
                $i += 1;
            }
            return $order;
        }

        return $this->visitNode($map, $camera, $map->nodeCount - 1);
    }

    public function findSubSectorIndex(MapData $map, Camera $camera): int {
        if (!$map->isValid() || $map->subSectorCount <= 0) {
            return -1;
        }

        if ($map->nodeCount <= 0) {
            return 0;
        }

        return $this->findSubSectorNode($map, $camera, $map->nodeCount - 1);
    }

    public function visitNode(MapData $map, Camera $camera, int $nodeIndex) {
        if ($nodeIndex < 0 || $nodeIndex >= $map->nodeCount) {
            return [];
        }

        $node = $map->nodes[$nodeIndex];
        int $side = $this->pointSide(
            $camera->x,
            $camera->y,
            $node->partition_x,
            $node->partition_y,
            $node->delta_x,
            $node->delta_y
        );

        int $nearChild = $node->right_child;
        int $farChild = $node->left_child;
        if ($side > 0) {
            $nearChild = $node->left_child;
            $farChild = $node->right_child;
        }

        $nearOrder = $this->visitChild($map, $camera, $nearChild);
        $farOrder = $this->visitChild($map, $camera, $farChild);
        return $this->appendOrder($nearOrder, $farOrder);
    }

    public function visitChild(MapData $map, Camera $camera, int $child) {
        if ($child >= 32768) {
            int $subSectorIndex = $child - 32768;
            if ($subSectorIndex >= 0 && $subSectorIndex < $map->subSectorCount) {
                return [$subSectorIndex];
            }
            return [];
        }

        return $this->visitNode($map, $camera, $child);
    }

    public function findSubSectorNode(MapData $map, Camera $camera, int $nodeIndex): int {
        if ($nodeIndex < 0 || $nodeIndex >= $map->nodeCount) {
            return -1;
        }

        $node = $map->nodes[$nodeIndex];
        int $side = $this->pointSide(
            $camera->x,
            $camera->y,
            $node->partition_x,
            $node->partition_y,
            $node->delta_x,
            $node->delta_y
        );

        int $nearChild = $node->right_child;
        if ($side > 0) {
            $nearChild = $node->left_child;
        }

        return $this->resolveSubSectorChild($map, $camera, $nearChild);
    }

    public function resolveSubSectorChild(MapData $map, Camera $camera, int $child): int {
        if ($child >= 32768) {
            int $subSectorIndex = $child - 32768;
            if ($subSectorIndex >= 0 && $subSectorIndex < $map->subSectorCount) {
                return $subSectorIndex;
            }
            return -1;
        }

        return $this->findSubSectorNode($map, $camera, $child);
    }

    public function appendOrder($left, $right) {
        $out = $left;
        int $i = 0;
        while ($i < count($right)) {
            $out[] = $right[$i];
            $i += 1;
        }
        return $out;
    }

    public function pointSide(
        int $px,
        int $py,
        int $partitionX,
        int $partitionY,
        int $deltaX,
        int $deltaY
    ): int {
        int $relX = $px - $partitionX;
        int $relY = $py - $partitionY;
        return ($relX * $deltaY) - ($relY * $deltaX);
    }
}
