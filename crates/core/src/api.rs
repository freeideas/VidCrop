//! Local HTTP API: everything the UI can do, scriptable. See specs/api.md.
//!
//! Only listens on 127.0.0.1, needs a bearer token, and refuses requests that carry an
//! `Origin` header, so web pages open in a browser can't drive it.

use crate::session::Core;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tiny_http::{Header, Method, Response, Server};

pub struct ApiInfo {
    pub url: String,
    pub token: String,
    pub file: PathBuf,
}

pub fn default_api_file() -> PathBuf {
    std::env::var_os("VIDCROP_API_FILE").map(PathBuf::from).unwrap_or_else(|| crate::paths::data_dir().join("api.json"))
}

fn new_token() -> String {
    if let Ok(t) = std::env::var("VIDCROP_API_TOKEN") {
        return t;
    }
    let mut b = [0u8; 24];
    getrandom::getrandom(&mut b).expect("no randomness available");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Starts the API on a background thread. `port` 0 picks a free one
/// (`VIDCROP_API_PORT` overrides). Writes `{url, token, pid}` to `api_file`.
pub fn start(core: Arc<Core>, port: u16, api_file: &Path) -> Result<ApiInfo, String> {
    let port = std::env::var("VIDCROP_API_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(port);
    let server = Server::http(("127.0.0.1", port)).map_err(|e| format!("couldn't start the API: {e}"))?;
    let port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(port);
    let url = format!("http://127.0.0.1:{port}");
    let token = new_token();
    write_api_file(api_file, &json!({ "url": url, "token": token, "pid": std::process::id() }))?;

    let server = Arc::new(server);
    let tok = token.clone();
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            let (core, tok) = (core.clone(), tok.clone());
            // One thread per request: `wait` blocks until a job finishes.
            std::thread::spawn(move || handle(req, &core, &tok));
        }
    });
    Ok(ApiInfo { url, token, file: api_file.to_path_buf() })
}

fn write_api_file(path: &Path, v: &Value) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use std::io::Write;
    let mut f = opts.open(path).map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
    f.write_all(serde_json::to_string_pretty(v).unwrap().as_bytes()).map_err(|e| e.to_string())
}

const HELP: &str = "VidCrop API. Send `Authorization: Bearer <token>` (token is in api.json).\n\
GET  /state            full app state\n\
POST /cmd  {\"cmd\": ...}  run a command, e.g. {\"cmd\":\"open\",\"path\":\"/x.mp4\"}\n\
Commands are listed in specs/api.md.\n";

fn handle(mut req: tiny_http::Request, core: &Arc<Core>, token: &str) {
    let headers: Vec<(String, String)> =
        req.headers().iter().map(|h| (h.field.as_str().as_str().to_ascii_lowercase(), h.value.as_str().to_string())).collect();
    let header = |name: &str| headers.iter().find(|(k, _)| k == &name.to_ascii_lowercase()).map(|(_, v)| v.clone());
    let json_resp = |code: u16, v: &Value| {
        Response::from_string(v.to_string())
            .with_status_code(code)
            .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
    };
    if header("Origin").is_some() {
        let _ = req.respond(json_resp(403, &json!({ "error": "browser requests are not allowed" })));
        return;
    }
    if req.url() == "/" {
        let _ = req.respond(Response::from_string(HELP));
        return;
    }
    if header("Authorization").as_deref() != Some(&format!("Bearer {token}")) {
        let _ = req.respond(json_resp(401, &json!({ "error": "missing or wrong token" })));
        return;
    }
    let result = match (req.method(), req.url()) {
        (Method::Get, "/state") => Ok(core.state()),
        (Method::Post, "/cmd") => {
            let mut body = String::new();
            match req.as_reader().read_to_string(&mut body) {
                Err(e) => Err(e.to_string()),
                Ok(_) => serde_json::from_str::<Value>(&body).map_err(|e| format!("bad JSON: {e}")).and_then(|v| core.exec_json(v)),
            }
        }
        _ => {
            let _ = req.respond(json_resp(404, &json!({ "error": "not found; see GET /" })));
            return;
        }
    };
    let _ = match result {
        Ok(v) => req.respond(json_resp(200, &v)),
        Err(e) => req.respond(json_resp(400, &json!({ "error": e }))),
    };
}
