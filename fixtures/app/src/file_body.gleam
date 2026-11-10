import gleam/option
import afterglow
import wisp

/// wisp.File responses aren't supported (no filesystem on a Worker) —
/// afterglow.serve should turn this into a 501, not crash.
pub fn start() {
  afterglow.serve(handle_request)
}

fn handle_request(_req: wisp.Request) -> wisp.Response {
  wisp.response(200) |> wisp.set_body(wisp.File("/tmp/x", 0, option.None))
}
