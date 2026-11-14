//! The download page and serving a folder: the two ways files come in and
//! out over the network, and how each of them stands.

use super::*;

/// Where serving the files in a folder stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Serving {
    #[default]
    Idle,
    Running {
        /// The address another device opens, once the server answers.
        at: String,
        /// How many requests have been answered.
        served: u64,
    },
    Failed(String),
}

/// Where a file being fetched from an address stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Fetch {
    #[default]
    Idle,
    Running {
        done: u64,
        total: Option<u64>,
    },
    Done {
        path: String,
        bytes: u64,
    },
    Failed(String),
    Stopped,
}

/// How the file manager orders its entries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    #[default]
    Name,
    Size,
    Modified,
    Type,
}

impl SortKey {
    /// The next way of ordering, round and round.
    pub fn next(self) -> SortKey {
        match self {
            SortKey::Name => SortKey::Size,
            SortKey::Size => SortKey::Modified,
            SortKey::Modified => SortKey::Type,
            SortKey::Type => SortKey::Name,
        }
    }

    /// What the page calls it.
    pub fn name(self, l: crate::lang::Lang) -> &'static str {
        match self {
            SortKey::Name => l.pick(["名称", "名稱", "Name"]),
            SortKey::Size => l.pick(["大小", "大小", "Size"]),
            SortKey::Modified => l.pick(["修改时间", "修改時間", "Modified"]),
            SortKey::Type => l.pick(["类型", "類型", "Type"]),
        }
    }
}

/// Which kind of file a name is, for sorting by type: the extension's group.
fn type_group(name: &str) -> u8 {
    let ext = name.rsplit('.').next().filter(|e| *e != name).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "heic" | "svg" => 1,
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "3gp" => 2,
        "mp3" | "flac" | "wav" | "m4a" | "ogg" | "aac" => 3,
        "zip" | "7z" | "tar" | "gz" | "xz" | "zst" | "rar" | "bz2" => 4,
        "apk" | "apks" | "img" | "bin" | "lz4" => 5,
        "txt" | "md" | "json" | "xml" | "log" | "csv" => 6,
        _ => 7,
    }
}

/// Put the entries in the order the file manager shows them: the folders
/// first when asked, then by the key and the side chosen.
pub fn sort_entries(rows: &mut [FileRow], key: SortKey, desc: bool, folders_first: bool) {
    use std::cmp::Ordering;
    let folder = |r: &FileRow| matches!(r.kind, FileKind::Folder { .. } | FileKind::Link { folder: true, .. });
    rows.sort_by(|a, b| {
        if folders_first && folder(a) != folder(b) {
            return if folder(a) { Ordering::Less } else { Ordering::Greater };
        }
        let name = || a.name.to_lowercase().cmp(&b.name.to_lowercase());
        let by = match key {
            SortKey::Name => name(),
            SortKey::Size => a.size.cmp(&b.size).then_with(name),
            SortKey::Modified => a.modified.cmp(&b.modified).then_with(name),
            SortKey::Type => type_group(&a.name).cmp(&type_group(&b.name)).then_with(name),
        };
        if desc { by.reverse() } else { by }
    });
}

impl App {
    /// Order the file manager's entries the way its state now says, and keep
    /// that order for the next start.
    pub(super) fn resort_files(&mut self) {
        if let Ok(entries) = &mut self.state.files.entries {
            sort_entries(entries, self.state.sort_key, self.state.sort_desc, self.state.sort_folders_first);
        }
        self.refresh();
        self.sort_save = Some((self.state.sort_key, self.state.sort_desc, self.state.sort_folders_first));
    }

    /// The order the file manager shows entries in, as the runtime last read
    /// it from the file it is kept in.
    pub fn take_sort(&mut self, key: SortKey, desc: bool, folders_first: bool) {
        self.state.sort_key = key;
        self.state.sort_desc = desc;
        self.state.sort_folders_first = folders_first;
        if let Ok(entries) = &mut self.state.files.entries {
            sort_entries(entries, key, desc, folders_first);
        }
        self.refresh();
    }

    /// The order to write down, once, after a change.
    pub fn sort_to_save(&mut self) -> Option<(SortKey, bool, bool)> {
        self.sort_save.take()
    }

    /// Begin fetching what the address typed on the page names.
    pub(super) fn fetch_start(&mut self, now: u64) -> Vec<Effect> {
        let l = self.state.lang;
        if self.fetching() {
            return Vec::new();
        }
        let url = self.state.fetch_url.trim().to_string();
        if url.is_empty() {
            self.say(l.pick(["请输入网址", "請輸入網址", "Type an address"]), now);
            return Vec::new();
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            self.say(
                l.pick([
                    "网址要以 http:// 或 https:// 开头",
                    "網址要以 http:// 或 https:// 開頭",
                    "The address must start with http:// or https://",
                ]),
                now,
            );
            return Vec::new();
        }
        self.state.fetch = Fetch::Running { done: 0, total: None };
        self.state.fetch_ended_at = None;
        self.refresh();
        vec![Effect::Fetch(url)]
    }

