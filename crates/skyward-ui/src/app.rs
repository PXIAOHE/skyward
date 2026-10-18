use crate::canvas::{Canvas, Run};
use crate::color::Rgba;
use crate::fling::{Fling, Tracker};
use crate::font::Fonts;
use crate::geom::Rect;
use crate::glow::{self, GlowClock};
use crate::motion::{self, Spring};
use crate::pages::{self, Ctx};
use crate::sprite::{Blurs, Sprite};
use crate::theme::Theme;
use crate::widgets::{self, Bar};
use skyward_info::{Charge, DataState, DeviceInfo};
use skyward_install::{Event, ImageKind, Outcome, Stage};
use skyward_vab::Slot;

/// The pages of the recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Decrypt,
    Home,
    Settings,
    Reboot,
    Wipe,
    Install,
    /// Receiving what the computer sends with `adb sideload`, and
    /// installing it.
    Sideload,
    About,
    Specs,
    /// How long the screen stays on with nothing touched.
    ScreenOff,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    Vibrate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirm {
    Reboot,
    Wipe,
    Flash,
}

/// Which slots an image goes to, by the slot the phone runs from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Current,
    Other,
    Both,
}

impl Pick {
    pub const ALL: [Pick; 3] = [Pick::Current, Pick::Other, Pick::Both];

    /// The slots this means on a phone running from `current`.
    pub fn slots(self, current: Slot) -> Vec<Slot> {
        match self {
            Pick::Current => vec![current],
            Pick::Other => vec![current.other()],
            Pick::Both => vec![Slot::A, Slot::B],
        }
    }
}

/// Where installing stands, as the installer last told.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Install {
    /// No installer runs.
    #[default]
    Idle,
    /// USB is being set up for sideload.
    Starting,
    Waiting,
    Checking { size: u64 },
    /// An image waits to be told where it goes.
    Choosing(Choice),
    /// `overwriting` says whether the update writes over the running
    /// system.
    Updating { version: Option<String>, stage: Option<Stage>, overwriting: bool },
    Flashing { partition: &'static str, stage: Option<Stage> },
    /// `damage` is what a failure may have left broken.
    Finished { outcome: Outcome, damage: Option<Damage> },
    /// The page was left and the installer is handing USB back.
    Ending,
}

/// What a failed install may have left broken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Damage {
    /// The update had begun writing over the running system.
    System,
    /// The image had begun going into this partition.
    Partition(&'static str),
}

/// An image, and where it goes so far.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub kind: ImageKind,
    pub size: u64,
    /// The partitions it fits, the likeliest first.
    pub partitions: Vec<&'static str>,
    /// Which of them is chosen.
    pub partition: usize,
    pub pick: Pick,
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

/// How long the screen stays on with nothing touched, in milliseconds, in
/// the order the page lists them; `None` keeps it on.
pub const SCREEN_OFF: [Option<u64>; 7] = [Some(15_000), Some(30_000), Some(60_000), Some(120_000), Some(300_000), Some(600_000), None];

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
    /// Start receiving a package over USB.
    Sideload,
    /// Send the image to one of the partitions it fits.
    FlashPartition(usize),
    /// Send the image to these slots.
    FlashSlots(Pick),
    /// Leave the recovery for this target at once.
    RebootTo(Target),
    /// Keep the screen on for this choice in [`SCREEN_OFF`].
    ScreenOff(usize),
    /// A control whose work is not wired up yet; touching it says so.
    Unavailable(&'static str),
}

pub struct Hit {
    pub rect: Rect,
    pub action: Action,
}

/// A rendered page in page coordinates. The status-bar band at its top is
/// left empty, and the `<`, the titles and the bar are laid over the page
/// as it scrolls.
pub struct Frame {
    pub canvas: Canvas,
    pub hits: Vec<Hit>,
    pub title: &'static str,
    pub back: bool,
    /// The page opens with its title in large type, scrolling away with it.
    pub header: bool,
    /// Where the page's content starts; once it slides under the top of
    /// the screen, the bar comes up over it.
    pub content_top: f32,
    /// The page sits on the About page's moving field.
    pub hero: Option<HeroArt>,
    /// Centre and span of the pattern grid, on the unlock page.
    pub pattern: Option<(f32, f32, f32)>,
}

/// What the About page lays over its field: the wordmark and version, which
/// stay put while the cards scroll, and the cards' coverage row by row.
pub struct HeroArt {
    pub wordmark: Sprite,
    pub version: Sprite,
    pub runs: Vec<Vec<Run>>,
}

impl Frame {
    /// The topmost control under a point in page coordinates.
    pub fn hit(&self, x: f32, y: f32) -> Option<&Hit> {
        self.hits.iter().rev().find(|h| h.rect.contains(x, y))
    }
}

/// Choices the pages show and the user changes, and where installing
/// stands.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub dark: bool,
    pub brightness: f32,
    /// How long the screen stays on with nothing touched, by its place in
    /// [`SCREEN_OFF`].
    pub screen_off: usize,
    pub vibrate: bool,
    pub reboot: usize,
    pub wipe: [bool; 4],
    pub pattern: Vec<usize>,
    /// How far the swipe-to-confirm knob is dragged, 0 to 1.
    pub confirm: f32,
    pub install: Install,
}

impl Default for State {
    fn default() -> Self {
        State {
            dark: false,
            brightness: 0.5,
            // Two minutes.
            screen_off: 3,
            vibrate: true,
            reboot: 0,
            wipe: [true, false, true, false],
            pattern: Vec::new(),
            confirm: 0.0,
            install: Install::Idle,
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
    Reboot(Target),
    /// A short tap of the vibration motor.
    Haptic,
    /// Start receiving a package over USB.
    Sideload,
    /// Write the image the installer holds to `partition` in the slots
    /// `pick` means.
    Flash { partition: &'static str, pick: Pick },
    /// Stop waiting for the computer, or for where an image goes.
    Cancel,
}

enum Drag {
    Idle,
    /// Finger down and not yet past the touch slop. `edge` is set when the
    /// press began at a screen edge (true for the left one).
    Press { x: f32, y: f32, action: Option<Action>, edge: Option<bool> },
    Scroll { last: f32 },
    /// Swiping in from a screen edge, the finger now `x` across; the cue
    /// keeps to height `cy`, set by where the swipe began. Past the
    /// threshold, lifting goes back.
    Edge { start: f32, from_left: bool, armed: bool, x: f32, cy: f32 },
    Slider(Rect),
    Confirm { which: Confirm, track: Rect, grab: f32 },
    Pattern,
    /// The finger that lit the dark panel; nothing it does counts until it
    /// lifts.
    Waking,
}

const TOAST_MS: u64 = 2000;
/// A finger lifted slower than this, in dp per second, leaves the page
/// where it is.
const MIN_FLING_DP: f32 = 50.0;
/// The fastest a page starts gliding, in dp per second.
const MAX_FLING_DP: f32 = 8000.0;
/// A glide running into a page's end faster than this, in dp per second,
/// swings out on the slower spring.
const HARD_BOUNCE_DP: f32 = 1667.0;
/// Scroll, in dp, over which a large title fades out.
const TITLE_FADE_DP: f32 = 20.0;
/// Blur, in dp, of a title faded out completely.
const TITLE_BLUR_DP: f32 = 25.0 / 3.0;
/// How far, in dp, a swipe from an edge has to come in to go back.
const BACK_SWIPE_DP: f32 = 56.0;
/// The back cue sits this far, in dp, above where the finger came down,
/// and stays at that height however the finger then wanders.
const CUE_LIFT_DP: f32 = 9.0;
/// On the About page, where the wordmark's area ends, in dp from the top:
/// the version has faded out by the time it passes under the bar.
const ABOUT_LOGO_DP: f32 = 160.0;
/// Scroll, in dp, over which the wordmark then shrinks and fades.
const ABOUT_LOGO_FADE_DP: f32 = 40.0;
/// Scroll, in dp, over which the About page's field fades out; the field's
/// discs sit in the band this far below the bar.
const ABOUT_FIELD_DP: f32 = 389.0;
/// The field is worked out every this many pixels and filled in between.
const FIELD_CELL: usize = 24;
/// Before the panel goes off it stays dim this long, in milliseconds, or
/// a fifth of the screen-off time when that is shorter.
const DIM_MS: u64 = 7000;
/// The dim level is this much below the brightness set, no brighter than
/// the ceiling, and never under the slider's floor unless the brightness
/// set is.
const DIM_DROP: f32 = 0.04;
const DIM_CEILING: f32 = 0.05;
/// The lowest brightness the slider sets.
const MIN_BRIGHTNESS: f32 = 0.02;
/// While the backlight eases, it steps once a frame of a 120 Hz panel.
const BACKLIGHT_STEP_MS: u64 = 8;
/// Just after the panel lights, a touch or key only keeps it awake: the
/// second tap of a wake habit, or the hand reaching for a phone whose
/// screen an install just lit, should not act. Android's double-tap window.
const WAKE_GUARD_MS: u64 = 400;

/// How a part of the screen eases between shown and hidden.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ease {
    /// A critically damped spring of this period, in seconds.
    Spring(f32),
    /// A straight run over this many milliseconds.
    Linear(u64),
}

impl Ease {
    fn progress(self, ms: u64) -> f32 {
        match self {
            Ease::Spring(period) => motion::settle(ms, period),
            Ease::Linear(span) => (ms as f32 / span as f32).min(1.0),
        }
    }
}

/// A part of the screen fading in or out: where it is headed, since when,
/// how far in it was then, and how it eases each way.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fade {
    on: bool,
    since: u64,
    from: f32,
    show: Ease,
    hide: Ease,
}

impl Fade {
    fn hidden(show: Ease, hide: Ease) -> Fade {
        Fade { on: false, since: 0, from: 0.0, show, hide }
    }

