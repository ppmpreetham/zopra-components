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
    clippy::return_self_not_must_use,
    clippy::map_unwrap_or
)]

use std::{cell::Cell, collections::HashMap, f32::consts::TAU, rc::Rc, time::Instant};

use chrono::NaiveDate;
use gpui_kit::{
    canvas, div, prelude::*, px, relative, rgb, App, Bounds, Context, Div, FocusHandle,
    Focusable, FontWeight, Hsla, KeyBinding, MouseMoveEvent, Pixels, Render, SharedString,
    Subscription, Window,
};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};

use super::{Dismiss, First, Last, Next, NextWeek, Prev, PrevWeek};

const CONTEXT: &str = "StatusBars";
const TAU_TIP: f32 = 0.05;
const TAU_DIM: f32 = 0.06;
const TAU_X: f32 = 0.04;
const TAU_UPTIME: f32 = 0.12;
const PING_PERIOD: f32 = 1.5;
const LOAD_PERIOD: f32 = 1.6;

/// Call once at startup, after `gpui_component::init(cx)`.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("left", Prev, Some(CONTEXT)),
        KeyBinding::new("right", Next, Some(CONTEXT)),
        KeyBinding::new("pageup", PrevWeek, Some(CONTEXT)),
        KeyBinding::new("pagedown", NextWeek, Some(CONTEXT)),
        KeyBinding::new("home", First, Some(CONTEXT)),
        KeyBinding::new("end", Last, Some(CONTEXT)),
        KeyBinding::new("escape", Dismiss, Some(CONTEXT)),
    ]);
}

// ---- data model --------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusLevel {
    Up,
    Degraded,
    Down,
    /// No data for the day.
    None,
}

#[derive(Clone, Debug)]
pub struct StatusIncident {
    pub title: SharedString,
    pub minutes: Option<u32>,
    pub status: Option<StatusLevel>,
}

impl StatusIncident {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self { title: title.into(), minutes: None, status: None }
    }
    pub fn minutes(mut self, minutes: u32) -> Self {
        self.minutes = Some(minutes);
        self
    }
    pub fn status(mut self, status: StatusLevel) -> Self {
        self.status = Some(status);
        self
    }
}

#[derive(Clone, Debug)]
pub struct StatusDay {
    pub date: NaiveDate,
    /// Overrides the status derived from the minutes below.
    pub status: Option<StatusLevel>,
    /// Minutes of full outage. Counts against uptime.
    pub downtime: Option<u32>,
    /// Minutes of degraded service. Shown, but doesn't count against uptime.
    pub degraded: Option<u32>,
    /// Overrides the day's uptime percentage (0..=100).
    pub uptime: Option<f32>,
    pub incidents: Vec<StatusIncident>,
}

impl StatusDay {
    pub fn new(date: NaiveDate) -> Self {
        Self { date, status: None, downtime: None, degraded: None, uptime: None, incidents: Vec::new() }
    }
    pub fn status(mut self, status: StatusLevel) -> Self {
        self.status = Some(status);
        self
    }
    pub fn downtime(mut self, minutes: u32) -> Self {
        self.downtime = Some(minutes);
        self
    }
    pub fn degraded(mut self, minutes: u32) -> Self {
        self.degraded = Some(minutes);
        self
    }
    pub fn uptime(mut self, percent: f32) -> Self {
        self.uptime = Some(percent);
        self
    }
    pub fn incident(mut self, incident: StatusIncident) -> Self {
        self.incidents.push(incident);
        self
    }
}

/// Today's UTC date.
pub fn status_today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

/// One entry per UTC day from `from` to `to` inclusive; missing days count as operational.
pub fn fill_status_days(
    from: NaiveDate,
    to: NaiveDate,
    entries: impl IntoIterator<Item = StatusDay>,
) -> Vec<StatusDay> {
    let by_date: HashMap<NaiveDate, StatusDay> = entries.into_iter().map(|d| (d.date, d)).collect();
    let mut days = Vec::new();
    let mut date = from;
    while date <= to {
        days.push(by_date.get(&date).cloned().unwrap_or_else(|| StatusDay::new(date)));
        let Some(next) = date.succ_opt() else { break };
        date = next;
    }
    days
}

pub fn day_status(day: &StatusDay) -> StatusLevel {
    if let Some(s) = day.status {
        s
    } else if day.downtime.unwrap_or(0) > 0 {
        StatusLevel::Down
    } else if day.degraded.unwrap_or(0) > 0 {
        StatusLevel::Degraded
    } else {
        StatusLevel::Up
    }
}

pub fn day_uptime(day: &StatusDay) -> f32 {
    day.uptime.unwrap_or_else(|| (100.0 - day.downtime.unwrap_or(0) as f32 / 1440.0 * 100.0).max(0.0))
}

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

