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

use std::sync::Arc;
use std::time::Instant;

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, button::Button, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use zopra::component;

pub use super::gpui::{RangeSliderConfig, RangeSliderEvent};

const THUMB_CONTEXT: &str = "RangeThumb";
const ZONE_H: f32 = 28.0;
const TAU_NUM: f32 = 0.06;
const TAU_GROW: f32 = 0.04;
const TAU_GHOST: f32 = 0.025;

pub use super::gpui::{Dec, DecLarge, Inc, IncLarge, ToEnd, ToStart};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RangeThumb {
    Low,
    High,
}

impl RangeThumb {
    #[must_use]
    pub const fn ix(self) -> usize {
        self as usize
    }
}

pub type RangeChangeCallback = Arc<dyn Fn(f32, f32, &mut Window, &mut App)>;

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

pub struct RangeSliderState {
    pub config: RangeSliderConfig,
    pub low: f32,
    pub high: f32,
    pub shown_low: f32,
    pub shown_high: f32,
    pub drag: Option<RangeThumb>,
    pub hover_thumb: Option<RangeThumb>,
    pub preview: Option<f32>,
    pub grow: [f32; 2],
    pub ghost_on: bool,
    pub ghost_l: f32,
    pub ghost_w: f32,
    pub labels: Vec<f32>,
    pub bounds: Bounds<Pixels>,
    pub focus: [FocusHandle; 2],
    pub animating: bool,
    pub last_frame: Option<Instant>,
}

