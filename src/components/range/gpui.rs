#![cfg(feature = "range")]
#![allow(
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
    clippy::manual_is_multiple_of,
    clippy::needless_range_loop,
    clippy::if_not_else,
    clippy::manual_memcpy
)]

use std::{cell::Cell, rc::Rc, time::Instant};

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, button::Button, h_flex, v_flex,
};
use gpui_kit::{
    App, Bounds, Context, DispatchPhase, EventEmitter, FocusHandle, Focusable, FontWeight,
    KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render,
    SharedString, Subscription, Window, actions, canvas, div, prelude::*, px,
};

actions!(range_slider, [Inc, Dec, IncLarge, DecLarge, ToStart, ToEnd]);

const THUMB_CONTEXT: &str = "RangeThumb";
const ZONE_H: f32 = 28.0;
const TAU_NUM: f32 = 0.06;
const TAU_GROW: f32 = 0.04;
const TAU_GHOST: f32 = 0.025;

/// Call once at startup, after `gpui_component::init(cx)`.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("left", Dec, Some(THUMB_CONTEXT)),
        KeyBinding::new("down", Dec, Some(THUMB_CONTEXT)),
        KeyBinding::new("right", Inc, Some(THUMB_CONTEXT)),
        KeyBinding::new("up", Inc, Some(THUMB_CONTEXT)),
        KeyBinding::new("pageup", IncLarge, Some(THUMB_CONTEXT)),
        KeyBinding::new("pagedown", DecLarge, Some(THUMB_CONTEXT)),
        KeyBinding::new("home", ToStart, Some(THUMB_CONTEXT)),
        KeyBinding::new("end", ToEnd, Some(THUMB_CONTEXT)),
    ]);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Thumb {
    Low,
    High,
}

impl Thumb {
    const fn ix(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Debug)]
pub enum RangeSliderEvent {
    /// Fired whenever either value changes (including mid-drag).
    Change { low: f32, high: f32 },
    /// Fired when an interaction finishes.
    ChangeEnd { low: f32, high: f32 },
}

/// Tunable parameters. Override a few with `..Default::default()`.
#[derive(Clone, Debug)]
pub struct RangeSliderConfig {
    pub label: SharedString,
    pub prefix: SharedString,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub default_low: f32,
    pub default_high: f32,
    /// Number of evenly spaced tick labels under the track.
    pub ticks: usize,
    pub track_height: f32,
    pub thumb_size: f32,
    pub thumb_hover_size: f32,
    /// Show the ghost span while hovering.
    pub hover_preview: bool,
    pub clear_button: bool,
}

impl Default for RangeSliderConfig {
    fn default() -> Self {
        Self {
            label: "Price Range".into(),
            prefix: "$".into(),
            min: 0.0,
            max: 1000.0,
            step: 10.0,
            default_low: 150.0,
            default_high: 650.0,
            ticks: 5,
            track_height: 8.0,
            thumb_size: 20.0,
            thumb_hover_size: 25.0,
            hover_preview: true,
            clear_button: true,
        }
    }
}

pub struct RangeSlider {
    config: RangeSliderConfig,
    low: f32,
    high: f32,
    /// Eased copies that drive the header numbers.
    shown_low: f32,
    shown_high: f32,

    drag: Option<Thumb>,
    hover_thumb: Option<Thumb>,
    preview: Option<f32>,
    /// Eased 0..1 growth per thumb.
    grow: [f32; 2],

    ghost_on: bool,
    ghost_l: f32,
    ghost_w: f32,

    labels: Vec<f32>,
    /// Written during prepaint; drives hit-testing and thumb layout.
    bounds: Rc<Cell<Bounds<Pixels>>>,
    focus: [FocusHandle; 2],
    animating: bool,
    last_frame: Option<Instant>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<RangeSliderEvent> for RangeSlider {}

impl Focusable for RangeSlider {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus[0].clone()
    }
}

fn group_digits(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3 + 1);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 {
        out.insert(0, '-');
    }
    out
}

impl RangeSlider {
    pub fn new(config: RangeSliderConfig, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = [
            cx.focus_handle().tab_stop(true),
            cx.focus_handle().tab_stop(true),
        ];
        let mut subs = Vec::new();
        for handle in &focus {
            subs.push(cx.on_focus(handle, window, |_, _, cx| cx.notify()));
            subs.push(cx.on_blur(handle, window, |_, _, cx| cx.notify()));
        }

        let mut this = Self {
            low: 0.0,
            high: 0.0,
            shown_low: 0.0,
            shown_high: 0.0,
            drag: None,
            hover_thumb: None,
            preview: None,
            grow: [0.0; 2],
            ghost_on: false,
            ghost_l: 0.0,
            ghost_w: 0.0,
            labels: Vec::new(),
            bounds: Rc::default(),
            focus,
            animating: false,
            last_frame: None,
            _subs: subs,
            config,
        };
        this.low = this.snap(this.config.default_low);
        this.high = this.snap(this.config.default_high).max(this.low);
        this.shown_low = this.low;
        this.shown_high = this.high;
        this.rebuild_labels();
        this
    }