/// Floors to two decimals (99.999 must not read as 100%) and trims trailing zeros.
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

// ---- config ----------------------------------------------------------------

/// Tunable parameters. Override a few with `..Default::default()`.
#[derive(Clone, Copy, Debug)]
pub struct StatusBarsConfig {
    /// Most days to show. Narrow containers step down to 60, then 30.
    pub max_days: usize,
    /// Narrowest a bar may draw (px) before fewer days are shown.
    pub min_bar_width: f32,
    /// Hides the name, status and axis, for a denser list.
    pub compact: bool,
    pub loading: bool,
    pub bar_height: f32,
    pub gap: f32,
    pub tooltip_width: f32,
    /// Animate the ping on down/degraded (keeps frames running in those states).
    pub pulse: bool,
    pub up_color: Hsla,
    pub degraded_color: Hsla,
    pub down_color: Hsla,
    /// `None` falls back to the theme's muted colour.
    pub empty_color: Option<Hsla>,
}

impl Default for StatusBarsConfig {
    fn default() -> Self {
        Self {
            max_days: 90,
            min_bar_width: 3.0,
            compact: false,
            loading: false,
            bar_height: 32.0,
            gap: 2.0,
            tooltip_width: 200.0,
            pulse: true,
            up_color: rgb(0x22c55e).into(),
            degraded_color: rgb(0xf59e0b).into(),
            down_color: rgb(0xef4444).into(),
            empty_color: None,
        }
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

// ---- view ------------------------------------------------------------------

pub struct StatusBars {
    label: SharedString,
    days: Vec<StatusDay>,
    config: StatusBarsConfig,
    current: Option<StatusLevel>,

    /// Visible slots: `Some(index into days)` or `None` for front padding.
    shown: Vec<Option<usize>>,
    shown_count: usize,
    dirty: bool,
    uptime: Option<f32>,
    uptime_shown: f32,

    width: f32,
    strip_bounds: Rc<Cell<Bounds<Pixels>>>,

    active: Option<usize>,
    tip_index: usize,
    keyboard: bool,
    tip_alpha: f32,
    dim: f32,
    tip_x: f32,
    tip_target_x: f32,

    clock: f32,
    animating: bool,
    last_frame: Option<Instant>,

    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}

impl Focusable for StatusBars {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl StatusBars {
    pub fn new(
        label: impl Into<SharedString>,
        days: Vec<StatusDay>,
        config: StatusBarsConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle().tab_stop(true);
        let subs = vec![
            cx.on_focus(&focus_handle, window, |this, _, cx| this.on_focus(cx)),
            cx.on_blur(&focus_handle, window, |this, _, cx| this.hide(cx)),
        ];

        Self {
            label: label.into(),
            days,
            config,
            current: None,
            shown: Vec::new(),
            shown_count: usize::MAX,
            dirty: true,
            uptime: None,
            uptime_shown: 0.0,
            width: 0.0,
            strip_bounds: Rc::default(),
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
            _subs: subs,
        }
    }

    pub fn set_days(&mut self, days: Vec<StatusDay>, cx: &mut Context<Self>) {
        self.days = days;
        self.dirty = true;
        self.active = None;
        self.wake(cx);
        cx.notify();
    }

    pub fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        self.config.loading = loading;
        self.wake(cx);
        cx.notify();
    }

    /// Overrides the header status (defaults to today's).
    pub fn set_current(&mut self, current: Option<StatusLevel>, cx: &mut Context<Self>) {
        self.current = current;
        cx.notify();
    }

    pub fn set_config(&mut self, config: StatusBarsConfig, cx: &mut Context<Self>) {
        self.config = config;
        self.shown_count = usize::MAX;
        self.wake(cx);
        cx.notify();
    }

    // ---- derived state -----------------------------------------------------

    fn visible_count(&self) -> usize {
        let c = &self.config;
        let fits = if self.width > 0.0 {
            ((self.width + c.gap) / (c.min_bar_width + c.gap)).floor() as usize
        } else {
            c.max_days
        };
        [c.max_days, 60, 30]
            .into_iter()
            .filter(|&n| n <= c.max_days)
            .find(|&n| n <= fits)
            .unwrap_or_else(|| fits.max(1))
    }

    /// Rebuilds the visible slots and stats; runs only when the count or data changes.
    fn rebuild(&mut self, count: usize) {
        let tail_start = self.days.len().saturating_sub(count);
        let pad = count.saturating_sub(self.days.len() - tail_start);
        self.shown = std::iter::repeat_n(None, pad)
            .chain((tail_start..self.days.len()).map(Some))
            .collect();
        self.shown_count = count;
        self.dirty = false;

        let (sum, n) = self
            .shown
            .iter()
            .flatten()
            .map(|&i| &self.days[i])
            .filter(|d| day_status(d) != StatusLevel::None)
            .fold((0.0f32, 0usize), |(s, n), d| (s + day_uptime(d), n + 1));
        let target = (n > 0).then(|| sum / n as f32);
        if self.uptime.is_none() {
            self.uptime_shown = target.unwrap_or(0.0); // first data appears without a count-up
        }
        self.uptime = target;
    }

    // ---- interaction -------------------------------------------------------

    fn wake(&mut self, cx: &mut Context<Self>) {
        if !self.animating {
            self.animating = true;
            self.last_frame = None;
            cx.notify();
        }
    }

    fn point(&mut self, index: usize, instant: bool, cx: &mut Context<Self>) {
        let n = self.shown.len().max(1) as f32;
        let x = (index as f32 + 0.5) * (self.width + self.config.gap) / n - self.config.gap / 2.0;
        self.tip_target_x = x;
        if instant || self.active.is_none() {
            self.tip_x = x;
        }
        self.active = Some(index);
        self.tip_index = index;
        self.wake(cx);
    }

    fn hide(&mut self, cx: &mut Context<Self>) {
        self.active = None;
        self.keyboard = false;
        self.wake(cx);
    }

    fn on_focus(&mut self, cx: &mut Context<Self>) {
        if self.config.loading || self.shown.is_empty() {
            return;
        }
        self.keyboard = true;
        let last = self.shown.len() - 1;
        self.point(self.active.unwrap_or(last), true, cx);
    }

    fn nav(&mut self, to: impl FnOnce(usize, usize) -> usize, cx: &mut Context<Self>) {
        if self.config.loading || self.shown.is_empty() {
            return;
        }
        let last = self.shown.len() - 1;
        let from = self.active.unwrap_or(last);
        self.keyboard = true;
        self.point(to(from, last).min(last), true, cx);
    }

    // ---- per-frame simulation ---------------------------------------------

    fn ping_active(&self, now: StatusLevel) -> bool {
        self.config.pulse && !self.config.loading && matches!(now, StatusLevel::Down | StatusLevel::Degraded)
    }

    fn now_status(&self) -> StatusLevel {
        self.current
            .or_else(|| self.days.last().map(day_status))
            .unwrap_or(StatusLevel::None)
    }

    fn step(&mut self, window: &mut Window) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .replace(now)
            .map_or(1.0 / 60.0, |p| (now - p).as_secs_f32().min(0.05));
        let ease = |tau: f32| 1.0 - (-dt / tau).exp();
        self.clock += dt;

        let hovering = self.active.is_some() && !self.config.loading;
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
            || self.config.loading
            || self.ping_active(self.now_status());

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

    // ---- element builders --------------------------------------------------

    fn render_header(&self, now: StatusLevel, st: &Palette) -> Div {
        let loading = self.config.loading;
        let dot = if loading { st.muted_fg.opacity(0.5) } else { st.dot(now) };

        let ping = (self.ping_active(now)).then(|| {
            let p = (self.clock % PING_PERIOD) / PING_PERIOD;
            let eased = 1.0 - (1.0 - p).powi(3);
            let size = 6.0 + 10.0 * eased;
            div()
                .absolute()
                .top(px(4.0 - size / 2.0))
                .left(px(4.0 - size / 2.0))
                .size(px(size))
                .rounded_full()
                .bg(st.dot(now).opacity(0.75 * (1.0 - p)))
        });

        h_flex()
            .mb(px(10.0))
            .justify_between()
            .gap_3()
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.label.clone()),
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
                    .child(if loading { "Checking…" } else { current_word(now) }),
            )
    }