    /// Take the address from the small file a computer can leave behind.
    pub(super) fn fetch_from_file(&mut self, now: u64) {
        let l = self.state.lang;
        let at = std::path::Path::new("/storage/emulated/0/Download/link.txt");
        match std::fs::read_to_string(at) {
            Ok(text) => match text.lines().map(str::trim).find(|line| !line.is_empty()) {
                Some(link) => {
                    self.state.fetch_url = link.to_string();
                    self.refresh();
                }
                None => {
                    self.state.fetch = Fetch::Failed(
                        l.pick(["文件里没有网址", "檔案裡沒有網址", "The file holds no address"]).to_string(),
                    )
                }
            },
            Err(_) => {
                self.state.fetch = Fetch::Failed(
                    l.pick([
                        "读不到 /sdcard/Download/link.txt",
                        "讀不到 /sdcard/Download/link.txt",
                        "Could not read /sdcard/Download/link.txt",
                    ])
                    .to_string(),
                );
            }
        }
        let _ = now;
        self.refresh();
    }

    /// How far the fetch under way has come, as the runtime reports it.
    pub fn fetch_progress(&mut self, done: u64, total: Option<u64>) {
        let stood = match self.state.fetch {
            Fetch::Running { done: was, total: had } => (was, had),
            _ => (u64::MAX, None),
        };
        if stood != (done, total) {
            self.state.fetch = Fetch::Running { done, total };
            self.dirty = true;
        }
    }

    /// How the fetch ended.
    pub fn fetch_ended(&mut self, ended: Result<(String, u64), String>, now: u64) {
        self.state.fetch = match ended {
            Ok((path, bytes)) => Fetch::Done { path, bytes },
            Err(why) => Fetch::Failed(why),
        };
        self.state.fetch_ended_at = Some(now);
        self.refresh();
    }

    /// The fetch was stopped before it ended.
    pub fn fetch_stopped(&mut self, now: u64) {
        self.state.fetch = Fetch::Stopped;
        self.state.fetch_ended_at = Some(now);
        self.refresh();
    }

    /// How long a card left by a finished fetch stays before the page is as it
    /// was: a failure is left up longer, so that it can be read.
    pub(super) fn fetch_card_left(&self, now: u64) -> bool {
        let Some(at) = self.state.fetch_ended_at else { return false };
        let stay = match self.state.fetch {
            Fetch::Failed(_) => 12000,
            _ => 6000,
        };
        now.saturating_sub(at) >= stay
    }

    /// Take away the card a finished fetch left, and the address with it.
    pub(super) fn clear_fetch_card(&mut self) {
        if matches!(self.state.fetch, Fetch::Running { .. } | Fetch::Idle) {
            return;
        }
        self.state.fetch = Fetch::Idle;
        self.state.fetch_ended_at = None;
        self.state.fetch_url.clear();
        self.refresh();
    }

    /// Whether a fetch is under way.
    pub fn fetching(&self) -> bool {
        matches!(self.state.fetch, Fetch::Running { .. })
    }

    /// How many requests the server has answered.
    pub fn serving_count(&mut self, served: u64) {
        if let Serving::Running { at, served: was } = &self.state.serving
            && *was != served
        {
            self.state.serving = Serving::Running { at: at.clone(), served };
            self.dirty = true;
        }
    }

    /// The server could not be brought up.
    pub fn serve_failed(&mut self, why: String) {
        self.state.serving = Serving::Failed(why);
        self.refresh();
    }

    /// The address another device reaches the page at.
    pub fn serving_at(&mut self, at: String) {
        if let Serving::Running { served, .. } = self.state.serving {
            self.state.serving = Serving::Running { at, served };
            self.refresh();
        }
    }

    /// The server stopped.
    pub fn serving_stopped(&mut self) {
        self.state.serving = Serving::Idle;
        self.refresh();
    }
}

impl App {
    /// Whether the panel is lit: touches are worked by the page then, and a
    /// dark one is where the double tap that lights it is looked for.
    pub fn panel_lit(&self) -> bool {
        self.display_on
    }

    /// The date the lock shows, as the system keeps it.
    pub fn set_date(&mut self, date: String) {
        if self.date != date {
            self.date = date;
            if self.locked {
                self.dirty = true;
            }
        }
    }

    /// Open `screen` over the page shown.
    pub fn go(&mut self, screen: Screen) {
        self.open(screen);
    }

    /// The page another device reaches to hand a link over, once it serves.
    pub fn set_portal(&mut self, at: Option<String>) {
        if self.state.fetch_portal != at {
            self.state.fetch_portal = at;
            self.dirty = true;
        }
    }

    /// A link another device typed on that page.
    pub fn hand_link(&mut self, link: String, now: u64) {
        self.state.fetch_url = link;
        let said = self.state.lang.pick([
            "已从另一台手机取回链接",
            "已從另一台手機取回連結",
            "The link came from the other device",
        ]);
        self.say(said, now);
        self.refresh();
    }

    /// How far along the bar under the sign has come, 0 to 1.
    pub fn data_progress(&self) -> f32 {
        self.state.data_progress
    }

    /// Whether the decrypt page is up and the data partition is still being
    /// opened, which is when the ring and the bar move.
    pub fn decrypt_waiting(&self) -> bool {
        self.screen() == Screen::Decrypt && matches!(self.state.data, Data::Closed | Data::Opening)
    }
}
