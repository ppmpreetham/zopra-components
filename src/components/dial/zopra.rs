#![cfg(feature = "dial")]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    clippy::float_cmp,
    clippy::match_same_arms,
    clippy::collapsible_if,
    clippy::many_single_char_names,
    clippy::redundant_closure_for_method_calls
)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use zopra::component;

pub use super::gpui::{
    CometDialEvent, Decrement, DecrementLarge, DialConfig, Increment, IncrementLarge, ToMax, ToMin,
    init as init_dial,
};

const CONTEXT: &str = "CometDial";

const R: f32 = 80.0;
const DEAD: f32 = 44.0;
const K: usize = 12; // comet segments
const V_FULL: f32 = 4.0;
const V_FLICK: f32 = 2.0;
const TAU_FOLLOW: f32 = 0.06;
const TAU_V: f32 = 0.05;
const DECEL: f32 = 0.99;
const DRAG_PX: f32 = 10.0;
const STALE: Duration = Duration::from_millis(80);
const ARC_STEP_DEG: f32 = 3.0;

#[derive(Clone, Copy, PartialEq)]
enum Edge {
    Lo,
    Hi,
}

struct Grip {
    at: Option<f32>,
    hist: VecDeque<(Instant, f32)>,
    side: Option<Edge>,
    moved: bool,
    down: Point<Pixels>,
}

#[derive(Clone, Copy)]
struct Spring {
    to: f32,
    bounce: f32,
}

pub type DialChangeCallback = Arc<dyn Fn(f32, &mut Window, &mut App)>;
pub type DialEndCallback = Arc<dyn Fn(f32, f32, f32, &mut Window, &mut App)>;

pub struct CometDialState {
    pub config: DialConfig,
    pub value: f32,
    pub reading: f32,
    pub vel: f32,
    spring: Option<Spring>,
    grip: Option<Grip>,
    pub smooth_v: f32,
    pub animating: bool,
    pub last_frame: Option<Instant>,
    pub text: SharedString,
    pub text_key: i64,
    pub bounds: Bounds<Pixels>,
    pub focus_handle: FocusHandle,
}

impl EventEmitter<CometDialEvent> for CometDialState {}

fn dist(a: Point<Pixels>, b: Point<Pixels>) -> f32 {
    f32::from(a.x - b.x).hypot(f32::from(a.y - b.y))
}

fn point_at(c: Point<Pixels>, r: f32, deg: f32) -> Point<Pixels> {
    let a = deg.to_radians();
    point(c.x + px(a.cos() * r), c.y + px(a.sin() * r))
}

fn velocity_of(hist: &VecDeque<(Instant, f32)>) -> f32 {
    match (hist.front(), hist.back()) {
        (Some(a), Some(b)) if hist.len() >= 2 => {
            let ms = ((b.0 - a.0).as_secs_f32() * 1000.0).max(1.0);
            (b.1 - a.1) / ms * 1000.0
        }
        _ => 0.0,
    }
}

impl CometDialState {
    #[must_use]
    pub fn new(value: f32, config: DialConfig, cx: &mut Context<Self>) -> Self {
        let value = value.clamp(config.min, config.max);
        let mut this = Self {
            config,
            value,
            reading: value,
            vel: 0.0,
            spring: None,
            grip: None,
            smooth_v: 0.0,
            animating: false,
            last_frame: None,
            text: SharedString::default(),
            text_key: i64::MIN,
            bounds: Bounds::default(),
            focus_handle: cx.focus_handle(),
        };
        this.value = this.snap(value);
        this.reading = this.value;
        this
    }

    #[must_use]
    pub const fn is_dragging(&self) -> bool {
        self.grip.is_some()
    }

    #[must_use]
    pub fn range(&self) -> f32 {
        (self.config.max - self.config.min).max(1e-9)
    }

    #[must_use]
    pub fn gap(&self) -> f32 {
        360.0 - self.config.sweep
    }

