import afterglow
import wisp

pub fn start() {
  afterglow.serve(handle_request)
}

/// Calls a function no module provides, the way an app hits an OTP function or NIF
/// afterglow lacks. The Worker answers 500 naming the function.
fn handle_request(_req: wisp.Request) -> wisp.Response {
  not_provided(1)
  wisp.ok()
}

@external(erlang, "afterglow_test_absent", "not_provided")
fn not_provided(x: Int) -> Nil
