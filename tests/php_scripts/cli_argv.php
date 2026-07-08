<?php

header('Content-Type: application/json');

$serverArgv = $_SERVER['argv'] ?? null;
$serverArgc = $_SERVER['argc'] ?? null;
$globalArgv = $argv ?? null;
$globalArgc = $argc ?? null;

echo json_encode([
    'server_argv_is_array' => is_array($serverArgv),
    'server_argc_type' => gettype($serverArgc),
    'server_argv' => $serverArgv,
    'server_argc' => $serverArgc,
    'global_argv_is_array' => is_array($globalArgv),
    'global_argc_type' => gettype($globalArgc),
    'global_argv' => $globalArgv,
    'global_argc' => $globalArgc,
    'stdin' => file_get_contents('php://stdin'),
], JSON_PRETTY_PRINT);
