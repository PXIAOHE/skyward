//! A server on the network the phone is on, for the files in a folder: a
//! browser on another device lists them, takes them, and puts others there.
//!
//! It answers on a port of its own, on every address the phone has, so that a
//! computer that reached the phone over a router and one joined to the phone's
//! own hotspot both find it.

use anyhow::{Context, Result};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// The port the files are served on.
pub const PORT: u16 = 8080;

/// The most a request's headers may take, and the most one upload may be.
const HEAD_MAX: usize = 32 << 10;

/// What the server is doing, as the page shows it.
pub struct Server {
    stop: Arc<AtomicBool>,
    /// How many requests have been answered, so the page can say it is alive.
    served: Arc<AtomicU64>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Server {
    /// Serve the files under `root` on [`PORT`], until the server is stopped.
    pub fn start(root: PathBuf) -> Result<Server> {
        let listener = TcpListener::bind(("0.0.0.0", PORT)).with_context(|| format!("listen on port {PORT}"))?;
        // Nothing waits in accept: a thread parked there would keep the port
        // bound after the page is left, and the next visit could not serve it.
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let served = Arc::new(AtomicU64::new(0));
        let (flag, count) = (stop.clone(), served.clone());
        let thread = thread::Builder::new()
            .name("files".into())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let root = root.clone();
                            let count = count.clone();
                            // Each request is answered on its own thread, so a
                            // large transfer does not hold up the rest.
                            let _ = thread::Builder::new().name("files-req".into()).spawn(move || {
                                let _ = answer(stream, &root);
                                count.fetch_add(1, Ordering::Relaxed);
                            });
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(80)),
                        Err(_) => thread::sleep(Duration::from_millis(80)),
                    }
                }
            })
            .context("start the thread serving the files")?;
        Ok(Server { stop, served, thread: Some(thread) })
    }

    /// How many requests have been answered.
    pub fn served(&self) -> u64 {
        self.served.load(Ordering::Relaxed)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One request: its method, its path, and what it carries.
struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn answer(mut stream: TcpStream, root: &Path) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let Some(request) = read_request(&mut stream)? else { return Ok(()) };

    // A path outside the folder served is turned away: `..` in an address must
    // not reach the rest of the phone.
    let wanted = match resolve(root, &request.path) {
        Some(path) => path,
        None => return reply(&mut stream, 403, "text/plain; charset=utf-8", b"forbidden"),
    };

    match request.method.as_str() {
        "GET" | "HEAD" => {
            if wanted.is_dir() {
                let body = listing(root, &wanted);
                let mut head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                if request.method == "HEAD" {
                    head = head.replace(&format!("Content-Length: {}\r\n", body.len()), &format!("Content-Length: {}\r\n", body.len()));
                }
                stream.write_all(head.as_bytes())?;
                if request.method == "GET" {
                    stream.write_all(body.as_bytes())?;
                }
            } else if wanted.is_file() {
                let mut file = fs::File::open(&wanted).with_context(|| format!("open {}", wanted.display()))?;
                let len = file.metadata()?.len();
                let name = wanted.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"{name}\"\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(head.as_bytes())?;
                if request.method == "GET" {
                    let mut buf = vec![0u8; 1 << 16];
                    loop {
                        let n = file.read(&mut buf)?;
                        if n == 0 {
                            break;
                        }
                        stream.write_all(&buf[..n])?;
                    }
                }
            } else {
                reply(&mut stream, 404, "text/plain; charset=utf-8", b"not found")?;
            }
        }
        "PUT" => {
            if let Some(dir) = wanted.parent() {
                let _ = fs::create_dir_all(dir);
            }
            fs::write(&wanted, &request.body).with_context(|| format!("write {}", wanted.display()))?;
            reply(&mut stream, 201, "text/plain; charset=utf-8", b"written")?;
        }
        "DELETE" => match fs::remove_file(&wanted) {
            Ok(()) => reply(&mut stream, 200, "text/plain; charset=utf-8", b"removed")?,
            Err(_) => reply(&mut stream, 404, "text/plain; charset=utf-8", b"not found")?,
        },
        _ => reply(&mut stream, 405, "text/plain; charset=utf-8", b"not allowed")?,
    }
    Ok(())
}

