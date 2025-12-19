<?php

header('Content-Type: application/json');

$action = $_GET['action'] ?? 'info';
$tmp_dir = sys_get_temp_dir() . '/php_sapi_test';

if (!is_dir($tmp_dir)) {
    mkdir($tmp_dir, 0755, true);
}

$result = ['action' => $action, 'tmp_dir' => $tmp_dir];

switch ($action) {
    case 'readwrite':
        $file = $tmp_dir . '/test_' . getmypid() . '.txt';
        $data = str_repeat('x', 1024);

        file_put_contents($file, $data);
        $read = file_get_contents($file);
        unlink($file);

        $result['bytes_written'] = strlen($data);
        $result['bytes_read'] = strlen($read);
        $result['match'] = $data === $read;
        break;

    case 'many_files':
        $count = min((int)($_GET['count'] ?? 10), 100);
        $files_created = 0;

        for ($i = 0; $i < $count; $i++) {
            $file = $tmp_dir . '/multi_' . getmypid() . '_' . $i . '.txt';
            file_put_contents($file, "file $i content");
            $files_created++;
        }

        for ($i = 0; $i < $count; $i++) {
            $file = $tmp_dir . '/multi_' . getmypid() . '_' . $i . '.txt';
            if (file_exists($file)) {
                unlink($file);
            }
        }

        $result['files_created'] = $files_created;
        break;

    case 'large_file':
        $size_kb = min((int)($_GET['size'] ?? 100), 1024);
        $file = $tmp_dir . '/large_' . getmypid() . '.bin';
        $data = str_repeat('L', $size_kb * 1024);

        $write_start = microtime(true);
        file_put_contents($file, $data);
        $write_time = microtime(true) - $write_start;

        $read_start = microtime(true);
        $read = file_get_contents($file);
        $read_time = microtime(true) - $read_start;

        unlink($file);

        $result['size_kb'] = $size_kb;
        $result['write_ms'] = round($write_time * 1000, 3);
        $result['read_ms'] = round($read_time * 1000, 3);
        $result['match'] = $data === $read;
        break;

    case 'sqlite':
        $db_file = $tmp_dir . '/test_' . getmypid() . '.db';

        try {
            $db = new SQLite3($db_file);
            $db->exec('CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY, name TEXT, value INTEGER)');

            $db->exec('BEGIN TRANSACTION');
            $stmt = $db->prepare('INSERT INTO items (name, value) VALUES (:name, :value)');
            $rows = min((int)($_GET['rows'] ?? 100), 1000);

            for ($i = 0; $i < $rows; $i++) {
                $stmt->bindValue(':name', "item_$i", SQLITE3_TEXT);
                $stmt->bindValue(':value', $i * 10, SQLITE3_INTEGER);
                $stmt->execute();
                $stmt->reset();
            }
            $db->exec('COMMIT');

            $count = $db->querySingle('SELECT COUNT(*) FROM items');
            $sum = $db->querySingle('SELECT SUM(value) FROM items');

            $db->close();
            unlink($db_file);

            $result['rows_inserted'] = $rows;
            $result['count'] = $count;
            $result['sum'] = $sum;
        } catch (Exception $e) {
            $result['error'] = $e->getMessage();
            if (file_exists($db_file)) {
                unlink($db_file);
            }
        }
        break;

    case 'glob':
        $pattern = $tmp_dir . '/*';
        $files = glob($pattern) ?: [];
        $result['pattern'] = $pattern;
        $result['count'] = count($files);
        break;

    case 'stat':
        $file = $tmp_dir . '/stat_test_' . getmypid() . '.txt';
        file_put_contents($file, 'stat test');

        $stat = stat($file);
        $result['size'] = $stat['size'];
        $result['mtime'] = $stat['mtime'];
        $result['mode'] = decoct($stat['mode']);

        unlink($file);
        break;

    default:
        $result['available_actions'] = [
            'readwrite' => 'Basic file read/write cycle',
            'many_files' => 'Create/delete many files (?count=N)',
            'large_file' => 'Large file read/write (?size=KB)',
            'sqlite' => 'SQLite operations (?rows=N)',
            'glob' => 'Directory glob',
            'stat' => 'File stat operations',
        ];
}

echo json_encode($result, JSON_PRETTY_PRINT);
