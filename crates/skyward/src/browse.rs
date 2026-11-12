//! Reading a page from an address typed on the screen: the text of it, and
//! the links in it, on a thread of its own so the page keeps up.

use anyhow::{bail, Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// How much of a page is read: past this the rest is dropped, so that a page
/// that never ends cannot fill the memory the recovery has.
const LIMIT: usize = 2 << 20;

/// How a page ended.
pub enum Ended {
    Read(Page),
    Failed(String),
    Stopped,
}

/// What a page holds: its title, its text broken into blocks, and the links
/// the text points at.
pub struct Page {
    pub title: String,
    pub blocks: Vec<Block>,
}

/// One piece of a page: a line of text, or a link with the text that names it.
pub enum Block {
    Text(String),
    /// A heading, shown larger.
    Heading(String),
    Link { text: String, url: String },
    Rule,
}

pub struct Worker {
    ended: Receiver<Ended>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start(url: String, servers: skyward_net::http::Servers) -> Result<Worker> {
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, ended) = mpsc::channel();
        let flag = stop.clone();
        let thread = thread::Builder::new()
            .name("browse".into())
            .spawn(move || {
                let out = match read(&url, &servers, &flag) {
                    Ok(page) => Ended::Read(page),
                    Err(_) if flag.load(Ordering::Relaxed) => Ended::Stopped,
                    Err(e) => Ended::Failed(format!("{e:#}")),
                };
                let _ = tx.send(out);
            })
            .context("start the reading thread")?;
        Ok(Worker { ended, stop, thread: Some(thread) })
    }

    pub fn ended(&self) -> Option<Ended> {
        self.ended.try_recv().ok()
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn read(url: &str, servers: &skyward_net::http::Servers, stop: &AtomicBool) -> Result<Page> {
    let agent = skyward_net::http::agent(servers.clone());
    let mut response = agent.get(url).call().context("ask the server for the page")?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        bail!("the server answered {status}");
    }
    let mut body = Vec::new();
    let mut reader = response.body_mut().with_config().limit(LIMIT as u64).reader();
    use std::io::Read as _;
    let mut chunk = [0u8; 16384];
    loop {
        if stop.load(Ordering::Relaxed) {
            bail!("stopped");
        }
        let n = reader.read(&mut chunk).context("read the page")?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
        if body.len() >= LIMIT {
            break;
        }
    }
    let html = String::from_utf8_lossy(&body);
    Ok(parse(&html, url))
}

/// The page `html` holds, as the reader shows it, with the links in it made
/// whole against `base`.
fn parse(html: &str, base: &str) -> Page {
    let mut title = String::new();
    let mut blocks: Vec<Block> = Vec::new();
    let mut text = String::new();
    // Whether the last thing put down was a link, and the link it was, so that
    // a link's text is not also read as ordinary text.
    let mut in_link: Option<String> = None;
    let mut link_text = String::new();
    let mut skip: Option<&str> = None;

    let bytes = html.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &html[i..];
        if let Some(tag) = skip {
            // Inside <script> or <style>: look only for its end.
            let close = format!("</{tag}");
            match rest.to_ascii_lowercase().find(&close) {
                Some(k) => {
                    i += k + close.len();
                    if let Some(next) = html[i..].find('>') {
                        i += next + 1;
                    }
                    skip = None;
                }
                None => break,
            }
            continue;
        }
        if rest.starts_with('<') {
            let Some(end) = rest.find('>') else { break };
            let tag = &rest[1..end];
            i += end + 1;
            let name = tag
                .trim_start_matches('/')
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            let closing = tag.starts_with('/');
            match name.as_str() {
                "script" | "style" | "noscript" => {
                    if !closing {
                        skip = Some(if name == "script" {
                            "script"
                        } else if name == "style" {
                            "style"
                        } else {
                            "noscript"
                        });
                    }
                }
                "title" => {}
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    flush(&mut text, &mut blocks, Block::Heading as fn(String) -> Block);
                }
                "p" | "div" | "br" | "li" | "tr" | "section" | "article" | "header" | "footer" | "blockquote" | "pre" => {
                    flush(&mut text, &mut blocks, Block::Text as fn(String) -> Block);
                }
                "hr" => {
                    flush(&mut text, &mut blocks, Block::Text as fn(String) -> Block);
                    blocks.push(Block::Rule);
                }
                "a" => {
                    if closing {
                        if let Some(href) = in_link.take() {
                            let said = link_text.trim().to_string();
                            if !said.is_empty() {
                                blocks.push(Block::Link { text: said, url: href });
                            }
                            link_text.clear();
                        }
                    } else if let Some(href) = attribute(tag, "href") {
                        flush(&mut text, &mut blocks, Block::Text as fn(String) -> Block);
                        in_link = Some(absolute(&href, base));
                    }
                }
                _ => {}
            }
            continue;
        }
        // Text: up to the next tag.
        let end = rest.find('<').unwrap_or(rest.len());
        let piece = &rest[..end];
        let decoded = unescape(piece);
        if let Some(_) = in_link {
            link_text.push_str(&decoded);
        } else {
            text.push_str(&decoded);
        }
        i += end;
    }
    flush(&mut text, &mut blocks, Block::Text as fn(String) -> Block);
    if let Some(href) = in_link.take() {
        let said = link_text.trim().to_string();
        if !said.is_empty() {
            blocks.push(Block::Link { text: said, url: href });
        }
    }

    // The title, from <title>, or else the first heading, or else the address.
    if let Some(k) = html.to_ascii_lowercase().find("<title") {
        if let Some(gt) = html[k..].find('>') {
            let start = k + gt + 1;
            if let Some(close) = html[start..].to_ascii_lowercase().find("</title") {
                title = unescape(&html[start..start + close]).trim().to_string();
            }
        }
    }
    if title.is_empty() {
        title = blocks
            .iter()
            .find_map(|b| match b {
                Block::Heading(t) | Block::Text(t) => Some(t.clone()),
                _ => None,
            })
            .unwrap_or_else(|| base.to_string());
    }

    // Nothing but white space between blocks is dropped, and runs of white
    // space inside one are folded to a single space.
    let blocks = blocks
        .into_iter()
        .filter_map(|b| match b {
            Block::Text(t) => {
                let t = fold(&t);
                (!t.is_empty()).then_some(Block::Text(t))
            }
            Block::Heading(t) => {
                let t = fold(&t);
                (!t.is_empty()).then_some(Block::Heading(t))
            }
            other => Some(other),
        })
        .collect();

    Page { title, blocks }
}

