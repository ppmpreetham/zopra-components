#![cfg(feature = "pixel-reveal")]
#![allow(
    dead_code,
    private_interfaces,
    unused_mut,
    clippy::all,
    clippy::pedantic,
    ambiguous_wide_pointer_comparisons
)]
#![allow(non_upper_case_globals, warnings)]

use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    time::Instant,
};

use fontdue::{Font as FdFont, FontSettings};
use gpui_kit::{
    App, Bounds, ContentMask, Context, EventEmitter, FontId, GlyphId, Hsla, MouseMoveEvent, Pixels,
    Point, Render, Rgba, SharedString, TextRun, Window, canvas, div, fill, font, point, prelude::*,
    px, rgb, size,
};

fn mixf(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn fract(x: f32) -> f32 {
    x - x.floor()
}
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn damp(from: f32, to: f32, rate: f32, dt: f32) -> f32 {
    if rate.is_finite() {
        from + (to - from) * (1.0 - (-rate * dt).exp())
    } else {
        to
    }
}

fn hash01(n: i32) -> f32 {
    let mut t = (0x9e37_79b9u32 ^ n as u32).wrapping_mul(0x85eb_ca6b);
    t = (t ^ (t >> 13)).wrapping_mul(0xc2b2_ae35);
    t ^= t >> 16;
    t as f32 / 4_294_967_296.0
}
fn pcg(v: u32) -> u32 {
    let state = v.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let word = ((state >> ((state >> 28) + 4)) ^ state).wrapping_mul(277_803_737);
    (word >> 22) ^ word
}
fn hash(x: i32, y: i32, salt: u32, seed: u32) -> f32 {
    let (cx, cy) = (x.wrapping_add(32768) as u32, y.wrapping_add(32768) as u32);
    let inner = pcg(salt.wrapping_add(seed));
    pcg(cx ^ pcg(cy ^ inner)) as f32 / 4_294_967_295.0
}
fn bayer2(x: f32, y: f32) -> f32 {
    let (x, y) = (x.floor(), y.floor());
    fract(x / 2.0 + y * y * 0.75)
}
fn bayer8(x: f32, y: f32) -> f32 {
    (bayer2(0.25 * x, 0.25 * y) * 0.25 + bayer2(0.5 * x, 0.5 * y)) * 0.25 + bayer2(x, y)
}
fn rgb3(c: Hsla) -> [f32; 3] {
    let c = Rgba::from(c);
    [c.r, c.g, c.b]
}
fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        mixf(a[0], b[0], t),
        mixf(a[1], b[1], t),
        mixf(a[2], b[2], t),
    ]
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

macro_rules! alias_enum {
    ($name:ident, $label:literal, $fallback:ident,
     { $($var:ident => [$($alias:literal),+ $(,)?]),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $($var),+ }
        impl Default for $name { fn default() -> Self { Self::$fallback } }
        impl $name {
            pub fn parse(value: &str) -> Self {
                let key = normalize(value);
                $( if [$($alias),+].contains(&key.as_str()) { return Self::$var; } )+
                eprintln!("[pixel-reveal] {}=\"{}\" is not recognised, using {:?}",
                    $label, value, Self::$fallback);
                Self::$fallback
            }
        }
        impl std::convert::From<&str> for $name { fn from(v: &str) -> Self { Self::parse(v) } }
    };
}

alias_enum!(Trigger, "trigger", Inview, {
    Load => ["load", "mount", "auto", "immediate"],
    Inview => ["inview", "enter", "view", "scroll", "onenter"],
    Scrub => ["scrub", "scrollscrub"],
    Pin => ["pin", "pinned", "sticky"],
    Manual => ["manual", "controlled", "ref"],
});
alias_enum!(Split, "split", Words, {
    Words => ["words", "word", "perword"],
    Chars => ["chars", "char", "characters", "letters"],
    Lines => ["lines", "line"],
    None => ["none", "block", "overall", "whole", "false"],
});
alias_enum!(Sweep, "sweep", Each, {
    Each => ["each", "unit", "units", "perword"],
    Whole => ["whole", "all", "overall", "flow", "block"],
});
alias_enum!(From, "from", Start, {
    Start => ["start", "first"],
    End => ["end", "last"],
    Center => ["center", "centre", "middle"],
    Edges => ["edges", "outside"],
    Random => ["random", "shuffle"],
});

alias_enum!(Direction, "direction", Up, {
    Up => ["up", "bottomtop", "bottomtotop", "frombottom"],
    Down => ["down", "topbottom", "toptobottom", "fromtop"],
    Left => ["left", "rightleft", "righttoleft"],
    Right => ["right", "leftright", "lefttoright"],
    Center => ["center", "centre", "out", "radial", "wavecenter"],
    Edges => ["edges", "in", "inward"],
    Diagonal => ["diagonal", "diag", "wavediagonal"],
    None => ["none", "pixelonly", "off"],
});
alias_enum!(Pattern, "pattern", Clusters, {
    Random => ["random", "noise", "wave"],
    Clusters => ["clusters", "clustered", "cluster", "blobs"],
    Scanlines => ["scanlines", "scanline", "scan"],
    Rain => ["rain", "columnrain", "columns"],
    Typewriter => ["typewriter", "reading"],
    Zigzag => ["zigzag", "boustrophedon"],
    Cascade => ["cascade", "stairs", "stripes"],
    Bayer => ["bayer", "dither", "ordered"],
    Flow => ["flow", "liquid", "organic"],
    None => ["none", "clean", "off"],
});
alias_enum!(Hover, "hover", None, {
    None => ["none", "off", "false", "null"],
    Lens => ["lens", "true", "pixel"],
});
alias_enum!(Preset, "preset", Materialize, {
    Materialize => ["materialize"],
    Signal => ["signal"],
    Typewriter => ["typewriter"],
    Dissolve => ["dissolve"],
    Rain => ["rain"],
    Radiate => ["radiate"],
    Bitmap => ["bitmap"],
    Glitch => ["glitch"],
    Flow => ["flow"],
});

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    f: f32,
    px: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pos {
    Num(f32),
    At { el: Anchor, vp: Anchor },
}

impl Pos {
    pub fn parse(s: &str) -> Pos {
        fn anchor(t: &str) -> Option<Anchor> {
            match t {
                "top" => Some(Anchor { f: 0.0, px: 0.0 }),
                "center" | "middle" => Some(Anchor { f: 0.5, px: 0.0 }),
                "bottom" => Some(Anchor { f: 1.0, px: 0.0 }),
                _ => {
                    let n: f32 = t.trim_end_matches(['%', 'p', 'x']).parse().ok()?;
                    Some(if t.ends_with('%') {
                        Anchor {
                            f: n / 100.0,
                            px: 0.0,
                        }
                    } else {
                        Anchor { f: 0.0, px: n }
                    })
                }
            }
        }
        let mut it = s.split_whitespace();
        let (a, b) = (it.next().unwrap_or("top"), it.next().unwrap_or("bottom"));
        match (anchor(a), anchor(b)) {
            (Some(el), Some(vp)) => Pos::At { el, vp },
            _ => {
                eprintln!("[pixel-reveal] can't read start/end \"{s}\" - use e.g. \"top 85%\"");
                Pos::Num(0.88)
            }
        }
    }
    fn resolve(self) -> (Anchor, Anchor) {
        match self {
            Pos::Num(v) => (Anchor { f: 0.0, px: 0.0 }, Anchor { f: v, px: 0.0 }),
            Pos::At { el, vp } => (el, vp),
        }
    }
}
impl std::convert::From<f32> for Pos {
    fn from(v: f32) -> Self {
        Pos::Num(v)
    }
}
impl std::convert::From<&str> for Pos {
    fn from(v: &str) -> Self {
        Pos::parse(v)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    In,
    Out,
    InOut,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    Linear,
    Pow(f32, Kind),
    Sine(Kind),
    Expo(Kind),
    Circ(Kind),
    Back(Kind),
}

impl Ease {
    pub fn parse(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        let (head, tail) = lower.split_once('.').unwrap_or((&lower, "out"));
        let kind = match tail {
            "in" => Kind::In,
            "inout" => Kind::InOut,
            _ => Kind::Out,
        };
        match head {
            "none" | "linear" => Self::Linear,
            "power1" | "quad" => Self::Pow(2.0, kind),
            "power2" | "cubic" => Self::Pow(3.0, kind),
            "power3" | "quart" => Self::Pow(4.0, kind),
            "power4" | "quint" => Self::Pow(5.0, kind),
            "sine" => Self::Sine(kind),
            "expo" => Self::Expo(kind),
            "circ" => Self::Circ(kind),
            "back" => Self::Back(kind),
            _ => {
                eprintln!("[pixel-reveal] unknown ease \"{name}\", using power2.out");
                Self::Pow(3.0, Kind::Out)
            }
        }
    }

