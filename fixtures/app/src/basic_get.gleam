import afterglow
import wisp

pub fn start() {
  afterglow.serve(handle_request)
}

fn handle_request(_req: wisp.Request) -> wisp.Response {
  wisp.ok() |> wisp.string_body("hello from afterglow")
}
