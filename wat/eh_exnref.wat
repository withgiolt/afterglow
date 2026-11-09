(module
  (tag $t (param i64))
  (import "host" "boom" (func $boom (param i64) (result i64)))
  (func $thrower (param i64) (result i64) (throw $t (call $boom (local.get 0))))
  (func (export "eh") (param i64) (result i64)
    (block $h (result i64)
      (try_table (catch $t $h)
        (drop (call $thrower (local.get 0))))
      (i64.const -1))))