    #[must_use]
    pub fn start(&self) -> f32 {
        90.0 + self.gap() / 2.0
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

    #[must_use]
    pub fn key_step(&self) -> f32 {
        if self.config.step > 0.0 {
            self.config.step
        } else {
            self.range() / 100.0
        }
    }

    fn local(&self, pos: Point<Pixels>) -> (f32, f32) {
        let b = self.bounds;
        let scale = (f32::from(b.size.width).min(f32::from(b.size.height)) / 200.0).max(1e-6);
        let c = b.center();
        (
            f32::from(pos.x - c.x) / scale,
            f32::from(pos.y - c.y) / scale,
        )
    }

    fn angle_at(&self, pos: Point<Pixels>, side: &mut Option<Edge>) -> f32 {
        let (x, y) = self.local(pos);
        let (sweep, gap) = (self.config.sweep, self.gap());
        let mut rel = (y.atan2(x).to_degrees() - self.start() + 720.0).rem_euclid(360.0);
        if rel > sweep {
            rel = match *side {
                Some(Edge::Hi) => sweep,
                Some(Edge::Lo) => 0.0,
                None if rel < sweep + gap / 2.0 => sweep,
                None => 0.0,
            };
        } else {
            *side = Some(if rel > sweep / 2.0 {
                Edge::Hi
            } else {
                Edge::Lo
            });
        }
        self.config.min + rel / sweep * self.range()
    }

    fn commit(
        &mut self,
        v: f32,
        finished: Option<(f32, f32)>,
        on_change: Option<&DialChangeCallback>,
        on_end: Option<&DialEndCallback>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let s = self.snap(v);
        if s != self.value {
            self.value = s;
            if let Some(cb) = on_change {
                cb(s, window, cx);
            }
        }
        if let Some((velocity, bounce)) = finished {
            if let Some(cb) = on_end {
                cb(s, velocity, bounce, window, cx);
            }
        }
    }

    fn wake(&mut self, cx: &mut Context<Self>) {
        if !self.animating {
            self.animating = true;
            self.last_frame = None;
        }
        cx.notify();
    }

    fn launch(&mut self, to: f32, bounce: f32, velocity: f32, cx: &mut Context<Self>) {
        if self.config.reduce_motion {
            self.reading = to;
            self.vel = 0.0;
            self.spring = None;
        } else {
            self.spring = Some(Spring { to, bounce });
            self.vel = velocity;
        }
        self.wake(cx);
    }

    pub fn on_down(
        &mut self,
        pos: Point<Pixels>,
        on_change: Option<&DialChangeCallback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.config.disabled || self.grip.is_some() {
            return;
        }
        let (x, y) = self.local(pos);
        if x.hypot(y) < DEAD {
            return;
        }
        window.focus(&self.focus_handle, cx);

        let mut side = None;
        let at = self.angle_at(pos, &mut side);
        let mut hist = VecDeque::with_capacity(4);
        hist.push_back((Instant::now(), at));
        self.grip = Some(Grip {
            at: None,
            hist,
            side,
            moved: false,
            down: pos,
        });

        self.commit(at, None, on_change, None, window, cx);
        let (to, bounce, v) = (self.snap(at), self.config.tap_bounce, self.vel);
        self.launch(to, bounce, v, cx);
    }

    pub fn on_move(
        &mut self,
        pos: Point<Pixels>,
        on_change: Option<&DialChangeCallback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(g) = self.grip.as_ref() else { return };
        if !g.moved && dist(pos, g.down) < DRAG_PX {
            return;
        }
        let mut side = g.side;
        let at = self.angle_at(pos, &mut side);
        if let Some(g) = self.grip.as_mut() {
            if !g.moved {
                g.moved = true;
                self.spring = None;
            }
            g.side = side;
            g.at = Some(at);
            if g.hist.len() == 4 {
                g.hist.pop_front();
            }
            g.hist.push_back((Instant::now(), at));
        }
        self.commit(at, None, on_change, None, window, cx);
        self.wake(cx);
    }

    pub fn on_up(
        &mut self,
        on_change: Option<&DialChangeCallback>,
        on_end: Option<&DialEndCallback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(g) = self.grip.take() else { return };
        let c = &self.config;

        if !g.moved {
            let tap = c.tap_bounce;
            self.commit(self.value, Some((0.0, tap)), on_change, on_end, window, cx);
            return;
        }

        let stale = g.hist.back().is_none_or(|(t, _)| t.elapsed() > STALE);
        let v = if stale { 0.0 } else { velocity_of(&g.hist) };
        let flick = (v.abs() / self.range() / V_FLICK).clamp(0.0, 1.0);
        let bounce = c.tap_bounce + (c.flick_bounce - c.tap_bounce) * flick;
        let coast = v / 1000.0 * (DECEL / (1.0 - DECEL)) * c.momentum;
        let to = self.snap(g.at.unwrap_or(self.value) + coast);

        self.commit(to, Some((v, bounce)), on_change, on_end, window, cx);
        self.launch(to, bounce, v, cx);
    }

    pub fn nudge(
        &mut self,
        to: f32,
        on_change: Option<&DialChangeCallback>,
        on_end: Option<&DialEndCallback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.config.disabled {
            return;
        }
        let to = self.snap(to);
        self.spring = None;
        self.reading = to;
        self.vel = 0.0;
        let tap = self.config.tap_bounce;
        self.commit(to, Some((0.0, tap)), on_change, on_end, window, cx);
        self.wake(cx);
    }

    fn step_spring(&mut self, dt: f32) {
        let Some(sp) = self.spring else { return };
        let k = 200.0 + self.config.speed.clamp(0.0, 100.0) / 100.0 * 700.0;
        let damping = 2.0 * k.sqrt() * (1.0 - sp.bounce.clamp(0.0, 0.9));

        let n = (dt / (1.0 / 240.0)).ceil().max(1.0) as usize;
        let h = dt / n as f32;
        for _ in 0..n {
            let a = -k * (self.reading - sp.to) - damping * self.vel;
            self.vel += a * h;
            self.reading += self.vel * h;
        }

        let eps = 1e-3 * self.range();
        if (self.reading - sp.to).abs() < eps && self.vel.abs() < eps {
            self.reading = sp.to;
            self.vel = 0.0;
            self.spring = None;
        }
    }

    pub fn step(&mut self, window: &mut Window) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .replace(now)
            .map_or(1.0 / 60.0, |p| (now - p).as_secs_f32().min(0.05));
        let prev = self.reading;
        let reduce = self.config.reduce_motion;

        match self.grip.as_ref().and_then(|g| g.at) {
            Some(at) => {
                self.reading = if reduce {
                    at
                } else {
                    prev + (at - prev) * (1.0 - (-dt / TAU_FOLLOW).exp())
                };
                self.vel = if dt > 0.0 {
                    (self.reading - prev) / dt
                } else {
                    0.0
                };
            }
            None => self.step_spring(dt),
        }

        let v_raw = if dt == 0.0 || reduce {
            0.0
        } else {
            (self.reading - prev) / dt / self.range()
        };
        self.smooth_v += (v_raw - self.smooth_v) * (1.0 - (-dt / TAU_V).exp());

        let moving = self.grip.is_some() || self.spring.is_some() || self.smooth_v.abs() > 0.002;
        if moving {
            window.request_animation_frame();
        } else {
            self.animating = false;
            self.last_frame = None;
        }
    }