    fn shown(show: Ease, hide: Ease) -> Fade {
        Fade { on: true, since: 0, from: 1.0, show, hide }
    }

    /// How far in, 0 to 1, at `now`.
    fn level(&self, now: u64) -> f32 {
        let (target, ease) = if self.on { (1.0, self.show) } else { (0.0, self.hide) };
        self.from + (target - self.from) * ease.progress(now.saturating_sub(self.since))
    }

    fn set(&mut self, on: bool, now: u64) {
        if on != self.on {
            self.from = self.level(now);
            self.since = now;
            self.on = on;
        }
    }

    fn moving(&self, now: u64) -> bool {
        self.level(now) != if self.on { 1.0 } else { 0.0 }
    }
}

/// How the bar's title comes and goes: sharpening in on a spring and
/// blurring away on a quicker one, or on the About page, fading in a
/// straight line together with the bar.
fn title_fade(hero: bool) -> Fade {
    if hero {
        Fade::hidden(Ease::Linear(150), Ease::Linear(150))
    } else {
        Fade::hidden(Ease::Spring(0.3), Ease::Spring(0.15))
    }
}

fn bar_fade(hero: bool) -> Fade {
    if hero {
        Fade::hidden(Ease::Linear(150), Ease::Linear(150))
    } else {
        Fade::hidden(Ease::Linear(100), Ease::Linear(100))
    }
}

/// The panel at full brightness: it dims over half a second and comes
/// back quickly when touched.
fn full_brightness() -> Fade {
    Fade::shown(Ease::Linear(150), Ease::Linear(500))
}

/// The level the panel dims to shortly before it goes off, for the
/// brightness set.
fn dim_level(full: f32) -> f32 {
    (full - DIM_DROP).clamp(MIN_BRIGHTNESS, DIM_CEILING).min(full)
}

/// How long the panel stays dim before going off, for a screen-off time.
fn dim_span(after: u64) -> u64 {
    DIM_MS.min(after / 5)
}

/// Lay the sprite `blurs` keeps, moved by (`dx`, `dy`), in `color` at
/// `level` of its strength, blurred by `blur` pixels.
fn draw_faded(out: &mut Canvas, blurs: &mut Blurs, dx: f32, dy: f32, color: Rgba, level: f32, blur: f32) {
    if level > 0.0 {
        blurs.get(blur).draw(out, dx, dy, color, level);
    }
}

pub struct App {
    pub state: State,
    stack: Vec<(Screen, f32)>,
    frame: Option<Frame>,
    /// The current page's large and bar titles, for its theme, with their
    /// blurred versions as they are needed.
    titles: Option<((Screen, bool), Blurs, Blurs)>,
    /// The `<` chip for the theme (dark or not), and where it goes.
    chip: Option<(bool, Canvas, i32, i32)>,
    /// The About page field's dither, made for the screen once.
    dither: Option<glow::Dither>,
    toast: Option<(String, u64)>,
    drag: Drag,
    tracker: Tracker,
    /// The page gliding on after a flick.
    fling: Option<Fling>,
    /// How far a dragging finger has pulled the page past an end:
    /// negative past the top, positive past the bottom.
    pull: f32,
    /// The page springing back after being pulled or thrown past an end.
    bounce: Option<Spring>,
    /// The back cue shrinking away after a swipe that did not go back:
    /// its edge, where it sits, and its depth.
    cue: Option<(bool, f32, Spring)>,
    title: Fade,
    bar: Fade,
    /// The About page's moving field, while that page shows.
    glow: Option<GlowClock>,
    display_on: bool,
    /// When the screen was last touched or a key pressed; set on the first
    /// tick, so the time before the UI started does not count.
    active: Option<u64>,
    /// When the panel last lit from dark, for the brief guard that keeps a
    /// stray touch or key from acting on a page just put up.
    woke_at: Option<u64>,
    /// The panel at full brightness, or easing to the dim level it takes
    /// shortly before going off.
    bright: Fade,
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
            titles: None,
            chip: None,
            dither: None,
            toast: None,
            drag: Drag::Idle,
            tracker: Tracker::default(),
            fling: None,
            pull: 0.0,
            bounce: None,
            cue: None,
            title: title_fade(start == Screen::About),
            bar: bar_fade(start == Screen::About),
            glow: None,
            display_on: true,
            active: None,
            woke_at: None,
            bright: full_brightness(),
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

    /// When time next has to pass through [`App::tick`]: at once while
    /// something on the lit screen moves on its own, a frame on while the
    /// backlight eases, else when a shown message should disappear or the
    /// panel should dim or go off.
    pub fn deadline(&self, now: u64) -> Option<u64> {
        let moving = self.display_on
            && (self.fling.is_some()
                || self.bounce.is_some()
                || self.cue.is_some()
                || self.title.moving(now)
                || self.bar.moving(now)
                || self.glowing());
        if moving {
            return Some(0);
        }
        let easing = self.bright.moving(now).then_some(now + BACKLIGHT_STEP_MS);
        [self.toast.as_ref().map(|t| t.1), easing, self.idle_due()].into_iter().flatten().min()
    }

    /// When sitting idle next changes the lit panel: when it dims, or once
    /// it is dimming, when it goes off.
    fn idle_due(&self) -> Option<u64> {
        let (active, after) = (self.active?, SCREEN_OFF[self.state.screen_off]?);
        if !self.display_on {
            return None;
        }
        Some(if self.bright.on { active + after - dim_span(after) } else { active + after })
    }

    /// Let time pass: the panel dims and goes off once left alone long
    /// enough, a gliding page moves on, a page past its end springs back,
    /// the bar and its title follow the scroll, and a message past its
    /// deadline goes away.
    pub fn tick(&mut self, now: u64) {
        let active = *self.active.get_or_insert(now);
        if let (true, Some(after)) = (self.display_on, SCREEN_OFF[self.state.screen_off]) {
            let idle = now.saturating_sub(active);
            if idle >= after {
                self.sleep();
            } else if idle >= after - dim_span(after) {
                self.bright.set(false, now);
            }
        }
        if self.toast.as_ref().is_some_and(|m| now >= m.1) {
            self.toast = None;
            self.dirty = true;
        }
        if let Some(fling) = self.fling {
            let (y, done) = fling.at(now);
            let next = y.clamp(0.0, self.max_scroll());
            let top = self.top_mut();
            if next != top.1 {
                top.1 = next;
                self.dirty = true;
            }
            if next != y {
                // The glide ran into an end: the speed it had left throws
                // the page past it, and it springs back.
                let speed = fling.velocity(now);
                let period = if speed.abs() <= self.theme().m.dp(HARD_BOUNCE_DP) { 0.4 } else { 0.55 };
                self.bounce = Some(Spring::new(now, 0.0, speed, period));
                self.fling = None;
            } else if done {
                self.fling = None;
            }
        }
        if let Some(b) = self.bounce {
            self.dirty = true;
            if b.at(now).1 {
                self.bounce = None;
            }
        }
        if let Some((_, _, s)) = self.cue {
            self.dirty = true;
            if s.at(now).1 {
                self.cue = None;
            }
        }
        if self.screen() == Screen::About {
            let look = self.theme().hero.glow;
            match &mut self.glow {
                Some(g) if std::ptr::eq(g.look(), look) => g.advance(now),
                _ => self.glow = Some(GlowClock::new(look, now)),
            }
            if self.glowing() {
                self.dirty = true;
            }
        }
        self.follow_scroll(now);
    }

    /// Whether the About page's field is on screen, moving.
    fn glowing(&self) -> bool {
        self.glow.is_some() && self.display_on && self.top().1 < self.theme().m.dp(ABOUT_FIELD_DP)
    }

    /// Bring the bar and its title in or out for where the page is
    /// scrolled: the title once the large one has faded, the bar once the
    /// content slides under it. On the About page both come once the
    /// version has faded out.
    fn follow_scroll(&mut self, now: u64) {
        let Some(fr) = &self.frame else { return };
        let t = self.theme();
        let scroll = self.top().1;
        let (title, bar) = if fr.hero.is_some() {
            let gone = scroll >= t.m.dp(ABOUT_LOGO_DP) - widgets::bar_bottom(&t);
            (gone, gone)
        } else {
            let under = (fr.content_top - widgets::bar_bottom(&t) - t.m.dp(20.0)).max(0.0);
            (fr.header && scroll >= t.m.dp(TITLE_FADE_DP), scroll > under)
        };
        self.title.set(title, now);
        self.bar.set(bar, now);
        if self.title.moving(now) || self.bar.moving(now) {
            self.dirty = true;
        }
    }

