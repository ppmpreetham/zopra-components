#![cfg(feature = "color-picker")]
#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    clippy::float_cmp,
    clippy::must_use_candidate,
    clippy::many_single_char_names,
    clippy::redundant_closure_for_method_calls,
    clippy::struct_excessive_bools,
    clippy::return_self_not_must_use,
    clippy::large_enum_variant
)]
//! Colour maths for the picker: parsing, formatting and conversions.
//! Pure `std`, no UI dependencies, so it can be unit-tested with `cargo test`.

pub type Rgb = [f32; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub h: f32,
    pub s: f32,
    pub v: f32,
    pub a: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Parsed {
    pub color: Color,
    pub clipped: bool,
}

#[must_use]
pub fn srgb_to_linear(c: f32) -> f32 {
    let a = c.abs();
    let l = if a <= 0.04045 { a / 12.92 } else { ((a + 0.055) / 1.055).powf(2.4) };
    l.copysign(c)
}

#[must_use]
pub fn linear_to_srgb(c: f32) -> f32 {
    let a = c.abs();
    let s = if a <= 0.003_130_8 { a * 12.92 } else { 1.055 * a.powf(1.0 / 2.4) - 0.055 };
    s.copysign(c)
}

#[must_use]
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Rgb {
    let hp = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match hp.floor() as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r + m, g + m, b + m]
}

