use include_dir::{include_dir, Dir};
use otium_core::{engine, runtime};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

static PUBLIC_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../public");

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        206 => "Partial Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

fn headers(
    socket: &mut TcpStream,
    status: u16,
    mime: &str,
    length: Option<u64>,
    extra: &[(String, String)],
) -> std::io::Result<()> {
    write!(
        socket,
        "HTTP/1.1 {status} {}\r\nContent-Type: {mime}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n",
        reason(status)
    )?;
    if let Some(length) = length {
        write!(socket, "Content-Length: {length}\r\n")?;
    }
    for (key, value) in extra {
        if !key.contains(['\r', '\n']) && !value.contains(['\r', '\n']) {
            write!(socket, "{key}: {value}\r\n")?;
        }
    }
    write!(socket, "\r\n")
}

fn bytes(
    socket: &mut TcpStream,
    status: u16,
    mime: &str,
    body: &[u8],
    head: bool,
) -> std::io::Result<()> {
    headers(
        socket,
        status,
        mime,
        Some(body.len() as u64),
        &[("Cache-Control".into(), "no-cache".into())],
    )?;
    if !head {
        socket.write_all(body)?;
    }
    Ok(())
}

fn get_mime(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".wasm") {
        "application/wasm"
    } else if path.ends_with(".json") {
        "application/json"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else if path.ends_with(".woff") {
        "font/woff"
    } else if path.ends_with(".ico") {
        "image/x-icon"
    } else if path.ends_with(".webp") {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

fn serve(socket: &mut TcpStream) -> std::io::Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(10)))?;
    socket.set_write_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(socket.try_clone()?);
    let mut line = String::new();
    reader.by_ref().take(8193).read_line(&mut line)?;
    if line.len() > 8192 {
        return bytes(socket, 400, "text/plain", b"Request too large", false);
    }
    let parts: Vec<_> = line.split_whitespace().collect();
    if parts.len() != 3 {
        return bytes(socket, 400, "text/plain", b"Invalid request", false);
    }
    let (method, target) = (parts[0].to_owned(), parts[1].to_owned());
    let head = method == "HEAD";
    if !["GET", "HEAD", "OPTIONS"].contains(&method.as_str()) {
        return bytes(socket, 405, "text/plain", b"Read-only API", head);
    }
    let mut range = None;
    let mut total = line.len();
    loop {
        line.clear();
        reader.by_ref().take(8193).read_line(&mut line)?;
        total += line.len();
        if total > 32768 || line.len() > 8192 {
            return bytes(socket, 400, "text/plain", b"Request too large", head);
        }
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("range") {
                range = Some(value.trim().to_owned());
            }
        }
    }
    if method == "OPTIONS" {
        return headers(
            socket,
            204,
            "text/plain",
            Some(0),
            &[
                ("Access-Control-Allow-Origin".into(), "*".into()),
                (
                    "Access-Control-Allow-Methods".into(),
                    "GET, HEAD, OPTIONS".into(),
                ),
            ],
        );
    }

    let path = target.split('?').next().unwrap_or("/");

    if path == "/api/media" || path == "/api/cinemana/subtitle" {
        let response = runtime().block_on(engine().open(&target, range.as_deref()));
        return match response {
            Ok(response) => {
                let mut body = response.body;
                let extra: Vec<_> = response.headers.into_iter().collect();
                headers(socket, response.status, &response.mime, None, &extra)?;
                if !head {
                    loop {
                        let chunk = runtime()
                            .block_on(body.read(65536))
                            .map_err(std::io::Error::other)?;
                        if chunk.is_empty() {
                            break;
                        }
                        socket.write_all(&chunk)?;
                    }
                }
                Ok(())
            }
            Err(_) => bytes(socket, 502, "text/plain", b"Media unavailable", head),
        };
    }

    if path.starts_with("/api/") {
        let response = runtime().block_on(engine().request(&target));
        return bytes(
            socket,
            response.status,
            "application/json; charset=utf-8",
            response.value.to_string().as_bytes(),
            head,
        );
    }

    // Static file serving from embedded PUBLIC_DIR
    let rel_path = path.trim_start_matches('/');
    let target_path = if rel_path.is_empty() {
        "index.html"
    } else {
        rel_path
    };

    if let Some(file) = PUBLIC_DIR.get_file(target_path) {
        let contents = file.contents();
        let mime = get_mime(target_path);
        return bytes(socket, 200, mime, contents, head);
    }

    // Try target_path/index.html
    let dir_index = format!("{}/index.html", target_path.trim_end_matches('/'));
    if let Some(file) = PUBLIC_DIR.get_file(&dir_index) {
        let contents = file.contents();
        return bytes(socket, 200, "text/html; charset=utf-8", contents, head);
    }

    // Fallback to index.html for SPA routing
    if let Some(index_file) = PUBLIC_DIR.get_file("index.html") {
        return bytes(socket, 200, "text/html; charset=utf-8", index_file.contents(), head);
    }

    bytes(socket, 404, "text/plain", b"Not found", head)
}

pub fn start_embedded_server() -> std::io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    
    std::thread::spawn(move || {
        let active = Arc::new(AtomicUsize::new(0));
        for socket in listener.incoming() {
            let Ok(mut socket) = socket else {
                continue;
            };
            if active.fetch_add(1, Ordering::AcqRel) >= 64 {
                active.fetch_sub(1, Ordering::AcqRel);
                let _ = socket.write_all(b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
                continue;
            }
            let active = active.clone();
            std::thread::spawn(move || {
                let _ = serve(&mut socket);
                active.fetch_sub(1, Ordering::AcqRel);
            });
        }
    });

    Ok(port)
}