    /// How far past its end the page shows at `now`: negative past the
    /// top, positive past the bottom.
    fn overscroll(&self, now: u64) -> f32 {
        if self.pull != 0.0 {
            motion::rubber(self.pull, self.h as f32)
        } else {
            self.bounce.map_or(0.0, |b| b.at(now).0)
        }
    }

    pub fn display_on(&self) -> bool {
        self.display_on
    }

    /// The backlight at `now`: off, or this share of its full brightness.
    pub fn panel(&self, now: u64) -> Option<f32> {
        if !self.display_on {
            return None;
        }
        let (full, level) = (self.state.brightness, self.bright.level(now));
        Some(full * level + dim_level(full) * (1.0 - level))
    }

    /// Something was touched or pressed: the panel stays at full
    /// brightness for the screen-off time from now.
    fn stir(&mut self, now: u64) {
        self.active = Some(now);
        self.bright.set(true, now);
    }

    /// Light the panel, as if just touched.
    fn wake(&mut self, now: u64) {
        if !self.display_on {
            self.display_on = true;
            self.dirty = true;
        }
        self.woke_at = Some(now);
        self.stir(now);
    }

    /// Whether the panel lit so recently that a touch or key should only
    /// keep it awake.
    fn just_woke(&self, now: u64) -> bool {
        self.woke_at.is_some_and(|w| now.saturating_sub(w) < WAKE_GUARD_MS)
    }