    pub const fn range(&self) -> (f32, f32) {
        (self.low, self.high)
    }

    pub fn set_range(&mut self, low: f32, high: f32, cx: &mut Context<Self>) {
        let low = self.snap(low);
        let high = self.snap(high).max(low);
        if (low, high) != (self.low, self.high) {
            self.low = low;
            self.high = high;
            cx.emit(RangeSliderEvent::Change { low, high });
        }
        self.wake(cx);
    }

    pub fn set_config(&mut self, config: RangeSliderConfig, cx: &mut Context<Self>) {
        self.config = config;
        self.rebuild_labels();
        self.set_range(self.low, self.high, cx);
    }

    fn span(&self) -> f32 {
        (self.config.max - self.config.min).max(1e-9)
    }

    fn snap(&self, v: f32) -> f32 {
        let c = &self.config;
        if c.step > 0.0 {
            (((v - c.min) / c.step).round() * c.step + c.min).clamp(c.min, c.max)
        } else {
            v.clamp(c.min, c.max)
        }
    }

    fn decimals(&self) -> usize {
        let s = self.config.step.to_string();
        s.find('.').map_or(0, |i| s.len() - i - 1)
    }

    fn fmt(&self, v: f32) -> String {
        let d = self.decimals();
        if d == 0 {
            group_digits(v.round() as i64)
        } else {
            format!("{v:.d$}")
        }
    }

    fn rebuild_labels(&mut self) {
        let c = &self.config;
        let n = c.ticks.max(2);
        let mut labels: Vec<f32> = Vec::with_capacity(n);
        for i in 0..n {
            let raw = c.min + i as f32 * (c.max - c.min) / (n - 1) as f32;
            let rounded = if c.step > 0.0 {
                ((raw - c.min) / c.step).round() * c.step + c.min
            } else {
                raw
            };
            if !labels.contains(&rounded) {
                labels.push(rounded);
            }
        }
        self.labels = labels;
    }

    fn usable(&self) -> f32 {
        (f32::from(self.bounds.get().size.width) - self.config.thumb_size).max(1.0)
    }

    fn x_of(&self, v: f32) -> f32 {
        let t = ((v - self.config.min) / self.span()).clamp(0.0, 1.0);
        self.config.thumb_size / 2.0 + t * self.usable()
    }

    fn value_at(&self, pos: Point<Pixels>) -> f32 {
        let x = f32::from(pos.x - self.bounds.get().origin.x);
        let t = ((x - self.config.thumb_size / 2.0) / self.usable()).clamp(0.0, 1.0);
        self.snap(self.config.min + t * self.span())
    }

    fn wake(&mut self, cx: &mut Context<Self>) {
        if !self.animating {
            self.animating = true;
            self.last_frame = None;
        }
        cx.notify();
    }

    fn set_thumb(&mut self, which: Thumb, v: f32, cx: &mut Context<Self>) {
        let v = self.snap(v);
        let (low, high) = match which {
            Thumb::Low => (v.min(self.high), self.high),
            Thumb::High => (self.low, v.max(self.low)),
        };
        if (low, high) != (self.low, self.high) {
            self.low = low;
            self.high = high;
            cx.emit(RangeSliderEvent::Change { low, high });
        }
        self.wake(cx);
    }

    fn end(&self, cx: &mut Context<Self>) {
        cx.emit(RangeSliderEvent::ChangeEnd {
            low: self.low,
            high: self.high,
        });
    }

    fn on_down(&mut self, pos: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let v = self.value_at(pos);
        let which = if v < self.low {
            Thumb::Low
        } else if v > self.high {
            Thumb::High
        } else if v - self.low <= self.high - v {
            Thumb::Low
        } else {
            Thumb::High
        };
        window.focus(&self.focus[which.ix()], cx);
        self.drag = Some(which);
        self.preview = None;
        self.set_thumb(which, v, cx);
    }

    fn on_drag_move(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(which) = self.drag {
            let v = self.value_at(pos);
            self.set_thumb(which, v, cx);
        }
    }

