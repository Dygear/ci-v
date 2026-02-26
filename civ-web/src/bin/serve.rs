use std::env;

use tiny_http::{Header, Response, Server};

// Embed the hand-written web assets.
const INDEX_HTML: &str = include_str!("../../index.html");
const STYLE_CSS: &str = include_str!("../../style.css");
const CIV_WEB_JS: &str = include_str!("../../civ-web.js");

// Embed the wasm-pack output.
// NOTE: You must run `wasm-pack build civ-web --target web --out-dir pkg` before building.
const PKG_JS: &str = include_str!("../../pkg/civ_web.js");
const PKG_WASM: &[u8] = include_bytes!("../../pkg/civ_web_bg.wasm");

fn main() {
    let port = env::args()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(8080);

    let addr = format!("127.0.0.1:{port}");
    let server = Server::http(&addr).unwrap_or_else(|e| {
        eprintln!("Failed to bind to {addr}: {e}");
        std::process::exit(1);
    });

    println!("Serving on http://{addr}");

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        let (content_type, body): (&str, Vec<u8>) = match url.as_str() {
            "/" | "/index.html" => ("text/html; charset=utf-8", INDEX_HTML.into()),
            "/style.css" => ("text/css; charset=utf-8", STYLE_CSS.into()),
            "/civ-web.js" => ("application/javascript; charset=utf-8", CIV_WEB_JS.into()),
            "/pkg/civ_web.js" => ("application/javascript; charset=utf-8", PKG_JS.into()),
            "/pkg/civ_web_bg.wasm" => ("application/wasm", PKG_WASM.to_vec()),
            _ => {
                let resp = Response::from_string("404 Not Found").with_status_code(404);
                let _ = request.respond(resp);
                continue;
            }
        };

        let header =
            Header::from_bytes("Content-Type", content_type).expect("valid content-type header");
        let resp = Response::from_data(body).with_header(header);
        let _ = request.respond(resp);
    }
}
