#![cfg(feature = "status-bars")]
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
    clippy::manual_midpoint,
    clippy::unused_self,
    clippy::redundant_closure,
    clippy::map_unwrap_or
)]

use std::f32::consts::TAU;
use std::time::Instant;

use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use zopra::component;

pub use super::gpui::{
    day_status, day_uptime, fill_status_days, init as init_status_bars, status_today,
    StatusBars as GpuiStatusBars, StatusBarsConfig, StatusDay, StatusIncident, StatusLevel,
};
use super::{Dismiss, First, Last, Next, NextWeek, Prev, PrevWeek};

const CONTEXT: &str = "StatusBars";
const TAU_TIP: f32 = 0.05;
const TAU_DIM: f32 = 0.06;
const TAU_X: f32 = 0.04;
const TAU_UPTIME: f32 = 0.12;
const PING_PERIOD: f32 = 1.5;
const LOAD_PERIOD: f32 = 1.6;

fn fmt_duration(m: u32) -> String {
    if m >= 60 {
        match m % 60 {
            0 => format!("{}h", m / 60),
            r => format!("{}h {}m", m / 60, r),
        }
    } else {
        format!("{}m", m.max(1))
    }
}

fn fmt_percent(v: f32) -> String {
    let floored = ((v * 100.0) + 1e-3).floor() / 100.0;
    let s = format!("{floored:.2}");
    format!("{}%", s.trim_end_matches('0').trim_end_matches('.'))
}

fn word(s: StatusLevel) -> &'static str {
    match s {
        StatusLevel::Up => "Operational",
        StatusLevel::Degraded => "Degraded",
        StatusLevel::Down => "Outage",
        StatusLevel::None => "No data",
    }
}

fn current_word(s: StatusLevel) -> &'static str {
    match s {
        StatusLevel::Degraded => "Degraded performance",
        other => word(other),
    }
}

struct Palette {
    up: Hsla,
    degraded: Hsla,
    down: Hsla,
    empty: Hsla,
    fg: Hsla,
    muted_fg: Hsla,
    border: Hsla,
    popover: Hsla,
    popover_fg: Hsla,
}

impl Palette {
    fn bar(&self, s: StatusLevel) -> Hsla {
        match s {
            StatusLevel::Up => self.up.opacity(0.75),
            StatusLevel::Degraded => self.degraded,
            StatusLevel::Down => self.down,
            StatusLevel::None => self.empty,
        }
    }
    fn dot(&self, s: StatusLevel) -> Hsla {
        match s {
            StatusLevel::Up => self.up,
            StatusLevel::Degraded => self.degraded,
            StatusLevel::Down => self.down,
            StatusLevel::None => self.muted_fg.opacity(0.5),
        }
    }
}

pub struct StatusBarsState {
    pub shown: Vec<Option<usize>>,
    pub shown_count: usize,
    pub dirty: bool,
    pub uptime: Option<f32>,
    pub uptime_shown: f32,

    pub width: f32,
    pub strip_bounds: Bounds<Pixels>,

    pub active: Option<usize>,
    pub tip_index: usize,
    pub keyboard: bool,
    pub tip_alpha: f32,
    pub dim: f32,
    pub tip_x: f32,
    pub tip_target_x: f32,

    pub clock: f32,
    pub animating: bool,
    pub last_frame: Option<Instant>,
    pub focus_handle: FocusHandle,
}

impl StatusBarsState {
    #[must_use]
    pub fn new(cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle().tab_stop(true);
        Self {
            shown: Vec::new(),
            shown_count: usize::MAX,
            dirty: true,
            uptime: None,
            uptime_shown: 0.0,
            width: 0.0,
            strip_bounds: Bounds::default(),
            active: None,
            tip_index: 0,
            keyboard: false,
            tip_alpha: 0.0,
            dim: 0.0,
            tip_x: 0.0,
            tip_target_x: 0.0,
            clock: 0.0,
            animating: false,
            last_frame: None,
            focus_handle,
        }
    }

    pub fn visible_count(&self, max_days: usize, min_bar_width: f32, gap: f32) -> usize {
        let fits = if self.width > 0.0 {
            ((self.width + gap) / (min_bar_width + gap)).floor() as usize
        } else {
            max_days
        };
        [max_days, 60, 30]
            .into_iter()
            .filter(|&n| n <= max_days)
            .find(|&n| n <= fits)
            .unwrap_or_else(|| fits.max(1))
    }