fn mat(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

const SRGB_TO_XYZ: [[f32; 3]; 3] = [
    [0.412_456_4, 0.357_576_1, 0.180_437_5],
    [0.212_672_9, 0.715_152_2, 0.072_175_0],
    [0.019_333_9, 0.119_192, 0.950_304_1],
];
const XYZ_TO_SRGB: [[f32; 3]; 3] = [
    [3.240_454_2, -1.537_138_5, -0.498_531_4],
    [-0.969_266, 1.876_010_8, 0.041_556_0],
    [0.055_643_4, -0.204_025_9, 1.057_225_2],
];

const D65_TO_D50: [[f32; 3]; 3] = [
    [1.047_929_8, 0.022_946_8, -0.050_192_2],
    [0.029_627_8, 0.990_434_5, -0.017_073_8],
    [-0.009_243_1, 0.015_055_1, 0.751_874_3],
];
const D50_TO_D65: [[f32; 3]; 3] = [
    [0.955_473_5, -0.023_098_5, 0.063_259_3],
    [-0.028_369_7, 1.009_995_5, 0.021_041_4],
    [0.012_314_0, -0.020_507_7, 1.330_365_9],
];
const D50_WHITE: [f32; 3] = [0.964_22, 1.0, 0.825_21];

#[must_use]
pub fn linear_to_oklab([r, g, b]: [f32; 3]) -> [f32; 3] {
    let l = (0.412_221_47 * r + 0.536_332_54 * g + 0.051_445_99 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_84 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

#[must_use]
pub fn oklab_to_linear([l, a, b]: [f32; 3]) -> [f32; 3] {
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_93 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_4 * s,
        -0.004_196_086 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
}

fn lab_f(t: f32) -> f32 {
    const EPS: f32 = 216.0 / 24389.0;
    const KAPPA: f32 = 24389.0 / 27.0;
    if t > EPS { t.cbrt() } else { (KAPPA * t + 16.0) / 116.0 }
}

fn linear_to_lab(lin: [f32; 3]) -> [f32; 3] {
    let xyz = mat(&D65_TO_D50, mat(&SRGB_TO_XYZ, lin));
    let f = [0, 1, 2].map(|i| lab_f(xyz[i] / D50_WHITE[i]));
    [116.0 * f[1] - 16.0, 500.0 * (f[0] - f[1]), 200.0 * (f[1] - f[2])]
}

fn lab_to_linear([l, a, b]: [f32; 3]) -> [f32; 3] {
    const EPS: f32 = 216.0 / 24389.0;
    const KAPPA: f32 = 24389.0 / 27.0;
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let inv = |f: f32| if f * f * f > EPS { f * f * f } else { (116.0 * f - 16.0) / KAPPA };
    let x = inv(fx);
    let y = if l > KAPPA * EPS { fy * fy * fy } else { l / KAPPA };
    let z = inv(fz);
    mat(&XYZ_TO_SRGB, mat(&D50_TO_D65, [x * D50_WHITE[0], y * D50_WHITE[1], z * D50_WHITE[2]]))
}

fn in_gamut(lin: [f32; 3]) -> bool {
    lin.iter().all(|&c| (-1e-4..=1.0001).contains(&c))
}

fn map_linear_to_srgb(lin: [f32; 3]) -> (Rgb, bool) {
    let encode = |t: [f32; 3]| t.map(|c| linear_to_srgb(c.clamp(0.0, 1.0)));
    if in_gamut(lin) {
        return (encode(lin), false);
    }
    let [l, a, b] = linear_to_oklab(lin);
    if l >= 1.0 {
        return ([1.0; 3], true);
    }
    if l <= 0.0 {
        return ([0.0; 3], true);
    }
    let (chroma, hue) = (a.hypot(b), b.atan2(a));
    let (mut lo, mut hi) = (0.0f32, chroma);
    for _ in 0..24 {
        let mid = f32::midpoint(lo, hi);
        if in_gamut(oklab_to_linear([l, mid * hue.cos(), mid * hue.sin()])) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (encode(oklab_to_linear([l, lo * hue.cos(), lo * hue.sin()])), true)
}

impl Color {
    #[must_use]
    pub fn new(h: f32, s: f32, v: f32, a: f32) -> Self {
        Self { h: h.rem_euclid(360.0), s: s.clamp(0.0, 1.0), v: v.clamp(0.0, 1.0), a: a.clamp(0.0, 1.0) }
    }

    #[must_use]
    pub fn to_rgb(self) -> Rgb {
        hsv_to_rgb(self.h, self.s, self.v)
    }

    #[must_use]
    pub fn from_rgb(rgb: Rgb, a: f32, hue_hint: f32) -> Self {
        let [r, g, b] = rgb.map(|c| c.clamp(0.0, 1.0));
        let max = r.max(g).max(b);
        let d = max - r.min(g).min(b);
        let h = if d < 1e-6 {
            hue_hint
        } else if max == r {
            ((g - b) / d).rem_euclid(6.0) * 60.0
        } else if max == g {
            ((b - r) / d + 2.0) * 60.0
        } else {
            ((r - g) / d + 4.0) * 60.0
        };
        Self::new(h, if max <= 0.0 { 0.0 } else { d / max }, max, a)
    }

    #[must_use]
    pub fn from_hsl(h: f32, s: f32, l: f32, a: f32) -> Self {
        let v = l + s * l.min(1.0 - l);
        Self::new(h, if v <= 0.0 { 0.0 } else { 2.0 * (1.0 - l / v) }, v, a)
    }

    #[must_use]
    pub fn from_hwb(h: f32, w: f32, b: f32, a: f32) -> Self {
        let sum = w + b;
        let (w, b) = if sum > 1.0 { (w / sum, b / sum) } else { (w, b) };
        let v = 1.0 - b;
        Self::new(h, if v <= 0.0 { 0.0 } else { 1.0 - w / v }, v, a)
    }

    #[must_use]
    pub fn hsl(self) -> (f32, f32, f32) {
        let l = self.v * (1.0 - self.s / 2.0);
        let s = if l <= 0.0 || l >= 1.0 { 0.0 } else { (self.v - l) / l.min(1.0 - l) };
        (self.h, s, l)
    }

    #[must_use]
    pub fn hwb(self) -> (f32, f32, f32) {
        (self.h, (1.0 - self.s) * self.v, 1.0 - self.v)
    }

    fn linear(self) -> [f32; 3] {
        self.to_rgb().map(srgb_to_linear)
    }

    #[must_use]
    pub fn oklab(self) -> [f32; 3] {
        linear_to_oklab(self.linear())
    }

    /// [L, C, h°]
    #[must_use]
    pub fn oklch(self) -> [f32; 3] {
        let [l, a, b] = self.oklab();
        let c = a.hypot(b);
        [l, c, if c < 4e-4 { 0.0 } else { b.atan2(a).to_degrees().rem_euclid(360.0) }]
    }

    #[must_use]
    pub fn lab(self) -> [f32; 3] {
        linear_to_lab(self.linear())
    }

    /// [L, C, h°]
    #[must_use]
    pub fn lch(self) -> [f32; 3] {
        let [l, a, b] = self.lab();
        let c = a.hypot(b);
        [l, c, if c < 0.02 { 0.0 } else { b.atan2(a).to_degrees().rem_euclid(360.0) }]
    }

    #[must_use]
    pub fn with_xy(self, x: f32, y: f32) -> Self {
        let c = Self::from_rgb(xy_to_rgb(x, y), self.a, self.h);
        Self { v: if self.v < 0.02 { 1.0 } else { self.v }, ..c }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorFormat {
    Hex,
    Rgb,
    Hsl,
    Hwb,
    Hsv,
    Oklch,
    Oklab,
    Lab,
    Lch,
}

impl ColorFormat {
    pub const ALL: [Self; 9] = [
        Self::Hex,
        Self::Rgb,
        Self::Hsl,
        Self::Hwb,
        Self::Hsv,
        Self::Oklch,
        Self::Oklab,
        Self::Lab,
        Self::Lch,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Hex => "HEX",
            Self::Rgb => "RGB",
            Self::Hsl => "HSL",
            Self::Hwb => "HWB",
            Self::Hsv => "HSV",
            Self::Oklch => "OKLCH",
            Self::Oklab => "OKLAB",
            Self::Lab => "LAB",
            Self::Lch => "LCH",
        }
    }
}

fn fmt_n(v: f32, dp: usize) -> String {
    let mut s = format!("{v:.dp$}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".into();
    }
    s
}

#[must_use]
pub fn format_color(c: Color, f: ColorFormat, hex_hash: bool) -> String {
    let alpha = if c.a >= 0.9995 { String::new() } else { format!(" / {}", fmt_n(c.a, 2)) };
    let byte = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    let [r, g, b] = c.to_rgb();

    match f {
        ColorFormat::Hex => {
            let mut s = format!("{}{:02x}{:02x}{:02x}", if hex_hash { "#" } else { "" }, byte(r), byte(g), byte(b));
            if c.a < 0.9995 {
                s.push_str(&format!("{:02x}", byte(c.a)));
            }
            s
        }
        ColorFormat::Rgb => format!("rgb({} {} {}{})", byte(r), byte(g), byte(b), alpha),
        ColorFormat::Hsl => {
            let (h, s, l) = c.hsl();
            format!("hsl({}deg {}% {}%{})", fmt_n(h, 0), fmt_n(s * 100.0, 0), fmt_n(l * 100.0, 0), alpha)
        }
        ColorFormat::Hwb => {
            let (h, w, bl) = c.hwb();
            format!("hwb({}deg {}% {}%{})", fmt_n(h, 0), fmt_n(w * 100.0, 0), fmt_n(bl * 100.0, 0), alpha)
        }
        ColorFormat::Hsv => {
            format!("hsv({}deg {}% {}%{})", fmt_n(c.h, 0), fmt_n(c.s * 100.0, 0), fmt_n(c.v * 100.0, 0), alpha)
        }
        ColorFormat::Oklch => {
            let [l, ch, h] = c.oklch();
            format!("oklch({} {} {}{})", fmt_n(l, 3), fmt_n(ch, 3), fmt_n(h, 1), alpha)
        }
        ColorFormat::Oklab => {
            let [l, la, lb] = c.oklab();
            format!("oklab({} {} {}{})", fmt_n(l, 3), fmt_n(la, 3), fmt_n(lb, 3), alpha)
        }
        ColorFormat::Lab => {
            let [l, la, lb] = c.lab();
            format!("lab({} {} {}{})", fmt_n(l, 1), fmt_n(la, 1), fmt_n(lb, 1), alpha)
        }
        ColorFormat::Lch => {
            let [l, ch, h] = c.lch();
            format!("lch({} {} {}{})", fmt_n(l, 1), fmt_n(ch, 1), fmt_n(h, 1), alpha)
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Unit {
    Plain,
    Percent,
    Deg,
    Rad,
    Grad,
    Turn,
}

fn token(t: &str) -> Option<(f32, Unit)> {
    if t == "none" {
        return Some((0.0, Unit::Plain));
    }
    let (body, unit) = if let Some(b) = t.strip_suffix('%') {
        (b, Unit::Percent)
    } else if let Some(b) = t.strip_suffix("deg") {
        (b, Unit::Deg)
    } else if let Some(b) = t.strip_suffix("grad") {
        (b, Unit::Grad)
    } else if let Some(b) = t.strip_suffix("rad") {
        (b, Unit::Rad)
    } else if let Some(b) = t.strip_suffix("turn") {
        (b, Unit::Turn)
    } else {
        (t, Unit::Plain)
    };
    let v: f32 = body.parse().ok()?;
    v.is_finite().then_some((v, unit))
}

fn val(t: &str, pct_full: f32) -> Option<f32> {
    let (v, u) = token(t)?;
    Some(if u == Unit::Percent { v / 100.0 * pct_full } else { v })
}

fn angle(t: &str) -> Option<f32> {
    let (v, u) = token(t)?;
    let deg = match u {
        Unit::Rad => v.to_degrees(),
        Unit::Grad => v * 0.9,
        Unit::Turn => v * 360.0,
        _ => v,
    };
    Some(deg.rem_euclid(360.0))
}

fn fraction(t: &str) -> Option<f32> {
    let (v, _) = token(t)?;
    Some((v / 100.0).clamp(0.0, 1.0))
}

fn finish(lin: [f32; 3], alpha: f32, hint: f32) -> Parsed {
    let (rgb, clipped) = map_linear_to_srgb(lin);
    Parsed { color: Color::from_rgb(rgb, alpha, hint), clipped }
}

const NAMED: &[(&str, u32)] = &[
    ("black", 0x000000),
    ("white", 0xffffff),
    ("red", 0xff0000),
    ("green", 0x008000),
    ("blue", 0x0000ff),
    ("yellow", 0xffff00),
    ("cyan", 0x00ffff),
    ("aqua", 0x00ffff),
    ("magenta", 0xff00ff),
    ("fuchsia", 0xff00ff),
    ("orange", 0xffa500),
    ("purple", 0x800080),
    ("pink", 0xffc0cb),
    ("gray", 0x808080),
    ("grey", 0x808080),
    ("silver", 0xc0c0c0),
    ("maroon", 0x800000),
    ("navy", 0x000080),
    ("teal", 0x008080),
    ("lime", 0x00ff00),
    ("olive", 0x808000),
    ("brown", 0xa52a2a),
    ("gold", 0xffd700),
    ("indigo", 0x4b0082),
    ("violet", 0xee82ee),
    ("coral", 0xff7f50),
    ("crimson", 0xdc143c),
    ("salmon", 0xfa8072),
    ("tomato", 0xff6347),
    ("turquoise", 0x40e0d0),
];

fn parse_named(s: &str, hint: f32) -> Option<Parsed> {
    if s == "transparent" {
        return Some(Parsed { color: Color::from_rgb([0.0; 3], 0.0, hint), clipped: false });
    }
    let (_, hex) = NAMED.iter().find(|(n, _)| *n == s)?;
    let ch = |shift: u32| ((hex >> shift) & 0xff) as f32 / 255.0;
    Some(Parsed { color: Color::from_rgb([ch(16), ch(8), ch(0)], 1.0, hint), clipped: false })
}

fn parse_hex(s: &str, hint: f32) -> Option<Parsed> {
    let h = s.strip_prefix('#').unwrap_or(s);
    let d = h.chars().map(|c| c.to_digit(16)).collect::<Option<Vec<u32>>>()?;
    let (r, g, b, a) = match d.len() {
        3 => (d[0] * 17, d[1] * 17, d[2] * 17, 255),
        4 => (d[0] * 17, d[1] * 17, d[2] * 17, d[3] * 17),
        6 => (d[0] * 16 + d[1], d[2] * 16 + d[3], d[4] * 16 + d[5], 255),
        8 => (d[0] * 16 + d[1], d[2] * 16 + d[3], d[4] * 16 + d[5], d[6] * 16 + d[7]),
        _ => return None,
    };
    let f = |x: u32| x as f32 / 255.0;
    Some(Parsed { color: Color::from_rgb([f(r), f(g), f(b)], f(a), hint), clipped: false })
}

fn parse_function(name: &str, body: &str, hint: f32) -> Option<Parsed> {
    let (main, alpha_tok) = match body.split_once('/') {
        Some((m, a)) => (m, Some(a.trim())),
        None => (body, None),
    };
    let toks: Vec<&str> = main.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).collect();
    let (c, alpha_tok) = match (toks.len(), alpha_tok) {
        (3, a) => (&toks[..3], a),
        (4, None) => (&toks[..3], Some(toks[3])),
        _ => return None,
    };
    let alpha = match alpha_tok {
        Some(t) => val(t, 1.0)?.clamp(0.0, 1.0),
        None => 1.0,
    };

    match name {
        "rgb" | "rgba" => {
            let ch = |t: &str| val(t, 255.0).map(|v| (v / 255.0).clamp(0.0, 1.0));
            Some(Parsed { color: Color::from_rgb([ch(c[0])?, ch(c[1])?, ch(c[2])?], alpha, hint), clipped: false })
        }
        "hsl" | "hsla" => Some(Parsed {
            color: Color::from_hsl(angle(c[0])?, fraction(c[1])?, fraction(c[2])?, alpha),
            clipped: false,
        }),
        "hwb" => Some(Parsed {
            color: Color::from_hwb(angle(c[0])?, fraction(c[1])?, fraction(c[2])?, alpha),
            clipped: false,
        }),
        "hsv" | "hsb" => Some(Parsed {
            color: Color::new(angle(c[0])?, fraction(c[1])?, fraction(c[2])?, alpha),
            clipped: false,
        }),
        "oklab" | "oklch" => {
            let mut l = val(c[0], 1.0)?;
            if !c[0].ends_with('%') && l > 1.0 {
                l /= 100.0;
            }
            let (a, b) = if name == "oklab" {
                (val(c[1], 0.4)?, val(c[2], 0.4)?)
            } else {
                let (ch, h) = (val(c[1], 0.4)?.max(0.0), angle(c[2])?.to_radians());
                (ch * h.cos(), ch * h.sin())
            };
            Some(finish(oklab_to_linear([l.clamp(0.0, 1.0), a, b]), alpha, hint))
        }
        "lab" | "lch" => {
            let l = val(c[0], 100.0)?.clamp(0.0, 100.0);
            let (a, b) = if name == "lab" {
                (val(c[1], 125.0)?, val(c[2], 125.0)?)
            } else {
                let (ch, h) = (val(c[1], 150.0)?.max(0.0), angle(c[2])?.to_radians());
                (ch * h.cos(), ch * h.sin())
            };
            Some(finish(lab_to_linear([l, a, b]), alpha, hint))
        }
        _ => None,
    }
}

#[must_use]
pub fn parse(input: &str, hue_hint: f32) -> Option<Parsed> {
    let s = input.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    if let Some(open) = s.find('(') {
        let body = s[open + 1..].strip_suffix(')')?;
        return parse_function(s[..open].trim(), body, hue_hint);
    }
    parse_hex(&s, hue_hint).or_else(|| parse_named(&s, hue_hint))
}

pub const D65_XY: (f32, f32) = (0.3127, 0.3290);
pub const SRGB_PRIMARIES: [(f32, f32); 3] = [(0.64, 0.33), (0.30, 0.60), (0.15, 0.06)];

/// CIE 1931 2° spectral locus, 380–700 nm in 10 nm steps.
const LOCUS: [(f32, f32); 33] = [
    (0.1741, 0.0050),
    (0.1738, 0.0049),
    (0.1733, 0.0048),
    (0.1726, 0.0048),
    (0.1714, 0.0051),
    (0.1689, 0.0069),
    (0.1644, 0.0109),
    (0.1566, 0.0177),
    (0.1440, 0.0297),
    (0.1241, 0.0578),
    (0.0913, 0.1327),
    (0.0454, 0.2950),
    (0.0082, 0.5384),
    (0.0139, 0.7502),
    (0.0743, 0.8338),
    (0.1547, 0.8059),
    (0.2296, 0.7543),
    (0.3016, 0.6923),
    (0.3731, 0.6245),
    (0.4441, 0.5547),
    (0.5125, 0.4866),
    (0.5752, 0.4242),
    (0.6270, 0.3725),
    (0.6658, 0.3340),
    (0.6915, 0.3083),
    (0.7079, 0.2920),
    (0.7190, 0.2809),
    (0.7260, 0.2740),
    (0.7300, 0.2700),
    (0.7320, 0.2680),
    (0.7334, 0.2666),
    (0.7344, 0.2656),
    (0.7347, 0.2653),
];

#[must_use]
pub fn spectral_locus() -> Vec<(f32, f32)> {
    const SUB: usize = 6;
    let n = LOCUS.len();
    let mut out = Vec::with_capacity(n * SUB);
    for i in 0..n - 1 {
        let p0 = LOCUS[i.saturating_sub(1)];
        let (p1, p2) = (LOCUS[i], LOCUS[i + 1]);
        let p3 = LOCUS[(i + 2).min(n - 1)];
        for k in 0..SUB {
            let t = k as f32 / SUB as f32;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| {
                0.5 * (2.0 * b + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3)
            };
            out.push((f(p0.0, p1.0, p2.0, p3.0), f(p0.1, p1.1, p2.1, p3.1)));
        }
    }
    out.push(LOCUS[n - 1]);
    out
}

pub fn xy_to_rgb(x: f32, y: f32) -> Rgb {
    let y = y.max(1e-4);
    let lin = mat(&XYZ_TO_SRGB, [x / y, 1.0, (1.0 - x - y) / y]).map(|c| c.max(0.0));
    let max = lin.iter().copied().fold(1e-6, f32::max);
    lin.map(|c| linear_to_srgb(c / max))
}

pub fn rgb_to_xy(rgb: Rgb) -> (f32, f32) {
    let [x, y, z] = mat(&SRGB_TO_XYZ, rgb.map(srgb_to_linear));
    let sum = (x + y + z).max(1e-9);
    if x + y + z < 1e-6 { D65_XY } else { (x / sum, y / sum) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(c: Color) -> String {
        format_color(c, ColorFormat::Hex, true)
    }

    #[test]
    fn parses_common_syntaxes_to_the_same_colour() {
        for s in ["#ff2e8c", "ff2e8c", "rgb(255 46 140)", "rgb(255, 46, 140)", "hsl(333deg 100% 59%)"] {
            assert_eq!(hex(parse(s, 0.0).unwrap().color), "#ff2e8c", "{s}");
        }
    }

    #[test]
    fn formats_known_values() {
        let red = parse("#ff0000", 0.0).unwrap().color;
        assert_eq!(format_color(red, ColorFormat::Oklch, true), "oklch(0.628 0.258 29.2)");
        assert_eq!(format_color(red, ColorFormat::Lab, true), "lab(54.3 80.8 69.9)");
        assert_eq!(format_color(red, ColorFormat::Hsl, true), "hsl(0deg 100% 50%)");
    }

    #[test]
    fn alpha_round_trips() {
        let c = parse("rgb(10 20 30 / 0.5)", 0.0).unwrap().color;
        assert!((c.a - 0.5).abs() < 1e-6);
        assert_eq!(format_color(c, ColorFormat::Rgb, true), "rgb(10 20 30 / 0.5)");
    }

    #[test]
    fn wide_gamut_input_is_mapped_and_flagged() {
        assert!(parse("oklch(0.7 0.4 150)", 0.0).unwrap().clipped);
        assert!(!parse("oklch(0.628 0.25 29.2)", 0.0).unwrap().clipped);
    }

    #[test]
    fn rejects_garbage() {
        for s in ["", "#12", "rgb(1 2)", "hsl(a b c)", "banana", "rgb(1 2 3"] {
            assert!(parse(s, 0.0).is_none(), "{s}");
        }
    }

    #[test]
    fn white_is_d65() {
        let (x, y) = rgb_to_xy([1.0; 3]);
        assert!((x - D65_XY.0).abs() < 1e-3 && (y - D65_XY.1).abs() < 1e-3);
    }

    #[test]
    fn greys_keep_their_hue() {
        let c = Color::from_rgb([0.5; 3], 1.0, 200.0);
        assert_eq!(c.h, 200.0);
    }
}
