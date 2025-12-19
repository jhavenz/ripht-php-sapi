<?php

header('Content-Type: application/json');

$action = $_GET['action'] ?? 'report';
$size = min((int)($_GET['size'] ?? 1000), 10000);

$memory_before = memory_get_usage(true);
$peak_before = memory_get_peak_usage(true);

switch ($action) {
    case 'allocate':
        $data = [];
        for ($i = 0; $i < $size; $i++) {
            $data[] = str_repeat('x', 1000);
        }
        $allocated = count($data);
        unset($data);
        break;

    case 'objects':
        $objects = [];
        for ($i = 0; $i < $size; $i++) {
            $obj = new stdClass();
            $obj->id = $i;
            $obj->data = str_repeat('y', 100);
            $objects[] = $obj;
        }
        $allocated = count($objects);
        unset($objects);
        break;

    case 'recursive':
        $depth = min($size, 100);
        function buildNested($depth) {
            if ($depth <= 0) return [];
            return ['child' => buildNested($depth - 1), 'data' => str_repeat('z', 100)];
        }
        $nested = buildNested($depth);
        $allocated = $depth;
        unset($nested);
        break;

    default:
        $allocated = 0;
}

gc_collect_cycles();

echo json_encode([
    'action' => $action,
    'requested_size' => $size,
    'allocated' => $allocated ?? 0,
    'memory_before' => $memory_before,
    'memory_after' => memory_get_usage(true),
    'peak_before' => $peak_before,
    'peak_after' => memory_get_peak_usage(true),
], JSON_PRETTY_PRINT);