    pub fn apply(self, t: f32) -> f32 {
        fn shape(kind: Kind, t: f32, f: impl Fn(f32) -> f32) -> f32 {
            match kind {
                Kind::In => f(t),
                Kind::Out => 1.0 - f(1.0 - t),
                Kind::InOut => {
                    if t < 0.5 {
                        0.5 * f(2.0 * t)
                    } else {
                        1.0 - 0.5 * f(2.0 * (1.0 - t))
                    }
                }
            }
        }
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Pow(n, k) => shape(k, t, |x| x.powf(n)),
            Self::Sine(k) => shape(k, t, |x| 1.0 - (x * std::f32::consts::FRAC_PI_2).cos()),
            Self::Expo(k) => shape(k, t, |x| {
                if x <= 0.0 {
                    0.0
                } else {
                    2f32.powf(10.0 * (x - 1.0))
                }
            }),
            Self::Circ(k) => shape(k, t, |x| 1.0 - (1.0 - x * x).max(0.0).sqrt()),
            Self::Back(k) => shape(k, t, |x| 2.70158 * x * x * x - 1.70158 * x * x),
        }
    }
}

macro_rules! options {
    ($($name:ident : $ty:ty = $default:expr),* $(,)?) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct Resolved { $(pub $name: $ty,)* }
        impl Resolved { fn defaults() -> Self { Self { $($name: $default,)* } } }

        #[derive(Clone, Debug, Default, PartialEq)]
        pub struct Options { preset: Option<Preset>, $($name: Option<$ty>,)* }
        impl Options {
            $(pub fn $name(mut self, v: impl std::convert::Into<$ty>) -> Self { self.$name = Some(v.into()); self })*
            fn apply_to(&self, r: &mut Resolved) { $(if let Some(v) = &self.$name { r.$name = v.clone(); })* }
        }
    };
}

fn hex(v: u32) -> Option<Hsla> {
    Some(rgb(v).into())
}

options! {
    trigger: Trigger = Trigger::Inview,
    start: Option<Pos> = None,
    end: Option<Pos> = None,
    once: bool = true,
    smooth: f32 = 0.3,
    delay: f32 = 0.0,
    reverse_speed: f32 = 1.6,
    split: Split = Split::Words,
    sweep: Sweep = Sweep::Each,
    from: From = From::Start,
    stagger: f32 = 0.06,
    duration: f32 = 1.5,
    ease: SharedString = "power1.inOut".into(),
    reveal_delay: f32 = 0.05,
    rise: f32 = 0.45,
    rise_duration: f32 = 1.1,
    rise_ease: SharedString = "expo.out".into(),
    direction: Direction = Direction::Up,
    pattern: Pattern = Pattern::Clusters,
    noise: f32 = 0.5,
    scatter: f32 = 0.08,
    spread: f32 = 0.8,
    pixel: f32 = 24.0,
    levels: u32 = 3,
    solid: f32 = 0.9,
    accent: Option<Hsla> = hex(0x1700c7),
    accent2: Option<Hsla> = None,
    accent_mix: f32 = 0.35,
    accent_strength: f32 = 1.0,
    accent_width: f32 = 0.45,
    color_noise: f32 = 0.03,
    sparkle: f32 = 0.03,
    flicker: f32 = 0.08,
    glitch: f32 = 0.0,
    hover: Hover = Hover::None,
    lens_radius: f32 = 110.0,
    seed: f32 = 0.0,
}

impl Options {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn preset(mut self, p: impl Into<Preset>) -> Self {
        self.preset = Some(p.into());
        self
    }
}

pub fn preset_options(p: Preset) -> Options {
    let o = Options::new();
    match p {
        Preset::Materialize => o,
        Preset::Signal => o
            .rise(0.0)
            .stagger(0.07)
            .duration(1.25)
            .ease("power2.out")
            .direction("right")
            .pattern("scanlines")
            .noise(0.6)
            .scatter(0.04)
            .spread(0.5)
            .pixel(16.0)
            .levels(3u32)
            .solid(0.9)
            .accent(hex(0xaeff00))
            .accent_width(0.6)
            .color_noise(0.04)
            .sparkle(0.18)
            .flicker(0.3)
            .glitch(0.35),
        Preset::Typewriter => o
            .split("chars")
            .stagger(0.024)
            .duration(0.45)
            .ease("power1.out")
            .reveal_delay(0.0)
            .rise(0.0)
            .direction("right")
            .pattern("none")
            .spread(0.35)
            .pixel(12.0)
            .levels(2u32)
            .solid(1.0)
            .accent_width(0.7)
            .sparkle(0.0)
            .flicker(0.2),
        Preset::Dissolve => o
            .split("lines")
            .sweep("whole")
            .duration(1.8)
            .ease("sine.inOut")
            .rise(0.0)
            .direction("none")
            .pattern("random")
            .spread(0.6)
            .pixel(12.0)
            .levels(2u32)
            .solid(0.0)
            .accent(None::<Hsla>)
            .color_noise(0.0)
            .sparkle(0.0)
            .flicker(0.0),
        Preset::Rain => o
            .split("chars")
            .from("random")
            .stagger(0.018)
            .duration(0.95)
            .rise(-0.6)
            .rise_duration(1.1)
            .direction("down")
            .pattern("rain")
            .noise(0.55)
            .spread(0.5)
            .pixel(16.0)
            .levels(3u32)
            .sparkle(0.2),
        Preset::Radiate => o
            .split("lines")
            .sweep("whole")
            .duration(2.2)
            .ease("sine.inOut")
            .rise(0.0)
            .direction("center")
            .pattern("bayer")
            .noise(0.35)
            .spread(0.4)
            .pixel(32.0)
            .levels(4u32)
            .solid(0.85)
            .accent_width(0.5),
        Preset::Bitmap => o
            .split("lines")
            .sweep("whole")
            .stagger(0.1)
            .duration(1.6)
            .ease("power1.out")
            .rise(0.35)
            .rise_duration(1.4)
            .direction("up")
            .pattern("bayer")
            .noise(0.5)
            .spread(0.35)
            .pixel(8.0)
            .levels(2u32)
            .solid(1.0)
            .accent(None::<Hsla>)
            .color_noise(0.0)
            .sparkle(0.0)
            .flicker(0.0),
        Preset::Glitch => o
            .rise(0.0)
            .stagger(0.04)
            .duration(1.0)
            .ease("power2.out")
            .direction("none")
            .pattern("scanlines")
            .noise(1.0)
            .pixel(24.0)
            .levels(3u32)
            .solid(0.9)
            .accent(hex(0xff3d00))
            .accent2(hex(0x00d1ff))
            .accent_mix(0.5)
            .accent_width(0.7)
            .color_noise(0.2)
            .sparkle(0.25)
            .flicker(0.45)
            .glitch(0.85),
        Preset::Flow => o
            .sweep("whole")
            .duration(2.4)
            .ease("none")
            .rise(0.6)
            .direction("diagonal")
            .pattern("flow")
            .noise(0.55)
            .spread(0.5)
            .pixel(24.0)
            .levels(4u32),
    }
}

pub fn resolve(o: &Options) -> Resolved {
    let mut r = Resolved::defaults();
    if let Some(p) = o.preset {
        preset_options(p).apply_to(&mut r);
    }
    o.apply_to(&mut r);

    let c = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
    let d = Resolved::defaults();
    r.smooth = c(r.smooth, 0.0, 5.0, d.smooth);
    r.delay = c(r.delay, 0.0, 60.0, d.delay);
    r.reverse_speed = c(r.reverse_speed, 0.05, 20.0, d.reverse_speed);
    r.stagger = c(r.stagger, 0.0, 5.0, d.stagger);
    r.duration = c(r.duration, 0.01, 60.0, d.duration);
    r.reveal_delay = c(r.reveal_delay, -5.0, 5.0, d.reveal_delay);
    r.rise = c(r.rise, -4.0, 4.0, d.rise);
    r.rise_duration = c(r.rise_duration, 0.01, 60.0, d.rise_duration);
    r.noise = c(r.noise, 0.0, 1.0, d.noise);
    r.scatter = c(r.scatter, 0.0, 1.0, d.scatter);
    r.spread = c(r.spread, 0.05, 1.0, d.spread);
    r.pixel = c(r.pixel, 1.0, 256.0, d.pixel);
    r.levels = r.levels.min(6);
    r.solid = c(r.solid, 0.0, 1.0, d.solid);
    r.accent_mix = c(r.accent_mix, 0.0, 1.0, d.accent_mix);
    r.accent_strength = c(r.accent_strength, 0.0, 1.0, d.accent_strength);
    r.accent_width = c(r.accent_width, 0.01, 0.9, d.accent_width);
    r.color_noise = c(r.color_noise, 0.0, 1.0, d.color_noise);
    r.sparkle = c(r.sparkle, 0.0, 1.0, d.sparkle);
    r.flicker = c(r.flicker, 0.0, 1.0, d.flicker);
    r.glitch = c(r.glitch, 0.0, 1.0, d.glitch);
    r.lens_radius = c(r.lens_radius, 4.0, 2000.0, d.lens_radius);
    r.seed = c(r.seed, -1e6, 1e6, d.seed);
    r
}

fn trigger_defaults(t: Trigger) -> (Option<Pos>, Option<Pos>) {
    match t {
        Trigger::Load | Trigger::Manual => (None, None),
        Trigger::Inview => (Some(Pos::parse("top 88%")), None),
        Trigger::Scrub => (Some(Pos::parse("top 92%")), Some(Pos::parse("bottom 55%"))),
        Trigger::Pin => (Some(Pos::Num(0.0)), Some(Pos::Num(0.85))),
    }
}

#[derive(Clone)]

