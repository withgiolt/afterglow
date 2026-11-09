;; bulk-memory, multivalue, sign-ext, nontrapping-float-to-int, plus mutable state for reuse.
(module
  (import "host" "twice" (func $twice (param i64) (result i64)))
  (memory (export "memory") 1)
  (data $seg "glowc")
  ;; multivalue: returns two results
  (func $pair (param i64) (result i64 i64) (local.get 0) (i64.mul (local.get 0) (i64.const 2)))
  (func (export "features") (param $k i64) (param $big f64) (result i64)
    (local $a i64) (local $b i64)
    ;; bulk-memory: memory.init, memory.fill, memory.copy, data.drop
    (memory.init $seg (i32.const 16) (i32.const 0) (i32.const 5))
    (data.drop $seg)
    (memory.fill (i32.const 32) (i32.const 7) (i32.const 8))
    (memory.copy (i32.const 64) (i32.const 16) (i32.const 5))
    (call $pair (local.get $k))
    (local.set $b) (local.set $a)
    ;; called with k=21, big=1e30: result = a + b (63)
    ;;        + sign-ext((k+234)=0xff as i8) (-1)
    ;;        + trunc_sat(big=1e30) clamps to i32 max -> 2147483647
    ;;        + load8_u(64) ('g' = 103) + load8_u(32) (7)
    (i64.add (i64.add (local.get $a) (local.get $b))
      (i64.add
        (i64.extend8_s (i64.add (local.get $k) (i64.const 234)))
        (i64.add
          (i64.extend_i32_s (i32.trunc_sat_f64_s (local.get $big)))
          (i64.add (i64.load8_u (i32.const 64)) (i64.load8_u (i32.const 32)))))))
  ;; instance reuse: counter kept in linear memory at offset 0
  (func (export "bump") (result i32)
    (i32.store (i32.const 0) (i32.add (i32.load (i32.const 0)) (i32.const 1)))
    (i32.load (i32.const 0)))
  ;; i64 across the JS boundary, both directions
  (func (export "i64_roundtrip") (param i64) (result i64)
    (call $twice (i64.add (local.get 0) (i64.const 1)))))
