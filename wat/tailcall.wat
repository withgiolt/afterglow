;; Mutually recursive return_call pair. ping(n, acc) -> pong(n-1, acc+1) -> ping ...
(module
  (func $ping (export "ping") (param $n i64) (param $acc i64) (result i64)
    (if (result i64) (i64.eqz (local.get $n))
      (then (local.get $acc))
      (else (return_call $pong (i64.sub (local.get $n) (i64.const 1)) (i64.add (local.get $acc) (i64.const 1))))))
  (func $pong (param $n i64) (param $acc i64) (result i64)
    (if (result i64) (i64.eqz (local.get $n))
      (then (local.get $acc))
      (else (return_call $ping (i64.sub (local.get $n) (i64.const 1)) (i64.add (local.get $acc) (i64.const 1)))))))