pub struct RevealFont {
    pub family: SharedString,
    pub bytes: Option<&'static [u8]>,
    pub fd: FdFont,
}

impl RevealFont {
    pub fn new(family: impl Into<SharedString>, bytes: &'static [u8]) -> Result<Rc<Self>, String> {
        let fd = FdFont::from_bytes(bytes, FontSettings::default()).map_err(|e| e.to_string())?;
        let fam: SharedString = family.into();
        let resolved_family = if fam.is_empty() {
            fd.name().unwrap_or("sans-serif").to_string().into()
        } else {
            fam
        };
        Ok(Rc::new(Self {
            family: resolved_family,
            bytes: Some(bytes),
            fd,
        }))
    }

    pub fn from_bytes(bytes: &'static [u8]) -> Result<Rc<Self>, String> {
        let fd = FdFont::from_bytes(bytes, FontSettings::default()).map_err(|e| e.to_string())?;
        let family: SharedString = fd.name().unwrap_or("sans-serif").to_string().into();
        Ok(Rc::new(Self {
            family,
            bytes: Some(bytes),
            fd,
        }))
    }

    pub fn os_or_fallback(
        family: impl Into<SharedString>,
        fallback_bytes: &'static [u8],
    ) -> Rc<Self> {
        let family_str: SharedString = family.into();
        if let Some(fd) = load_os_fontdue(&family_str) {
            return Rc::new(Self {
                family: family_str,
                bytes: None,
                fd,
            });
        }
        Self::from_bytes(fallback_bytes).expect("valid fallback font bytes")
    }

    pub fn register(&self, cx: &mut App) -> gpui_kit::Result<()> {
        if let Some(bytes) = self.bytes {
            cx.text_system().add_fonts(vec![Cow::Borrowed(bytes)])?;
        }
        Ok(())
    }
}