    /// Turn the panel off. A finger on the glass lets go of what it held,
    /// and the page stops where it is.
    fn sleep(&mut self) {
        self.display_on = false;
        self.bright = full_brightness();
        match std::mem::replace(&mut self.drag, Drag::Idle) {
            Drag::Confirm { .. } => {
                self.state.confirm = 0.0;
                self.refresh();
            }
            Drag::Pattern => {
                self.state.pattern.clear();
                self.refresh();
            }
            _ => {}
        }
        self.fling = None;
        self.bounce = None;
        self.pull = 0.0;
        self.cue = None;
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

    /// Draw what the panel shows now: the visible part of the page, pulled
    /// past an end if it is, its large title fading as it scrolls, the bar
    /// and its title over a scrolled page, the `<`, the status bar, any
    /// message, and the back cue while a swipe from an edge is under way.
    pub fn compose(&mut self, out: &mut Canvas, f: &Fonts, info: &DeviceInfo, data: DataState, clock: &str, now: u64) {
        self.frame(f, info, data);
        let t = self.theme();
        let m = t.m;
        if self.toast.as_ref().is_some_and(|m| now >= m.1) {
            self.toast = None;
        }
        let key = (self.screen(), self.state.dark);
        if self.titles.as_ref().is_none_or(|(k, _, _)| *k != key) {
            let title = self.frame.as_ref().expect("rendered above").title;
            self.titles = Some((key, Blurs::new(widgets::big_title(&t, f, title)), Blurs::new(widgets::small_title(&t, f, title))));
        }
        if self.chip.as_ref().is_none_or(|c| c.0 != t.dark) {
            let (art, x, y) = widgets::back_chip(&t);
            self.chip = Some((t.dark, art, x, y));
        }
        let scroll = self.top().1;
        let offset = (scroll + self.overscroll(now)).round();
        let frame = self.frame.as_ref().expect("rendered above");
        let (_, big, small) = self.titles.as_mut().expect("drawn above");

        if let Some(art) = &frame.hero {
            // The field fills the screen and fades as the page scrolls; the
            // cards scroll over it. The version fades and shrinks away
            // first, then the wordmark.
            let glow = self.glow.as_ref().expect("the About page keeps its field");
            let fade = (1.0 - scroll / m.dp(ABOUT_FIELD_DP)).clamp(0.0, 1.0);
            let top = (widgets::bar_bottom(&t) + m.dp(ABOUT_FIELD_DP)) / m.h as f32;
            let field = glow::sample(glow, out.w as usize, out.h as usize, FIELD_CELL, top, t.pal.bg, fade);
            let dither = self.dither.get_or_insert_with(|| glow::Dither::new(out.w as usize, out.h as usize));
            glow::paint(out, &field, fade, &glow::Layer { canvas: &frame.canvas, runs: &art.runs, offset: offset as isize }, dither);
            let range = m.dp(ABOUT_LOGO_DP) - widgets::bar_bottom(&t);
            let v = (scroll / range).clamp(0.0, 1.0);
            art.version.draw_scaled(out, 0.0, 0.0, 1.0 - 0.1 * v, t.hero.version, 1.0 - v);
            let l = ((scroll - range) / m.dp(ABOUT_LOGO_FADE_DP)).clamp(0.0, 1.0);
            glow::ink_wordmark(out, &field, &glow.look().ink, &art.wordmark, 1.0 - 0.1 * l, 1.0 - l, fade, dither);
        } else {
            // The page rows on screen, and the page colour past its ends.
            let row = out.w as usize * 4;
            let first = (-offset).clamp(0.0, out.h as f32) as usize;
            let last = (frame.canvas.h as f32 - offset).clamp(first as f32, out.h as f32) as usize;
            if first < last {
                let from = (offset as isize + first as isize) as usize * row;
                out.data[first * row..last * row].copy_from_slice(&frame.canvas.data[from..from + (last - first) * row]);
            }
            out.fill_rect(0.0, 0.0, out.w as f32, first as f32, t.pal.bg);
            out.fill_rect(0.0, last as f32, out.w as f32, (out.h as usize - last) as f32, t.pal.bg);
            if frame.header {
                let level = (1.0 - scroll / m.dp(TITLE_FADE_DP)).clamp(0.0, 1.0);
                draw_faded(out, big, 0.0, -offset, t.pal.text, level, (1.0 - level) * m.dp(TITLE_BLUR_DP));
            }
        }
        let bar = self.bar.level(now);
        if bar > 0.0 {
            let stops = if frame.hero.is_some() { &widgets::ABOUT_BAR_MASK[..] } else { &widgets::BAR_MASK[..] };
            widgets::bar_mask(out, &t, stops, t.pal.bg, bar);
        }
        if frame.back {
            let (_, chip, x, y) = self.chip.as_ref().expect("drawn above");
            out.lay(chip, *x, *y, bar);
            widgets::back_arrow(out, &t, t.pal.text);
        }
        let level = self.title.level(now);
        let blur = if frame.hero.is_some() { 0.0 } else { (1.0 - level) * m.dp(TITLE_BLUR_DP) };
        draw_faded(out, small, 0.0, 0.0, t.pal.text, level, blur);

        let battery = info.battery.as_ref().map(|b| (b.level, b.charge == Charge::Charging));
        widgets::status_bar(out, &t, f, &Bar { clock, battery }, t.pal.status);
        if let Some((text, _)) = &self.toast {
            widgets::toast(out, &t, f, text);
        }
        match self.drag {
            Drag::Edge { start, from_left, armed, x, cy } => {
                let travel = if from_left { x - start } else { start - x };
                widgets::back_cue(out, &t, from_left, cy, widgets::back_cue_depth(&t, travel), armed);
            }
            _ => {
                if let Some((from_left, cy, s)) = self.cue {
                    widgets::back_cue(out, &t, from_left, cy, s.at(now).0.max(0.0), false);
                }
            }
        }
    }

    /// Put the page back where it rests and the bar away, for a page just
    /// opened or returned to.
    fn settle_page(&mut self) {
        self.fling = None;
        self.pull = 0.0;
        self.bounce = None;
        self.titles = None;
        self.glow = None;
        let hero = self.screen() == Screen::About;
        self.title = title_fade(hero);
        self.bar = bar_fade(hero);
    }

    fn say(&mut self, text: &str, now: u64) {
        self.toast = Some((text.to_string(), now + TOAST_MS));
        self.dirty = true;
    }

    fn open(&mut self, screen: Screen) {
        self.stack.push((screen, 0.0));
        self.settle_page();
        self.refresh();
    }

    /// Go back a page. Leaving the sideload page stops what can be stopped;
    /// while a package is being read or written, the page stays.
    fn back(&mut self, now: u64) -> Vec<Effect> {
        let mut fx = Vec::new();
        if self.stack.len() <= 1 {
            return fx;
        }
        if self.screen() == Screen::Sideload {
            match self.state.install {
                Install::Checking { .. } => {
                    self.say("正在读取安装包，读取完成后才能返回", now);
                    return fx;
                }
                Install::Updating { .. } | Install::Flashing { .. } => {
                    self.say("正在安装，安装完成后才能返回", now);
                    return fx;
                }
                Install::Starting | Install::Waiting | Install::Choosing(_) => {
                    self.state.install = Install::Ending;
                    fx.push(Effect::Cancel);
                }
                Install::Finished { .. } => self.state.install = Install::Idle,
                Install::Idle | Install::Ending => {}
            }
        }
        self.stack.pop();
        self.state.confirm = 0.0;
        self.settle_page();
        self.refresh();
        fx
    }

    /// Hear from the installer. Once the sideload page is left, only its
    /// end matters. An image waiting for its partition, and the end, light
    /// a dark panel: both want the user.
    pub fn install(&mut self, ev: Event, now: u64) {
        if self.state.install == Install::Ending {
            if matches!(ev, Event::Finished(_)) {
                self.state.install = Install::Idle;
            }
            return;
        }
        if matches!(ev, Event::Image { .. } | Event::Finished(_)) {
            self.wake(now);
        }
        let install = &mut self.state.install;
        match ev {
            Event::Waiting => *install = Install::Waiting,
            Event::Checking { size } => *install = Install::Checking { size },
            Event::Installing { version } => *install = Install::Updating { version, stage: None, overwriting: false },
            Event::Overwriting => {
                if let Install::Updating { overwriting, .. } = install {
                    *overwriting = true;
                }
            }
            Event::Image { kind, size, partitions } => {
                *install = Install::Choosing(Choice { kind, size, partitions, partition: 0, pick: Pick::Current })
            }
            Event::Progress(stage) => match install {
                Install::Updating { stage: shown, .. } | Install::Flashing { stage: shown, .. } => *shown = Some(stage),
                _ => {}
            },
            Event::Finished(outcome) => {
                let damage = match (&outcome, &*install) {
                    (Outcome::Failed(_), Install::Updating { overwriting: true, .. }) => Some(Damage::System),
                    (Outcome::Failed(_), Install::Flashing { partition, stage: Some(Stage::Flashing { done, .. }) }) if *done > 0 => {
                        Some(Damage::Partition(partition))
                    }
                    _ => None,
                };
                *install = Install::Finished { outcome, damage };
            }
        }
        if self.screen() == Screen::Sideload {
            self.refresh();
        }
    }

    /// Carry out a tapped control.
    fn act(&mut self, action: Action, now: u64) -> Vec<Effect> {
        match action {
            Action::Open(s) => self.open(s),
            Action::Skip => {
                *self.top_mut() = (Screen::Home, 0.0);
                self.settle_page();
                self.refresh();
            }
            Action::Back => return self.back(now),
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
            Action::Sideload => match self.state.install {
                Install::Idle | Install::Finished { .. } => {
                    self.state.install = Install::Starting;
                    if self.screen() == Screen::Sideload {
                        self.refresh();
                    } else {
                        self.open(Screen::Sideload);
                    }
                    return vec![Effect::Sideload];
                }
                _ => self.say("上一次安装还在收尾，请稍后再尝试", now),
            },
            Action::FlashPartition(k) => {
                if let Install::Choosing(choice) = &mut self.state.install {
                    choice.partition = k;
                    self.refresh();
                }
            }
            Action::FlashSlots(pick) => {
                if let Install::Choosing(choice) = &mut self.state.install {
                    choice.pick = pick;
                    self.refresh();
                }
            }
            Action::RebootTo(target) => return vec![Effect::Reboot(target)],
            Action::ScreenOff(k) => {
                self.state.screen_off = k;
                self.refresh();
            }
            Action::Unavailable(text) => self.say(text, now),
        }
        Vec::new()
    }

    pub fn touch(&mut self, ev: Touch, f: &Fonts, now: u64) -> Vec<Effect> {
        if !self.display_on {
            // A finger on the dark panel lights it, and does nothing else
            // until it lifts.
            if matches!(ev, Touch::Down(..)) {
                self.wake(now);
                self.drag = Drag::Waking;
            }
            return Vec::new();
        }
        // A finger landing just after the panel lit only keeps it awake.
        if matches!(ev, Touch::Down(..)) && self.just_woke(now) {
            self.stir(now);
            self.drag = Drag::Waking;
            return Vec::new();
        }
        self.stir(now);
        let t = self.theme();
        let m = t.m;
        let (slop, edge) = (m.dp(8.0), m.dp(20.0));
        let scroll = self.top().1;
        let mut fx = Vec::new();
        match ev {
            Touch::Down(x, y) => {
                self.tracker.clear();
                self.tracker.add(now, y);
                let side = if x < edge {
                    Some(true)
                } else if x > m.w as f32 - edge {
                    Some(false)
                } else {
                    None
                };
                // The first page has nothing to go back to: its edges are
                // like the rest of it.
                let at_edge = side.filter(|_| self.stack.len() > 1);
                // A swipe in from an edge goes back over whatever the page is
                // doing: a finger landing there leaves a gliding or springing
                // page alone until it drags the page, and presses nothing.
                if at_edge.is_some() && self.in_motion(now) {
                    self.drag = Drag::Press { x, y, action: None, edge: at_edge };
                    return fx;
                }
                // Anywhere else it stops such a page and holds it; the
                // control under the finger is not pressed.
                if self.catch(now) {
                    self.drag = Drag::Scroll { last: y };
                    return fx;
                }
                // The `<` stays put while the page scrolls; under the bar
                // only it answers.
                let back = widgets::back_rect(&t);
                let hit = if self.frame.as_ref().is_some_and(|fr| fr.back) && back.contains(x, y) {
                    Some((Action::Back, back))
                } else if self.bar.on && y < widgets::bar_bottom(&t) {
                    None
                } else {
                    self.frame.as_ref().and_then(|fr| fr.hit(x, y + scroll)).map(|h| (h.action, h.rect))
                };
                let action = hit.map(|h| h.0);
                self.drag = match hit {
                    Some((Action::Brightness, track)) => {
                        self.set_brightness(&t, track, x);
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
                    _ => Drag::Press { x, y, action, edge: at_edge },
                };
            }
            Touch::Move(x, y) => match std::mem::replace(&mut self.drag, Drag::Idle) {
                Drag::Press { x: x0, y: y0, action, edge: at_edge } => {
                    self.tracker.add(now, y);
                    let (dx, dy) = (x - x0, y - y0);
                    self.drag = match at_edge {
                        Some(from_left) if dx.abs() > slop && dx.abs() > dy.abs() => {
                            self.dirty = true;
                            Drag::Edge { start: x0, from_left, armed: false, x, cy: y0 - m.dp(CUE_LIFT_DP) }
                        }
                        _ if dy.abs() > slop => {
                            // A page still gliding or springing back is caught
                            // where it has got to; one at rest follows the
                            // finger from where it came down.
                            if !self.catch(now) {
                                self.drag_by(y0 - y);
                            }
                            Drag::Scroll { last: y }
                        }
                        _ => Drag::Press { x: x0, y: y0, action, edge: at_edge },
                    };
                }
                Drag::Scroll { last } => {
                    self.tracker.add(now, y);
                    self.drag_by(last - y);
                    self.drag = Drag::Scroll { last: y };
                }
                Drag::Edge { start, from_left, armed, cy, .. } => {
                    let travel = if from_left { x - start } else { start - x };
                    let ready = travel > m.dp(BACK_SWIPE_DP);
                    // A tap of the motor as the `<` appears, as the system
                    // gives.
                    if ready && !armed && self.state.vibrate {
                        fx.push(Effect::Haptic);
                    }
                    self.drag = Drag::Edge { start, from_left, armed: ready, x, cy };
                    self.dirty = true;
                }
                Drag::Slider(track) => {
                    self.set_brightness(&t, track, x);
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
                Drag::Waking => self.drag = Drag::Waking,
                Drag::Idle => {}
            },
            Touch::Up => match std::mem::replace(&mut self.drag, Drag::Idle) {
                Drag::Press { action: Some(a), .. } => fx.extend(self.act(a, now)),
                Drag::Scroll { .. } if self.pull != 0.0 => {
                    // A page let go past its end springs back.
                    self.bounce = Some(Spring::new(now, motion::rubber(self.pull, self.h as f32), 0.0, 0.4));
                    self.pull = 0.0;
                    self.dirty = true;
                }
                Drag::Scroll { .. } => {
                    // A finger lifted while still moving sends the page gliding on.
                    let speed = -self.tracker.velocity(now);
                    if speed.abs() >= m.dp(MIN_FLING_DP) {
                        let speed = speed.clamp(-m.dp(MAX_FLING_DP), m.dp(MAX_FLING_DP));
                        self.fling = Some(Fling::new(now, scroll, speed, self.px_per_dp * 160.0));
                    }
                }
                Drag::Edge { armed: true, .. } => fx.extend(self.back(now)),
                Drag::Edge { start, from_left, x, cy, .. } => {
                    // Short of going back, the cue sinks back into the edge.
                    let travel = if from_left { x - start } else { start - x };
                    let depth = widgets::back_cue_depth(&t, travel);
                    self.cue = Some((from_left, cy, Spring::new(now, depth, 0.0, 0.15)));
                    self.dirty = true;
                }
                Drag::Confirm { which, track, .. } => {
                    let done = self.state.confirm > 0.9;
                    self.state.confirm = 0.0;
                    self.repaint_confirm(&t, f, which, track);
                    if done {
                        match which {
                            Confirm::Reboot => fx.push(Effect::Reboot(TARGETS[self.state.reboot])),
                            Confirm::Wipe => self.say("清除功能还没实现", now),
                            Confirm::Flash => {
                                if let Install::Choosing(choice) = &self.state.install {
                                    let partition = choice.partitions[choice.partition];
                                    fx.push(Effect::Flash { partition, pick: choice.pick });
                                    self.state.install = Install::Flashing { partition, stage: None };
                                    self.refresh();
                                }
                            }
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
        // Any button stops a gliding page.
        self.fling = None;
        if !self.display_on {
            // Any button lights the dark panel, and does only that.
            self.wake(now);
            return Vec::new();
        }
        // A button pressed just after the panel lit only keeps it awake, so
        // a power press meant to wake it does not turn it back off.
        if self.just_woke(now) {
            self.stir(now);
            return Vec::new();
        }
        if key == Key::Power {
            self.sleep();
            return Vec::new();
        }
        self.stir(now);
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

    /// Whether the page is gliding or springing back at `now`.
    fn in_motion(&self, now: u64) -> bool {
        self.fling.is_some_and(|fl| !fl.at(now).1) || self.bounce.is_some_and(|b| !b.at(now).1)
    }

    /// Stop a page gliding or springing back where it has got to, for a
    /// finger to hold: one past its end stays out there as the finger's
    /// pull. Whether there was one.
    fn catch(&mut self, now: u64) -> bool {
        let glide = self.fling.take().is_some_and(|fl| !fl.at(now).1);
        let spring = self.bounce.take().map(|b| b.at(now)).filter(|&(_, done)| !done);
        if let Some((offset, _)) = spring {
            self.pull = motion::unrubber(offset, self.h as f32);
        }
        glide || spring.is_some()
    }

    /// Move the page `d` pixels under a dragging finger (positive moves it
    /// up). Past an end the finger's travel goes into the pull, which the
    /// page follows with growing resistance; coming back undoes the pull
    /// before the page scrolls again.
    fn drag_by(&mut self, d: f32) {
        self.dirty = true;
        let mut d = d;
        if self.pull != 0.0 {
            let pull = self.pull + d;
            if pull * self.pull > 0.0 {
                self.pull = pull;
                return;
            }
            self.pull = 0.0;
            d = pull;
        }
        let max = self.max_scroll();
        let top = self.top_mut();
        let next = top.1 + d;
        top.1 = next.clamp(0.0, max);
        self.pull = next - top.1;
    }

    fn on_knob(&self, track: Rect, x: f32) -> bool {
        let knob_x = track.x + (track.w - track.h) * self.state.confirm;
        x >= knob_x && x <= knob_x + track.h
    }

    fn set_brightness(&mut self, t: &Theme, track: Rect, x: f32) {
        self.state.brightness = ((x - track.x) / track.w).clamp(MIN_BRIGHTNESS, 1.0);
        if let Some(fr) = self.frame.as_mut() {
            fr.canvas.fill_rect(track.x, track.y, track.w, track.h, t.pal.card);
            widgets::slider(&mut fr.canvas, t, track.x, track.y, track.w, self.state.brightness);
        }
        self.dirty = true;
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
    use skyward_install::{Failure, Refusal};
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
        tap_at(app, f, i, action, 0)
    }

    fn tap_at(app: &mut App, f: &Fonts, i: &DeviceInfo, action: Action, now: u64) -> Vec<Effect> {
        let (x, y) = spot(app, f, i, action);
        let mut fx = app.touch(Touch::Down(x, y), f, now);
        fx.extend(app.touch(Touch::Up, f, now));
        fx
    }

    /// Tap the `<`, which stays at the top of the screen.
    fn tap_back(app: &mut App, f: &Fonts, i: &DeviceInfo, now: u64) {
        app.frame(f, i, DataState::Unmounted);
        let (x, y) = widgets::back_rect(&app.theme()).centre();
        app.touch(Touch::Down(x, y), f, now);
        app.touch(Touch::Up, f, now);
    }

    #[test]
    fn rows_open_pages_and_back_returns() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        tap(&mut app, &f, &i, Action::Open(Screen::Settings));
        assert_eq!(app.screen(), Screen::Settings);
        tap_back(&mut app, &f, &i, 0);
        assert_eq!(app.screen(), Screen::Home);
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Home, "back on the first page stays there");
    }

    #[test]
    fn the_back_control_stays_put_while_the_page_scrolls() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        tap(&mut app, &f, &i, Action::Open(Screen::Settings));
        app.frame(&f, &i, DataState::Unmounted);
        app.touch(Touch::Down(700.0, 2400.0), &f, 0);
        app.touch(Touch::Move(700.0, 2200.0), &f, 0);
        app.touch(Touch::Move(700.0, 2100.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        assert_eq!(app.top().1, 300.0);
        tap_back(&mut app, &f, &i, 0);
        assert_eq!(app.screen(), Screen::Home);
    }

    /// `screen` opened from the home page, so there is a page to go back
    /// to.
    fn opened(screen: Screen, f: &Fonts, i: &DeviceInfo) -> App {
        let mut app = App::new(Screen::Home, W, H, PPD);
        tap(&mut app, f, i, Action::Open(screen));
        app
    }

    #[test]
    fn swipe_from_the_left_edge_goes_back() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Reboot, &f, &i);
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        app.touch(Touch::Move(90.0, 1605.0), &f, 0);
        app.touch(Touch::Move(320.0, 1610.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        assert_eq!(app.screen(), Screen::Home);
    }

    #[test]
    fn a_short_edge_swipe_does_not_go_back() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Reboot, &f, &i);
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        app.touch(Touch::Move(80.0, 1600.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        assert_eq!(app.screen(), Screen::Reboot);
    }

    #[test]
    fn the_first_page_has_no_edge_swipe_back() {
        let f = fonts();
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        app.touch(Touch::Move(80.0, 1600.0), &f, 8);
        assert!(app.touch(Touch::Move(400.0, 1600.0), &f, 16).is_empty(), "no tap of the motor");
        assert!(!matches!(app.drag, Drag::Edge { .. }), "no cue");
        app.touch(Touch::Up, &f, 24);
        assert_eq!(app.screen(), Screen::Home);
        assert_eq!(app.deadline(24), app.idle_due(), "nothing left on screen to settle");
    }

    #[test]
    fn the_back_cue_taps_once_as_it_arms_and_sinks_when_let_go() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Reboot, &f, &i);
        let ready = app.theme().m.dp(BACK_SWIPE_DP);
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        assert!(app.touch(Touch::Move(80.0, 1600.0), &f, 8).is_empty());
        assert_eq!(app.touch(Touch::Move(20.0 + ready, 1600.0), &f, 16), vec![Effect::Haptic]);
        assert!(app.touch(Touch::Move(60.0 + ready, 1600.0), &f, 24).is_empty(), "one tap while armed");
        app.touch(Touch::Move(40.0, 1600.0), &f, 32);
        app.touch(Touch::Up, &f, 40);
        assert_eq!(app.screen(), Screen::Reboot, "pulled back in, it does not go back");
        assert_eq!(app.deadline(40), Some(0), "the cue is sinking back");
        app.tick(1000);
        assert_eq!(app.deadline(1000), app.idle_due());
    }

    #[test]
    fn the_back_cue_stays_quiet_with_touch_vibration_off() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Reboot, &f, &i);
        app.state.vibrate = false;
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        app.touch(Touch::Move(80.0, 1600.0), &f, 8);
        assert!(app.touch(Touch::Move(400.0, 1600.0), &f, 16).is_empty());
    }

    #[test]
    fn the_back_cue_keeps_to_the_height_the_swipe_began_at() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Reboot, &f, &i);
        let at = 1600.0 - app.theme().m.dp(CUE_LIFT_DP);
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        app.touch(Touch::Move(80.0, 1610.0), &f, 8);
        app.touch(Touch::Move(200.0, 1300.0), &f, 16);
        app.touch(Touch::Move(150.0, 1900.0), &f, 24);
        let Drag::Edge { cy, .. } = app.drag else { panic!("still swiping in from the edge") };
        assert_eq!(cy, at, "the finger wandered up and down");
        app.touch(Touch::Move(40.0, 2100.0), &f, 32);
        app.touch(Touch::Up, &f, 40);
        assert_eq!(app.cue.as_ref().map(|c| c.1), Some(at), "it sinks back where it was");
    }

    #[test]
    fn pulling_past_the_top_resists_and_springs_back() {
        let f = fonts();
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        app.touch(Touch::Down(700.0, 1000.0), &f, 0);
        app.touch(Touch::Move(700.0, 1300.0), &f, 100);
        app.touch(Touch::Move(700.0, 1800.0), &f, 600);
        let shown = app.overscroll(600);
        assert!(shown < 0.0 && shown > -700.0, "the page follows less than the finger: {shown}");
        assert_eq!(app.top().1, 0.0);
        // Dragging back first undoes the pull.
        app.touch(Touch::Move(700.0, 1400.0), &f, 1000);
        assert!(app.overscroll(1000) > shown);
        app.touch(Touch::Up, &f, 1400);
        assert!(app.overscroll(1400) < 0.0);
        assert_eq!(app.deadline(1400), Some(0));
        app.tick(1500);
        assert!(app.overscroll(1500) > app.overscroll(1400), "on its way back");
        app.tick(3000);
        assert_eq!(app.overscroll(3000), 0.0);
        assert_eq!(app.deadline(3000), app.idle_due());
    }

    #[test]
    fn a_page_pulled_past_its_end_can_be_caught() {
        let f = fonts();
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        app.touch(Touch::Down(700.0, 1000.0), &f, 0);
        app.touch(Touch::Move(700.0, 1500.0), &f, 500);
        app.touch(Touch::Up, &f, 1000);
        app.tick(1050);
        let held = app.overscroll(1050);
        app.touch(Touch::Down(700.0, 1200.0), &f, 1050);
        app.tick(1500);
        assert!((app.overscroll(1500) - held).abs() < 1.0, "the finger holds it where it was");
        app.touch(Touch::Up, &f, 1600);
        app.tick(4000);
        assert_eq!(app.overscroll(4000), 0.0);
    }

    #[test]
    fn scrolling_back_down_past_the_top_pulls_from_the_top() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        app.frame(&f, &i, DataState::Unmounted);
        app.touch(Touch::Down(700.0, 2000.0), &f, 0);
        app.touch(Touch::Move(700.0, 1700.0), &f, 100);
        assert_eq!(app.top().1, 300.0);
        app.touch(Touch::Move(700.0, 2100.0), &f, 500);
        assert_eq!(app.top().1, 0.0);
        assert!(app.overscroll(500) < 0.0, "the last 100 px pull the page down");
    }

    #[test]
    fn dragging_scrolls_and_does_not_tap() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        let (x, y) = spot(&mut app, &f, &i, Action::Toggle(Switch::Vibrate));
        assert!(y < H as f32, "the switch is on the first screen");
        app.touch(Touch::Down(x, y), &f, 0);
        app.touch(Touch::Move(x, y - 100.0), &f, 0);
        app.touch(Touch::Move(x, y - 300.0), &f, 0);
        app.touch(Touch::Up, &f, 0);
        assert!(app.state.vibrate, "the switch was not flipped");
        assert_eq!(app.top().1, 300.0);
    }