    fn render_footer(&self, count: usize, st: &Palette) -> Div {
        let first = self
            .shown
            .first()
            .copied()
            .flatten()
            .map(|i| self.days[i].date.format("%b %-d").to_string())
            .unwrap_or_else(|| format!("{count} days ago"));

        let uptime_txt = if self.config.loading {
            "Loading…".to_string()
        } else if self.uptime.is_none() {
            "No data yet".to_string()
        } else {
            format!("{} uptime", fmt_percent(self.uptime_shown))
        };

        let rule = || div().h(px(1.0)).flex_1().bg(st.border);

        h_flex()
            .mt(px(8.0))
            .gap_3()
            .text_size(px(11.0))
            .text_color(st.muted_fg.opacity(0.8))
            .child(div().flex_none().child(first))
            .child(rule())
            .child(div().flex_none().text_color(st.muted_fg).child(uptime_txt))
            .child(rule())
            .child(div().flex_none().child("Today"))
    }

    fn render_tip(&self, st: &Palette) -> Div {
        let day = self.shown.get(self.tip_index).copied().flatten().map(|i| &self.days[i]);
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

        v_flex()
            .w(px(self.config.tooltip_width))
            .px(px(10.0))
            .py(px(8.0))
            .rounded_lg()
            .border_1()
            .border_color(st.border)
            .bg(st.popover)
            .text_color(st.popover_fg)
            .text_size(px(12.0))
            .shadow_lg()
            .opacity(self.tip_alpha)
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
            }))
    }
}

