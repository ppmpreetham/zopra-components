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

use std::{cell::Cell, f32::consts::SQRT_2, rc::Rc, sync::Arc, time::Duration};

use gpui_kit::{
    black, canvas, div, fill, Corners, PaintQuad, img, linear_color_stop, linear_gradient, point, prelude::*, px, size,
    transparent_black, white, Background, Bounds, ClipboardItem, Context, DispatchPhase, Div, Entity,
    EventEmitter, FontWeight, Focusable as _, Hsla, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ObjectFit, PathBuilder, Pixels, Point, Render, RenderImage, Rgba, SharedString, Stateful,
    Subscription, Task, Window,
};
use gpui_kit::component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex, ActiveTheme as _, IconName, Sizable as _,
};
use image::{Frame, RgbaImage};
use smallvec::SmallVec;

use super::color::{
    format_color, hsv_to_rgb, parse, rgb_to_xy, spectral_locus, xy_to_rgb, Color, ColorFormat, Rgb,
    D65_XY, SRGB_PRIMARIES,
};

const PAD: f32 = 12.0;
const THUMB: f32 = 16.0;
const SLIDER_H: f32 = 20.0;
const TRACK_H: f32 = 12.0;
const RING_INNER: f32 = 0.74;
const IMG_SCALE: u32 = 2; // rasterise at 2x so HiDPI stays crisp

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerMode {
    /// Hue/saturation disc.
    Wheel,
    /// Hue ring with a saturation/value square.
    Ring,
    /// CIE 1931 xy chromaticity diagram.
    Cie,
}

impl PickerMode {
    const ALL: [Self; 3] = [Self::Wheel, Self::Ring, Self::Cie];