    /// Flick the page up from `start`: the finger travels `step` px every
    /// 8 ms five times and lifts 8 ms after its last move. Returns when it
    /// lifted.
    fn flick(app: &mut App, f: &Fonts, start: u64, step: f32) -> u64 {
        app.touch(Touch::Down(700.0, 2400.0), f, start);
        for k in 1..=5u64 {
            app.touch(Touch::Move(700.0, 2400.0 - step * k as f32), f, start + 8 * k);
        }
        let lift = start + 48;
        app.touch(Touch::Up, f, lift);
        lift
    }

    #[test]
    fn a_flick_glides_on_and_comes_to_rest() {
        let f = fonts();
        // Every reading there is, for a page long enough to glide on.
        let battery = skyward_info::Battery {
            level: 80,
            charge: Charge::Discharging,
            temp: Some(300),
            health: Some("Good".into()),
            cycles: Some(169),
            design: Some(5_500_000),
            full: Some(5_095_000),
            voltage: Some(4_200_000),
        };
        let i = DeviceInfo {
            os_version: Some("OS3.0.305.0.WMACNXM".into()),
            android: Some("16".into()),
            battery: Some(battery),
            ..info()
        };
        let mut app = App::new(Screen::Specs, W, H, PPD);
        app.frame(&f, &i, DataState::Unmounted);
        let lift = flick(&mut app, &f, 1000, 20.0);
        let lifted = app.top().1;
        assert_eq!(lifted, 100.0);
        assert_eq!(app.deadline(lift), Some(0), "the page moves on after the lift");
        app.tick(lift + 50);
        assert!(app.top().1 > lifted);
        for ms in (100..5000).step_by(8) {
            app.tick(lift + ms);
        }
        assert_eq!(app.deadline(lift + 5000), app.idle_due(), "the glide comes to rest");
        let rest = app.top().1;
        assert!(rest > lifted + 300.0 && rest < app.max_scroll(), "{rest}");
    }

