(module
  (import "host" "boom" (func $boom (param i64) (result i64)))
  (tag $t (param i64))
  (func $thrower (param i64) (result i64) (throw $t (call $boom (local.get 0))))
  (func (export "eh") (param i64) (result i64)
    try (result i64)
      local.get 0
      call $thrower
    catch $t
    end))
