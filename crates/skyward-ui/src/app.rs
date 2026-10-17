use crate::canvas::Canvas;
use crate::font::Fonts;
use crate::geom::Rect;
use crate::pages::{self, Ctx};
use crate::theme::Theme;
use crate::widgets::{self, Bar};
use skyward_info::{Charge, DataState, DeviceInfo};

/// The pages of the recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Decrypt,
    Home,
    Settings,
    Reboot,
    Wipe,
    About,
    Specs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    Vibrate,
    Sideload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirm {
    Reboot,
    Wipe,
}

/// Where the reboot page sends the device, in the order it lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    System,
    Recovery,
    Fastboot,
    Bootloader,
    Edl,
    PowerOff,
}

pub const TARGETS: [Target; 6] = [Target::System, Target::Recovery, Target::Fastboot, Target::Bootloader, Target::Edl, Target::PowerOff];

/// What a control does when touched.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Open(Screen),
    /// Leave the unlock page for the home page without keeping it behind.
    Skip,
    Back,
    Theme { dark: bool },
    Toggle(Switch),
    RebootTarget(usize),
    WipeItem(usize),
    /// The brightness track: touching or dragging it sets the level.
    Brightness,
    /// A swipe-to-confirm track; dragging the knob to the far end confirms.
    Confirm(Confirm),
    /// The pattern grid on the unlock page.
    Pattern,
    /// A control whose work is not wired up yet; touching it says so.
    Unavailable(&'static str),
}

pub struct Hit {
    pub rect: Rect,
    pub action: Action,
}

/// A rendered page in page coordinates. The status-bar band at its top is
/// left empty: the bar is laid over the page when it is shown.
pub struct Frame {
    pub canvas: Canvas,
    pub hits: Vec<Hit>,
    pub title: &'static str,
    pub back: bool,
    /// The page sits on the About backdrop.
    pub hero: bool,
    /// Centre and span of the pattern grid, on the unlock page.
    pub pattern: Option<(f32, f32, f32)>,
}

impl Frame {
    /// The topmost control under a point in page coordinates.
    pub fn hit(&self, x: f32, y: f32) -> Option<&Hit> {
        self.hits.iter().rev().find(|h| h.rect.contains(x, y))
    }
}

/// Choices the pages show and the user changes.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub dark: bool,
    pub brightness: f32,
    pub vibrate: bool,
    pub sideload: bool,
    pub reboot: usize,
    pub wipe: [bool; 4],
    pub pattern: Vec<usize>,
    /// How far the swipe-to-confirm knob is dragged, 0 to 1.
    pub confirm: f32,
}

impl Default for State {
    fn default() -> Self {
        State {
            dark: false,
            brightness: 0.5,
            vibrate: true,
            sideload: false,
            reboot: 0,
            wipe: [true, false, true, false],
            pattern: Vec::new(),
            confirm: 0.0,
        }
    }
}

/// One finger, in screen pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Touch {
    Down(f32, f32),
    Move(f32, f32),
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Power,
    VolumeUp,
    VolumeDown,
    Back,
}

/// Work the runtime carries out on the UI's behalf.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// Set the panel backlight to this fraction of its maximum.
    Backlight(f32),
    /// Turn the panel off or back on.
    Display(bool),
    Reboot(Target),
}

enum Drag {
    Idle,
    /// Finger down and not yet past the touch slop. `edge` is set when the
    /// press began at a screen edge (true for the left one).
    Press { x: f32, y: f32, action: Option<Action>, edge: Option<bool> },
    Scroll { last: f32 },
    /// Swiping in from a screen edge; past the threshold, lifting goes back.
    Edge { start: f32, from_left: bool, armed: bool },
    Slider(Rect),
    Confirm { which: Confirm, track: Rect, grab: f32 },
    Pattern,
}

const TOAST_MS: u64 = 2000;

pub struct App {
    pub state: State,
    stack: Vec<(Screen, f32)>,
    frame: Option<Frame>,
    toast: Option<(String, u64)>,
    drag: Drag,
    display_on: bool,
    dirty: bool,
    w: u32,
    h: u32,
    px_per_dp: f32,
}

