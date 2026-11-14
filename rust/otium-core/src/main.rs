//! Small read-only HTTP host for the same engine bundled in Android.
//! Deliberately binds loopback by default; set OTIUM_BIND for a reverse-proxied deployment.
use otium_core::{engine, runtime};
use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.get(1).map(String::as_str) == Some("--request") {
        let response = runtime()
            .block_on(engine().request(args.get(2).map(String::as_str).unwrap_or("/api/health")));
        println!("{}", response.value);
        if response.status >= 400 {
            std::process::exit(1);
        }
        return;
    }
    let port = env::var("PORT").unwrap_or_else(|_| "3000".into());
    let bind = env::var("ZOET_BIND")
        .or_else(|_| env::var("OTIUM_BIND"))
        .unwrap_or_else(|_| format!("127.0.0.1:{port}"));
    let public = env::var_os("ZOET_PUBLIC")
        .or_else(|| env::var_os("OTIUM_PUBLIC"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../public"));
    let public = public
        .canonicalize()
        .expect("ZOET_PUBLIC or OTIUM_PUBLIC must point to the public directory");
    let listener = TcpListener::bind(&bind).expect("Unable to bind server");
    println!("ZOET by Otium Rust server: http://{bind}");
    let active = Arc::new(AtomicUsize::new(0));
    for socket in listener.incoming() {
        let Ok(mut socket) = socket else {
            continue;
        };
        if active.fetch_add(1, Ordering::AcqRel) >= 64 {
            active.fetch_sub(1, Ordering::AcqRel);
            let _=socket.write_all(b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
            continue;
        }
        let public = public.clone();
        let active = active.clone();
        std::thread::spawn(move || {
            let _ = serve(&mut socket, &public);
            active.fetch_sub(1, Ordering::AcqRel);
        });
    }
}
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
    write!(socket,"HTTP/1.1 {status} {}\r\nContent-Type: {mime}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n",reason(status))?;
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
        &[
            ("Cache-Control".into(), "no-store".into()),
            ("Access-Control-Allow-Origin".into(), "*".into()),
            ("Access-Control-Allow-Methods".into(), "GET, HEAD, OPTIONS".into()),
            ("Access-Control-Allow-Headers".into(), "*".into()),
            ("Access-Control-Allow-Private-Network".into(), "true".into()),
        ],
    )?;
    if !head {
        socket.write_all(body)?;
    }
    Ok(())
}
fn serve(socket: &mut TcpStream, public: &std::path::Path) -> std::io::Result<()> {
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
                ("Access-Control-Allow-Headers".into(), "*".into()),
                ("Access-Control-Allow-Private-Network".into(), "true".into()),
                ("Access-Control-Max-Age".into(), "86400".into()),
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
    let decoded = url::form_urlencoded::parse(format!("x={}", path.replace('+', "%2B")).as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default();
    let path = public.join(decoded.trim_start_matches('/'));
    let path = if path.is_dir() {
        path.join("index.html")
    } else {
        path
    };
    let path = match path.canonicalize() {
        Ok(p) if p.starts_with(public) => p,
        Ok(_) => return bytes(socket, 403, "text/plain", b"Forbidden", head),
        Err(_) => public.join("index.html"),
    };
    let mut file = match fs::File::open(&path) {
        Ok(file) => file,
        Err(_) => return bytes(socket, 404, "text/plain", b"Not found", head),
    };
    let mime = match path.extension().and_then(|s| s.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "wasm" => "application/wasm",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ico" => "image/x-icon",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    };
    headers(
        socket,
        200,
        mime,
        Some(file.metadata()?.len()),
        &[
            ("Cache-Control".into(), "no-cache".into()),
            ("Access-Control-Allow-Origin".into(), "*".into()),
        ],
    )?;
    if !head {
        std::io::copy(&mut file, socket)?;
    }
    Ok(())
}