/// The request `stream` carries, or nothing where it carries none.
fn read_request(stream: &mut TcpStream) -> Result<Option<Request>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    // The headers, up to the empty line.
    let mut length = 0usize;
    let mut taken = 0usize;
    loop {
        let mut header = String::new();
        let n = reader.read_line(&mut header)?;
        if n == 0 {
            break;
        }
        taken += n;
        if taken > HEAD_MAX {
            return Ok(None);
        }
        let trimmed = header.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut body).with_context(|| format!("read the {length} bytes sent"))?;
    }
    Ok(Some(Request { method, path, body }))
}

fn reply(stream: &mut TcpStream, code: u16, kind: &str, body: &[u8]) -> Result<()> {
    let head = format!(
        "HTTP/1.1 {code} {}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        match code {
            200 => "OK",
            201 => "Created",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            _ => "OK",
        },
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    Ok(())
}

/// The file `path` names, under `root`; nothing where it would reach outside.
fn resolve(root: &Path, path: &str) -> Option<PathBuf> {
    let path = path.split(['?', '#']).next().unwrap_or("/");
    let decoded = percent_decode(path);
    let mut out = root.to_path_buf();
    for part in decoded.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            name => out.push(name),
        }
    }
    Some(out)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(&text[i + 1..i + 3], 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// The page that lists `dir`, with a way up and a way to put a file there.
fn listing(root: &Path, dir: &Path) -> String {
    let shown = dir.strip_prefix(root).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let mut rows = String::new();
    if !shown.is_empty() {
        rows.push_str("<tr><td colspan=2><a href=\"../\">..</a></td></tr>\n");
    }
    let mut entries: Vec<_> = match fs::read_dir(dir) {
        Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
        Err(_) => Vec::new(),
    };
    entries.sort_by_key(|e| {
        let dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        (!dir, e.file_name().to_string_lossy().to_lowercase())
    });
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let href = format!("{}{}", url_encode(&name), if is_dir { "/" } else { "" });
        let shown_name = if is_dir { format!("{name}/") } else { name.clone() };
        let size_text = if is_dir { String::new() } else { size_of(size) };
        rows.push_str(&format!(
            "<tr><td><a href=\"{href}\">{shown_name}</a></td><td class=size>{size_text}</td></tr>\n"
        ));
    }
    let title = if shown.is_empty() { "Files".to_string() } else { shown.clone() };
    format!(
        r#"<!doctype html><html lang=zh><head><meta charset=utf-8>
<meta name=viewport content="width=device-width,initial-scale=1">
<title>{title}</title>
<style>
body{{font:15px/1.6 system-ui,sans-serif;margin:0;padding:16px;background:#111;color:#eee}}
h1{{font-size:18px;margin:0 0 12px}}
table{{width:100%;border-collapse:collapse}}
td{{padding:8px 4px;border-bottom:1px solid #2a2a2a}}
.size{{text-align:right;color:#888;white-space:nowrap}}
a{{color:#4da3ff;text-decoration:none}}
form{{margin-top:20px;padding-top:16px;border-top:1px solid #2a2a2a}}
input[type=file]{{color:#eee}}
button{{margin-top:8px;padding:8px 16px;border:0;border-radius:8px;background:#4da3ff;color:#000;font-weight:600}}
</style></head><body>
<h1>{title}</h1>
<table>
{rows}
</table>
<form method=post enctype=multipart/form-data onsubmit="up(event)">
<input type=file name=f multiple>
<button>上传到此处</button>
</form>
<script>
async function up(e){{
  e.preventDefault();
  const files = document.querySelector('input[type=file]').files;
  for (const f of files) {{
    await fetch('{shown_prefix}' + encodeURIComponent(f.name), {{method:'PUT', body:f}});
  }}
  location.reload();
}}
</script>
</body></html>"#,
        title = html_escape(&title),
        rows = rows,
        shown_prefix = if shown.is_empty() { String::new() } else { format!("{}/", url_encode(&shown)) },
    )
}

fn size_of(bytes: u64) -> String {
    const K: f64 = 1024.0;
    let b = bytes as f64;
    if b >= K * K * K {
        format!("{:.1} GB", b / (K * K * K))
    } else if b >= K * K {
        format!("{:.1} MB", b / (K * K))
    } else if b >= K {
        format!("{:.0} KB", b / K)
    } else {
        format!("{bytes} B")
    }
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn url_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*byte as char),
            b'/' => out.push('/'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}