    fn on_up(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
            self.end(cx);
            self.wake(cx);
        }
    }

    fn on_hover_move(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        if self.drag.is_some() || !self.config.hover_preview {
            return;
        }
        let v = self.value_at(pos);
        if self.preview != Some(v) {
            self.preview = Some(v);
            self.wake(cx);
        }
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        let (min, max) = (self.config.min, self.config.max);
        self.set_range(min, max, cx);
        self.end(cx);
    }

    fn nudge(&mut self, which: Thumb, steps: f32, cx: &mut Context<Self>) {
        let cur = if which == Thumb::Low {
            self.low
        } else {
            self.high
        };
        let step = if self.config.step > 0.0 {
            self.config.step
        } else {
            self.span() / 100.0
        };
        self.set_thumb(which, cur + steps * step, cx);
        self.end(cx);
    }

    fn jump(&mut self, which: Thumb, to_end: bool, cx: &mut Context<Self>) {
        let v = match (which, to_end) {
            (Thumb::Low, false) => self.config.min,
            (Thumb::Low, true) => self.high,
            (Thumb::High, false) => self.low,
            (Thumb::High, true) => self.config.max,
        };
        self.set_thumb(which, v, cx);
        self.end(cx);
    }

    fn ghost_target(&self) -> Option<(f32, f32)> {
        let p = self
            .preview
            .filter(|_| self.drag.is_none() && self.config.hover_preview)?;
        if p < self.low {
            let (a, b) = (self.x_of(p), self.x_of(self.low));
            Some((a, b - a))
        } else if p > self.high {
            let (a, b) = (self.x_of(self.high), self.x_of(p));
            Some((a, b - a))
        } else {
            None
        }
    }

    fn step(&mut self, window: &mut Window) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .replace(now)
            .map_or(1.0 / 60.0, |p| (now - p).as_secs_f32().min(0.05));
        let ease = |tau: f32| 1.0 - (-dt / tau).exp();

        self.shown_low += (self.low - self.shown_low) * ease(TAU_NUM);
        self.shown_high += (self.high - self.shown_high) * ease(TAU_NUM);

        let grow_target = [Thumb::Low, Thumb::High].map(|t| {
            if self.hover_thumb == Some(t) || self.drag == Some(t) {
                1.0
            } else {
                0.0
            }
        });
        let mut grow_moving = false;
        for i in 0..2 {
            self.grow[i] += (grow_target[i] - self.grow[i]) * ease(TAU_GROW);
            grow_moving |= (grow_target[i] - self.grow[i]).abs() > 0.005;
        }

        let mut ghost_moving = false;
        match self.ghost_target() {
            Some((l, w)) => {
                if !self.ghost_on {
                    self.ghost_l = l;
                    self.ghost_w = w;
                } else {
                    self.ghost_l += (l - self.ghost_l) * ease(TAU_GHOST);
                    self.ghost_w += (w - self.ghost_w) * ease(TAU_GHOST);
                    ghost_moving = (l - self.ghost_l).abs() > 0.2 || (w - self.ghost_w).abs() > 0.2;
                }
                self.ghost_on = true;
            }
            None => self.ghost_on = false,
        }

        let num_moving =
            (self.low - self.shown_low).abs() > 0.05 || (self.high - self.shown_high).abs() > 0.05;

        if num_moving || grow_moving || ghost_moving {
            window.request_animation_frame();
        } else {
            self.shown_low = self.low;
            self.shown_high = self.high;
            for i in 0..2 {
                self.grow[i] = grow_target[i];
            }
            self.animating = false;
            self.last_frame = None;
        }
    }

    fn render_thumb(
        &self,
        which: Thumb,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = &self.config;
        let theme = cx.theme();
        let i = which.ix();
        let value = if which == Thumb::Low {
            self.low
        } else {
            self.high
        };

        let size = c.thumb_size + (c.thumb_hover_size - c.thumb_size) * self.grow[i];
        let x = self.x_of(value) - size / 2.0;
        let focused = self.focus[i].is_focused(window);
        let border = if focused { theme.ring } else { theme.primary };

        div()
            .id(("range-thumb", i))
            .track_focus(&self.focus[i])
            .key_context(THUMB_CONTEXT)
            .absolute()
            .left(px(x))
            .top(px((ZONE_H - size) / 2.0))
            .size(px(size))
            .rounded_full()
            .border_2()
            .border_color(border)
            .bg(theme.background)
            .shadow_md()
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.hover_thumb = Some(which);
                } else if this.hover_thumb == Some(which) {
                    this.hover_thumb = None;
                }
                this.wake(cx);
            }))
            .on_action(cx.listener(move |this, _: &Inc, _, cx| this.nudge(which, 1.0, cx)))
            .on_action(cx.listener(move |this, _: &Dec, _, cx| this.nudge(which, -1.0, cx)))
            .on_action(cx.listener(move |this, _: &IncLarge, _, cx| this.nudge(which, 10.0, cx)))
            .on_action(cx.listener(move |this, _: &DecLarge, _, cx| this.nudge(which, -10.0, cx)))
            .on_action(cx.listener(move |this, _: &ToStart, _, cx| this.jump(which, false, cx)))
            .on_action(cx.listener(move |this, _: &ToEnd, _, cx| this.jump(which, true, cx)))
    }
}