    const fn label(self) -> &'static str {
        match self {
            Self::Wheel => "Wheel",
            Self::Ring => "Ring",
            Self::Cie => "CIE xy",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ColorPickerConfig {
    pub initial: SharedString,
    pub mode: PickerMode,
    pub show_mode_switcher: bool,
    pub formats: Vec<ColorFormat>,
    pub alpha: bool,
    pub hex_hash: bool,
    pub width: f32,
    pub swatch_cols: usize,
    pub swatch_rows: usize,
}

impl Default for ColorPickerConfig {
    fn default() -> Self {
        Self {
            initial: "#ff2e8c".into(),
            mode: PickerMode::Wheel,
            show_mode_switcher: true,
            formats: vec![ColorFormat::Hex, ColorFormat::Rgb, ColorFormat::Hsl, ColorFormat::Oklch],
            alpha: true,
            hex_hash: true,
            width: 280.0,
            swatch_cols: 10,
            swatch_rows: 2,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ColorPickerEvent {
    Change(Hsla),
    ChangeEnd(Hsla),
    SwatchesChanged(Vec<Option<Hsla>>),
}

#[derive(Clone, Copy, PartialEq)]
enum Target {
    Wheel,
    RingHue,
    Square,
    Cie,
    Value,
    Alpha,
}

struct Row {
    format: ColorFormat,
    input: Entity<InputState>,
    /// Set when the user pressed Enter on text that doesn't parse.
    invalid: bool,
}

struct Raster {
    w: u32,
    h: u32,
    bgra: Vec<u8>,
}

#[derive(Clone, Copy)]
struct Geom {
    c: f32,
    rad: f32,
    r_i: f32,
    side: f32,
}

fn geom(s: f32) -> Geom {
    let rad = s / 2.0 - 0.5;
    let r_i = rad * RING_INNER;
    Geom { c: s / 2.0, rad, r_i, side: r_i * SQRT_2 * 0.94 }
}

/// Uniform scale + x offset that fit x∈[0,0.8], y∈[0,0.9] into an s×s box.
fn cie_xform(s: f32) -> (f32, f32) {
    let sc = s / 0.9;
    (sc, (s - 0.8 * sc) / 2.0)
}

fn hsla_of(c: Color) -> Hsla {
    let [r, g, b] = c.to_rgb();
    Rgba { r, g, b, a: c.a }.into()
}

fn solid(c: Color) -> Hsla {
    let [r, g, b] = c.to_rgb();
    Rgba { r, g, b, a: 1.0 }.into()
}

pub struct ColorPicker {
    config: ColorPickerConfig,
    color: Color,
    clipped: bool,
    rows: Vec<Row>,
    rows_dirty: bool,
    swatches: Vec<Option<Color>>,
    copied: Option<usize>,
    copied_task: Option<Task<()>>,
    drag: Option<Target>,
    images: [Option<Arc<RenderImage>>; 3],
    pending: [bool; 3],
    locus: Arc<Vec<(f32, f32)>>,
    surface: Rc<Cell<Bounds<Pixels>>>,
    value_bar: Rc<Cell<Bounds<Pixels>>>,
    alpha_bar: Rc<Cell<Bounds<Pixels>>>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ColorPickerEvent> for ColorPicker {}

impl ColorPicker {
    pub fn new(config: ColorPickerConfig, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let color = parse(&config.initial, 0.0).map_or_else(|| Color::new(333.0, 0.82, 1.0, 1.0), |p| p.color);

        let mut rows = Vec::new();
        let mut subs = Vec::new();
        for (ix, &format) in config.formats.iter().enumerate() {
            let text = format_color(color, format, config.hex_hash);
            let input = cx.new(|cx| InputState::new(window, cx).default_value(text));
            subs.push(cx.subscribe(&input, move |this, _, ev: &InputEvent, cx| this.on_input(ix, ev, cx)));
            rows.push(Row { format, input, invalid: false });
        }

        let mut this = Self {
            swatches: vec![None; config.swatch_cols * config.swatch_rows],
            color,
            clipped: false,
            rows,
            rows_dirty: false,
            copied: None,
            copied_task: None,
            drag: None,
            images: [None, None, None],
            pending: [false; 3],
            locus: Arc::new(spectral_locus()),
            surface: Rc::default(),
            value_bar: Rc::default(),
            alpha_bar: Rc::default(),
            _subs: subs,
            config,
        };
        this.ensure_image(this.config.mode, cx);
        this
    }

    #[must_use]
    pub const fn color(&self) -> Color {
        self.color
    }

    #[must_use]
    pub fn hsla(&self) -> Hsla {
        hsla_of(self.color)
    }

    #[must_use]
    pub fn value(&self, format: ColorFormat) -> String {
        format_color(self.color, format, self.config.hex_hash)
    }

    pub fn set_value(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        match parse(text, self.color.h) {
            Some(p) => {
                self.apply(p.color, p.clipped, cx);
                true
            }
            None => false,
        }
    }

    pub fn set_hsla(&mut self, hsla: Hsla, cx: &mut Context<Self>) {
        let rgba: Rgba = hsla.into();
        self.apply(Color::from_rgb([rgba.r, rgba.g, rgba.b], rgba.a, self.color.h), false, cx);
    }

    pub fn set_mode(&mut self, mode: PickerMode, cx: &mut Context<Self>) {
        self.config.mode = mode;
        self.ensure_image(mode, cx);
        cx.notify();
    }

    pub fn set_swatches(&mut self, swatches: Vec<Option<Hsla>>, cx: &mut Context<Self>) {
        let len = self.swatches.len();
        self.swatches = swatches
            .into_iter()
            .map(|s| {
                s.map(|h| {
                    let r: Rgba = h.into();
                    Color::from_rgb([r.r, r.g, r.b], r.a, 0.0)
                })
            })
            .chain(std::iter::repeat(None))
            .take(len)
            .collect();
        cx.notify();
    }


    fn apply(&mut self, color: Color, clipped: bool, cx: &mut Context<Self>) {
        if color != self.color || clipped != self.clipped {
            self.color = color;
            self.clipped = clipped;
            self.rows_dirty = true;
            cx.emit(ColorPickerEvent::Change(hsla_of(color)));
            cx.notify();
        }
    }

    fn text_for(&self, ix: usize) -> String {
        format_color(self.color, self.rows[ix].format, self.config.hex_hash)
    }

    fn on_input(&mut self, ix: usize, ev: &InputEvent, cx: &mut Context<Self>) {
        match ev {
            InputEvent::Change => {
                let text = self.rows[ix].input.read(cx).value();
                if &*text == self.text_for(ix).as_str() {
                    return;
                }
                if let Some(p) = parse(&text, self.color.h) {
                    self.rows[ix].invalid = false;
                    self.apply(p.color, p.clipped, cx);
                }
            }
            InputEvent::PressEnter { .. } => {
                let text = self.rows[ix].input.read(cx).value();
                if parse(&text, self.color.h).is_none() {
                    self.rows[ix].invalid = true;
                    cx.notify();
                }
            }
            InputEvent::Blur => {
                self.rows[ix].invalid = false;
                self.rows_dirty = true;
                cx.notify();
            }
            _ => {}
        }
    }

    fn sync_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.rows_dirty {
            return;
        }
        self.rows_dirty = false;
        for row in &self.rows {
            if row.input.read(cx).focus_handle(cx).is_focused(window) {
                continue;
            }
            let text = format_color(self.color, row.format, self.config.hex_hash);
            row.input.update(cx, |state, cx| {
                if &*state.value() != text.as_str() {
                    state.set_value(text, window, cx);
                }
            });
        }
    }

    fn copy(&mut self, ix: usize, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(self.text_for(ix)));
        self.copied = Some(ix);
        self.copied_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1200)).await;
            this.update(cx, |this, cx| {
                this.copied = None;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    fn swatch_click(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(c) = self.swatches[ix] { self.apply(c, false, cx) } else {
            self.swatches[ix] = Some(self.color);
            self.emit_swatches(cx);
        }
    }

    fn swatch_remove(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.swatches[ix].take().is_some() {
            self.emit_swatches(cx);
        }
    }

    fn emit_swatches(&mut self, cx: &mut Context<Self>) {
        cx.emit(ColorPickerEvent::SwatchesChanged(self.swatches.iter().map(|s| s.map(hsla_of)).collect()));
        cx.notify();
    }

    fn surface_size(&self) -> f32 {
        self.config.width - PAD * 2.0
    }

    fn surface_down(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let s = self.surface_size();
        let target = match self.config.mode {
            PickerMode::Wheel => Target::Wheel,
            PickerMode::Cie => Target::Cie,
            PickerMode::Ring => {
                let g = geom(s);
                let b = self.surface.get();
                let (dx, dy) = (f32::from(pos.x - b.origin.x) - g.c, f32::from(pos.y - b.origin.y) - g.c);
                let in_square = dx.abs() <= g.side / 2.0 + 4.0 && dy.abs() <= g.side / 2.0 + 4.0;
                if !in_square && dx.hypot(dy) >= g.r_i * 0.95 { Target::RingHue } else { Target::Square }
            }
        };
        self.start_drag(target, pos, cx);
    }

    fn start_drag(&mut self, target: Target, pos: Point<Pixels>, cx: &mut Context<Self>) {
        self.drag = Some(target);
        self.apply_pointer(target, pos, cx);
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
            cx.emit(ColorPickerEvent::ChangeEnd(hsla_of(self.color)));
            cx.notify();
        }
    }

    fn bar_t(bar: &Cell<Bounds<Pixels>>, x: Pixels, inner: f32) -> f32 {
        let b = bar.get();
        ((f32::from(x - b.origin.x) - THUMB / 2.0) / (inner - THUMB)).clamp(0.0, 1.0)
    }

    fn apply_pointer(&mut self, target: Target, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let s = self.surface_size();
        let g = geom(s);
        let b = self.surface.get();
        let (px_, py_) = (f32::from(pos.x - b.origin.x), f32::from(pos.y - b.origin.y));
        let (dx, dy) = (px_ - g.c, py_ - g.c);
        let mut c = self.color;

        match target {
            Target::Wheel => {
                c.h = (-dy).atan2(dx).to_degrees().rem_euclid(360.0);
                c.s = (dx.hypot(dy) / g.rad).min(1.0);
            }
            Target::RingHue => {
                c.h = (-dx).atan2(-dy).to_degrees().rem_euclid(360.0);
            }
            Target::Square => {
                c.s = ((dx + g.side / 2.0) / g.side).clamp(0.0, 1.0);
                c.v = 1.0 - ((dy + g.side / 2.0) / g.side).clamp(0.0, 1.0);
            }
            Target::Cie => {
                let (sc, xoff) = cie_xform(s);
                let x = ((px_ - xoff) / sc).clamp(0.0, 0.8);
                let y = ((s - py_) / sc).clamp(0.001, 0.9);
                c = c.with_xy(x, y);
            }
            Target::Value => c.v = Self::bar_t(&self.value_bar, pos.x, s),
            Target::Alpha => c.a = Self::bar_t(&self.alpha_bar, pos.x, s),
        }
        self.apply(Color::new(c.h, c.s, c.v, c.a), false, cx);
    }

    fn ensure_image(&mut self, mode: PickerMode, cx: &mut Context<Self>) {
        let slot = mode as usize;
        if self.images[slot].is_some() || self.pending[slot] {
            return;
        }
        self.pending[slot] = true;
        let n = (self.surface_size() * IMG_SCALE as f32).round() as u32;
        let locus = self.locus.clone();

        cx.spawn(async move |this, cx| {
            let raster = cx
                .background_executor()
                .spawn(async move {
                    match mode {
                        PickerMode::Wheel => raster_wheel(n),
                        PickerMode::Ring => raster_ring(n),
                        PickerMode::Cie => raster_cie(n, &locus),
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                this.images[slot] = to_image(raster);
                this.pending[slot] = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn render_modes(&self, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let (bg, active_bg, muted_fg) = (theme.muted, theme.background, theme.muted_foreground);
        let current = self.config.mode;

        h_flex().p(px(2.0)).gap(px(2.0)).rounded_lg().bg(bg).children(PickerMode::ALL.map(|m| {
            div()
                .id(("cp-mode", m as usize))
                .flex_1()
                .py(px(4.0))
                .rounded_md()
                .text_center()
                .text_size(px(11.0))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .text_color(if m == current { theme.foreground } else { muted_fg })
                .when(m == current, |d| d.bg(active_bg).shadow_sm())
                .child(m.label())
                .on_click(cx.listener(move |this, _, _, cx| this.set_mode(m, cx)))
        }))
    }

    fn render_surface(&self, cx: &Context<Self>) -> Stateful<Div> {
        let s = self.surface_size();
        let c = self.color;
        let g = geom(s);
        let mode = self.config.mode;
        let probe = self.surface.clone();
        let placeholder = cx.theme().muted.opacity(0.4);

        let root = div()
            .id("cp-surface")
            .relative()
            .flex_none()
            .size(px(s))
            .cursor_crosshair()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| this.surface_down(ev.position, cx)),
            )
            .child(canvas(move |b, _, _| probe.set(b), |_, (), _, _| {}).absolute().size_full());

        let root = match &self.images[mode as usize] {
            Some(image) => root.child(
                img(image.clone()).absolute().top_0().left_0().size(px(s)).object_fit(ObjectFit::Fill),
            ),
            None => root.child(div().absolute().top_0().left_0().size(px(s)).rounded_full().bg(placeholder)),
        };

        match mode {
            PickerMode::Wheel => {
                let h = c.h.to_radians();
                let (mx, my) = (g.c + c.s * g.rad * h.cos(), g.c - c.s * g.rad * h.sin());
                root
                    .child(
                        div()
                            .absolute()
                            .top(px(1.0))
                            .left(px(1.0))
                            .size(px(s - 2.0))
                            .rounded_full()
                            .bg(black().opacity(1.0 - c.v)),
                    )
                    .child(marker(mx, my, 18.0, solid(c)))
            }
            PickerMode::Ring => {
                let th = c.h.to_radians();
                let mid = f32::midpoint(g.rad, g.r_i);
                let thick = g.rad - g.r_i;
                let pure = solid(Color { s: 1.0, v: 1.0, ..c });
                let (sx, sy) = (g.c - g.side / 2.0, g.c - g.side / 2.0);

                let square = div()
                    .absolute()
                    .left(px(sx))
                    .top(px(sy))
                    .size(px(g.side))
                    .rounded(px(4.0))
                    .bg(pure)
                    .child(
                        div().absolute().size_full().rounded(px(4.0)).bg(linear_gradient(
                            90.0,
                            linear_color_stop(white(), 0.0),
                            linear_color_stop(white().opacity(0.0), 1.0),
                        )),
                    )
                    .child(
                        div().absolute().size_full().rounded(px(4.0)).bg(linear_gradient(
                            180.0,
                            linear_color_stop(black().opacity(0.0), 0.0),
                            linear_color_stop(black(), 1.0),
                        )),
                    );

                root.child(square)
                    .child(marker(g.c - mid * th.sin(), g.c - mid * th.cos(), (thick * 0.9).max(16.0), pure))
                    .child(marker(sx + c.s * g.side, sy + (1.0 - c.v) * g.side, 14.0, solid(c)))
            }
            PickerMode::Cie => {
                let (sc, xoff) = cie_xform(s);
                let (x, y) = rgb_to_xy(Color { v: 1.0, ..c }.to_rgb());
                let (line, tri) = (cx.theme().foreground.opacity(0.7), cx.theme().foreground.opacity(0.45));
                root.child(cie_overlay(self.locus.clone(), s, line, tri))
                    .child(marker(xoff + x * sc, s - y * sc, 16.0, solid(c)))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn slider(
        &self,
        id: &'static str,
        probe: Rc<Cell<Bounds<Pixels>>>,
        track: Background,
        checker_behind: bool,
        t: f32,
        thumb_fill: Hsla,
        target: Target,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let inner = self.surface_size();
        div()
            .id(id)
            .relative()
            .w_full()
            .h(px(SLIDER_H))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| this.start_drag(target, ev.position, cx)),
            )
            .child(canvas(move |b, _, _| probe.set(b), |_, (), _, _| {}).absolute().size_full())
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px((SLIDER_H - TRACK_H) / 2.0))
                    .h(px(TRACK_H))
                    .rounded(px(5.0))
                    .when(checker_behind, |d| d.child(checker()))
                    .child(div().absolute().size_full().rounded(px(5.0)).bg(track)),
            )
            .child(
                div()
                    .absolute()
                    .top(px((SLIDER_H - THUMB) / 2.0))
                    .left(px(t * (inner - THUMB)))
                    .size(px(THUMB))
                    .rounded_full()
                    .border_2()
                    .border_color(white())
                    .bg(thumb_fill)
                    .shadow_md(),
            )
    }

    fn render_row(&self, ix: usize, row: &Row, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let (muted_fg, danger) = (theme.muted_foreground, theme.danger);
        let copied = self.copied == Some(ix);

        h_flex()
            .gap(px(6.0))
            .child(
                div()
                    .w(px(46.0))
                    .flex_none()
                    .text_size(px(10.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(muted_fg)
                    .child(row.format.label()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .rounded_md()
                    .border_1()
                    .border_color(if row.invalid { danger } else { transparent_black() })
                    .child(Input::new(&row.input).small()),
            )
            .child(
                Button::new(("cp-copy", ix))
                    .ghost()
                    .xsmall()
                    .icon(if copied { IconName::Check } else { IconName::Copy })
                    .on_click(cx.listener(move |this, _, _, cx| this.copy(ix, cx))),
            )
    }

    fn render_palette(&self, cx: &Context<Self>) -> Div {
        let cx: &Context<Self> = cx;
        let theme = cx.theme();
        let (border, muted_fg) = (theme.border, theme.muted_foreground);
        let cols = self.config.swatch_cols.max(1);
        let cell = ((self.surface_size() - 40.0 - 10.0 - 4.0 * (cols as f32 - 1.0)) / cols as f32).max(10.0);

        let preview = div()
            .relative()
            .flex_none()
            .size(px(40.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(border)
            .child(checker())
            .child(div().absolute().size_full().rounded(px(6.0)).bg(hsla_of(self.color)));

        let grid = v_flex().gap(px(4.0)).children(self.swatches.chunks(cols).enumerate().map(|(r, chunk)| {
            h_flex().gap(px(4.0)).children(chunk.iter().enumerate().map(move |(i, sw)| {
                let ix = r * cols + i;
                div()
                    .id(("cp-swatch", ix))
                    .flex_none()
                    .size(px(cell))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(border)
                    .cursor_pointer()
                    .map(|d| match sw {
                        Some(c) => d.bg(hsla_of(*c)),
                        None => d.hover(move |s| s.bg(border.opacity(0.35))),
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.swatch_click(ix, cx)))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _: &MouseDownEvent, _, cx| this.swatch_remove(ix, cx)),
                    )
            }))
        }));

        v_flex().gap(px(6.0)).child(h_flex().gap(px(10.0)).items_start().child(preview).child(grid)).child(
            div().text_size(px(10.0)).text_color(muted_fg).child("Click an empty slot to save · right-click to remove"),
        )
    }
}

impl Render for ColorPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_rows(window, cx);
        let cx: &Context<Self> = cx;

        let c = self.color;
        let theme = cx.theme();
        let (popover, border, muted_fg) = (theme.popover, theme.border, theme.muted_foreground);
        let mode = self.config.mode;

        let value_track = linear_gradient(
            90.0,
            linear_color_stop(solid(Color { v: 1.0, ..c }), 0.0),
            linear_color_stop(black(), 1.0),
        );
        let alpha_track = linear_gradient(
            90.0,
            linear_color_stop(solid(c).opacity(0.0), 0.0),
            linear_color_stop(solid(c), 1.0),
        );

        let entity = cx.entity();
        let dragging = self.drag;
        let drag_layer = canvas(
            |_, _, _| (),
            move |_, (), window, _| {
                let Some(target) = dragging else { return };
                let e = entity.clone();
                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    e.update(cx, |this, cx| {
                        if ev.pressed_button == Some(MouseButton::Left) {
                            this.apply_pointer(target, ev.position, cx);
                        } else {
                            this.end_drag(cx);
                        }
                    });
                });
                let e = entity;
                window.on_mouse_event(move |ev: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && ev.button == MouseButton::Left {
                        e.update(cx, ColorPicker::end_drag);
                    }
                });
            },
        )
        .absolute()
        .size_full();

        let modes_el = if self.config.show_mode_switcher {
            Some(self.render_modes(cx))
        } else {
            None
        };

        let surface_el = self.render_surface(cx);

        let value_slider_el = if mode == PickerMode::Ring {
            None
        } else {
            Some(self.slider("cp-value", self.value_bar.clone(), value_track, false, c.v, solid(c), Target::Value, cx))
        };

        let alpha_slider_el = if self.config.alpha {
            Some(self.slider("cp-alpha", self.alpha_bar.clone(), alpha_track, true, c.a, hsla_of(c), Target::Alpha, cx))
        } else {
            None
        };

        let rows_el: Vec<_> = self.rows.iter().enumerate().map(|(ix, row)| self.render_row(ix, row, cx)).collect();

        let palette_el = self.render_palette(cx);

        v_flex()
            .relative()
            .w(px(self.config.width))
            .p(px(PAD))
            .gap(px(10.0))
            .rounded_xl()
            .border_1()
            .border_color(border)
            .bg(popover)
            .shadow_lg()
            .when_some(modes_el, gpui_kit::ParentElement::child)
            .child(surface_el)
            .when_some(value_slider_el, gpui_kit::ParentElement::child)
            .when_some(alpha_slider_el, gpui_kit::ParentElement::child)
            .child(v_flex().gap(px(6.0)).children(rows_el))
            .when(self.clipped, |d| {
                d.child(
                    div()
                        .text_size(px(10.5))
                        .text_color(muted_fg)
                        .child("That colour is outside sRGB, so it was mapped to the nearest displayable one."),
                )
            })
            .child(palette_el)
            .child(drag_layer)
    }
}

fn marker(x: f32, y: f32, d: f32, fill_color: Hsla) -> Div {
    div()
        .absolute()
        .left(px(x - d / 2.0))
        .top(px(y - d / 2.0))
        .size(px(d))
        .rounded_full()
        .border_2()
        .border_color(white())
        .bg(fill_color)
        .shadow_md()
}

fn checker() -> impl IntoElement {
    canvas(
        |_, _, _| (),
        |bounds, (), window, _| {
            window.paint_quad(fill(bounds, white()));
            let cell = 6.0f32;
            let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let dark = gpui_kit::rgb(0xd0d0d0);
            let (mut y, mut row) = (0.0f32, 0u32);
            while y < h {
                let mut x = if row % 2 == 0 { cell } else { 0.0 };
                while x < w {
                    let sz = size(px(cell.min(w - x)), px(cell.min(h - y)));
                    window.paint_quad(fill(Bounds::new(bounds.origin + point(px(x), px(y)), sz), dark));
                    x += cell * 2.0;
                }
                y += cell;
                row += 1;
            }
        },
    )
    .absolute()
    .size_full()
}

fn cie_overlay(locus: Arc<Vec<(f32, f32)>>, s: f32, line: Hsla, tri: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let (sc, xoff) = cie_xform(s);
            let at = |x: f32, y: f32| bounds.origin + point(px(xoff + x * sc), px(s - y * sc));

            let stroke = |window: &mut Window, pts: &[(f32, f32)], width: f32, color: Hsla| {
                let mut b = PathBuilder::stroke(px(width));
                for (i, &(x, y)) in pts.iter().enumerate() {
                    if i == 0 { b.move_to(at(x, y)) } else { b.line_to(at(x, y)) }
                }
                b.close();
                if let Ok(path) = b.build() {
                    window.paint_path(path, color);
                }
            };
            stroke(window, &locus, 1.5, line);
            stroke(window, &SRGB_PRIMARIES, 1.0, tri);

            let (wx, wy) = D65_XY;
            let r = 3.0;
            let dot = Bounds::centered_at(at(wx, wy), size(px(r * 2.0), px(r * 2.0)));
            window.paint_quad(PaintQuad { corner_radii: Corners::all(px(r)), ..fill(dot, tri) });
        },
    )
    .absolute()
    .size_full()
}

fn put(buf: &mut [u8], w: u32, x: u32, y: u32, rgb: Rgb, cov: f32) {
    let i = ((y * w + x) * 4) as usize;
    let q = |c: f32| (c.clamp(0.0, 1.0) * cov * 255.0 + 0.5) as u8;
    buf[i] = q(rgb[2]);
    buf[i + 1] = q(rgb[1]);
    buf[i + 2] = q(rgb[0]);
    buf[i + 3] = (cov * 255.0 + 0.5) as u8;
}

fn raster_wheel(n: u32) -> Raster {
    let mut bgra = vec![0u8; (n * n * 4) as usize];
    let c = n as f32 / 2.0;
    let rad = c - 1.0;
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let r = dx.hypot(dy);
            let cov = (rad + 0.5 - r).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let hue = (-dy).atan2(dx).to_degrees().rem_euclid(360.0);
            put(&mut bgra, n, x, y, hsv_to_rgb(hue, (r / rad).min(1.0), 1.0), cov);
        }
    }
    Raster { w: n, h: n, bgra }
}

fn raster_ring(n: u32) -> Raster {
    let mut bgra = vec![0u8; (n * n * 4) as usize];
    let c = n as f32 / 2.0;
    let (r_o, r_i) = (c - 1.0, (c - 1.0) * RING_INNER);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let r = dx.hypot(dy);
            let cov = (r_o + 0.5 - r).clamp(0.0, 1.0) * (r - r_i + 0.5).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let hue = (-dx).atan2(-dy).to_degrees().rem_euclid(360.0);
            put(&mut bgra, n, x, y, hsv_to_rgb(hue, 1.0, 1.0), cov);
        }
    }
    Raster { w: n, h: n, bgra }
}