    pub fn refresh_text(&mut self) {
        let decimals = {
            let s = self.config.step.to_string();
            s.find('.').map_or(0, |i| s.len() - i - 1)
        };
        let shown = self.reading.clamp(self.config.min, self.config.max);
        let key = (shown * 10f32.powi(decimals as i32)).round() as i64;
        if key != self.text_key {
            self.text_key = key;
            self.text = format!("{shown:.decimals$}").into();
        }
    }
}

#[derive(Clone, Copy)]
pub struct DialFrame {
    pub start: f32,
    pub end: f32,
    pub ang: f32,
    pub f: f32,
    pub s: f32,
    pub dir: f32,
    pub thickness: f32,
    pub comet_width: f32,
    pub comet_reach: f32,
    pub accent: Hsla,
    pub ink: Hsla,
    pub dragging: bool,
}

fn stroke_arc(
    window: &mut Window,
    c: Point<Pixels>,
    r: f32,
    a0: f32,
    a1: f32,
    w: f32,
    color: Hsla,
) {
    if a1 - a0 < 0.01 || color.a <= 0.003 {
        return;
    }
    let n = ((a1 - a0) / ARC_STEP_DEG).ceil().max(2.0) as usize;
    let mut b = PathBuilder::stroke(px(w));
    b.move_to(point_at(c, r, a0));
    for i in 1..=n {
        b.line_to(point_at(c, r, a0 + (a1 - a0) * i as f32 / n as f32));
    }
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

fn dot(window: &mut Window, c: Point<Pixels>, r: f32, color: Hsla) {
    let bounds = Bounds::centered_at(c, size(px(r * 2.0), px(r * 2.0)));
    window.paint_quad(PaintQuad {
        corner_radii: Corners::all(px(r)),
        ..fill(bounds, color)
    });
}

pub fn paint_dial(window: &mut Window, bounds: Bounds<Pixels>, fr: &DialFrame) {
    let scale = f32::from(bounds.size.width).min(f32::from(bounds.size.height)) / 200.0;
    let c = bounds.center();
    let r = R * scale;
    let t = fr.thickness * scale;

    let track = fr.ink.opacity(0.24);
    stroke_arc(window, c, r, fr.start, fr.end, t, track);
    dot(window, point_at(c, r, fr.start), t / 2.0, track);
    dot(window, point_at(c, r, fr.end), t / 2.0, track);

    if fr.f > 0.0005 {
        let lit = fr.accent.opacity(0.7);
        stroke_arc(window, c, r, fr.start, fr.ang, t, lit);
        dot(window, point_at(c, r, fr.start), t / 2.0, lit);
    }

    let seg = fr.comet_reach * fr.s / K as f32;
    if seg >= 0.01 {
        for j in 0..K {
            let (j_f, k_f) = (j as f32, K as f32);
            let (mut a0, mut a1) = if fr.dir > 0.0 {
                (fr.ang - (j_f + 1.0) * seg, fr.ang - j_f * seg)
            } else {
                (fr.ang + j_f * seg, fr.ang + (j_f + 1.0) * seg)
            };
            a0 = a0.clamp(fr.start, fr.end);
            a1 = a1.clamp(fr.start, fr.end);
            let w = 1.0 - j_f / k_f;
            let width = (fr.thickness + fr.comet_width * w * fr.s) * scale;
            stroke_arc(window, c, r, a0, a1, width, fr.accent.opacity(w * fr.s));
        }
    }

    dot(
        window,
        point_at(c, r, fr.ang),
        fr.thickness * 1.8 * scale,
        fr.accent,
    );
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[component]
pub fn dial(
    value: Option<f32>,
    config: Option<DialConfig>,
    size: Option<f32>,
    min: Option<f32>,
    max: Option<f32>,
    step: Option<f32>,
    unit: Option<SharedString>,
    speed: Option<f32>,
    accent: Option<Hsla>,
    ink: Option<Hsla>,
    disabled: Option<bool>,
    on_change: Option<DialChangeCallback>,
    on_end: Option<DialEndCallback>,
) {
    let mut cfg = config.unwrap_or_default();
    if let Some(s) = size {
        cfg.size = s;
    }
    if let Some(m) = min {
        cfg.min = m;
    }
    if let Some(m) = max {
        cfg.max = m;
    }
    if let Some(st) = step {
        cfg.step = st;
    }
    if let Some(u) = unit {
        cfg.unit = u;
    }
    if let Some(sp) = speed {
        cfg.speed = sp;
    }
    if let Some(acc) = accent {
        cfg.accent = Some(acc);
    }
    if let Some(i) = ink {
        cfg.ink = Some(i);
    }
    if let Some(dis) = disabled {
        cfg.disabled = dis;
    }

    let initial_val = value.unwrap_or(cfg.min);
    let state_holder = window.use_state(cx, |_window, cx| {
        cx.new(|cx| CometDialState::new(initial_val, cfg.clone(), cx))
    });

    let state = state_holder.read(cx).clone();

    if let Some(external_val) = value {
        let is_dragging = state.read(cx).is_dragging();
        let current_val = state.read(cx).value;
        if !is_dragging && (external_val - current_val).abs() > f32::EPSILON {
            state.update(cx, |this, cx| {
                let s = this.snap(external_val);
                if s != this.value {
                    this.value = s;
                    this.launch(s, this.config.tap_bounce, this.vel, cx);
                }
            });
        }
    }

    if state.read(cx).animating {
        state.update(cx, |this, _cx| {
            this.step(window);
        });
    }

    state.update(cx, |this, _cx| {
        this.refresh_text();
    });

    let c = state.read(cx).config.clone();
    let theme = cx.theme();
    let accent_color = c.accent.unwrap_or(theme.foreground);
    let ink_color = c.ink.unwrap_or(theme.foreground);

    let f = ((state.read(cx).reading - c.min) / state.read(cx).range()).clamp(0.0, 1.0);
    let start = state.read(cx).start();
    let frame = DialFrame {
        start,
        end: start + c.sweep,
        ang: start + f * c.sweep,
        f,
        s: (state.read(cx).smooth_v.abs() / V_FULL).clamp(0.0, 1.0),
        dir: if state.read(cx).smooth_v < 0.0 {
            -1.0
        } else {
            1.0
        },
        thickness: c.thickness,
        comet_width: c.comet_width,
        comet_reach: c.comet_reach,
        accent: accent_color,
        ink: ink_color,
        dragging: state.read(cx).is_dragging(),
    };

    let size_px = c.size;
    let is_disabled = c.disabled;
    let unit_str = c.unit;
    let display_text = state.read(cx).text.clone();
    let focus_handle = state.read(cx).focus_handle.clone();

    let state_down = state.clone();
    let state_probe = state.clone();
    let state_inc = state.clone();
    let state_dec = state.clone();
    let state_inc_lg = state.clone();
    let state_dec_lg = state.clone();
    let state_min = state.clone();
    let state_max = state.clone();

    let on_change_down = on_change.clone();
    let on_change_inc = on_change.clone();
    let on_change_dec = on_change.clone();
    let on_change_inc_lg = on_change.clone();
    let on_change_dec_lg = on_change.clone();
    let on_change_min = on_change.clone();
    let on_change_max = on_change.clone();

    let on_end_inc = on_end.clone();
    let on_end_dec = on_end.clone();
    let on_end_inc_lg = on_end.clone();
    let on_end_dec_lg = on_end.clone();
    let on_end_min = on_end.clone();
    let on_end_max = on_end.clone();

    div()
        .id("dial")
        .key_context(CONTEXT)
        .track_focus(&focus_handle)
        .relative()
        .size(px(size_px))
        .when(is_disabled, |d| d.opacity(0.55))
        .when(!is_disabled, |d| d.cursor_grab())
        .on_action(move |_: &Increment, window, cx| {
            state_inc.update(cx, |this, cx| {
                let step = this.key_step();
                let next = this.value + step;
                this.nudge(
                    next,
                    on_change_inc.as_ref(),
                    on_end_inc.as_ref(),
                    window,
                    cx,
                );
            });
        })
        .on_action(move |_: &Decrement, window, cx| {
            state_dec.update(cx, |this, cx| {
                let step = this.key_step();
                let prev = this.value - step;
                this.nudge(
                    prev,
                    on_change_dec.as_ref(),
                    on_end_dec.as_ref(),
                    window,
                    cx,
                );
            });
        })
        .on_action(move |_: &IncrementLarge, window, cx| {
            state_inc_lg.update(cx, |this, cx| {
                let step = this.key_step() * 10.0;
                let next = this.value + step;
                this.nudge(
                    next,
                    on_change_inc_lg.as_ref(),
                    on_end_inc_lg.as_ref(),
                    window,
                    cx,
                );
            });
        })
        .on_action(move |_: &DecrementLarge, window, cx| {
            state_dec_lg.update(cx, |this, cx| {
                let step = this.key_step() * 10.0;
                let prev = this.value - step;
                this.nudge(
                    prev,
                    on_change_dec_lg.as_ref(),
                    on_end_dec_lg.as_ref(),
                    window,
                    cx,
                );
            });
        })
        .on_action(move |_: &ToMin, window, cx| {
            state_min.update(cx, |this, cx| {
                let min_val = this.config.min;
                this.nudge(
                    min_val,
                    on_change_min.as_ref(),
                    on_end_min.as_ref(),
                    window,
                    cx,
                );
            });
        })
        .on_action(move |_: &ToMax, window, cx| {
            state_max.update(cx, |this, cx| {
                let max_val = this.config.max;
                this.nudge(
                    max_val,
                    on_change_max.as_ref(),
                    on_end_max.as_ref(),
                    window,
                    cx,
                );
            });
        })
        .on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, window, cx| {
            state_down.update(cx, |this, cx| {
                this.on_down(ev.position, on_change_down.as_ref(), window, cx);
            });
        })
        .child(
            canvas(
                move |bounds, _window, cx| {
                    state_probe.update(cx, |this, _cx| {
                        this.bounds = bounds;
                    });
                },
                move |bounds, (), window, _cx| {
                    paint_dial(window, bounds, &frame);

                    if frame.dragging {
                        let e_move = state.clone();
                        let on_change_m = on_change.clone();
                        window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            e_move.update(cx, |this, cx| {
                                if ev.pressed_button == Some(MouseButton::Left) {
                                    this.on_move(ev.position, on_change_m.as_ref(), window, cx);
                                } else {
                                    this.on_up(on_change_m.as_ref(), None, window, cx);
                                }
                            });
                        });

                        let e_up = state.clone();
                        let on_change_u = on_change.clone();
                        let on_end_u = on_end.clone();
                        window.on_mouse_event(move |ev: &MouseUpEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture && ev.button == MouseButton::Left {
                                e_up.update(cx, |this, cx| {
                                    this.on_up(on_change_u.as_ref(), on_end_u.as_ref(), window, cx);
                                });
                            }
                        });
                    }
                },
            )
            .absolute()
            .size_full(),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    h_flex()
                        .items_baseline()
                        .gap(px(2.0))
                        .text_size(px(size_px * 0.16))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(ink_color)
                        .child(display_text)
                        .when(!unit_str.is_empty(), |d| {
                            d.child(
                                div()
                                    .text_size(px(size_px * 0.08))
                                    .opacity(0.5)
                                    .child(unit_str),
                            )
                        }),
                ),
        )
}