impl App {
    pub fn new(start: Screen, w: u32, h: u32, px_per_dp: f32) -> Self {
        App {
            state: State::default(),
            stack: vec![(start, 0.0)],
            frame: None,
            toast: None,
            drag: Drag::Idle,
            display_on: true,
            dirty: true,
            w,
            h,
            px_per_dp,
        }
    }

    pub fn theme(&self) -> Theme {
        if self.state.dark {
            Theme::dark(self.w, self.h, self.px_per_dp)
        } else {
            Theme::light(self.w, self.h, self.px_per_dp)
        }
    }

    pub fn screen(&self) -> Screen {
        self.top().0
    }

    fn top(&self) -> &(Screen, f32) {
        self.stack.last().expect("the page stack always holds a page")
    }

    fn top_mut(&mut self) -> &mut (Screen, f32) {
        self.stack.last_mut().expect("the page stack always holds a page")
    }

    /// Whether the panel needs a new frame since the last call.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// When a shown message should disappear, if one is shown.
    pub fn deadline(&self) -> Option<u64> {
        self.toast.as_ref().map(|t| t.1)
    }

    /// Let time pass: a message past its deadline goes away.
    pub fn tick(&mut self, now: u64) {
        if self.toast.as_ref().is_some_and(|m| now >= m.1) {
            self.toast = None;
            self.dirty = true;
        }
    }

    pub fn display_on(&self) -> bool {
        self.display_on
    }

    /// Something laid over the page changed (the clock): compose again.
    pub fn redraw(&mut self) {
        self.dirty = true;
    }

    /// The page's readings changed (battery, storage): render it again.
    pub fn refresh(&mut self) {
        self.frame = None;
        self.dirty = true;
    }

    /// Render the current page if what it shows has changed.
    pub fn frame(&mut self, f: &Fonts, info: &DeviceInfo, data: DataState) -> &Frame {
        if self.frame.is_none() {
            let t = self.theme();
            let screen = self.screen();
            let cx = Ctx { t: &t, f, info, data, state: &self.state };
            let frame = pages::render(screen, &cx);
            self.frame = Some(frame);
            self.clamp_scroll();
        }
        self.frame.as_ref().expect("rendered above")
    }

    fn max_scroll(&self) -> f32 {
        self.frame.as_ref().map_or(0.0, |f| (f.canvas.h as f32 - self.h as f32).max(0.0))
    }

    fn clamp_scroll(&mut self) {
        let max = self.max_scroll();
        let top = self.top_mut();
        top.1 = top.1.clamp(0.0, max);
    }

    /// Whether the compact bar covers the top of the page.
    fn bar_shown(&self, t: &Theme) -> bool {
        self.top().1 > t.m.dp(60.0)
    }

    /// Draw what the panel shows now: the visible part of the page, the bar
    /// over a scrolled page, the status bar and any message.
    pub fn compose(&mut self, out: &mut Canvas, f: &Fonts, info: &DeviceInfo, data: DataState, clock: &str, now: u64) {
        self.frame(f, info, data);
        let t = self.theme();
        if self.toast.as_ref().is_some_and(|m| now >= m.1) {
            self.toast = None;
        }
        let scroll = self.top().1.round() as usize;
        let bar_shown = self.bar_shown(&t);
        let frame = self.frame.as_ref().expect("rendered above");
        let row = out.w as usize * 4;
        let shown = (frame.canvas.h as usize).saturating_sub(scroll).min(out.h as usize);
        out.data[..shown * row].copy_from_slice(&frame.canvas.data[scroll * row..(scroll + shown) * row]);
        if shown < out.h as usize {
            let bg = if frame.hero { t.hero.bottom } else { t.pal.bg };
            out.fill_rect(0.0, shown as f32, out.w as f32, (out.h as usize - shown) as f32, bg);
        }
        let (bg, chip, ink, status) = if frame.hero {
            (t.hero.top, t.hero.card, t.hero.title, t.hero.status)
        } else {
            (t.pal.bg, t.pal.card, t.pal.text, t.pal.status)
        };
        if bar_shown {
            widgets::top_bar(out, &t, f, frame.title, frame.back, bg, chip, ink);
        }
        let battery = info.battery.as_ref().map(|b| (b.level, b.charge == Charge::Charging));
        widgets::status_bar(out, &t, f, &Bar { clock, battery }, status);
        if let Some((text, _)) = &self.toast {
            widgets::toast(out, &t, f, text);
        }
    }