impl Render for RangeSlider {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.animating {
            self.step(window);
        }

        let c = self.config.clone();
        let theme = cx.theme();
        let (fg, muted_fg, primary, track_bg) = (
            theme.foreground,
            theme.muted_foreground,
            theme.primary,
            theme.secondary,
        );

        let is_default = self.low == c.min && self.high == c.max;
        let track_top = (ZONE_H - c.track_height) / 2.0;
        let (x_low, x_high) = (self.x_of(self.low), self.x_of(self.high));

        let probe = self.bounds.clone();
        let entity = cx.entity();
        let dragging = self.drag.is_some();

        let amount = |v: f32| {
            div()
                .text_size(px(20.0))
                .font_weight(FontWeight::BOLD)
                .text_color(fg)
                .child(format!("{}{}", c.prefix, self.fmt(v)))
        };

        let zone = div()
            .id("range-zone")
            .relative()
            .w_full()
            .h(px(ZONE_H))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                    this.on_down(ev.position, window, cx)
                }),
            )
            .on_mouse_move(
                cx.listener(|this, ev: &MouseMoveEvent, _, cx| this.on_hover_move(ev.position, cx)),
            )
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !*hovered && this.preview.take().is_some() {
                    this.wake(cx);
                }
            }))
            .child(
                canvas(
                    {
                        let entity = entity.clone();
                        move |bounds, window, cx| {
                            let prev = probe.replace(bounds);
                            if (prev.size.width - bounds.size.width).abs() > px(0.5) {
                                let entity = entity;
                                window
                                    .defer(cx, move |_, cx| entity.update(cx, |_, cx| cx.notify()));
                            }
                        }
                    },
                    move |_, (), window, _| {
                        if !dragging {
                            return;
                        }
                        let e = entity.clone();
                        window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            e.update(cx, |this, cx| {
                                if ev.pressed_button == Some(MouseButton::Left) {
                                    this.on_drag_move(ev.position, cx);
                                } else {
                                    this.on_up(cx);
                                }
                            });
                        });
                        let e = entity;
                        window.on_mouse_event(move |ev: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture && ev.button == MouseButton::Left {
                                e.update(cx, |this, cx| this.on_up(cx));
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
            // Track.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(track_top))
                    .h(px(c.track_height))
                    .rounded_full()
                    .bg(track_bg),
            )
            // Selected range.
            .child(
                div()
                    .absolute()
                    .left(px(x_low))
                    .w(px((x_high - x_low).max(0.0)))
                    .top(px(track_top))
                    .h(px(c.track_height))
                    .rounded_full()
                    .bg(primary),
            )
            // Ghost preview.
            .when(self.ghost_on && self.ghost_w > 0.0, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(px(self.ghost_l))
                        .w(px(self.ghost_w))
                        .top(px(track_top))
                        .h(px(c.track_height))
                        .rounded_full()
                        .bg(primary.opacity(0.3)),
                )
            })
            .child(self.render_thumb(Thumb::Low, window, cx))
            .child(self.render_thumb(Thumb::High, window, cx));

        v_flex()
            .w_full()
            .max_w(px(384.0))
            .gap(px(20.0))
            .child(
                h_flex()
                    .items_start()
                    .justify_between()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(muted_fg)
                                    .child(c.label.to_uppercase()),
                            )
                            .child(
                                h_flex()
                                    .items_baseline()
                                    .gap(px(6.0))
                                    .child(amount(self.shown_low))
                                    .child(div().text_color(muted_fg).child("–"))
                                    .child(amount(self.shown_high)),
                            ),
                    )
                    .when(c.clear_button, |d| {
                        d.child(
                            Button::new("range-clear")
                                .outline()
                                .xsmall()
                                .icon(IconName::Close)
                                .label("Clear")
                                .disabled(is_default)
                                .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
                        )
                    }),
            )
            .child(
                v_flex().gap(px(8.0)).child(zone).child(
                    h_flex()
                        .justify_between()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(muted_fg.opacity(0.6))
                        .children(
                            self.labels
                                .iter()
                                .map(|&v| format!("{}{}", c.prefix, self.fmt(v))),
                        ),
                ),
            )
    }
}