impl RangeSliderState {
    #[must_use]
    pub fn new(config: RangeSliderConfig, cx: &mut Context<Self>) -> Self {
        let focus = [
            cx.focus_handle().tab_stop(true),
            cx.focus_handle().tab_stop(true),
        ];
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
            bounds: Bounds::default(),
            focus,
            animating: false,
            last_frame: None,
            config,
        };
        this.low = this.snap(this.config.default_low);
        this.high = this.snap(this.config.default_high).max(this.low);
        this.shown_low = this.low;
        this.shown_high = this.high;
        this.rebuild_labels();
        this
    }

    #[must_use]
    pub fn span(&self) -> f32 {
        (self.config.max - self.config.min).max(1e-9)
    }

    #[must_use]
    pub fn snap(&self, v: f32) -> f32 {
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

    #[must_use]
    pub fn fmt(&self, v: f32) -> String {
        let d = self.decimals();
        if d == 0 {
            group_digits(v.round() as i64)
        } else {
            format!("{v:.d$}")
        }
    }

    pub fn rebuild_labels(&mut self) {
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
        (f32::from(self.bounds.size.width) - self.config.thumb_size).max(1.0)
    }

    fn x_of(&self, v: f32) -> f32 {
        let c = &self.config;
        let frac = ((v - c.min) / self.span()).clamp(0.0, 1.0);
        c.thumb_size / 2.0 + frac * self.usable()
    }

    fn value_at(&self, pos: Point<Pixels>) -> f32 {
        let c = &self.config;
        let local_x = f32::from(pos.x - self.bounds.left());
        let x = local_x - c.thumb_size / 2.0;
        let frac = (x / self.usable()).clamp(0.0, 1.0);
        self.snap(c.min + frac * self.span())
    }

    fn wake(&mut self, cx: &mut Context<Self>) {
        if !self.animating {
            self.animating = true;
            self.last_frame = None;
        }
        cx.notify();
    }

    pub fn set_thumb(
        &mut self,
        which: RangeThumb,
        v: f32,
        on_change: Option<&RangeChangeCallback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let v = self.snap(v);
        let (low, high) = match which {
            RangeThumb::Low => (v.min(self.high), self.high),
            RangeThumb::High => (self.low, v.max(self.low)),
        };
        if (low, high) != (self.low, self.high) {
            self.low = low;
            self.high = high;
            if let Some(cb) = on_change {
                cb(low, high, window, cx);
            }
        }
        self.wake(cx);
    }

    pub fn on_down(
        &mut self,
        pos: Point<Pixels>,
        on_change: Option<&RangeChangeCallback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let v = self.value_at(pos);
        let (d_low, d_high) = ((v - self.low).abs(), (v - self.high).abs());
        let which = if d_low < d_high {
            RangeThumb::Low
        } else if d_high < d_low {
            RangeThumb::High
        } else if v < self.low {
            RangeThumb::Low
        } else {
            RangeThumb::High
        };
        window.focus(&self.focus[which.ix()], cx);
        self.drag = Some(which);
        self.preview = None;
        self.set_thumb(which, v, on_change, window, cx);
    }

    pub fn on_drag_move(
        &mut self,
        pos: Point<Pixels>,
        on_change: Option<&RangeChangeCallback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(which) = self.drag {
            let v = self.value_at(pos);
            self.set_thumb(which, v, on_change, window, cx);
        }
    }

    pub fn on_up(
        &mut self,
        on_end: Option<&RangeChangeCallback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.drag.take().is_some() {
            if let Some(cb) = on_end {
                cb(self.low, self.high, window, cx);
            }
            self.wake(cx);
        }
    }

    pub fn on_hover_move(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        if self.drag.is_some() || !self.config.hover_preview {
            return;
        }
        let v = self.value_at(pos);
        if self.preview != Some(v) {
            self.preview = Some(v);
            self.wake(cx);
        }
    }

    pub fn clear(
        &mut self,
        on_change: Option<&RangeChangeCallback>,
        on_end: Option<&RangeChangeCallback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (min, max) = (self.config.min, self.config.max);
        self.low = min;
        self.high = max;
        if let Some(cb) = on_change {
            cb(min, max, window, cx);
        }
        if let Some(cb) = on_end {
            cb(min, max, window, cx);
        }
        self.wake(cx);
    }

    pub fn nudge(
        &mut self,
        which: RangeThumb,
        steps: f32,
        on_change: Option<&RangeChangeCallback>,
        on_end: Option<&RangeChangeCallback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cur = if which == RangeThumb::Low {
            self.low
        } else {
            self.high
        };
        let step = if self.config.step > 0.0 {
            self.config.step
        } else {
            self.span() / 100.0
        };
        self.set_thumb(which, cur + steps * step, on_change, window, cx);
        if let Some(cb) = on_end {
            cb(self.low, self.high, window, cx);
        }
    }

    pub fn jump(
        &mut self,
        which: RangeThumb,
        to_end: bool,
        on_change: Option<&RangeChangeCallback>,
        on_end: Option<&RangeChangeCallback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let v = match (which, to_end) {
            (RangeThumb::Low, false) => self.config.min,
            (RangeThumb::Low, true) => self.high,
            (RangeThumb::High, false) => self.low,
            (RangeThumb::High, true) => self.config.max,
        };
        self.set_thumb(which, v, on_change, window, cx);
        if let Some(cb) = on_end {
            cb(self.low, self.high, window, cx);
        }
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

    pub fn step(&mut self, window: &mut Window) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .replace(now)
            .map_or(1.0 / 60.0, |p| (now - p).as_secs_f32().min(0.05));
        let ease = |tau: f32| 1.0 - (-dt / tau).exp();

        self.shown_low += (self.low - self.shown_low) * ease(TAU_NUM);
        self.shown_high += (self.high - self.shown_high) * ease(TAU_NUM);

        let grow_target = [RangeThumb::Low, RangeThumb::High].map(|t| {
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
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[component]
pub fn range_slider(
    low: Option<f32>,
    high: Option<f32>,
    config: Option<RangeSliderConfig>,
    min: Option<f32>,
    max: Option<f32>,
    step: Option<f32>,
    label: Option<SharedString>,
    prefix: Option<SharedString>,
    ticks: Option<usize>,
    clear_button: Option<bool>,
    hover_preview: Option<bool>,
    on_change: Option<RangeChangeCallback>,
    on_end: Option<RangeChangeCallback>,
) {
    let mut cfg = config.unwrap_or_default();
    if let Some(m) = min {
        cfg.min = m;
    }
    if let Some(m) = max {
        cfg.max = m;
    }
    if let Some(s) = step {
        cfg.step = s;
    }
    if let Some(l) = label {
        cfg.label = l;
    }
    if let Some(p) = prefix {
        cfg.prefix = p;
    }
    if let Some(t) = ticks {
        cfg.ticks = t;
    }
    if let Some(c) = clear_button {
        cfg.clear_button = c;
    }
    if let Some(h) = hover_preview {
        cfg.hover_preview = h;
    }
    if let Some(l) = low {
        cfg.default_low = l;
    }
    if let Some(h) = high {
        cfg.default_high = h;
    }

    let state_holder = window.use_state(cx, |_window, cx| {
        cx.new(|cx| RangeSliderState::new(cfg.clone(), cx))
    });

    let state = state_holder.read(cx).clone();

    if let (Some(ext_low), Some(ext_high)) = (low, high) {
        let is_dragging = state.read(cx).drag.is_some();
        let (cur_low, cur_high) = (state.read(cx).low, state.read(cx).high);
        if !is_dragging
            && ((ext_low - cur_low).abs() > f32::EPSILON
                || (ext_high - cur_high).abs() > f32::EPSILON)
        {
            state.update(cx, |this, cx| {
                let s_low = this.snap(ext_low);
                let s_high = this.snap(ext_high).max(s_low);
                if (s_low, s_high) != (this.low, this.high) {
                    this.low = s_low;
                    this.high = s_high;
                    this.wake(cx);
                }
            });
        }
    }

    if state.read(cx).animating {
        state.update(cx, |this, _cx| {
            this.step(window);
        });
    }

    let c = state.read(cx).config.clone();
    let theme = cx.theme();
    let (fg, muted_fg, primary, track_bg) = (
        theme.foreground,
        theme.muted_foreground,
        theme.primary,
        theme.secondary,
    );

    let is_default = state.read(cx).low == c.min && state.read(cx).high == c.max;
    let track_top = (ZONE_H - c.track_height) / 2.0;
    let (x_low, x_high) = (
        state.read(cx).x_of(state.read(cx).low),
        state.read(cx).x_of(state.read(cx).high),
    );

    let state_probe = state.clone();
    let state_down = state.clone();
    let state_move = state.clone();
    let state_hover = state.clone();
    let state_clear = state.clone();

    let on_change_down = on_change.clone();
    let on_change_clear = on_change.clone();
    let on_end_clear = on_end.clone();

    let dragging = state.read(cx).drag.is_some();
    let ghost_on = state.read(cx).ghost_on;
    let ghost_l = state.read(cx).ghost_l;
    let ghost_w = state.read(cx).ghost_w;

    let shown_low_val = state.read(cx).shown_low;
    let shown_high_val = state.read(cx).shown_high;
    let fmt_low = state.read(cx).fmt(shown_low_val);
    let fmt_high = state.read(cx).fmt(shown_high_val);
    let formatted_labels: Vec<String> = state
        .read(cx)
        .labels
        .iter()
        .map(|&v| format!("{}{}", c.prefix, state.read(cx).fmt(v)))
        .collect();
    let prefix_str = c.prefix.clone();

    let render_thumb_el = |which: RangeThumb, window: &Window, cx: &App| {
        let i = which.ix();
        let value = if which == RangeThumb::Low {
            state.read(cx).low
        } else {
            state.read(cx).high
        };
        let size = c.thumb_size + (c.thumb_hover_size - c.thumb_size) * state.read(cx).grow[i];
        let x = state.read(cx).x_of(value) - size / 2.0;
        let focused = state.read(cx).focus[i].is_focused(window);
        let border = if focused { theme.ring } else { theme.primary };

        let state_thumb_hover = state.clone();
        let state_inc = state.clone();
        let state_dec = state.clone();
        let state_inc_lg = state.clone();
        let state_dec_lg = state.clone();
        let state_start = state.clone();
        let state_end = state.clone();

        let oc_inc = on_change.clone();
        let oe_inc = on_end.clone();
        let oc_dec = on_change.clone();
        let oe_dec = on_end.clone();
        let oc_inc_lg = on_change.clone();
        let oe_inc_lg = on_end.clone();
        let oc_dec_lg = on_change.clone();
        let oe_dec_lg = on_end.clone();
        let oc_start = on_change.clone();
        let oe_start = on_end.clone();
        let oc_end = on_change.clone();
        let oe_end = on_end.clone();

        div()
            .id(("range-thumb", i))
            .track_focus(&state.read(cx).focus[i])
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
            .on_hover(move |hovered: &bool, _window, cx| {
                state_thumb_hover.update(cx, |this, cx| {
                    if *hovered {
                        this.hover_thumb = Some(which);
                    } else if this.hover_thumb == Some(which) {
                        this.hover_thumb = None;
                    }
                    this.wake(cx);
                });
            })
            .on_action(move |_: &Inc, window, cx| {
                state_inc.update(cx, |this, cx| {
                    this.nudge(which, 1.0, oc_inc.as_ref(), oe_inc.as_ref(), window, cx);
                });
            })
            .on_action(move |_: &Dec, window, cx| {
                state_dec.update(cx, |this, cx| {
                    this.nudge(which, -1.0, oc_dec.as_ref(), oe_dec.as_ref(), window, cx);
                });
            })
            .on_action(move |_: &IncLarge, window, cx| {
                state_inc_lg.update(cx, |this, cx| {
                    this.nudge(
                        which,
                        10.0,
                        oc_inc_lg.as_ref(),
                        oe_inc_lg.as_ref(),
                        window,
                        cx,
                    );
                });
            })
            .on_action(move |_: &DecLarge, window, cx| {
                state_dec_lg.update(cx, |this, cx| {
                    this.nudge(
                        which,
                        -10.0,
                        oc_dec_lg.as_ref(),
                        oe_dec_lg.as_ref(),
                        window,
                        cx,
                    );
                });
            })
            .on_action(move |_: &ToStart, window, cx| {
                state_start.update(cx, |this, cx| {
                    this.jump(
                        which,
                        false,
                        oc_start.as_ref(),
                        oe_start.as_ref(),
                        window,
                        cx,
                    );
                });
            })
            .on_action(move |_: &ToEnd, window, cx| {
                state_end.update(cx, |this, cx| {
                    this.jump(which, true, oc_end.as_ref(), oe_end.as_ref(), window, cx);
                });
            })
    };

    let thumb_low = render_thumb_el(RangeThumb::Low, window, cx);
    let thumb_high = render_thumb_el(RangeThumb::High, window, cx);

    let zone = div()
        .id("range-zone")
        .relative()
        .w_full()
        .h(px(ZONE_H))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, window, cx| {
            state_down.update(cx, |this, cx| {
                this.on_down(ev.position, on_change_down.as_ref(), window, cx);
            });
        })
        .on_mouse_move(move |ev: &MouseMoveEvent, _window, cx| {
            state_move.update(cx, |this, cx| {
                this.on_hover_move(ev.position, cx);
            });
        })
        .on_hover(move |hovered: &bool, _window, cx| {
            if !*hovered {
                state_hover.update(cx, |this, cx| {
                    if this.preview.take().is_some() {
                        this.wake(cx);
                    }
                });
            }
        })
        .child(
            canvas(
                move |bounds, window, cx| {
                    let prev_width = state_probe.read(cx).bounds.size.width;
                    state_probe.update(cx, |this, _cx| {
                        this.bounds = bounds;
                    });
                    if (prev_width - bounds.size.width).abs() > px(0.5) {
                        let s = state_probe.clone();
                        window.defer(cx, move |_, cx| {
                            s.update(cx, |_, cx| cx.notify());
                        });
                    }
                },
                move |_, (), window, _cx| {
                    if !dragging {
                        return;
                    }
                    let e_move = state.clone();
                    let on_change_m = on_change.clone();
                    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| {
                        if phase != DispatchPhase::Capture {
                            return;
                        }
                        e_move.update(cx, |this, cx| {
                            if ev.pressed_button == Some(MouseButton::Left) {
                                this.on_drag_move(ev.position, on_change_m.as_ref(), window, cx);
                            } else {
                                this.on_up(None, window, cx);
                            }
                        });
                    });

                    let e_up = state.clone();
                    let on_end_u = on_end.clone();
                    window.on_mouse_event(move |ev: &MouseUpEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture && ev.button == MouseButton::Left {
                            e_up.update(cx, |this, cx| {
                                this.on_up(on_end_u.as_ref(), window, cx);
                            });
                        }
                    });
                },
            )
            .absolute()
            .size_full(),
        )
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
        .when(ghost_on && ghost_w > 0.0, |d| {
            d.child(
                div()
                    .absolute()
                    .left(px(ghost_l))
                    .w(px(ghost_w))
                    .top(px(track_top))
                    .h(px(c.track_height))
                    .rounded_full()
                    .bg(primary.opacity(0.3)),
            )
        })
        .child(thumb_low)
        .child(thumb_high);

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
                                .child(
                                    div()
                                        .text_size(px(20.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(fg)
                                        .child(format!("{prefix_str}{fmt_low}")),
                                )
                                .child(div().text_color(muted_fg).child("–"))
                                .child(
                                    div()
                                        .text_size(px(20.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(fg)
                                        .child(format!("{prefix_str}{fmt_high}")),
                                ),
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
                            .on_click(move |_, window, cx| {
                                state_clear.update(cx, |this, cx| {
                                    this.clear(
                                        on_change_clear.as_ref(),
                                        on_end_clear.as_ref(),
                                        window,
                                        cx,
                                    );
                                });
                            }),
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
                    .children(formatted_labels),
            ),
        )
}
