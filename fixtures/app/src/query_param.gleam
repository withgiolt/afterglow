import afterglow
import wisp

/// Exercises wisp.get_query -> gleam/uri -> uri_string.
pub fn start() {
  afterglow.serve(handle_request)
}

fn handle_request(req: wisp.Request) -> wisp.Response {
  case wisp.get_query(req) {
    [#("msg", msg), ..] -> wisp.ok() |> wisp.string_body(msg)
    _ -> wisp.ok() |> wisp.string_body("no msg")
  }
}