    fn say(&mut self, text: &str, now: u64) {
        self.toast = Some((text.to_string(), now + TOAST_MS));
        self.dirty = true;
    }

    fn open(&mut self, screen: Screen) {
        self.stack.push((screen, 0.0));
        self.refresh();
    }

    fn back(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
            self.state.confirm = 0.0;
            self.refresh();
        }
    }

    /// Carry out a tapped control.
    fn act(&mut self, action: Action, now: u64) -> Vec<Effect> {
        match action {
            Action::Open(s) => self.open(s),
            Action::Skip => {
                *self.top_mut() = (Screen::Home, 0.0);
                self.refresh();
            }
            Action::Back => self.back(),
            Action::Theme { dark } => {
                if self.state.dark != dark {
                    self.state.dark = dark;
                    self.refresh();
                }
            }
            Action::Toggle(Switch::Vibrate) => {
                self.state.vibrate = !self.state.vibrate;
                self.refresh();
            }
            Action::Toggle(Switch::Sideload) => {
                self.state.sideload = !self.state.sideload;
                self.refresh();
            }
            Action::RebootTarget(i) => {
                self.state.reboot = i;
                self.refresh();
            }
            Action::WipeItem(i) => {
                self.state.wipe[i] = !self.state.wipe[i];
                self.refresh();
            }
            Action::Confirm(_) => self.say("按住圆形按钮向右滑动", now),
            Action::Brightness | Action::Pattern => {}
            Action::Unavailable(text) => self.say(text, now),
        }
        Vec::new()
    }

    pub fn touch(&mut self, ev: Touch, f: &Fonts, now: u64) -> Vec<Effect> {
        if !self.display_on {
            return Vec::new();
        }
        let t = self.theme();
        let m = t.m;
        let (slop, edge) = (m.dp(8.0), m.dp(20.0));
        let scroll = self.top().1;
        let mut fx = Vec::new();
        match ev {
            Touch::Down(x, y) => {
                // Over a scrolled page the compact bar covers the top; only
                // its `<` answers there.
                let hit = if self.bar_shown(&t) && y < widgets::top_bar_height(&t) {
                    let back = self.frame.as_ref().is_some_and(|fr| fr.back);
                    let r = widgets::back_rect(&t);
                    (back && r.contains(x, y)).then_some((Action::Back, r))
                } else {
                    self.frame.as_ref().and_then(|fr| fr.hit(x, y + scroll)).map(|h| (h.action, h.rect))
                };
                let action = hit.map(|h| h.0);
                self.drag = match hit {
                    Some((Action::Brightness, track)) => {
                        fx.push(self.set_brightness(&t, track, x));
                        Drag::Slider(track)
                    }
                    Some((Action::Confirm(which), track)) if self.on_knob(track, x) => {
                        let knob_x = track.x + (track.w - track.h) * self.state.confirm;
                        Drag::Confirm { which, track, grab: x - knob_x }
                    }
                    Some((Action::Pattern, _)) => {
                        self.state.pattern.clear();
                        self.trace(&t, x, y + scroll);
                        Drag::Pattern
                    }
                    _ => {
                        let at_edge = if x < edge {
                            Some(true)
                        } else if x > m.w as f32 - edge {
                            Some(false)
                        } else {
                            None
                        };
                        Drag::Press { x, y, action, edge: at_edge }
                    }
                };
            }
            Touch::Move(x, y) => match std::mem::replace(&mut self.drag, Drag::Idle) {
                Drag::Press { x: x0, y: y0, action, edge: at_edge } => {
                    let (dx, dy) = (x - x0, y - y0);
                    self.drag = match at_edge {
                        Some(from_left) if dx.abs() > slop && dx.abs() > dy.abs() => Drag::Edge { start: x0, from_left, armed: false },
                        _ if dy.abs() > slop => {
                            self.scroll_by(y0 - y);
                            Drag::Scroll { last: y }
                        }
                        _ => Drag::Press { x: x0, y: y0, action, edge: at_edge },
                    };
                }
                Drag::Scroll { last } => {
                    self.scroll_by(last - y);
                    self.drag = Drag::Scroll { last: y };
                }
                Drag::Edge { start, from_left, .. } => {
                    let travel = if from_left { x - start } else { start - x };
                    self.drag = Drag::Edge { start, from_left, armed: travel > m.dp(56.0) };
                }
                Drag::Slider(track) => {
                    fx.push(self.set_brightness(&t, track, x));
                    self.drag = Drag::Slider(track);
                }
                Drag::Confirm { which, track, grab } => {
                    let travel = (track.w - track.h).max(1.0);
                    self.state.confirm = ((x - grab - track.x) / travel).clamp(0.0, 1.0);
                    self.repaint_confirm(&t, f, which, track);
                    self.drag = Drag::Confirm { which, track, grab };
                }
                Drag::Pattern => {
                    self.trace(&t, x, y + scroll);
                    self.drag = Drag::Pattern;
                }
                Drag::Idle => {}
            },
            Touch::Up => match std::mem::replace(&mut self.drag, Drag::Idle) {
                Drag::Press { action: Some(a), .. } => fx.extend(self.act(a, now)),
                Drag::Edge { armed: true, .. } => self.back(),
                Drag::Confirm { which, track, .. } => {
                    let done = self.state.confirm > 0.9;
                    self.state.confirm = 0.0;
                    self.repaint_confirm(&t, f, which, track);
                    if done {
                        match which {
                            Confirm::Reboot => fx.push(Effect::Reboot(TARGETS[self.state.reboot])),
                            Confirm::Wipe => self.say("清除功能还没实现", now),
                        }
                    }
                }
                Drag::Pattern => {
                    let enough = self.state.pattern.len() >= 4;
                    self.state.pattern.clear();
                    self.repaint_pattern(&t);
                    self.say(if enough { "解密还没接入手机的安全服务" } else { "至少连接 4 个点" }, now);
                }
                _ => {}
            },
        }
        fx
    }

    pub fn key(&mut self, key: Key, now: u64) -> Vec<Effect> {
        if key == Key::Power {
            self.display_on = !self.display_on;
            self.dirty = self.display_on;
            return vec![Effect::Display(self.display_on)];
        }
        if !self.display_on {
            return Vec::new();
        }
        let step = self.h as f32 / 3.0;
        match key {
            Key::Back => self.act(Action::Back, now),
            Key::VolumeUp => {
                self.scroll_by(-step);
                Vec::new()
            }
            Key::VolumeDown => {
                self.scroll_by(step);
                Vec::new()
            }
            Key::Power => unreachable!("handled above"),
        }
    }

    fn scroll_by(&mut self, d: f32) {
        let max = self.max_scroll();
        let top = self.top_mut();
        let next = (top.1 + d).clamp(0.0, max);
        if next != top.1 {
            top.1 = next;
            self.dirty = true;
        }
    }

    fn on_knob(&self, track: Rect, x: f32) -> bool {
        let knob_x = track.x + (track.w - track.h) * self.state.confirm;
        x >= knob_x && x <= knob_x + track.h
    }

    fn set_brightness(&mut self, t: &Theme, track: Rect, x: f32) -> Effect {
        self.state.brightness = ((x - track.x) / track.w).clamp(0.02, 1.0);
        if let Some(fr) = self.frame.as_mut() {
            fr.canvas.fill_rect(track.x, track.y, track.w, track.h, t.pal.card);
            widgets::slider(&mut fr.canvas, t, track.x, track.y, track.w, self.state.brightness);
        }
        self.dirty = true;
        Effect::Backlight(self.state.brightness)
    }

    fn repaint_confirm(&mut self, t: &Theme, f: &Fonts, which: Confirm, track: Rect) {
        let (label, danger) = pages::confirm_label(which);
        if let Some(fr) = self.frame.as_mut() {
            fr.canvas.fill_rect(track.x, track.y, track.w, track.h, t.pal.bg);
            widgets::confirm(&mut fr.canvas, t, f, track.y, label, danger, self.state.confirm);
        }
        self.dirty = true;
    }

    /// Add the dot under the finger to the pattern being drawn.
    fn trace(&mut self, t: &Theme, x: f32, y: f32) {
        let Some((cx, cy, span)) = self.frame.as_ref().and_then(|fr| fr.pattern) else { return };
        let reach = t.m.dp(32.0);
        let dots = widgets::pattern_dots(cx, cy, span);
        let near = dots.iter().position(|&(dx, dy)| (dx - x).hypot(dy - y) < reach);
        if let Some(i) = near {
            if !self.state.pattern.contains(&i) {
                self.state.pattern.push(i);
                self.repaint_pattern(t);
            }
        }
    }

    fn repaint_pattern(&mut self, t: &Theme) {
        let Some(fr) = self.frame.as_mut() else { return };
        let Some((cx, cy, span)) = fr.pattern else { return };
        let r = span / 2.0 + t.m.dp(16.0);
        fr.canvas.fill_rect(cx - r, cy - r, r * 2.0, r * 2.0, t.pal.bg);
        widgets::pattern_lock(&mut fr.canvas, t, cx, cy, span, &self.state.pattern);
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skyward_info::{Boot, Cpu, Storage};
    use skyward_vab::SnapshotState;

    // A Xiaomi 13 Ultra panel: 1440 × 3200 at 560 dpi.
    const W: u32 = 1440;
    const H: u32 = 3200;
    const PPD: f32 = 3.5;

    fn fonts() -> Fonts {
        Fonts::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources/fonts/MiSansVF.ttf"), 0).unwrap()
    }

    fn info() -> DeviceInfo {
        DeviceInfo {
            model: "Xiaomi 13 Ultra".into(),
            codename: "ishtar".into(),
            soc: None,
            cpu: Cpu { cores: 8, max_khz: Some(3_187_200) },
            ram_kib: 15_556_972,
            panel: Some((W, H)),
            screen: Some(6.73),
            os_version: None,
            android: None,
            security_patch: None,
            kernel: "5.15.178".into(),
            storage: Storage { total: 496_550_785_024, free: None },
            battery: None,
            slot: None,
            snapshot: SnapshotState::None,
            boot: Boot { unlocked: None, verified: None },
        }
    }

    /// Where on screen the first control doing `action` sits.
    fn spot(app: &mut App, f: &Fonts, i: &DeviceInfo, action: Action) -> (f32, f32) {
        let (x, y) = app.frame(f, i, DataState::Unmounted).hits.iter().find(|h| h.action == action).expect("control on the page").rect.centre();
        (x, y - app.top().1)
    }

    fn tap(app: &mut App, f: &Fonts, i: &DeviceInfo, action: Action) -> Vec<Effect> {
        let (x, y) = spot(app, f, i, action);
        let mut fx = app.touch(Touch::Down(x, y), f, 0);
        fx.extend(app.touch(Touch::Up, f, 0));
        fx
    }

    #[test]
    fn rows_open_pages_and_back_returns() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        tap(&mut app, &f, &i, Action::Open(Screen::Settings));
        assert_eq!(app.screen(), Screen::Settings);
        tap(&mut app, &f, &i, Action::Back);
        assert_eq!(app.screen(), Screen::Home);
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Home, "back on the first page stays there");
    }

    #[test]
    fn swipe_from_the_left_edge_goes_back() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        tap(&mut app, &f, &i, Action::Open(Screen::Reboot));
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        app.touch(Touch::Move(90.0, 1605.0), &f, 0);
        app.touch(Touch::Move(320.0, 1610.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        assert_eq!(app.screen(), Screen::Home);
    }

    #[test]
    fn a_short_edge_swipe_does_not_go_back() {
        let f = fonts();
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        app.touch(Touch::Move(80.0, 1600.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        assert_eq!(app.screen(), Screen::Reboot);
    }

    #[test]
    fn dragging_scrolls_and_does_not_tap() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        let (x, y) = spot(&mut app, &f, &i, Action::Toggle(Switch::Vibrate));
        assert!(y < H as f32, "the switch is on the first screen");
        app.touch(Touch::Down(x, y), &f, 0);
        app.touch(Touch::Move(x, y - 100.0), &f, 0);
        app.touch(Touch::Move(x, y - 600.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        assert!(app.state.vibrate, "the switch was not flipped");
        assert_eq!(app.top().1, 600.0);
    }

    #[test]
    fn theme_previews_switch_the_theme() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        tap(&mut app, &f, &i, Action::Theme { dark: true });
        assert!(app.state.dark);
        assert!(app.theme().dark);
        tap(&mut app, &f, &i, Action::Theme { dark: false });
        assert!(!app.state.dark);
    }

    #[test]
    fn dragging_the_track_sets_brightness() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        let track = app.frame(&f, &i, DataState::Unmounted).hits.iter().find(|h| h.action == Action::Brightness).unwrap().rect;
        let y = track.centre().1 - app.top().1;
        let fx = app.touch(Touch::Down(track.x + track.w * 0.25, y), &f, 0);
        assert_eq!(fx, vec![Effect::Backlight(0.25)]);
        let fx = app.touch(Touch::Move(track.x + track.w * 0.75, y + 40.0), &f, 0);
        assert_eq!(fx, vec![Effect::Backlight(0.75)]);
        app.touch(Touch::Up, &f, 0);
        assert_eq!(app.state.brightness, 0.75);
    }

    #[test]
    fn swiping_the_knob_reboots_to_the_chosen_target() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        tap(&mut app, &f, &i, Action::RebootTarget(3));
        let track = app.frame(&f, &i, DataState::Unmounted).hits.iter().find(|h| h.action == Action::Confirm(Confirm::Reboot)).unwrap().rect;
        let y = track.centre().1;
        app.touch(Touch::Down(track.x + track.h / 2.0, y), &f, 0);
        app.touch(Touch::Move(track.x + track.w / 2.0, y), &f, 0);
        app.touch(Touch::Move(track.x + track.w, y), &f, 0);
        let fx = app.touch(Touch::Up, &f, 0);
        assert_eq!(fx, vec![Effect::Reboot(Target::Bootloader)]);
        assert_eq!(app.state.confirm, 0.0, "the knob springs back");
    }

    #[test]
    fn letting_go_halfway_does_not_reboot() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        let track = app.frame(&f, &i, DataState::Unmounted).hits.iter().find(|h| h.action == Action::Confirm(Confirm::Reboot)).unwrap().rect;
        let y = track.centre().1;
        app.touch(Touch::Down(track.x + track.h / 2.0, y), &f, 0);
        app.touch(Touch::Move(track.x + track.w / 2.0, y), &f, 0);
        assert!(app.touch(Touch::Up, &f, 0).is_empty());
    }

    #[test]
    fn unfinished_features_explain_themselves() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        tap(&mut app, &f, &i, Action::Unavailable("安装还没实现"));
        assert_eq!(app.deadline(), Some(TOAST_MS));
        app.tick(TOAST_MS);
        assert_eq!(app.deadline(), None);
    }

    #[test]
    fn power_key_turns_the_panel_off_and_on() {
        let mut app = App::new(Screen::Home, W, H, PPD);
        assert_eq!(app.key(Key::Power, 0), vec![Effect::Display(false)]);
        assert!(!app.display_on());
        assert_eq!(app.key(Key::Power, 0), vec![Effect::Display(true)]);
    }
}