    pub fn rebuild(&mut self, days: &[StatusDay], count: usize) {
        let tail_start = days.len().saturating_sub(count);
        let pad = count.saturating_sub(days.len() - tail_start);
        self.shown = std::iter::repeat_n(None, pad)
            .chain((tail_start..days.len()).map(Some))
            .collect();
        self.shown_count = count;
        self.dirty = false;

        let (sum, n) = self
            .shown
            .iter()
            .flatten()
            .map(|&i| &days[i])
            .filter(|d| day_status(d) != StatusLevel::None)
            .fold((0.0f32, 0usize), |(s, n), d| (s + day_uptime(d), n + 1));
        let target = (n > 0).then(|| sum / n as f32);
        if self.uptime.is_none() {
            self.uptime_shown = target.unwrap_or(0.0);
        }
        self.uptime = target;
    }

    pub fn wake(&mut self, cx: &mut Context<Self>) {
        if !self.animating {
            self.animating = true;
            self.last_frame = None;
        }
        cx.notify();
    }

    pub fn point(&mut self, index: usize, instant: bool, gap: f32, cx: &mut Context<Self>) {
        let n = self.shown.len().max(1) as f32;
        let x = (index as f32 + 0.5) * (self.width + gap) / n - gap / 2.0;
        self.tip_target_x = x;
        if instant || self.active.is_none() {
            self.tip_x = x;
        }
        self.active = Some(index);
        self.tip_index = index;
        self.wake(cx);
    }

    pub fn hide(&mut self, cx: &mut Context<Self>) {
        self.active = None;
        self.keyboard = false;
        self.wake(cx);
    }

    pub fn on_focus(&mut self, loading: bool, gap: f32, cx: &mut Context<Self>) {
        if loading || self.shown.is_empty() {
            return;
        }
        self.keyboard = true;
        let last = self.shown.len() - 1;
        self.point(self.active.unwrap_or(last), true, gap, cx);
    }

    pub fn nav(&mut self, loading: bool, gap: f32, cx: &mut Context<Self>, to: impl FnOnce(usize, usize) -> usize) {
        if loading || self.shown.is_empty() {
            return;
        }
        let last = self.shown.len() - 1;
        let from = self.active.unwrap_or(last);
        self.keyboard = true;
        self.point(to(from, last).min(last), true, gap, cx);
    }

    fn ping_active(&self, now: StatusLevel, pulse: bool, loading: bool) -> bool {
        pulse && !loading && matches!(now, StatusLevel::Down | StatusLevel::Degraded)
    }

    pub fn step(&mut self, now_status: StatusLevel, pulse: bool, loading: bool, window: &mut Window) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .replace(now)
            .map_or(1.0 / 60.0, |p| (now - p).as_secs_f32().min(0.05));
        let ease = |tau: f32| 1.0 - (-dt / tau).exp();
        self.clock += dt;

        let hovering = self.active.is_some() && !loading;
        let (tip_t, dim_t) = if hovering { (1.0, 1.0) } else { (0.0, 0.0) };
        self.tip_alpha += (tip_t - self.tip_alpha) * ease(TAU_TIP);
        self.dim += (dim_t - self.dim) * ease(TAU_DIM);
        self.tip_x += (self.tip_target_x - self.tip_x) * ease(TAU_X);
        let uptime_t = self.uptime.unwrap_or(self.uptime_shown);
        self.uptime_shown += (uptime_t - self.uptime_shown) * ease(TAU_UPTIME);

        let moving = (tip_t - self.tip_alpha).abs() > 0.01
            || (dim_t - self.dim).abs() > 0.01
            || (self.tip_target_x - self.tip_x).abs() > 0.1
            || (uptime_t - self.uptime_shown).abs() > 0.0005
            || loading
            || self.ping_active(now_status, pulse, loading);

        if moving {
            window.request_animation_frame();
        } else {
            self.tip_alpha = tip_t;
            self.dim = dim_t;
            self.tip_x = self.tip_target_x;
            self.uptime_shown = uptime_t;
            self.animating = false;
            self.last_frame = None;
        }
    }
}

