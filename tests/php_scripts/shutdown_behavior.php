<?php

header('Content-Type: application/json');

$action = $_GET['action'] ?? 'normal';

$shutdown_ran = false;
$destructor_ran = false;

register_shutdown_function(function () use (&$shutdown_ran) {
    $shutdown_ran = true;
    file_put_contents('/tmp/php_shutdown_test.txt', 'shutdown');
});

class TestObject {
    public function __destruct() {
        file_put_contents('/tmp/php_destructor_test.txt', 'destructor');
    }
}

$obj = new TestObject();

switch ($action) {
    case 'exit_code':
        echo json_encode(['will_exit' => true, 'code' => 42]);
        exit(42);

    case 'die_message':
        echo json_encode(['will_die' => true]);
        die('Dying with message');

    case 'return':
        echo json_encode(['will_return' => true]);
        return;

    case 'fatal':
        echo json_encode(['will_fatal' => true]);
        nonexistent_function();
        break;

    default:
        echo json_encode([
            'action' => $action,
            'status' => 'completed normally',
            'shutdown_registered' => true,
            'object_created' => true,
        ], JSON_PRETTY_PRINT);
}