fn load_os_fontdue(family: &str) -> Option<FdFont> {
    let target = family.to_ascii_lowercase().replace(' ', "");
    let dirs: &[&str] = if cfg!(target_os = "windows") {
        &["C:\\Windows\\Fonts"]
    } else if cfg!(target_os = "macos") {
        &["/System/Library/Fonts", "/Library/Fonts"]
    } else {
        &["/usr/share/fonts", "/usr/local/share/fonts"]
    };

    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(ext.as_str(), "ttf" | "otf") {
                continue;
            }
            let stem_clean = stem.to_ascii_lowercase().replace([' ', '-', '_'], "");
            if stem_clean == target || stem_clean.starts_with(&target) {
                if let Ok(bytes) = std::fs::read(&path) {
                    if let Ok(fd) = FdFont::from_bytes(bytes, FontSettings::default()) {
                        return Some(fd);
                    }
                }
            }
        }
    }
    None
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextTransform {
    #[default]
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug)]
pub struct TextStyle {
    pub font_size: f32,
    pub line_height: f32,
    pub color: Hsla,
    pub letter_spacing: f32,
    pub transform: TextTransform,
    pub align: TextAlign,
    pub max_dpr: f32,
}
impl TextStyle {
    pub fn new(font_size: f32, color: Hsla) -> Self {
        Self {
            font_size,
            line_height: font_size * 1.2,
            color,
            letter_spacing: 0.0,
            transform: TextTransform::None,
            align: TextAlign::Left,
            max_dpr: 3.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Span {
    pub text: SharedString,
    pub color: Option<Hsla>,
}
impl Span {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            color: None,
        }
    }
    pub fn color(mut self, c: Hsla) -> Self {
        self.color = Some(c);
        self
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct R {
    l: f32,
    t: f32,
    r: f32,
    b: f32,
}
impl R {
    fn union(self, o: R) -> R {
        R {
            l: self.l.min(o.l),
            t: self.t.min(o.t),
            r: self.r.max(o.r),
            b: self.b.max(o.b),
        }
    }
    fn hits(&self, o: &R) -> bool {
        self.l < o.r && o.l < self.r && self.t < o.b && o.t < self.b
    }
}
fn union_opt(a: Option<R>, b: R) -> Option<R> {
    Some(a.map_or(b, |a| a.union(b)))
}

struct GlyphRec {
    id: GlyphId,
    font_id: FontId,
    x: f32,
    base: f32,
    color: Hsla,
    ink: Option<R>,
    line: usize,
}

struct Unit {
    glyphs: std::ops::Range<usize>,
    ink: R,
    line: usize,
    height: f32,
}
struct Line {
    ink: R,
}

struct Atlas {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    sat: Vec<u32>,
}
impl Atlas {
    fn sum(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> u32 {
        let (x0, x1) = (
            (x0 - self.x).clamp(0, self.w),
            (x1 - self.x).clamp(0, self.w),
        );
        let (y0, y1) = (
            (y0 - self.y).clamp(0, self.h),
            (y1 - self.y).clamp(0, self.h),
        );
        let s = (self.w + 1) as usize;
        let at = |x: i32, y: i32| self.sat[y as usize * s + x as usize];
        (at(x1, y1) + at(x0, y0))
            .wrapping_sub(at(x1, y0))
            .wrapping_sub(at(x0, y1))
    }
}

#[derive(Clone, Copy)]
struct Grid {
    size: i32,
    m: i32,
    k: i32,
    levels: u32,
}

/// Coarse block = m * 2^k texels (m is 1 or 3) so every block maps onto an exact mip level.
fn choose_grid(pixel: f32, dpr: f32, max_levels: u32) -> Grid {
    let target = (pixel * dpr).max(2.0);
    let mut best: Option<(i32, i32, i32, f32)> = None;
    for k in 0..=10 {
        for m in [1, 3] {
            let size = m * (1 << k);
            if size < 2 {
                continue;
            }
            let err = (size as f32 / target).log2().abs();
            if best.map_or(true, |b| err < b.3 - 1e-9) {
                best = Some((size, m, k, err));
            }
        }
    }
    let (size, m, k, _) = best.unwrap();
    let available = if m == 3 { k + 1 } else { k } as u32;
    Grid {
        size,
        m,
        k,
        levels: max_levels.min(available),
    }
}

struct GlyphBitmap {
    xmin: i32,
    ymin: i32,
    w: i32,
    h: i32,
    data: Vec<u8>,
}
type GlyphCache = HashMap<(u16, u32), Rc<GlyphBitmap>>;

fn bitmap(cache: &mut GlyphCache, font: &FdFont, id: u16, px: f32) -> Rc<GlyphBitmap> {
    cache
        .entry((id, px.to_bits()))
        .or_insert_with(|| {
            let (m, data) = font.rasterize_indexed(id, px);
            Rc::new(GlyphBitmap {
                xmin: m.xmin,
                ymin: m.ymin,
                w: m.width as i32,
                h: m.height as i32,
                data,
            })
        })
        .clone()
}

struct Layout {
    width: f32,
    height: f32,
    glyphs: Vec<GlyphRec>,
    units: Vec<Unit>,
    lines: Vec<Line>,
    ink: R,
    atlases: Vec<Atlas>,
    line_span: Vec<(f32, f32)>,
    grid: Grid,
    dpr: f32,
    font_size: f32,
    plain: bool,
}

struct BuildInput<'a> {
    window: &'a Window,
    font: &'a RevealFont,
    style: &'a TextStyle,
    spans: &'a [Span],
    width: f32,
    split: Split,
    pixel: f32,
    levels: u32,
    dpr: f32,
    cache: &'a RefCell<GlyphCache>,
}

const MAX_TEXELS: i64 = 16 * 1024 * 1024;

fn transform_word(w: &str, t: TextTransform) -> String {
    match t {
        TextTransform::None => w.to_string(),
        TextTransform::Uppercase => w.to_uppercase(),
        TextTransform::Lowercase => w.to_lowercase(),
        TextTransform::Capitalize => {
            let mut c = w.chars();
            c.next().map_or(String::new(), |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        }
    }
}

struct Token {
    color: Hsla,
    joined: bool,
    width: f32,
    glyphs: Vec<(GlyphId, FontId, f32)>,
}

fn build_layout(inp: &BuildInput) -> Option<Layout> {
    let ts = inp.window.text_system();
    let st = inp.style;
    let family = inp.font.family.clone();
    let shape = |text: &str, color: Hsla| {
        let run = TextRun {
            len: text.len(),
            font: font(family.clone()),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        ts.shape_line(text.to_string().into(), px(st.font_size), &[run], None)
    };

    let mut tokens: Vec<Token> = Vec::new();
    let mut prev_space = true;
    for span in inp.spans {
        let color = span.color.unwrap_or(st.color);
        let data: &str = &span.text;
        let mut word_start: Option<usize> = None;
        let mut flush = |tokens: &mut Vec<Token>, from: usize, to: usize, prev_space: bool| {
            let word = transform_word(&data[from..to], st.transform);
            let shaped = shape(&word, color);
            let mut glyphs = Vec::new();
            for run in &shaped.runs {
                for (gi, g) in run.glyphs.iter().enumerate() {
                    glyphs.push((
                        g.id,
                        run.font_id,
                        f32::from(g.position.x) + st.letter_spacing * gi as f32,
                    ));
                }
            }
            let width = f32::from(shaped.width) + st.letter_spacing * glyphs.len() as f32;
            tokens.push(Token {
                color,
                joined: from == 0 && !prev_space,
                width,
                glyphs,
            });
        };
        for (i, ch) in data.char_indices() {
            match (ch.is_whitespace(), word_start) {
                (false, None) => word_start = Some(i),
                (true, Some(s)) => {
                    flush(&mut tokens, s, i, prev_space);
                    prev_space = true;
                    word_start = None;
                }
                _ => {}
            }
            if !ch.is_whitespace() {
                prev_space = false;
            }
        }
        if let Some(s) = word_start {
            flush(&mut tokens, s, data.len(), prev_space);
        }
        prev_space = data
            .chars()
            .last()
            .map_or(prev_space, |c| c.is_whitespace());
    }
    if tokens.is_empty() {
        return None;
    }

    let space = shape(" ", st.color);
    let (space_w, ascent, descent) = (
        f32::from(space.width) + st.letter_spacing,
        f32::from(space.ascent),
        f32::from(space.descent).abs(),
    );
    let content_h = ascent + descent;

    let mut groups: Vec<std::ops::Range<usize>> = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        match groups.last_mut() {
            Some(g) if t.joined => g.end = i + 1,
            _ => groups.push(i..i + 1),
        }
    }
    let wrap = if inp.width > 0.0 {
        inp.width
    } else {
        f32::INFINITY
    };
    let mut lines_tok: Vec<Vec<(usize, f32)>> = vec![Vec::new()];
    let mut line_w: Vec<f32> = vec![0.0];
    let mut x = 0.0f32;
    for g in &groups {
        let gw: f32 = tokens[g.clone()].iter().map(|t| t.width).sum();
        let empty = lines_tok.last().unwrap().is_empty();
        if !empty && x + space_w + gw > wrap {
            lines_tok.push(Vec::new());
            line_w.push(0.0);
            x = 0.0;
        }
        let mut cx = if lines_tok.last().unwrap().is_empty() {
            0.0
        } else {
            x + space_w
        };
        for ti in g.clone() {
            lines_tok.last_mut().unwrap().push((ti, cx));
            cx += tokens[ti].width;
        }
        x = cx;
        *line_w.last_mut().unwrap() = x;
    }

    let px_size = st.font_size * inp.dpr;
    let mut glyphs: Vec<GlyphRec> = Vec::new();
    let mut tok_range: Vec<std::ops::Range<usize>> = vec![0..0; tokens.len()];
    let mut lines: Vec<Line> = Vec::new();
    let mut cache = inp.cache.borrow_mut();
    for (li, items) in lines_tok.iter().enumerate() {
        let off = match st.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => ((inp.width.max(line_w[li])) - line_w[li]) / 2.0,
            TextAlign::Right => inp.width.max(line_w[li]) - line_w[li],
        };
        let base = li as f32 * st.line_height + (st.line_height - content_h) / 2.0 + ascent;
        let mut line_ink: Option<R> = None;
        for &(ti, tx) in items {
            let start = glyphs.len();
            for &(id, font_id, gx) in &tokens[ti].glyphs {
                let x = off + tx + gx;
                let bm = bitmap(&mut cache, &inp.font.fd, id.0 as u16, px_size);
                let ink = (bm.w > 0 && bm.h > 0).then(|| R {
                    l: x + bm.xmin as f32 / inp.dpr,
                    r: x + (bm.xmin + bm.w) as f32 / inp.dpr,
                    t: base - (bm.ymin + bm.h) as f32 / inp.dpr,
                    b: base - bm.ymin as f32 / inp.dpr,
                });
                if let Some(r) = ink {
                    line_ink = union_opt(line_ink, r);
                }
                glyphs.push(GlyphRec {
                    id,
                    font_id,
                    x,
                    base,
                    color: tokens[ti].color,
                    ink,
                    line: li,
                });
            }
            tok_range[ti] = start..glyphs.len();
        }
        let top = li as f32 * st.line_height;
        lines.push(Line {
            ink: line_ink.unwrap_or(R {
                l: 0.0,
                t: top,
                r: 1.0,
                b: top + st.line_height,
            }),
        });
    }
    drop(cache);

    let ranges: Vec<std::ops::Range<usize>> = match inp.split {
        Split::None => vec![0..glyphs.len()],
        Split::Chars => (0..glyphs.len()).map(|i| i..i + 1).collect(),
        Split::Lines => {
            let mut v: Vec<std::ops::Range<usize>> = Vec::new();
            for (i, g) in glyphs.iter().enumerate() {
                match v.last_mut() {
                    Some(r) if glyphs[r.start].line == g.line => r.end = i + 1,
                    _ => v.push(i..i + 1),
                }
            }
            v
        }
        Split::Words => groups
            .iter()
            .map(|g| tok_range[g.start].start..tok_range[g.end - 1].end)
            .collect(),
    };
    let mut units = Vec::new();
    for r in ranges {
        let ink = glyphs[r.clone()]
            .iter()
            .filter_map(|g| g.ink)
            .fold(None, union_opt);
        if let (Some(ink), false) = (ink, r.is_empty()) {
            units.push(Unit {
                line: glyphs[r.start].line,
                glyphs: r,
                ink,
                height: content_h,
            });
        }
    }
    if units.is_empty() {
        return None;
    }
    let whole = lines.iter().map(|l| l.ink).fold(None, union_opt).unwrap();

    let mut layout = Layout {
        width: inp.width,
        height: lines.len() as f32 * st.line_height,
        glyphs,
        units,
        lines,
        ink: whole,
        atlases: Vec::new(),
        line_span: Vec::new(),
        grid: choose_grid(inp.pixel, inp.dpr, inp.levels),
        dpr: inp.dpr,
        font_size: st.font_size,
        plain: false,
    };
    build_atlases(&mut layout, inp);
    Some(layout)
}

fn build_atlases(layout: &mut Layout, inp: &BuildInput) {
    let mut dpr = inp.dpr;
    for _ in 0..5 {
        let grid = choose_grid(inp.pixel, dpr, inp.levels);
        let g = grid.size;
        let mut total = 0i64;
        let cells: Vec<(i32, i32, i32, i32)> = layout
            .units
            .iter()
            .map(|u| {
                let x = (((u.ink.l * dpr) - 3.0) / g as f32).floor() as i32 * g;
                let y = (((u.ink.t * dpr) - 3.0) / g as f32).floor() as i32 * g;
                let w = ((((u.ink.r * dpr) + 3.0) / g as f32).ceil() as i32) * g - x;
                let h = ((((u.ink.b * dpr) + 3.0) / g as f32).ceil() as i32) * g - y;
                total += w as i64 * h as i64;
                (x, y, w, h)
            })
            .collect();
        if total > MAX_TEXELS || cells.iter().any(|c| c.2 > 8192 || c.3 > 8192) {
            dpr *= 0.7;
            continue;
        }

        let px_size = layout.font_size * dpr;
        let mut cache = inp.cache.borrow_mut();
        let mut atlases = Vec::with_capacity(cells.len());
        for (u, &(x, y, w, h)) in layout.units.iter().zip(&cells) {
            let mut alpha = vec![0u8; (w * h) as usize];
            for rec in &layout.glyphs[u.glyphs.clone()] {
                let bm = bitmap(&mut cache, &inp.font.fd, rec.id.0 as u16, px_size);
                let left = (rec.x * dpr).round() as i32 + bm.xmin - x;
                let top = (rec.base * dpr).round() as i32 - bm.ymin - bm.h - y;
                for row in 0..bm.h {
                    let ty = top + row;
                    if ty < 0 || ty >= h {
                        continue;
                    }
                    for col in 0..bm.w {
                        let tx = left + col;
                        if tx < 0 || tx >= w {
                            continue;
                        }
                        let dst = &mut alpha[(ty * w + tx) as usize];
                        *dst = (*dst).max(bm.data[(row * bm.w + col) as usize]);
                    }
                }
            }

            let s = (w + 1) as usize;
            let mut sat = vec![0u32; s * (h + 1) as usize];
            for yy in 0..h as usize {
                let mut run = 0u32;
                for xx in 0..w as usize {
                    run += alpha[yy * w as usize + xx] as u32;
                    sat[(yy + 1) * s + xx + 1] = sat[yy * s + xx + 1] + run;
                }
            }
            atlases.push(Atlas { x, y, w, h, sat });
        }
        layout.line_span = {
            let mut spans = vec![(f32::INFINITY, f32::NEG_INFINITY); layout.lines.len()];
            for (u, a) in layout.units.iter().zip(&atlases) {
                let s = &mut spans[u.line];
                s.0 = s.0.min(a.y as f32 / dpr);
                s.1 = s.1.max((a.y + a.h) as f32 / dpr);
            }
            spans
        };
        layout.atlases = atlases;
        layout.grid = grid;
        layout.dpr = dpr;
        return;
    }
    layout.plain = true;
}

#[derive(Clone, Copy)]
struct Uniforms {
    whole: bool,
    progress: f32,
    mask: bool,
    direction: usize,
    pattern: usize,
    noise: f32,
    scatter: f32,
    spread: f32,
    solid: f32,
    accent: [f32; 3],
    accent2: [f32; 3],
    accent_mix: f32,
    accent_strength: f32,
    accent_width: f32,
    color_noise: f32,
    sparkle: f32,
    flicker: f32,
    glitch: f32,
    seed: u32,
    time: f32,
    lens: [f32; 4],
}

struct Shader<'a> {
    u: &'a Uniforms,
    l: &'a Layout,
    unit: &'a Unit,
    atlas: &'a Atlas,
    ox: f32,
    oy: f32,
    screen_dpr: f32,
    crisp: Vec<(i32, i32, i32, i32)>,
}

impl Shader<'_> {
    fn h(&self, x: i32, y: i32, salt: u32) -> f32 {
        hash(x, y, salt, self.u.seed)
    }

    fn vnoise(&self, x: f32, y: f32, salt: u32) -> f32 {
        let (ix, iy) = (x.floor(), y.floor());
        let (fx, fy) = (fract(x), fract(y));
        let (fx, fy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
        let (ix, iy) = (ix as i32, iy as i32);
        let a = self.h(ix, iy, salt);
        let b = self.h(ix + 1, iy, salt);
        let c = self.h(ix, iy + 1, salt);
        let d = self.h(ix + 1, iy + 1, salt);
        mixf(mixf(a, b, fx), mixf(c, d, fx), fy)
    }

    fn ramp(&self, nx: f32, ny: f32, rw: f32, rh: f32) -> f32 {
        match self.u.direction {
            0 => 1.0 - ny,
            1 => ny,
            2 => 1.0 - nx,
            3 => nx,
            4 | 5 => {
                let radial =
                    (((nx - 0.5) * rw).hypot((ny - 0.5) * rh)) / (0.5 * rw.hypot(rh)).max(1.0);
                if self.u.direction == 4 {
                    radial
                } else {
                    1.0 - radial
                }
            }
            6 => (nx + 1.0 - ny) * 0.5,
            _ => 0.5,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn pattern(&self, cx: i32, cy: i32, cell_px_x: f32, nx: f32, ry: f32, rh: f32) -> f32 {
        let (fx, fy) = (cx as f32, cy as f32);
        match self.u.pattern {
            0 => self.h(cx, cy, 1),
            1 => {
                let v = self.vnoise(fx * 0.34, fy * 0.34, 2) * 0.65
                    + self.vnoise(fx * 0.83 + 7.0, fy * 0.83 + 7.0, 3) * 0.35;
                smoothstep(0.2, 0.8, v)
            }
            2 => 0.84 * self.h(0, cy, 4) + 0.16 * self.h(cx, cy, 5),
            3 => 0.84 * self.h(cx, 0, 6) + 0.16 * self.h(cx, cy, 7),
            4 => {
                if self.u.whole {
                    let line = &self.l.lines[self.unit.line];
                    let x = ((cell_px_x - line.ink.l) / (line.ink.r - line.ink.l).max(1.0))
                        .clamp(0.0, 1.0);
                    (self.unit.line as f32 + x) / (self.l.lines.len() as f32).max(1.0)
                } else {
                    nx
                }
            }
            5 => {
                let g = self.l.grid.size as f32;
                let top = (ry * self.l.dpr / g).floor();
                let rows = (((ry + rh) * self.l.dpr / g).ceil() - top).max(1.0);
                let row = (fy - top).clamp(0.0, rows - 1.0);
                let x = if row % 2.0 < 0.5 { nx } else { 1.0 - nx };
                (row + x) / rows
            }
            6 => fract((fx + fy) * 0.2),
            7 => bayer8(fx, fy),
            8 => {
                let (px_, py_) = (fx * 0.16, fy * 0.16);
                let (wx, wy) = (
                    self.vnoise(px_ + 3.7, py_ + 3.7, 8),
                    self.vnoise(px_ + 11.3, py_ + 11.3, 9),
                );
                let v = self.vnoise(px_ * 1.3 + wx * 2.4, py_ * 1.3 + wy * 2.4, 10) * 0.7
                    + self.vnoise(px_ * 2.7 + wx, py_ * 2.7 + wy, 11) * 0.3;
                smoothstep(0.22, 0.78, v)
            }
            _ => 0.5,
        }
    }

    fn field(&self, cx: i32, cy: i32, ppx: f32, ppy: f32) -> f32 {
        let rect = if self.u.whole {
            self.l.ink
        } else {
            self.unit.ink
        };
        let (rw, rh) = (rect.r - rect.l, rect.b - rect.t);
        let nx = ((ppx - rect.l) / rw.max(1.0)).clamp(0.0, 1.0);
        let ny = ((ppy - rect.t) / rh.max(1.0)).clamp(0.0, 1.0);
        let mut f = if self.u.direction == 7 {
            self.pattern(cx, cy, ppx, nx, rect.t, rh)
        } else if self.u.pattern == 9 {
            self.ramp(nx, ny, rw, rh)
        } else {
            mixf(
                self.ramp(nx, ny, rw, rh),
                self.pattern(cx, cy, ppx, nx, rect.t, rh),
                self.u.noise,
            )
        };
        f += (self.h(cx, cy, 12) - 0.5) * self.u.scatter;
        f.clamp(0.0, 1.0)
    }

    fn lens_cap(&self, x: f32, y: f32) -> f32 {
        let [lx, ly, z, r] = self.u.lens;
        if z <= 0.0 {
            return 1.0;
        }
        let d = (x - lx).hypot(y - ly);
        1.0 - z * 0.86 * (1.0 - smoothstep(r * 0.15, r, d))
    }

    fn snap(&self, v: f32) -> f32 {
        (v * self.screen_dpr).round() / self.screen_dpr
    }

    fn quad(&self, window: &mut Window, bx: i32, by: i32, b: i32, color: Hsla) {
        let d = self.l.dpr;
        let (x0, y0) = (
            self.ox + bx as f32 * b as f32 / d,
            self.oy + by as f32 * b as f32 / d,
        );
        let (x1, y1) = (x0 + b as f32 / d, y0 + b as f32 / d);
        let (x0, y0, x1, y1) = (self.snap(x0), self.snap(y0), self.snap(x1), self.snap(y1));
        window.paint_quad(fill(
            Bounds::new(
                point(px(x0), px(y0)),
                size(px((x1 - x0).max(0.0)), px((y1 - y0).max(0.0))),
            ),
            color,
        ));
    }

    fn cell(&mut self, window: &mut Window, cx: i32, cy: i32, progress: f32) {
        let g = self.l.grid.size;
        let d = self.l.dpr;
        let (ppx, ppy) = (
            (cx as f32 + 0.5) * g as f32 / d,
            (cy as f32 + 0.5) * g as f32 / d,
        );
        let f = self.field(cx, cy, ppx, ppy);
        let q = ((progress - f * (1.0 - self.u.spread)) / self.u.spread).clamp(0.0, 1.0);
        if q <= 0.0 {
            return;
        }
        self.visit(window, cx, cy, 0, q);
    }

    fn visit(&mut self, window: &mut Window, bx: i32, by: i32, level: u32, q: f32) {
        let levels = self.l.grid.levels;
        let b = self.l.grid.size >> level;
        let d = self.l.dpr;
        let life = q.min(self.lens_cap(
            (bx as f32 + 0.5) * b as f32 / d,
            (by as f32 + 0.5) * b as f32 / d,
        ));

        if level >= levels {
            self.crisp
                .push((bx * b, by * b, (bx + 1) * b, (by + 1) * b)); // levels == 0
            return;
        }
        let k = level + 1;
        let jitter = self.h(bx, by, 20 + k) - 0.5;
        let threshold = mixf(0.12, 0.84, (k as f32 - 0.5 + jitter * 0.9) / levels as f32);
        if life >= threshold {
            if k >= levels {
                self.crisp
                    .push((bx * b, by * b, (bx + 1) * b, (by + 1) * b));
            } else {
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    self.visit_child(window, bx * 2 + dx, by * 2 + dy, k, q);
                }
            }
            return;
        }
        self.mosaic(window, bx, by, level, life);
    }

    fn visit_child(&mut self, window: &mut Window, bx: i32, by: i32, level: u32, q: f32) {
        self.visit(window, bx, by, level, q);
    }

    fn mosaic(&self, window: &mut Window, bx: i32, by: i32, level: u32, life: f32) {
        let u = self.u;
        let b = self.l.grid.size >> level;
        let (mut sx, sy) = (bx, by);

        if u.glitch > 0.0 && life < 0.55 {
            let tick = (u.time * 14.0).floor() as i32;
            if self.h(by, tick, 30 + level) < u.glitch * (1.0 - life / 0.55) {
                let shift = ((self.h(by, tick, 31) - 0.5) * 7.0).floor() as i32;
                let first = self.atlas.x / b;
                sx = (bx + shift).clamp(first, first + self.atlas.w / b - 1);
            }
        }

        let sum = self.atlas.sum(sx * b, sy * b, (sx + 1) * b, (sy + 1) * b);
        let cover = sum as f32 / (255.0 * (b * b) as f32);

        let mut ink = rgb3(self.l.glyphs[self.unit.glyphs.start].color);
        let accent = if u.accent_mix >= self.h(sx, sy, 40 + level) {
            u.accent2
        } else {
            u.accent
        };
        let heat = u.accent_strength
            * (1.0
                - smoothstep(
                    0.0,
                    u.accent_width,
                    life + (self.h(sx, sy, 41 + level) - 0.5) * 0.18,
                ));

        let fine = level as f32 / (self.l.grid.levels as f32 - 1.0).max(1.0);
        let mut alpha = mixf(
            cover,
            smoothstep(mixf(0.04, 0.16, fine), mixf(0.3, 0.5, fine), cover),
            u.solid,
        );
        if cover >= 0.06 {
            alpha = alpha.max(heat);
        }
        let mut ink_accent = false;
        if cover < 0.004 && u.sparkle > 0.0 {
            let spark = 1.0 - life / 0.32;
            if spark > 0.0 && self.h(sx, sy, 42 + level) < u.sparkle {
                alpha = spark * 0.85;
                ink = accent;
                ink_accent = true;
            }
        }
        if u.flicker > 0.0 && life < 0.42 {
            let tick = (u.time * 20.0 + self.h(sx, sy, 43) * 7.0).floor() as i32;
            if self.h(
                sx.wrapping_add(tick.wrapping_mul(13)),
                sy.wrapping_add(tick.wrapping_mul(13)),
                44,
            ) < u.flicker * (1.0 - life / 0.42)
            {
                alpha *= 0.15;
            }
        }
        let jit = [
            self.h(sx, sy, 45) - 0.5,
            self.h(sx, sy, 46) - 0.5,
            self.h(sx, sy, 47) - 0.5,
        ];
        for i in 0..3 {
            ink[i] = (ink[i] + jit[i] * u.color_noise * 2.0 * (1.0 - life)).clamp(0.0, 1.0);
        }
        let _ = ink_accent;

        if alpha <= 0.002 {
            return;
        }
        let c = mix3(ink, accent, heat);
        let color: Hsla = Rgba {
            r: c[0],
            g: c[1],
            b: c[2],
            a: alpha.clamp(0.0, 1.0),
        }
        .into();
        self.quad(window, bx, by, b, color);
    }
}

struct PaintParams {
    layout: Rc<Layout>,
    anim: Vec<[f32; 2]>,
    uniforms: Uniforms,
    state: State,
    clip_below: Option<f32>,
    rise: bool,
    lens_active: bool,
}

fn paint_glyphs(
    window: &mut Window,
    l: &Layout,
    range: std::ops::Range<usize>,
    ox: f32,
    oy: f32,
    clip: Option<R>,
) {
    for g in &l.glyphs[range] {
        if let (Some(c), Some(ink)) = (clip, g.ink) {
            let inflated = R {
                l: ink.l - 1.0,
                t: ink.t - 1.0,
                r: ink.r + 1.0,
                b: ink.b + 1.0,
            };
            if !inflated.hits(&c) {
                continue;
            }
        } else if clip.is_some() && g.ink.is_none() {
            continue;
        }
        window
            .paint_glyph(
                point(px(ox + g.x), px(oy + g.base)),
                g.font_id,
                g.id,
                px(l.font_size),
                g.color,
            )
            .ok();
    }
}

fn paint_reveal(window: &mut Window, bounds: Bounds<Pixels>, p: &PaintParams) {
    let l = &*p.layout;
    let scale = window.scale_factor();
    let ox = (f32::from(bounds.origin.x) * scale).round() / scale;
    let oy = (f32::from(bounds.origin.y) * scale).round() / scale;

    let vp = window.viewport_size();
    let margin = 200.0;
    if oy > f32::from(vp.height) + margin || oy + l.height < -margin {
        return;
    }
    let clip_h = p
        .clip_below
        .map_or(f32::from(vp.height), |c| c.min(f32::from(vp.height)));
    let mask = ContentMask {
        bounds: Bounds::new(point(px(0.0), px(0.0)), size(vp.width, px(clip_h.max(0.0)))),
    };

    window.with_content_mask(Some(mask), |window| {
        if matches!(p.state, State::Done | State::Static) || l.plain {
            paint_glyphs(window, l, 0..l.glyphs.len(), ox, oy, None);
            return;
        }
        for (i, unit) in l.units.iter().enumerate() {
            let [reveal, rise_y] = p.anim[i];
            let u = &p.uniforms;
            let progress = if u.whole { u.progress } else { reveal };
            if progress <= 0.0 {
                continue;
            }
            let unit_oy = oy + rise_y;

            let line_mask = |window: &mut Window, f: &mut dyn FnMut(&mut Window)| {
                if p.rise {
                    let (t, b) = l.line_span[unit.line];
                    let m = ContentMask {
                        bounds: Bounds::new(
                            point(px(ox - 4.0), px(oy + t)),
                            size(px(l.width + 8.0), px((b - t).max(0.0))),
                        ),
                    };
                    window.with_content_mask(Some(m), |w| f(w));
                } else {
                    f(window);
                }
            };

            let near_lens = p.lens_active && {
                let [lx, ly, _, r] = u.lens;
                lx > unit.ink.l - r
                    && lx < unit.ink.r + r
                    && ly > unit.ink.t - r
                    && ly < unit.ink.b + r
            };
            if progress >= 1.0 && !near_lens {
                line_mask(window, &mut |w| {
                    paint_glyphs(w, l, unit.glyphs.clone(), ox, unit_oy, None)
                });
                continue;
            }

            let atlas = &l.atlases[i];
            let mut sh = Shader {
                u,
                l,
                unit,
                atlas,
                ox,
                oy: unit_oy,
                screen_dpr: scale,
                crisp: Vec::new(),
            };
            let g = l.grid.size;
            line_mask(window, &mut |w| {
                for cy in atlas.y / g..(atlas.y + atlas.h) / g {
                    for cx in atlas.x / g..(atlas.x + atlas.w) / g {
                        sh.cell(w, cx, cy, progress);
                    }
                }
                sh.crisp.sort_by_key(|c| (c.1, c.0));
                let mut merged: Vec<(i32, i32, i32, i32)> = Vec::new();
                for c in sh.crisp.drain(..) {
                    match merged.last_mut() {
                        Some(m) if m.1 == c.1 && m.3 == c.3 && m.2 == c.0 => m.2 = c.2,
                        _ => merged.push(c),
                    }
                }
                let d = l.dpr;
                for (x0, y0, x1, y1) in merged {
                    if atlas.sum(x0, y0, x1, y1) == 0 {
                        continue;
                    }
                    let r = R {
                        l: x0 as f32 / d,
                        t: y0 as f32 / d,
                        r: x1 as f32 / d,
                        b: y1 as f32 / d,
                    };
                    let m = ContentMask {
                        bounds: Bounds::new(
                            point(px(ox + r.l), px(unit_oy + r.t)),
                            size(px(r.r - r.l), px(r.b - r.t)),
                        ),
                    };
                    w.with_content_mask(Some(m), |w| {
                        paint_glyphs(w, l, unit.glyphs.clone(), ox, unit_oy, Some(r))
                    });
                }
            });
        }
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Not started: nothing painted.
    Hidden,
    /// Animating (the effect is drawing).
    Active,
    /// Finished: plain text.
    Done,
    /// Effect unavailable (reduced motion, empty text, atlas budget): plain text.
    Static,
}

#[derive(Clone, Debug)]
pub enum PixelRevealEvent {
    Start,
    Complete,
    StateChanged(State),
}

pub struct PixelReveal {
    pub(crate) spans: Vec<Span>,
    pub(crate) style: TextStyle,
    pub(crate) font: Rc<RevealFont>,
    pub(crate) options: Options,
    pub(crate) o: Resolved,
    pub reduce_motion: bool,
    pub can_hover: bool,

    pub(crate) layout: Option<Rc<Layout>>,
    pub(crate) stale: bool,
    pub(crate) built_width: f32,
    pub(crate) built_dpr: f32,
    pub(crate) cache: RefCell<GlyphCache>,

    pub(crate) time: f32,
    pub(crate) total: f32,
    pub(crate) dir: i8,
    pub(crate) wait: f32,
    pub(crate) fired: bool,
    pub(crate) pending_seek: Option<f32>,
    pub(crate) order: Vec<f32>,
    pub(crate) rise_dist: Vec<f32>,
    pub(crate) ease_reveal: Ease,
    pub(crate) ease_rise: Ease,
    pub(crate) anim: Vec<[f32; 2]>,
    pub(crate) whole_progress: f32,
    pub(crate) time_dirty: bool,
    pub(crate) revealed: bool,
    pub(crate) warned_pin: bool,
    pub(crate) pin_progress: Option<f32>,
    pub(crate) clip_below: Option<f32>,
    pub(crate) events: Vec<PixelRevealEvent>,

    pub(crate) inside: bool,
    pub(crate) lens: f32,
    pub(crate) lens_x: f32,
    pub(crate) lens_y: f32,
    pub(crate) pointer: Option<Point<Pixels>>,

    pub(crate) state: State,
    pub(crate) clock: f32,
    pub(crate) animating: bool,
    pub(crate) last_frame: Option<Instant>,
    pub(crate) bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl EventEmitter<PixelRevealEvent> for PixelReveal {}

impl PixelReveal {
    pub fn new(
        text: impl Into<SharedString>,
        font: Rc<RevealFont>,
        style: TextStyle,
        options: Options,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self::with_spans(vec![Span::new(text)], font, style, options)
    }

    pub fn with_spans(
        spans: Vec<Span>,
        font: Rc<RevealFont>,
        style: TextStyle,
        options: Options,
    ) -> Self {
        let o = resolve(&options);
        Self {
            spans,
            style,
            font,
            options,
            reduce_motion: false,
            can_hover: true,
            layout: None,
            stale: true,
            built_width: 0.0,
            built_dpr: 0.0,
            cache: RefCell::default(),
            time: 0.0,
            total: 1.0,
            dir: 0,
            wait: 0.0,
            fired: false,
            pending_seek: None,
            order: Vec::new(),
            rise_dist: Vec::new(),
            ease_reveal: Ease::parse(&o.ease),
            ease_rise: Ease::parse(&o.rise_ease),
            anim: Vec::new(),
            whole_progress: 0.0,
            time_dirty: true,
            revealed: false,
            warned_pin: false,
            pin_progress: None,
            clip_below: None,
            events: Vec::new(),
            inside: false,
            lens: 0.0,
            lens_x: 0.0,
            lens_y: 0.0,
            pointer: None,
            state: State::Hidden,
            clock: 0.0,
            animating: true,
            last_frame: None,
            bounds: Rc::default(),
            o,
        }
    }

    pub fn state(&self) -> State {
        self.state
    }
    pub fn progress(&self) -> f32 {
        if self.layout.is_some() {
            self.time / self.total
        } else {
            0.0
        }
    }
    pub fn play(&mut self, cx: &mut Context<Self>) {
        self.wait = 0.0;
        self.dir = 1;
        self.wake(cx);
    }
    pub fn reverse(&mut self, cx: &mut Context<Self>) {
        self.wait = 0.0;
        self.dir = -1;
        self.wake(cx);
    }
    pub fn restart(&mut self, cx: &mut Context<Self>) {
        self.time = 0.0;
        self.time_dirty = true;
        self.play(cx);
    }
    pub fn replay(&mut self, cx: &mut Context<Self>) {
        self.restart(cx);
    }
    pub fn pause(&mut self, cx: &mut Context<Self>) {
        self.dir = 0;
        self.wait = 0.0;
        cx.notify();
    }
    pub fn seek(&mut self, p: f32, cx: &mut Context<Self>) {
        let p = p.clamp(0.0, 1.0);
        self.dir = 0;
        self.wait = 0.0;
        if self.layout.is_some() {
            self.time = p * self.total;
        } else {
            self.pending_seek = Some(p);
        }
        self.time_dirty = true;
        self.wake(cx);
    }
    pub fn set_progress(&mut self, p: f32, cx: &mut Context<Self>) {
        self.seek(p, cx);
    }
    pub fn set_pin_progress(&mut self, p: Option<f32>, cx: &mut Context<Self>) {
        self.pin_progress = p;
        cx.notify();
    }
    pub fn set_clip_below(&mut self, y: Option<f32>, cx: &mut Context<Self>) {
        self.clip_below = y;
        cx.notify();
    }
    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.spans = vec![Span::new(text)];
        self.stale = true;
        cx.notify();
    }
    pub fn set_spans(&mut self, spans: Vec<Span>, cx: &mut Context<Self>) {
        self.spans = spans;
        self.stale = true;
        cx.notify();
    }
    pub fn set_style(&mut self, style: TextStyle, cx: &mut Context<Self>) {
        self.style = style;
        self.stale = true;
        cx.notify();
    }

    pub fn set_options(&mut self, options: Options, cx: &mut Context<Self>) {
        let next = resolve(&options);
        if next == self.o {
            return;
        }
        let a = &self.o;
        let layout = a.split != next.split || a.pixel != next.pixel || a.levels != next.levels;
        let timeline = a.sweep != next.sweep
            || a.from != next.from
            || a.stagger != next.stagger
            || a.duration != next.duration
            || a.ease != next.ease
            || a.reveal_delay != next.reveal_delay
            || a.rise != next.rise
            || a.rise_duration != next.rise_duration
            || a.rise_ease != next.rise_ease
            || a.seed != next.seed;
        let trigger = a.trigger != next.trigger
            || a.start != next.start
            || a.end != next.end
            || a.once != next.once
            || a.delay != next.delay
            || a.smooth != next.smooth;

        let was_done = self.layout.is_some() && self.time >= self.total;
        self.options = options;
        self.o = next;
        if layout {
            self.stale = true;
        }
        if timeline && self.layout.is_some() {
            self.compute_timeline(was_done);
        }
        if trigger {
            self.fired = false;
            self.dir = 0;
            self.wait = 0.0;
            if !matches!(self.o.trigger, Trigger::Scrub | Trigger::Pin) {
                self.time = 0.0;
            }
            self.time_dirty = true;
        }
        self.wake(cx);
    }

    fn wake(&mut self, cx: &mut Context<Self>) {
        if !self.animating {
            self.animating = true;
            self.last_frame = None;
        }
        cx.notify();
    }

    fn rebuild(&mut self, width: f32, dpr: f32, window: &Window) {
        self.stale = false;
        self.built_width = width;
        self.built_dpr = dpr;
        let built = build_layout(&BuildInput {
            window,
            font: &self.font,
            style: &self.style,
            spans: &self.spans,
            width,
            split: self.o.split,
            pixel: self.o.pixel,
            levels: self.o.levels,
            dpr,
            cache: &self.cache,
        });
        let was_done = self.layout.is_some() && self.time >= self.total;
        self.layout = built.map(Rc::new);
        if self.layout.is_some() {
            self.compute_timeline(was_done);
            self.animating = true;
        }
    }

    fn compute_timeline(&mut self, was_done: bool) {
        let Some(layout) = &self.layout else { return };
        let o = &self.o;
        let n = layout.units.len();
        let mid = (n as f32 - 1.0) / 2.0;

        let mut order = vec![0.0f32; n];
        if o.from == From::Random {
            let seed = (1000.0 * o.seed).round() as i32;
            let mut idx: Vec<usize> = (0..n).collect();
            idx.sort_by(|&a, &b| {
                hash01(7919 * a as i32 + seed)
                    .partial_cmp(&hash01(7919 * b as i32 + seed))
                    .unwrap()
            });
            for (rank, unit) in idx.into_iter().enumerate() {
                order[unit] = rank as f32;
            }
        } else {
            for (i, v) in order.iter_mut().enumerate() {
                let i = i as f32;
                *v = match o.from {
                    From::End => n as f32 - 1.0 - i,
                    From::Center => (i - mid).abs(),
                    From::Edges => mid - (i - mid).abs(),
                    _ => i,
                };
            }
        }
        let max_order = order.iter().cloned().fold(0.0, f32::max);
        let stagger_span = max_order * o.stagger;

        self.rise_dist = layout.units.iter().map(|u| o.rise * u.height).collect();
        let reveal_end = (if o.sweep == Sweep::Whole {
            0.0
        } else {
            stagger_span
        }) + o.reveal_delay
            + o.duration;
        let rise_end = if o.rise != 0.0 {
            stagger_span + o.rise_duration
        } else {
            0.0
        };
        self.total = reveal_end.max(rise_end).max(0.001);
        self.order = order;
        self.ease_reveal = Ease::parse(&o.ease);
        self.ease_rise = Ease::parse(&o.rise_ease);
        self.anim = vec![[0.0, 0.0]; n];

        if let Some(p) = self.pending_seek.take() {
            self.time = p * self.total;
        } else {
            self.time = if was_done {
                self.total
            } else {
                self.time.min(self.total)
            };
        }
        self.time_dirty = true;
    }

    fn evaluate(&mut self) {
        let o = &self.o;
        let t = self.time;
        let mut any = false;
        for i in 0..self.anim.len() {
            let start = self.order[i] * o.stagger;
            let mut reveal = (t - start - o.reveal_delay) / o.duration;
            reveal = if reveal <= 0.0 {
                0.0
            } else if reveal >= 1.0 {
                1.0
            } else {
                self.ease_reveal.apply(reveal)
            };
            any |= reveal > 0.0;
            let mut rise = 1.0;
            if self.rise_dist[i] != 0.0 {
                rise = (t - start) / o.rise_duration;
                rise = if rise <= 0.0 {
                    0.0
                } else if rise >= 1.0 {
                    1.0
                } else {
                    self.ease_rise.apply(rise)
                };
            }
            self.anim[i] = [reveal, (1.0 - rise) * self.rise_dist[i]];
        }
        if o.sweep == Sweep::Whole {
            let mut p = (t - o.reveal_delay) / o.duration;
            p = if p <= 0.0 {
                0.0
            } else if p >= 1.0 {
                1.0
            } else {
                self.ease_reveal.apply(p)
            };
            self.whole_progress = p;
            any = p > 0.0;
        }
        self.revealed = any;
    }

    fn reached(&self, pos: Pos, top: f32, height: f32, vp_h: f32) -> bool {
        let (el, vp) = pos.resolve();
        top + el.f * height + el.px <= vp.f * vp_h + vp.px
    }

    /// scroll offset = s - S_a equals -(top + off_a - vpPos_a).
    fn scrub_progress(&mut self, top: f32, height: f32, vp_h: f32) -> f32 {
        if self.o.trigger == Trigger::Pin {
            if let Some(p) = self.pin_progress {
                let from = match self.o.start {
                    Some(Pos::Num(v)) => v,
                    _ => 0.0,
                };
                let to = match self.o.end {
                    Some(Pos::Num(v)) => v,
                    _ => 0.85,
                };
                return ((p - from) / (to - from).max(1e-4)).clamp(0.0, 1.0);
            }
            if !self.warned_pin {
                self.warned_pin = true;
                eprintln!(
                    "[pixel-reveal] trigger=pin needs set_pin_progress(); scrubbing through the viewport instead"
                );
            }
        }
        let scrub = self.o.trigger == Trigger::Scrub;
        let (start, end) = if scrub {
            (
                self.o.start.unwrap_or(Pos::parse("top 92%")),
                self.o.end.unwrap_or(Pos::parse("bottom 55%")),
            )
        } else {
            (Pos::parse("top 92%"), Pos::parse("bottom 55%"))
        };
        let off = |p: Pos| {
            let (el, vp) = p.resolve();
            (el.f * height + el.px, vp.f * vp_h + vp.px)
        };
        let ((ea, va), (eb, vb)) = (off(start), off(end));
        let span = (eb - vb) - (ea - va);
        let a_scrolled = -(top + ea - va);
        if span < 1.0 {
            f32::from(-(top + eb - vb) >= 0.0)
        } else {
            (a_scrolled / span).clamp(0.0, 1.0)
        }
    }

    fn drive(&mut self, dt: f32, top: f32, height: f32, vp_h: f32) {
        let (d_start, _) = trigger_defaults(self.o.trigger);
        match self.o.trigger {
            Trigger::Load => {
                if !self.fired {
                    self.fired = true;
                    self.wait = if self.time <= 0.0 { self.o.delay } else { 0.0 };
                    self.dir = 1;
                }
            }
            Trigger::Inview => {
                let pos = self.o.start.or(d_start).unwrap();
                let inside = self.reached(pos, top, height, vp_h);
                if inside && !self.fired {
                    self.fired = true;
                    self.wait = if self.time <= 0.0 { self.o.delay } else { 0.0 };
                    self.dir = 1;
                } else if !inside && self.fired && !self.o.once {
                    self.fired = false;
                    self.wait = 0.0;
                    self.dir = -1;
                }
            }
            Trigger::Scrub | Trigger::Pin => {
                let target = self.scrub_progress(top, height, vp_h) * self.total;
                self.dir = 0;
                self.time = if self.o.smooth > 0.0 {
                    damp(self.time, target, 3.0 / self.o.smooth, dt)
                } else {
                    target
                };
                if (self.time - target).abs() < 1e-4 {
                    self.time = target;
                }
                return;
            }
            Trigger::Manual => {}
        }
        if self.dir != 0 {
            self.advance(dt);
        }
    }

    fn advance(&mut self, mut dt: f32) {
        if self.wait > 0.0 {
            self.wait -= dt;
            if self.wait > 0.0 {
                return;
            }
            dt = -self.wait;
            self.wait = 0.0;
        }
        self.time +=
            dt * if self.dir > 0 {
                1.0
            } else {
                self.o.reverse_speed
            } * self.dir as f32;
        if self.time >= self.total {
            self.time = self.total;
            self.dir = 0;
        } else if self.time <= 0.0 {
            self.time = 0.0;
            self.dir = 0;
        }
    }

    fn uniforms(&self, layout: &Layout) -> Uniforms {
        let o = &self.o;
        let text = rgb3(self.style.color);
        let accent = o.accent.map_or(text, rgb3);
        Uniforms {
            whole: o.sweep == Sweep::Whole,
            progress: self.whole_progress,
            mask: o.rise != 0.0,
            direction: o.direction as usize,
            pattern: o.pattern as usize,
            noise: o.noise,
            scatter: o.scatter,
            spread: o.spread,
            solid: o.solid,
            accent,
            accent2: o.accent2.map_or(accent, rgb3),
            accent_mix: if o.accent2.is_some() {
                o.accent_mix
            } else {
                0.0
            },
            accent_strength: if o.accent.is_some() {
                o.accent_strength
            } else {
                0.0
            },
            accent_width: o.accent_width,
            color_noise: o.color_noise,
            sparkle: o.sparkle,
            flicker: o.flicker,
            glitch: o.glitch,
            seed: (65535.0 * hash01((1000.0 * o.seed).round() as i32 + 7)).floor() as u32,
            time: self.clock,
            lens: [self.lens_x, self.lens_y, self.lens, o.lens_radius],
        }
        .tap(|_| {
            let _ = layout;
        })
    }
}

trait Tap: Sized {
    fn tap(self, f: impl FnOnce(&Self)) -> Self {
        f(&self);
        self
    }
}
impl Tap for Uniforms {}

impl Render for PixelReveal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let b = self.bounds.get();
        let (width, left, top) = (
            f32::from(b.size.width),
            f32::from(b.origin.x),
            f32::from(b.origin.y),
        );
        let dpr = window.scale_factor().min(self.style.max_dpr);
        let vp_h = f32::from(window.viewport_size().height);

        if width > 0.0
            && (self.stale || (width - self.built_width).abs() > 0.5 || dpr != self.built_dpr)
        {
            self.rebuild(width, dpr, window);
        }

        let now = Instant::now();
        let dt = if self.animating {
            self.last_frame
                .replace(now)
                .map_or(1.0 / 60.0, |p| (now - p).as_secs_f32().min(0.1))
        } else {
            self.last_frame = None;
            0.0
        };
        self.clock += dt;

        let height = self
            .layout
            .as_ref()
            .map_or(self.style.line_height, |l| l.height);
        let static_text = self.reduce_motion || self.layout.as_ref().is_some_and(|l| l.plain);
        let mut moving = false;

        if self.layout.is_some() && !static_text {
            let prev = self.time;
            self.drive(dt, top, height, vp_h);
            if self.time != prev {
                self.time_dirty = true;
                if prev <= 0.0 && self.time > 0.0 {
                    self.events.push(PixelRevealEvent::Start);
                }
                if prev < self.total && self.time >= self.total {
                    self.events.push(PixelRevealEvent::Complete);
                }
            }

            if self.o.hover == Hover::Lens && self.can_hover {
                let target = if self.inside { 1.0 } else { 0.0 };
                let prev_lens = self.lens;
                self.lens = damp(self.lens, target, if target > 0.0 { 6.0 } else { 3.5 }, dt);
                if target == 0.0 && self.lens < 0.01 {
                    self.lens = 0.0;
                }
                if let Some(p) = self.pointer {
                    let (px_, py_) = (f32::from(p.x) - left, f32::from(p.y) - top);
                    if prev_lens == 0.0 {
                        self.lens_x = px_;
                        self.lens_y = py_;
                    }
                    let (nx, ny) = (
                        damp(self.lens_x, px_, 9.0, dt),
                        damp(self.lens_y, py_, 9.0, dt),
                    );
                    moving |= (nx - self.lens_x).abs() + (ny - self.lens_y).abs() > 0.02;
                    self.lens_x = nx;
                    self.lens_y = ny;
                }
                moving |= self.lens != prev_lens || (target - self.lens).abs() > 0.01;
            } else if self.lens != 0.0 {
                self.lens = 0.0;
            }

            if self.time_dirty {
                self.evaluate();
                self.time_dirty = false;
            }
            let done = self.time >= self.total;
            let next = if done && self.lens == 0.0 {
                State::Done
            } else if self.revealed || self.lens > 0.0 {
                State::Active
            } else {
                State::Hidden
            };
            if next != self.state {
                self.state = next;
                self.events.push(PixelRevealEvent::StateChanged(next));
            }

            let animated = self.o.flicker > 0.0 || self.o.glitch > 0.0;
            let scrubbing = matches!(self.o.trigger, Trigger::Scrub | Trigger::Pin);
            moving |= self.dir != 0 || (animated && !done && self.revealed);
            if scrubbing {
                moving |= self.o.smooth > 0.0;
            }
        } else if static_text && self.state != State::Static {
            self.state = State::Static;
            self.events
                .push(PixelRevealEvent::StateChanged(State::Static));
        }

        for e in std::mem::take(&mut self.events) {
            cx.emit(e);
        }
        if moving {
            self.animating = true;
            let entity = cx.entity();
            window.on_next_frame(move |_, cx| {
                entity.update(cx, |_, cx| cx.notify());
            });
        } else {
            self.animating = false;
            self.last_frame = None;
        }

        let params = self.layout.clone().map(|layout| PaintParams {
            uniforms: self.uniforms(&layout),
            anim: self.anim.clone(),
            state: self.state,
            clip_below: self.clip_below,
            rise: self.o.rise != 0.0,
            lens_active: self.lens > 0.0,
            layout,
        });

        let probe = self.bounds.clone();
        let entity = cx.entity();

        div()
            .id("pixel-reveal")
            .relative()
            .w_full()
            .h(px(height))
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, _| {
                this.pointer = Some(ev.position);
            }))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.inside = *hovered;
                this.wake(cx);
            }))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let prev = probe.replace(bounds);
                        let moved = (prev.size.width - bounds.size.width).abs() > px(0.5)
                            || (prev.origin.y - bounds.origin.y).abs() > px(0.5);
                        if moved {
                            let entity = entity.clone();
                            window.defer(cx, move |_, cx| entity.update(cx, |_, cx| cx.notify()));
                        }
                    },
                    move |bounds, (), window, _| {
                        if let Some(p) = &params {
                            paint_reveal(window, bounds, p);
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}