fn raster_cie(n: u32, locus: &[(f32, f32)]) -> Raster {
    let nf = n as f32;
    let (sc, xoff) = cie_xform(nf);
    let poly: Vec<(f32, f32)> = locus.iter().map(|&(x, y)| (xoff + x * sc, nf - y * sc)).collect();
    let mut bgra = vec![0u8; (n * n * 4) as usize];
    let mut xs: Vec<f32> = Vec::with_capacity(8);

    for row in 0..n {
        let yc = row as f32 + 0.5;
        xs.clear();
        for i in 0..poly.len() {
            let (x0, y0) = poly[i];
            let (x1, y1) = poly[(i + 1) % poly.len()];
            if (y0 <= yc) != (y1 <= yc) {
                xs.push(x0 + (yc - y0) / (y1 - y0) * (x1 - x0));
            }
        }
        xs.sort_by(f32::total_cmp);
        for pair in xs.as_chunks::<2>().0 {
            let from = ((pair[0] - 0.5).ceil() as i32).max(0);
            let to = ((pair[1] - 0.5).floor() as i32).min(n as i32 - 1);
            for x in from..=to {
                let (cx, cy) = ((x as f32 + 0.5 - xoff) / sc, (nf - yc) / sc);
                put(&mut bgra, n, x as u32, row, xy_to_rgb(cx, cy), 1.0);
            }
        }
    }
    Raster { w: n, h: n, bgra }
}

fn to_image(r: Raster) -> Option<Arc<RenderImage>> {
    let buf = RgbaImage::from_raw(r.w, r.h, r.bgra)?;
    Some(Arc::new(RenderImage::new(SmallVec::<[Frame; 1]>::from_elem(Frame::new(buf), 1))))
}
