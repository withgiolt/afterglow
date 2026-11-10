import afterglow
import wisp

pub fn start() {
  afterglow.serve(handle_request)
}

fn handle_request(_req: wisp.Request) -> wisp.Response {
  wisp.not_found()
}