    #[test]
    fn a_glide_into_the_end_swings_past_it_and_back() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        app.frame(&f, &i, DataState::Unmounted);
        let lift = flick(&mut app, &f, 1000, 80.0);
        let mut furthest: f32 = 0.0;
        for ms in (8..2000).step_by(8) {
            app.tick(lift + ms);
            furthest = furthest.max(app.overscroll(lift + ms));
        }
        assert_eq!(app.top().1, app.max_scroll());
        assert!(furthest > app.theme().m.dp(8.0), "it ran on past the end: {furthest}");
        assert_eq!(app.overscroll(lift + 2000), 0.0);
        assert_eq!(app.deadline(lift + 2000), app.idle_due());
    }

    #[test]
    fn a_finger_that_stops_before_lifting_leaves_the_page_still() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        app.frame(&f, &i, DataState::Unmounted);
        app.touch(Touch::Down(700.0, 2400.0), &f, 1000);
        app.touch(Touch::Move(700.0, 2200.0), &f, 1008);
        app.touch(Touch::Move(700.0, 2000.0), &f, 1016);
        app.touch(Touch::Up, &f, 1100);
        assert!(app.fling.is_none());
        assert_eq!(app.top().1, 400.0);
    }

    #[test]
    fn touching_a_gliding_page_stops_it_without_pressing() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        app.frame(&f, &i, DataState::Unmounted);
        let lift = flick(&mut app, &f, 1000, 20.0);
        app.tick(lift + 30);
        assert!(app.fling.is_some(), "the page is gliding");
        let held = app.top().1;
        let (x, y) = spot(&mut app, &f, &i, Action::Toggle(Switch::Vibrate));
        assert!(y > 0.0 && y < H as f32, "the switch is on screen");
        app.touch(Touch::Down(x, y), &f, lift + 40);
        app.tick(lift + 500);
        assert_eq!(app.top().1, held, "the page stopped under the finger");
        app.touch(Touch::Up, &f, lift + 520);
        assert!(app.state.vibrate, "the switch was not flipped");
        assert!(app.fling.is_none());
    }

    #[test]
    fn an_edge_swipe_goes_back_over_a_gliding_page() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Settings, &f, &i);
        app.frame(&f, &i, DataState::Unmounted);
        let lift = flick(&mut app, &f, 1000, 20.0);
        app.tick(lift + 30);
        app.touch(Touch::Down(10.0, 1600.0), &f, lift + 40);
        app.touch(Touch::Move(90.0, 1605.0), &f, lift + 48);
        let under = app.top().1;
        app.tick(lift + 64);
        assert!(app.top().1 > under, "the page glides on under the swipe");
        app.touch(Touch::Move(400.0, 1610.0), &f, lift + 72);
        app.touch(Touch::Up, &f, lift + 80);
        assert_eq!(app.screen(), Screen::Home);
    }

    #[test]
    fn a_finger_dragging_from_the_edge_catches_a_gliding_page() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Settings, &f, &i);
        app.frame(&f, &i, DataState::Unmounted);
        let lift = flick(&mut app, &f, 1000, 20.0);
        app.tick(lift + 30);
        app.touch(Touch::Down(10.0, 1600.0), &f, lift + 40);
        app.tick(lift + 48);
        assert!(app.fling.is_some(), "landing at the edge leaves the glide going");
        app.touch(Touch::Move(12.0, 1500.0), &f, lift + 56);
        assert!(app.fling.is_none(), "dragging the page catches it");
        let held = app.top().1;
        app.touch(Touch::Move(12.0, 1450.0), &f, lift + 64);
        assert_eq!(app.top().1, held + 50.0, "and it follows the finger from there");
    }

    #[test]
    fn a_button_stops_a_gliding_page() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        app.frame(&f, &i, DataState::Unmounted);
        let lift = flick(&mut app, &f, 1000, 20.0);
        assert!(app.fling.is_some(), "the page is gliding");
        app.key(Key::Back, lift + 20);
        assert!(app.fling.is_none());
    }

    /// Drag the page to scroll `to`, resting before lifting so it does not
    /// glide, and let the time pass. The finger first goes 100 px, past the
    /// touch slop, and comes back to the mark.
    fn scroll_to(app: &mut App, f: &Fonts, to: f32, now: u64) {
        let from = app.top().1;
        app.touch(Touch::Down(700.0, 2400.0), f, now);
        app.touch(Touch::Move(700.0, 2300.0), f, now + 200);
        app.touch(Touch::Move(700.0, 2400.0 - (to - from)), f, now + 400);
        app.touch(Touch::Up, f, now + 800);
        app.tick(now + 800);
        assert!((app.top().1 - to).abs() < 0.01, "{} {to}", app.top().1);
    }

    #[test]
    fn the_bar_title_comes_after_the_large_title_fades_and_the_bar_once_content_slides_under() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Settings, W, H, PPD);
        let fr = app.frame(&f, &i, DataState::Unmounted);
        let content_top = fr.content_top;
        let m = app.theme().m;
        scroll_to(&mut app, &f, m.dp(TITLE_FADE_DP) - 2.0, 0);
        assert!(!app.title.on && !app.bar.on);
        scroll_to(&mut app, &f, m.dp(TITLE_FADE_DP) + 2.0, 1000);
        assert!(app.title.on, "the large title is gone");
        assert!(!app.bar.on, "the content is not under the bar yet");
        assert!(app.title.level(1900) < 1.0 && app.title.level(1900) > 0.0, "it eases in");
        assert_eq!(app.title.level(3000), 1.0);
        let under = content_top - widgets::bar_bottom(&app.theme()) - m.dp(20.0);
        scroll_to(&mut app, &f, under + 10.0, 4000);
        assert!(app.bar.on);
        assert_eq!(app.bar.level(4800 + 100), 1.0, "the bar comes in 100 ms");
        scroll_to(&mut app, &f, 0.0, 6000);
        assert!(!app.title.on && !app.bar.on);
    }

    #[test]
    fn the_about_page_brings_title_and_bar_together_once_the_version_fades() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::About, W, H, PPD);
        app.frame(&f, &i, DataState::Unmounted);
        let t = app.theme();
        let gone = t.m.dp(ABOUT_LOGO_DP) - widgets::bar_bottom(&t);
        scroll_to(&mut app, &f, gone - 4.0, 0);
        assert!(!app.title.on && !app.bar.on);
        scroll_to(&mut app, &f, gone + 4.0, 1000);
        assert!(app.title.on && app.bar.on);
        assert!((app.bar.level(1800 + 75) - 0.5).abs() < 0.01, "a straight 150 ms fade");
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
        assert!(app.touch(Touch::Down(track.x + track.w * 0.25, y), &f, 0).is_empty());
        assert_eq!(app.panel(0), Some(0.25));
        assert!(app.touch(Touch::Move(track.x + track.w * 0.75, y + 40.0), &f, 0).is_empty());
        assert_eq!(app.panel(0), Some(0.75));
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
        tap(&mut app, &f, &i, Action::Unavailable("备份还没实现"));
        assert_eq!(app.deadline(0), Some(TOAST_MS));
        app.tick(TOAST_MS);
        assert_eq!(app.deadline(TOAST_MS), app.idle_due());
    }

    /// The sideload page, opened from the installing page, with the
    /// installer started and waiting for the computer.
    fn sideloading(f: &Fonts, i: &DeviceInfo) -> App {
        let mut app = opened(Screen::Install, f, i);
        assert_eq!(tap(&mut app, f, i, Action::Sideload), vec![Effect::Sideload]);
        assert_eq!(app.screen(), Screen::Sideload);
        assert_eq!(app.state.install, Install::Starting);
        app.install(Event::Waiting, 0);
        app
    }

    #[test]
    fn leaving_while_waiting_cancels_and_the_end_clears_it() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        assert_eq!(app.state.install, Install::Waiting);
        app.frame(&f, &i, DataState::Unmounted);
        app.touch(Touch::Down(10.0, 1600.0), &f, 0);
        app.touch(Touch::Move(90.0, 1600.0), &f, 8);
        app.touch(Touch::Move(400.0, 1600.0), &f, 16);
        assert_eq!(app.touch(Touch::Up, &f, 24), vec![Effect::Cancel]);
        assert_eq!(app.screen(), Screen::Install);
        assert_eq!(app.state.install, Install::Ending);
        // Whatever the installer was in the middle of, only its end counts.
        app.install(Event::Checking { size: 4096 }, 0);
        assert_eq!(app.state.install, Install::Ending);
        tap(&mut app, &f, &i, Action::Sideload);
        assert_eq!(app.screen(), Screen::Install, "no second installer while the first is ending");
        app.install(Event::Finished(Outcome::Cancelled), 0);
        assert_eq!(app.state.install, Install::Idle);
    }

    #[test]
    fn an_image_goes_where_it_is_sent_once_swiped() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.install(Event::Checking { size: 201_326_592 }, 0);
        let kind = ImageKind::Boot { kernel: true, ramdisk: skyward_install::Ramdisk::Recovery };
        app.install(Event::Image { kind, size: 201_326_592, partitions: vec!["recovery", "boot"] }, 0);
        // The image lit the panel, so the user looks and taps past the wake
        // guard.
        tap_at(&mut app, &f, &i, Action::FlashPartition(1), 1000);
        tap_at(&mut app, &f, &i, Action::FlashSlots(Pick::Both), 1000);
        let track = app.frame(&f, &i, DataState::Unmounted).hits.iter().find(|h| h.action == Action::Confirm(Confirm::Flash)).unwrap().rect;
        let y = track.centre().1 - app.top().1;
        app.touch(Touch::Down(track.x + track.h / 2.0, y), &f, 1000);
        app.touch(Touch::Move(track.x + track.w, y), &f, 1000);
        assert_eq!(app.touch(Touch::Up, &f, 1000), vec![Effect::Flash { partition: "boot", pick: Pick::Both }]);
        assert_eq!(app.state.install, Install::Flashing { partition: "boot", stage: None });
        app.install(Event::Progress(Stage::Flashing { done: 10, total: 40 }), 1000);
        assert_eq!(app.state.install, Install::Flashing { partition: "boot", stage: Some(Stage::Flashing { done: 10, total: 40 }) });
        app.key(Key::Back, 1000);
        assert_eq!(app.screen(), Screen::Sideload, "a partition being written keeps the page");
        app.install(Event::Finished(Outcome::Flashed { partition: "boot", slots: vec![Slot::A, Slot::B] }), 1000);
        assert!(matches!(app.state.install, Install::Finished { damage: None, .. }), "a flash that worked broke nothing");
        assert_eq!(tap_at(&mut app, &f, &i, Action::RebootTo(Target::System), 2000), vec![Effect::Reboot(Target::System)]);
        app.key(Key::Back, 2000);
        assert_eq!(app.screen(), Screen::Install);
        assert_eq!(app.state.install, Install::Idle);
    }

    #[test]
    fn an_update_shows_its_steps_and_cannot_be_left() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.install(Event::Checking { size: 7_103_043_868 }, 0);
        app.install(Event::Installing { version: Some("OS3.0.306.0.WMACNXM".into()) }, 0);
        app.install(Event::Overwriting, 0);
        app.install(Event::Progress(Stage::Writing { done: 1 << 30, total: 7 << 30 }), 0);
        assert!(matches!(app.state.install, Install::Updating { overwriting: true, stage: Some(_), .. }));
        let shown = app.frame(&f, &i, DataState::Unmounted).hits.len();
        assert_eq!(shown, 0, "nothing on the page to tap while installing");
        app.key(Key::Back, 0);
        assert_eq!(app.screen(), Screen::Sideload);
        assert_eq!(app.deadline(0), Some(TOAST_MS), "it says why");
        app.install(Event::Finished(Outcome::Failed(Failure::Engine(9))), 0);
        assert!(matches!(app.state.install, Install::Finished { damage: Some(Damage::System), .. }));
        // Sending again starts over on the same page, past the wake guard.
        assert_eq!(tap_at(&mut app, &f, &i, Action::Sideload, 1000), vec![Effect::Sideload]);
        assert_eq!(app.state.install, Install::Starting);
        assert_eq!(app.screen(), Screen::Sideload);
    }

    #[test]
    fn a_refused_package_or_an_update_that_kept_the_system_broke_nothing() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.install(Event::Checking { size: 10 }, 0);
        app.install(Event::Finished(Outcome::Refused(Refusal::Unknown)), 0);
        assert_eq!(app.state.install, Install::Finished { outcome: Outcome::Refused(Refusal::Unknown), damage: None });
        tap_at(&mut app, &f, &i, Action::Sideload, 1000);
        app.install(Event::Waiting, 1000);
        app.install(Event::Installing { version: None }, 1000);
        app.install(Event::Finished(Outcome::Failed(Failure::Engine(21))), 1000);
        assert!(matches!(app.state.install, Install::Finished { damage: None, .. }));
    }

    #[test]
    fn an_image_that_failed_part_way_names_its_partition() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.install(Event::Image { kind: ImageKind::Dtbo, size: 1 << 20, partitions: vec!["dtbo"] }, 0);
        app.state.install = Install::Flashing { partition: "dtbo", stage: None };
        app.install(Event::Progress(Stage::Flashing { done: 1 << 20, total: 4 << 20 }), 0);
        app.install(Event::Finished(Outcome::Failed(Failure::Disconnected)), 0);
        assert!(matches!(app.state.install, Install::Finished { damage: Some(Damage::Partition("dtbo")), .. }));
    }

    #[test]
    fn fastboot_cannot_be_chosen_until_it_is_wired_up() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        let hits = &app.frame(&f, &i, DataState::Unmounted).hits;
        let fastboot = TARGETS.iter().position(|t| *t == Target::Fastboot).unwrap();
        assert!(!hits.iter().any(|h| h.action == Action::RebootTarget(fastboot)));
        tap(&mut app, &f, &i, Action::Unavailable("用户空间 fastboot 还没接入"));
        assert_eq!(app.state.reboot, 0, "the target stays on system");
        assert_eq!(app.deadline(0), Some(TOAST_MS));
    }

    #[test]
    fn power_key_turns_the_panel_off_and_on() {
        let mut app = App::new(Screen::Home, W, H, PPD);
        assert!(app.key(Key::Power, 0).is_empty());
        assert_eq!(app.panel(0), None);
        assert!(app.key(Key::Power, 10).is_empty());
        assert_eq!(app.panel(10), Some(0.5));
    }

    /// With the default two minutes, when the panel dims and when it goes
    /// off.
    const DIMS: u64 = 113_000;
    const OFF: u64 = 120_000;

    #[test]
    fn left_alone_the_panel_dims_then_goes_off() {
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.tick(0);
        assert_eq!(app.deadline(0), Some(DIMS));
        app.tick(DIMS - 1);
        assert_eq!(app.panel(DIMS - 1), Some(0.5));
        app.tick(DIMS);
        assert_eq!(app.deadline(DIMS), Some(DIMS + BACKLIGHT_STEP_MS), "the backlight steps down a frame at a time");
        let midway = app.panel(DIMS + 250).unwrap();
        assert!(midway < 0.5 && midway > dim_level(0.5), "{midway}");
        assert_eq!(app.panel(DIMS + 500), Some(dim_level(0.5)));
        assert_eq!(app.deadline(DIMS + 500), Some(OFF), "dim until it goes off");
        app.tick(OFF);
        assert_eq!(app.panel(OFF), None);
        assert!(!app.display_on());
        assert_eq!(app.deadline(OFF), None, "nothing to wait for on a dark panel");
    }

    #[test]
    fn time_before_the_first_tick_does_not_count_as_idle() {
        let mut app = App::new(Screen::Home, W, H, PPD);
        // The UI started long after boot; the idle clock begins at the
        // first tick, not at zero.
        app.tick(5_000_000);
        assert_eq!(app.panel(5_000_000), Some(0.5));
        assert_eq!(app.deadline(5_000_000), Some(5_000_000 + DIMS));
        app.tick(5_000_000 + OFF);
        assert_eq!(app.panel(5_000_000 + OFF), None);
    }

    #[test]
    fn the_dim_level_sits_below_the_brightness_set() {
        assert_eq!(dim_level(1.0), DIM_CEILING);
        assert_eq!(dim_level(0.06), MIN_BRIGHTNESS);
        assert_eq!(dim_level(0.03), MIN_BRIGHTNESS);
        assert_eq!(dim_level(0.01), 0.01, "never brighter than the level set");
        assert_eq!(dim_span(15_000), 3000);
        assert_eq!(dim_span(120_000), DIM_MS);
    }

    #[test]
    fn a_touch_while_dim_brightens_and_still_counts() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.tick(0);
        app.tick(DIMS);
        assert_eq!(app.panel(DIMS + 500), Some(dim_level(0.5)));
        let (x, y) = spot(&mut app, &f, &i, Action::Open(Screen::Settings));
        app.touch(Touch::Down(x, y), &f, DIMS + 600);
        app.touch(Touch::Up, &f, DIMS + 650);
        assert_eq!(app.screen(), Screen::Settings, "the tap counts");
        assert_eq!(app.panel(DIMS + 800), Some(0.5), "back to full brightness");
        assert_eq!(app.deadline(DIMS + 800), Some(DIMS + 650 + DIMS), "the wait starts over");
    }

    #[test]
    fn a_touch_on_the_dark_panel_only_lights_it() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Home, W, H, PPD);
        let (x, y) = spot(&mut app, &f, &i, Action::Open(Screen::Settings));
        app.key(Key::Power, 0);
        assert!(app.touch(Touch::Down(x, y), &f, 100).is_empty());
        assert_eq!(app.panel(100), Some(0.5));
        assert!(app.touch(Touch::Move(x, y - 300.0), &f, 120).is_empty());
        assert!(app.touch(Touch::Up, &f, 140).is_empty());
        assert_eq!(app.screen(), Screen::Home, "the waking tap pressed nothing");
        assert_eq!(app.top().1, 0.0, "nor scrolled the page");
        tap_at(&mut app, &f, &i, Action::Open(Screen::Settings), 600);
        assert_eq!(app.screen(), Screen::Settings, "a tap past the guard does");
    }

    #[test]
    fn a_key_lights_the_dark_panel_and_does_nothing_else() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Settings, &f, &i);
        app.key(Key::Power, 0);
        assert!(app.key(Key::Back, 10).is_empty());
        assert!(app.display_on());
        assert_eq!(app.screen(), Screen::Settings, "back did not go back");
    }

    #[test]
    fn a_stray_press_just_after_the_panel_lights_only_keeps_it_awake() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Reboot, &f, &i);
        app.key(Key::Power, 0);
        // A key wakes the panel; the next, within the guard, does not sleep
        // it again or act.
        app.key(Key::Power, 100);
        assert!(app.display_on());
        assert!(app.key(Key::Power, 300).is_empty());
        assert!(app.display_on(), "a second power press did not turn it off");
        assert!(app.key(Key::Back, 350).is_empty());
        assert_eq!(app.screen(), Screen::Reboot, "back did not go back");
        // Past the guard, power turns it off as usual.
        app.key(Key::Power, 600);
        assert!(!app.display_on());
    }

    #[test]
    fn a_stray_touch_just_after_an_install_lights_the_panel_presses_nothing() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.key(Key::Power, 0);
        app.install(Event::Finished(Outcome::Installed { slot: Slot::A }), 100);
        assert!(app.display_on(), "the finished install lit the panel");
        // A finger reaching for the phone lands on 重启到系统; within the
        // guard it only keeps the panel awake.
        assert!(tap_at(&mut app, &f, &i, Action::RebootTo(Target::System), 200).is_empty());
        assert_eq!(app.screen(), Screen::Sideload, "the stray tap did not reboot");
        // Past the guard the same tap reboots.
        assert_eq!(
            tap_at(&mut app, &f, &i, Action::RebootTo(Target::System), 700),
            vec![Effect::Reboot(Target::System)]
        );
    }

    #[test]
    fn a_finger_held_as_the_panel_goes_off_lets_go() {
        let (f, i) = (fonts(), info());
        let mut app = App::new(Screen::Reboot, W, H, PPD);
        let track = app.frame(&f, &i, DataState::Unmounted).hits.iter().find(|h| h.action == Action::Confirm(Confirm::Reboot)).unwrap().rect;
        let y = track.centre().1;
        app.touch(Touch::Down(track.x + track.h / 2.0, y), &f, 0);
        app.touch(Touch::Move(track.x + track.w * 0.6, y), &f, 10);
        assert!(app.state.confirm > 0.0);
        app.key(Key::Power, 20);
        assert_eq!(app.state.confirm, 0.0, "the knob is back at the start");
        app.touch(Touch::Move(track.x + track.w, y), &f, 30);
        assert!(app.touch(Touch::Up, &f, 40).is_empty(), "no reboot from a dark panel");
        assert!(!app.display_on());
    }

    #[test]
    fn never_leaves_the_panel_on() {
        let mut app = App::new(Screen::Home, W, H, PPD);
        app.state.screen_off = SCREEN_OFF.iter().position(Option::is_none).unwrap();
        app.tick(0);
        assert_eq!(app.deadline(0), None);
        let day = 24 * 3600 * 1000;
        app.tick(day);
        assert_eq!(app.panel(day), Some(0.5));
    }

    #[test]
    fn the_installer_lights_the_panel_when_it_needs_the_user() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        app.key(Key::Power, 0);
        app.install(Event::Checking { size: 1 << 20 }, 10);
        assert!(!app.display_on(), "reading the package does not light it");
        app.install(Event::Image { kind: ImageKind::Dtbo, size: 1 << 20, partitions: vec!["dtbo"] }, 20);
        assert!(app.display_on(), "an image waiting for its partition does");
        assert_eq!(app.deadline(20), Some(20 + DIMS));
        app.state.install = Install::Flashing { partition: "dtbo", stage: None };
        app.key(Key::Power, 500);
        app.install(Event::Progress(Stage::Flashing { done: 1, total: 2 }), 510);
        assert!(!app.display_on(), "progress does not");
        app.install(Event::Finished(Outcome::Flashed { partition: "dtbo", slots: vec![Slot::A] }), 520);
        assert!(app.display_on(), "the end does");
    }

    #[test]
    fn an_install_ending_after_its_page_was_left_keeps_the_panel_dark() {
        let (f, i) = (fonts(), info());
        let mut app = sideloading(&f, &i);
        assert_eq!(app.key(Key::Back, 0), vec![Effect::Cancel]);
        app.key(Key::Power, 10);
        app.install(Event::Finished(Outcome::Cancelled), 20);
        assert!(!app.display_on());
        assert_eq!(app.state.install, Install::Idle);
    }

    #[test]
    fn choosing_how_long_the_screen_stays_on() {
        let (f, i) = (fonts(), info());
        let mut app = opened(Screen::Settings, &f, &i);
        tap(&mut app, &f, &i, Action::Open(Screen::ScreenOff));
        assert_eq!(app.screen(), Screen::ScreenOff);
        let frame = app.frame(&f, &i, DataState::Unmounted);
        assert_eq!(frame.title, "息屏时间");
        assert!(frame.back);
        // The choices sit in order, top to bottom.
        let actions: Vec<Action> = frame.hits.iter().map(|h| h.action).collect();
        assert_eq!(actions, (0..SCREEN_OFF.len()).map(Action::ScreenOff).collect::<Vec<_>>());
        tap(&mut app, &f, &i, Action::ScreenOff(0));
        assert_eq!(app.state.screen_off, 0);
        assert_eq!(app.screen(), Screen::ScreenOff, "the page stays");
        // Fifteen seconds, dim for the last three.
        assert_eq!(app.deadline(0), Some(12_000));
        app.tick(15_000);
        assert!(!app.display_on());
    }
}