impl Render for StatusBars {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.visible_count();
        if self.dirty || count != self.shown_count {
            self.rebuild(count);
        }
        if self.animating {
            self.step(window);
        }

        let c = self.config;
        let theme = cx.theme();
        let st = Palette {
            up: c.up_color,
            degraded: c.degraded_color,
            down: c.down_color,
            empty: c.empty_color.unwrap_or(theme.muted),
            fg: theme.foreground,
            muted_fg: theme.muted_foreground,
            border: theme.border,
            popover: theme.popover,
            popover_fg: theme.popover_foreground,
        };
        let now = self.now_status();

        // Bars: plain divs; hover dimming and the active lift are cheap style changes.
        let pulse = if c.loading { 0.55 + 0.25 * (self.clock * TAU / LOAD_PERIOD).sin() } else { 1.0 };
        let bars: Vec<_> = self
            .shown
            .iter()
            .enumerate()
            .map(|(i, slot)| {
                let status = slot.map_or(StatusLevel::None, |ix| day_status(&self.days[ix]));
                let is_active = self.active == Some(i);
                let color = if c.loading { st.empty.opacity(pulse) } else { st.bar(status) };
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(if is_active { c.bar_height * 1.08 } else { c.bar_height }))
                    .rounded(px(2.0))
                    .bg(color)
                    .opacity(if is_active { 1.0 } else { 1.0 - 0.65 * self.dim })
            })
            .collect();

        let probe = self.strip_bounds.clone();
        let entity = cx.entity();

        let strip = h_flex()
            .id("status-strip")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .relative()
            .h(px(c.bar_height))
            .items_end()
            .gap(px(c.gap))
            .on_action(cx.listener(|this, _: &Prev, _, cx| this.nav(|f, _| f.saturating_sub(1), cx)))
            .on_action(cx.listener(|this, _: &Next, _, cx| this.nav(|f, _| f + 1, cx)))
            .on_action(cx.listener(|this, _: &PrevWeek, _, cx| this.nav(|f, _| f.saturating_sub(7), cx)))
            .on_action(cx.listener(|this, _: &NextWeek, _, cx| this.nav(|f, _| f + 7, cx)))
            .on_action(cx.listener(|this, _: &First, _, cx| this.nav(|_, _| 0, cx)))
            .on_action(cx.listener(|this, _: &Last, _, cx| this.nav(|_, l| l, cx)))
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.hide(cx)))
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                let n = this.shown.len();
                let b = this.strip_bounds.get();
                let w = f32::from(b.size.width);
                if this.config.loading || n == 0 || w <= 0.0 {
                    return;
                }
                let t = f32::from(ev.position.x - b.origin.x) / w;
                let i = ((t * n as f32).floor().max(0.0) as usize).min(n - 1);
                if this.active != Some(i) {
                    this.keyboard = false;
                    this.point(i, false, cx);
                }
            }))
            .children(bars)
            // Bounds probe: feeds hit-testing and the responsive day count.
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let prev = probe.replace(bounds);
                        if (prev.size.width - bounds.size.width).abs() > px(0.5) {
                            let entity = entity.clone();
                            window.defer(cx, move |_, cx| {
                                entity.update(cx, |this, cx| {
                                    this.width = f32::from(bounds.size.width);
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

        let tip = (self.tip_alpha > 0.01 && !self.shown.is_empty()).then(|| {
            let w = c.tooltip_width;
            // Slide the tooltip left in proportion to position so it stays inside the strip.
            let frac = if self.width > 0.0 { (self.tip_x / self.width).clamp(0.0, 1.0) } else { 0.0 };
            div()
                .absolute()
                .bottom(relative(1.0))
                .mb(px(10.0))
                .left(px(self.tip_x - frac * w))
                .child(self.render_tip(&st))
        });

        v_flex()
            .id("status-bars")
            .w_full()
            .min_w_0()
            .text_color(st.fg)
            .when(!c.compact, |d| d.child(self.render_header(now, &st)))
            .child(
                div()
                    .id("status-wrap")
                    .relative()
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        if !*hovered && !this.keyboard {
                            this.hide(cx);
                        }
                    }))
                    .child(strip)
                    .when_some(tip, |d, tip| d.child(tip)),
            )
            .when(!c.compact, |d| d.child(self.render_footer(count, &st)))
    }
}
