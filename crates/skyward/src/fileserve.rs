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
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
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

/// The page that lists `dir`: a card for each entry, the way a drive on the
/// web shows one, with a way up and a way to put files there.
fn listing(root: &Path, dir: &Path) -> String {
    let shown = dir.strip_prefix(root).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let mut cards = String::new();
    if !shown.is_empty() {
        cards.push_str(
            "<a class=card href=\"../\"><div class=icon>&#8593;</div><div class=name>..</div><div class=meta>上级</div></a>\n",
        );
    }
    let mut entries: Vec<_> = match fs::read_dir(dir) {
        Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
        Err(_) => Vec::new(),
    };
    entries.sort_by_key(|e| {
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        (!is_dir, e.file_name().to_string_lossy().to_lowercase())
    });
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let href = format!("/{}{}", url_encode(&name), if is_dir { "/" } else { "" });
        let (icon, meta) = if is_dir {
            ("&#128193;".to_string(), "文件夹".to_string())
        } else {
            (icon_for(&name), size_of(size))
        };
        cards.push_str(&format!(
            "<a class=card href=\"{href}\"><div class=icon>{icon}</div><div class=name title=\"{esc}\">{name}</div><div class=meta>{meta}</div></a>\n",
            esc = html_escape(&name),
        ));
    }
    if cards.is_empty() {
        cards.push_str("<div class=empty>这个文件夹是空的</div>\n");
    }

    // The way up the tree, as breadcrumbs.
    let mut crumbs = "<a href=\"/\">根目录</a>".to_string();
    if !shown.is_empty() {
        let mut walked = String::new();
        for part in shown.split('/').filter(|p| !p.is_empty()) {
            if !walked.is_empty() {
                walked.push('/');
            }
            walked.push_str(part);
            crumbs.push_str(&format!(" <span>/</span> <a href=\"{}/\">{}</a>", url_encode(&walked), html_escape(part)));
        }
    }

    let count = cards.matches("class=card").count().saturating_sub(if shown.is_empty() { 0 } else { 1 });
    // The address a file goes to, made whole from the top: a path the browser
    // resolves against the page's own would put the folder in twice.
    let prefix = if shown.is_empty() { "/".to_string() } else { format!("/{}/", url_encode(&shown)) };
    format!(
        r#"<!doctype html><html lang=zh><head><meta charset=utf-8>
<meta name=viewport content="width=device-width,initial-scale=1">
<title>Skyward 无线传输</title>
<style>
:root{{--bg:#0f1115;--card:#181b22;--card-hover:#1f2430;--line:#262b36;--text:#e6e9ef;--dim:#8b93a3;--accent:#4d8dff}}
*{{box-sizing:border-box}}
body{{margin:0;padding:0;background:var(--bg);color:var(--text);font:15px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif}}
header{{position:sticky;top:0;z-index:10;background:rgba(15,17,21,.92);backdrop-filter:blur(12px);border-bottom:1px solid var(--line)}}
.bar{{max-width:1080px;margin:0 auto;padding:16px 20px;display:flex;align-items:center;gap:16px;flex-wrap:wrap}}
.logo{{display:flex;align-items:center;gap:10px;font-weight:700;font-size:16px;white-space:nowrap}}
.logo span.dot{{width:9px;height:9px;border-radius:50%;background:#3ddc84;box-shadow:0 0 8px #3ddc84}}
.crumbs{{flex:1;min-width:200px;font-size:14px;color:var(--dim);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}
.crumbs a{{color:var(--dim);text-decoration:none}}
.crumbs a:hover{{color:var(--accent)}}
.crumbs span{{opacity:.4;margin:0 4px}}
.actions{{display:flex;gap:8px}}
button{{padding:9px 18px;border:0;border-radius:9px;background:var(--accent);color:#fff;font-size:14px;font-weight:600;cursor:pointer;transition:.15s}}
button:hover{{filter:brightness(1.12)}}
button.ghost{{background:transparent;border:1px solid var(--line);color:var(--text)}}
button.ghost:hover{{border-color:var(--accent);color:var(--accent)}}
main{{max-width:1080px;margin:0 auto;padding:20px}}
.grid{{display:grid;grid-template-columns:repeat(auto-fill,minmax(158px,1fr));gap:12px}}
.card{{display:flex;flex-direction:column;align-items:center;gap:8px;padding:18px 12px;background:var(--card);border:1px solid transparent;border-radius:14px;text-decoration:none;color:inherit;transition:.15s;overflow:hidden}}
.card:hover{{background:var(--card-hover);border-color:var(--line);transform:translateY(-2px)}}
.card .icon{{font-size:32px;line-height:1;filter:saturate(1.1)}}
.card .name{{width:100%;font-size:13px;text-align:center;word-break:break-all;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden}}
.card .meta{{font-size:11px;color:var(--dim)}}
.empty{{grid-column:1/-1;padding:60px 0;text-align:center;color:var(--dim)}}
#drop{{margin-top:20px;padding:36px;border:2px dashed var(--line);border-radius:16px;text-align:center;color:var(--dim);transition:.15s;cursor:pointer}}
#drop.over{{border-color:var(--accent);color:var(--accent);background:rgba(77,141,255,.06)}}
#drop strong{{display:block;font-size:15px;color:var(--text);margin-bottom:6px}}
#bar{{margin-top:14px;height:6px;border-radius:3px;background:var(--line);overflow:hidden;display:none}}
#bar>i{{display:block;height:100%;width:0;background:var(--accent);transition:width .2s}}
footer{{max-width:1080px;margin:0 auto;padding:24px 20px 40px;color:var(--dim);font-size:12px;text-align:center}}
</style></head><body>
<header><div class=bar>
<div class=logo><span class=dot></span>Skyward</div>
<div class=crumbs>{crumbs}</div>
<div class=actions>
<button class=ghost onclick="pick()">上传文件</button>
</div>
</div></header>
<main>
<div class=grid>
{cards}
</div>
<div id=drop onclick="pick()">
<strong>拖放文件到这里，或点击选择</strong>
上传到当前文件夹
</div>
<div id=bar><i></i></div>
</main>
<footer>共 {count} 项 · 由 Skyward 恢复模式提供</footer>
<input id=file type=file multiple hidden>
<script>
const input=document.getElementById('file'),drop=document.getElementById('drop'),bar=document.getElementById('bar'),fill=bar.querySelector('i');
function pick(){{input.click()}}
input.onchange=()=>{{if(input.files.length)up([...input.files]);input.value=''}};
['dragenter','dragover'].forEach(e=>drop.addEventListener(e,ev=>{{ev.preventDefault();drop.classList.add('over')}}));
['dragleave','drop'].forEach(e=>drop.addEventListener(e,ev=>{{ev.preventDefault();drop.classList.remove('over')}}));
drop.addEventListener('drop',ev=>{{if(ev.dataTransfer.files.length)up([...ev.dataTransfer.files])}});
async function up(files){{
  bar.style.display='block';
  for(let i=0;i<files.length;i++){{
    const f=files[i];
    await new Promise((done,no)=>{{
      const x=new XMLHttpRequest();
      x.open('PUT','{prefix}'+encodeURIComponent(f.name));
      x.upload.onprogress=e=>{{if(e.lengthComputable){{
        const pct=((i+e.loaded/e.total)/files.length*100).toFixed(0);
        fill.style.width=pct+'%';
      }}}};
      x.onload=()=>done(); x.onerror=()=>no();
      x.send(f);
    }});
  }}
  fill.style.width='100%';
  location.reload();
}}
</script>
</body></html>"#,
        crumbs = crumbs,
        cards = cards,
        count = count,
        prefix = prefix,
    )
}

/// A picture for a name, by what it ends with.
fn icon_for(name: &str) -> String {
    let ext = name.rsplit('.').next().filter(|e| *e != name).unwrap_or("").to_lowercase();
    let face = match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "heic" | "svg" => "&#128444;",
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "3gp" => "&#127909;",
        "mp3" | "flac" | "wav" | "m4a" | "ogg" | "aac" => "&#127925;",
        "zip" | "7z" | "tar" | "gz" | "xz" | "zst" | "rar" | "bz2" => "&#128230;",
        "apk" | "apks" => "&#128241;",
        "img" | "bin" | "lz4" => "&#128190;",
        "txt" | "md" | "log" => "&#128196;",
        "pdf" => "&#128209;",
        "json" | "xml" | "csv" => "&#128202;",
        _ => "&#128196;",
    };
    face.to_string()
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