/// Put what has been read so far down as a block of `kind`.
fn flush(text: &mut String, blocks: &mut Vec<Block>, kind: fn(String) -> Block) {
    let taken = std::mem::take(text);
    let folded = fold(&taken);
    if !folded.is_empty() {
        blocks.push(kind(folded));
    }
}

/// Runs of white space folded to one space, with the ends trimmed.
fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(ch);
        }
    }
    out
}

/// The value of `name` in `tag`, as `<a href="...">` writes it.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let at = lower.find(name)?;
    let rest = &tag[at + name.len()..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let (quote, rest) = match rest.chars().next()? {
        q @ ('"' | '\'') => (Some(q), &rest[1..]),
        _ => (None, rest),
    };
    let end = match quote {
        Some(q) => rest.find(q).unwrap_or(rest.len()),
        None => rest.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(rest.len()),
    };
    Some(unescape(&rest[..end]))
}

/// `href` made whole against `base`.
fn absolute(href: &str, base: &str) -> String {
    let href = href.trim();
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if href.starts_with("//") {
        let scheme = if base.starts_with("https") { "https:" } else { "http:" };
        return format!("{scheme}{href}");
    }
    if href.starts_with('#') || href.starts_with("javascript:") || href.starts_with("mailto:") {
        return String::new();
    }
    // The scheme and host of the page, then the path.
    let mut parts = base.splitn(4, '/');
    let scheme = parts.next().unwrap_or("https:");
    let host = parts.next().unwrap_or("");
    let root = format!("{scheme}//{host}");
    if href.starts_with('/') {
        return format!("{root}{href}");
    }
    // Relative to the folder the page is in.
    let path = base.split(['?', '#']).next().unwrap_or(base);
    let folder = match path.rfind('/') {
        Some(k) if k > root.len() => &path[..k],
        _ => root.as_str(),
    };
    format!("{folder}/{href}")
}

/// The text `s` names, with the entities a page writes turned back.
fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let Some(semi) = after[..after.len().min(12)].find(';') else {
            out.push('&');
            rest = &after[1..];
            continue;
        };
        let name = &after[1..semi];
        let put = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "hellip" => Some('…'),
            "laquo" => Some('«'),
            "raquo" => Some('»'),
            "copy" => Some('©'),
            "reg" => Some('®'),
            _ => {
                if let Some(num) = name.strip_prefix('#') {
                    let code = if let Some(hex) = num.strip_prefix(['x', 'X']) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        num.parse::<u32>().ok()
                    };
                    code.and_then(char::from_u32)
                } else {
                    None
                }
            }
        };
        match put {
            Some(ch) => {
                out.push(ch);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_read_for_its_text_and_links() {
        let html = "<html><head><title>Hi</title><style>p{color:red}</style></head>\
                    <body><h1>Head</h1><p>Some   text &amp; more.</p>\
                    <a href=\"/a/b\">Go there</a></body></html>";
        let p = parse(html, "https://example.com/x/y");
        assert_eq!(p.title, "Hi");
        assert!(matches!(&p.blocks[0], Block::Heading(t) if t == "Head"));
        assert!(matches!(&p.blocks[1], Block::Text(t) if t == "Some text & more."));
        assert!(matches!(&p.blocks[2], Block::Link { text, url } if text == "Go there" && url == "https://example.com/a/b"));
    }

    #[test]
    fn an_address_is_made_whole_against_the_page() {
        assert_eq!(absolute("/a", "https://e.com/x/y"), "https://e.com/a");
        assert_eq!(absolute("b", "https://e.com/x/y"), "https://e.com/x/b");
        assert_eq!(absolute("//cdn.com/a", "https://e.com/x"), "https://cdn.com/a");
        assert_eq!(absolute("#top", "https://e.com/x"), "");
    }

    #[test]
    fn entities_are_turned_back() {
        assert_eq!(unescape("a &lt; b &amp; c &#65; &#x42;"), "a < b & c A B");
        assert_eq!(unescape("no entities"), "no entities");
        assert_eq!(unescape("stray & alone"), "stray & alone");
    }
}