/// A reactive status bars component displaying daily operational history, uptime, and incident tooltips.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[component]
pub fn status_bars(
    label: Option<SharedString>,
    days: Option<Vec<StatusDay>>,
    config: Option<StatusBarsConfig>,
    current: Option<StatusLevel>,
    max_days: Option<usize>,
    min_bar_width: Option<f32>,
    compact: Option<bool>,
    loading: Option<bool>,
    bar_height: Option<f32>,
    gap: Option<f32>,
    tooltip_width: Option<f32>,
    pulse: Option<bool>,
    up_color: Option<Hsla>,
    degraded_color: Option<Hsla>,
    down_color: Option<Hsla>,
    empty_color: Option<Hsla>,
) {
    let mut cfg = config.unwrap_or_default();
    if let Some(m) = max_days { cfg.max_days = m; }
    if let Some(w) = min_bar_width { cfg.min_bar_width = w; }
    if let Some(c) = compact { cfg.compact = c; }
    if let Some(l) = loading { cfg.loading = l; }
    if let Some(h) = bar_height { cfg.bar_height = h; }
    if let Some(g) = gap { cfg.gap = g; }
    if let Some(t) = tooltip_width { cfg.tooltip_width = t; }
    if let Some(p) = pulse { cfg.pulse = p; }
    if let Some(u) = up_color { cfg.up_color = u; }
    if let Some(d) = degraded_color { cfg.degraded_color = d; }
    if let Some(d) = down_color { cfg.down_color = d; }
    if let Some(e) = empty_color { cfg.empty_color = Some(e); }

    let all_days = days.unwrap_or_default();
    let lbl = label.unwrap_or_else(|| SharedString::from("System Status"));

    let state_holder = window.use_state(cx, |_window, cx| {
        cx.new(|cx| StatusBarsState::new(cx))
    });

    let state = state_holder.read(cx).clone();

    let now_status = current
        .or_else(|| all_days.last().map(day_status))
        .unwrap_or(StatusLevel::None);

    let count = state.read(cx).visible_count(cfg.max_days, cfg.min_bar_width, cfg.gap);
    if state.read(cx).dirty || count != state.read(cx).shown_count {
        state.update(cx, |this, _cx| {
            this.rebuild(&all_days, count);
        });
    }

    if state.read(cx).animating {
        state.update(cx, |this, _cx| {
            this.step(now_status, cfg.pulse, cfg.loading, window);
        });
    }

    let theme = cx.theme();
    let st = Palette {
        up: cfg.up_color,
        degraded: cfg.degraded_color,
        down: cfg.down_color,
        empty: cfg.empty_color.unwrap_or(theme.muted),
        fg: theme.foreground,
        muted_fg: theme.muted_foreground,
        border: theme.border,
        popover: theme.popover,
        popover_fg: theme.popover_foreground,
    };

    let ping_active = cfg.pulse && !cfg.loading && matches!(now_status, StatusLevel::Down | StatusLevel::Degraded);
    let ping = ping_active.then(|| {
        let p = (state.read(cx).clock % PING_PERIOD) / PING_PERIOD;
        let eased = 1.0 - (1.0 - p).powi(3);
        let size = 6.0 + 10.0 * eased;
        div()
            .absolute()
            .top(px(4.0 - size / 2.0))
            .left(px(4.0 - size / 2.0))
            .size(px(size))
            .rounded_full()
            .bg(st.dot(now_status).opacity(0.75 * (1.0 - p)))
    });

    let dot = if cfg.loading { st.muted_fg.opacity(0.5) } else { st.dot(now_status) };

    let header = h_flex()
        .mb(px(10.0))
        .justify_between()
        .gap_3()
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .child(lbl),
        )
        .child(
            h_flex()
                .flex_none()
                .gap(px(6.0))
                .text_size(px(12.0))
                .text_color(st.muted_fg)
                .child(
                    div()
                        .relative()
                        .size(px(8.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when_some(ping, |d, ring| d.child(ring))
                        .child(div().size(px(6.0)).rounded_full().bg(dot)),
                )
                .child(if cfg.loading { "Checking…" } else { current_word(now_status) }),
        );

    let first = state.read(cx)
        .shown
        .first()
        .copied()
        .flatten()
        .map(|i| all_days[i].date.format("%b %-d").to_string())
        .unwrap_or_else(|| format!("{count} days ago"));

    let uptime_txt = if cfg.loading {
        "Loading…".to_string()
    } else if state.read(cx).uptime.is_none() {
        "No data yet".to_string()
    } else {
        format!("{} uptime", fmt_percent(state.read(cx).uptime_shown))
    };

    let rule = || div().h(px(1.0)).flex_1().bg(st.border);

    let footer = h_flex()
        .mt(px(8.0))
        .gap_3()
        .text_size(px(11.0))
        .text_color(st.muted_fg.opacity(0.8))
        .child(div().flex_none().child(first))
        .child(rule())
        .child(div().flex_none().text_color(st.muted_fg).child(uptime_txt))
        .child(rule())
        .child(div().flex_none().child("Today"));

    let pulse_loading = if cfg.loading {
        0.55 + 0.25 * (state.read(cx).clock * TAU / LOAD_PERIOD).sin()
    } else {
        1.0
    };

    let bars: Vec<_> = state.read(cx)
        .shown
        .iter()
        .enumerate()
        .map(|(i, slot)| {
            let status = slot.map_or(StatusLevel::None, |ix| day_status(&all_days[ix]));
            let is_active = state.read(cx).active == Some(i);
            let color = if cfg.loading { st.empty.opacity(pulse_loading) } else { st.bar(status) };
            div()
                .flex_1()
                .min_w_0()
                .h(px(if is_active { cfg.bar_height * 1.08 } else { cfg.bar_height }))
                .rounded(px(2.0))
                .bg(color)
                .opacity(if is_active { 1.0 } else { 1.0 - 0.65 * state.read(cx).dim })
        })
        .collect();

    let tip = (state.read(cx).tip_alpha > 0.01 && !state.read(cx).shown.is_empty()).then(|| {
        let w = cfg.tooltip_width;
        let frac = if state.read(cx).width > 0.0 {
            (state.read(cx).tip_x / state.read(cx).width).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let day = state.read(cx)
            .shown
            .get(state.read(cx).tip_index)
            .copied()
            .flatten()
            .map(|i| &all_days[i]);
        let status = day.map_or(StatusLevel::None, day_status);

        let date_txt = day.map_or("Before tracking".to_string(), |d| d.date.format("%a, %b %-d").to_string());
        let extra = day.and_then(|d| {
            if !d.incidents.is_empty() {
                return None;
            }
            match status {
                StatusLevel::Down => d.downtime.map(|m| format!("· {} down", fmt_duration(m))),
                StatusLevel::Degraded => d.degraded.map(|m| format!("· {}", fmt_duration(m))),
                _ => None,
            }
        });

        let tip_content = v_flex()
            .w(px(cfg.tooltip_width))
            .px(px(10.0))
            .py(px(8.0))
            .rounded_lg()
            .border_1()
            .border_color(st.border)
            .bg(st.popover)
            .text_color(st.popover_fg)
            .text_size(px(12.0))
            .shadow_lg()
            .opacity(state.read(cx).tip_alpha)
            .child(
                h_flex()
                    .justify_between()
                    .gap_4()
                    .child(div().font_weight(FontWeight::MEDIUM).child(date_txt))
                    .when(status != StatusLevel::None, |d| {
                        d.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(st.muted_fg)
                                .child(fmt_percent(day.map_or(0.0, day_uptime))),
                        )
                    }),
            )
            .child(
                h_flex()
                    .mt(px(4.0))
                    .gap(px(6.0))
                    .text_color(st.muted_fg)
                    .child(div().flex_none().size(px(6.0)).rounded_full().bg(st.dot(status)))
                    .child(word(status))
                    .when_some(extra, |d, e| d.child(div().text_color(st.muted_fg.opacity(0.8)).child(e))),
            )
            .when(status == StatusLevel::Up && day.is_some_and(|d| d.incidents.is_empty()), |d| {
                d.child(div().mt(px(4.0)).text_color(st.muted_fg.opacity(0.8)).child("No incidents"))
            })
            .children(day.into_iter().flat_map(|d| d.incidents.iter()).map(|inc| {
                div()
                    .mt(px(6.0))
                    .pt(px(6.0))
                    .border_t_1()
                    .border_color(st.border)
                    .text_color(st.muted_fg)
                    .child(match inc.minutes {
                        Some(m) => format!("{} · {}", inc.title, fmt_duration(m)),
                        None => inc.title.to_string(),
                    })
            }));

        div()
            .absolute()
            .bottom(relative(1.0))
            .mb(px(10.0))
            .left(px(state.read(cx).tip_x - frac * w))
            .child(tip_content)
    });

    let state_canvas = state.clone();
    let state_mouse = state.clone();
    let state_hover = state.clone();
    let state_actions = state.clone();
    let gap_val = cfg.gap;
    let loading_val = cfg.loading;
    let focus_handle = state.read(cx).focus_handle.clone();

    let strip = h_flex()
        .id("status-strip")
        .key_context(CONTEXT)
        .track_focus(&focus_handle)
        .relative()
        .h(px(cfg.bar_height))
        .items_end()
        .gap(px(cfg.gap))
        .on_action({
            let state = state_actions.clone();
            move |_: &Prev, _window, cx| {
                state.update(cx, |this, cx| this.nav(loading_val, gap_val, cx, |f, _| f.saturating_sub(1)));
            }
        })
        .on_action({
            let state = state_actions.clone();
            move |_: &Next, _window, cx| {
                state.update(cx, |this, cx| this.nav(loading_val, gap_val, cx, |f, _| f + 1));
            }
        })
        .on_action({
            let state = state_actions.clone();
            move |_: &PrevWeek, _window, cx| {
                state.update(cx, |this, cx| this.nav(loading_val, gap_val, cx, |f, _| f.saturating_sub(7)));
            }
        })
        .on_action({
            let state = state_actions.clone();
            move |_: &NextWeek, _window, cx| {
                state.update(cx, |this, cx| this.nav(loading_val, gap_val, cx, |f, _| f + 7));
            }
        })
        .on_action({
            let state = state_actions.clone();
            move |_: &First, _window, cx| {
                state.update(cx, |this, cx| this.nav(loading_val, gap_val, cx, |_, _| 0));
            }
        })
        .on_action({
            let state = state_actions.clone();
            move |_: &Last, _window, cx| {
                state.update(cx, |this, cx| this.nav(loading_val, gap_val, cx, |_, l| l));
            }
        })
        .on_action({
            let state = state_actions;
            move |_: &Dismiss, _window, cx| {
                state.update(cx, |this, cx| this.hide(cx));
            }
        })
        .on_mouse_move(move |ev: &MouseMoveEvent, _window, cx| {
            let n = state_mouse.read(cx).shown.len();
            let b = state_mouse.read(cx).strip_bounds;
            let w = f32::from(b.size.width);
            if loading_val || n == 0 || w <= 0.0 {
                return;
            }
            let t = f32::from(ev.position.x - b.origin.x) / w;
            let i = ((t * n as f32).floor().max(0.0) as usize).min(n - 1);
            if state_mouse.read(cx).active != Some(i) {
                state_mouse.update(cx, |this, cx| {
                    this.keyboard = false;
                    this.point(i, false, gap_val, cx);
                });
            }
        })
        .children(bars)
        .child(
            canvas(
                move |bounds, window, cx| {
                    let prev_w = state_canvas.read(cx).width;
                    let new_w = f32::from(bounds.size.width);
                    state_canvas.update(cx, |this, _cx| {
                        this.strip_bounds = bounds;
                    });
                    if (prev_w - new_w).abs() > 0.5 {
                        let state = state_canvas.clone();
                        window.defer(cx, move |_, cx| {
                            state.update(cx, |this, cx| {
                                this.width = new_w;
                                cx.notify();
                            });
                        });
                    }
                },
                |_, (), _, _| {},
            )
            .absolute()
            .size_full(),
        );

    v_flex()
        .id("status-bars")
        .w_full()
        .min_w_0()
        .text_color(st.fg)
        .when(!cfg.compact, |d| d.child(header))
        .child(
            div()
                .id("status-wrap")
                .relative()
                .on_hover(move |hovered: &bool, _window, cx| {
                    if !*hovered && !state_hover.read(cx).keyboard {
                        state_hover.update(cx, |this, cx| {
                            this.hide(cx);
                        });
                    }
                })
                .child(strip)
                .when_some(tip, |d, tip| d.child(tip)),
        )
        .when(!cfg.compact, |d| d.child(footer))
}
