//! `vidcrop` command-line tool: headless editing, the API server without a window,
//! and a tiny client for talking to a running app.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::Arc;
use vidcrop_core::session::{Core, Headless};
use vidcrop_core::{api, ffmpeg};

const USAGE: &str = "\
vidcrop probe FILE
    Print what VidCrop knows about a video, as JSON.
vidcrop export FILE [-o OUT] [--crop W:H:X:Y] [--cut START-END]... [--fast]
    Crop and cut without opening a window. Times are in seconds.
vidcrop serve [--port N]
    Run the API with no window (for scripts and tests). Prints the URL and token.
vidcrop api JSON
    Send one command to a running VidCrop (app or `serve`), e.g.
    vidcrop api '{\"cmd\":\"state\"}'";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("probe") if args.len() == 2 => ffmpeg::probe(&args[1]).map(|m| serde_json::to_value(m).unwrap()),
        Some("export") if args.len() >= 2 => export(&args[1..]),
        Some("serve") => serve(&args[1..]),
        Some("api") if args.len() == 2 => call_api(&args[1]),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    match result {
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

fn run(core: &Arc<Core>, v: Value) -> Result<Value, String> {
    core.exec_json(v)
}

fn export(args: &[String]) -> Result<Value, String> {
    let core = Core::new(Box::new(Headless));
    run(&core, json!({ "cmd": "open", "path": args[0] }))?;
    let (mut output, mut mode) = (Value::Null, "exact");
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().cloned().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "-o" => output = json!(val()?),
            "--fast" => mode = "fast",
            "--crop" => {
                let v = val()?;
                let n: Vec<u32> = v.split(':').map(|x| x.parse().map_err(|_| format!("bad --crop {v}"))).collect::<Result<_, _>>()?;
                let [w, h, x, y] = n[..] else { return Err("--crop wants W:H:X:Y".into()) };
                run(&core, json!({ "cmd": "set_crop", "rect": { "x": x, "y": y, "w": w, "h": h } }))?;
            }
            "--cut" => {
                let v = val()?;
                let (s, e) = v.split_once('-').ok_or(format!("bad --cut {v}"))?;
                let (s, e): (f64, f64) = (s.parse().map_err(|_| "bad start")?, e.parse().map_err(|_| "bad end")?);
                run(&core, json!({ "cmd": "delete_range", "start": s, "end": e }))?;
            }
            _ => return Err(format!("unknown option {a}")),
        }
    }
    let started = run(&core, json!({ "cmd": "export", "output": output, "mode": mode }))?;
    eprintln!("saving to {}", started["output"].as_str().unwrap_or(""));
    let job = run(&core, json!({ "cmd": "wait", "job": started["job"] }))?;
    if job["status"] != "done" {
        return Err(job["error"].as_str().unwrap_or("save failed").to_string());
    }
    Ok(job)
}

fn serve(args: &[String]) -> Result<Value, String> {
    let port = match args {
        [flag, p] if flag == "--port" => p.parse().map_err(|_| "bad port")?,
        [] => 0,
        _ => return Err("usage: vidcrop serve [--port N]".into()),
    };
    let core = Core::new(Box::new(Headless));
    let info = api::start(core, port, &api::default_api_file())?;
    println!("{}", json!({ "url": info.url, "token": info.token, "api_file": info.file }));
    std::io::stdout().flush().ok();
    loop {
        std::thread::park();
    }
}

/// Minimal HTTP client so tests don't need curl.
fn call_api(body: &str) -> Result<Value, String> {
    let file = api::default_api_file();
    let conn: Value = serde_json::from_str(&std::fs::read_to_string(&file).map_err(|e| format!("{}: {e} (is VidCrop running?)", file.display()))?)
        .map_err(|e| e.to_string())?;
    let url = conn["url"].as_str().ok_or("bad api file")?;
    let addr = url.trim_start_matches("http://");
    let mut s = std::net::TcpStream::connect(addr).map_err(|e| format!("can't reach VidCrop at {url}: {e}"))?;
    let req = format!(
        "POST /cmd HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        conn["token"].as_str().unwrap_or(""),
        body.len()
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut resp = String::new();
    s.read_to_string(&mut resp).map_err(|e| e.to_string())?;
    let (head, body) = resp.split_once("\r\n\r\n").ok_or("bad response")?;
    let v: Value = serde_json::from_str(body).map_err(|e| format!("bad response: {e}"))?;
    if head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.0 200") {
        Ok(v)
    } else {
        Err(v["error"].as_str().unwrap_or(body).to_string())
    }
}
