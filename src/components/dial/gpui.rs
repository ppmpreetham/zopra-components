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
    clippy::must_use_candidate,
    clippy::semicolon_if_nothing_returned,
    clippy::match_same_arms,
    clippy::many_single_char_names,
    clippy::redundant_closure_for_method_calls
)]

use std::{cell::Cell, collections::VecDeque, rc::Rc, time::{Duration, Instant}};

use gpui_kit::{
    actions, canvas, div, fill, point, prelude::*, px, size, App, Bounds, Context, Corners,
    DispatchPhase, EventEmitter, FocusHandle, Focusable, FontWeight, Hsla, KeyBinding,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, PathBuilder, Pixels,
    Point, SharedString, Window,
};
use gpui_kit::component::{h_flex, ActiveTheme as _};

actions!(
    comet_dial,
    [Increment, Decrement, IncrementLarge, DecrementLarge, ToMin, ToMax]
);

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

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", Increment, Some(CONTEXT)),
        KeyBinding::new("right", Increment, Some(CONTEXT)),
        KeyBinding::new("down", Decrement, Some(CONTEXT)),
        KeyBinding::new("left", Decrement, Some(CONTEXT)),
        KeyBinding::new("shift-up", IncrementLarge, Some(CONTEXT)),
        KeyBinding::new("shift-right", IncrementLarge, Some(CONTEXT)),
        KeyBinding::new("pageup", IncrementLarge, Some(CONTEXT)),
        KeyBinding::new("shift-down", DecrementLarge, Some(CONTEXT)),
        KeyBinding::new("shift-left", DecrementLarge, Some(CONTEXT)),
        KeyBinding::new("pagedown", DecrementLarge, Some(CONTEXT)),
        KeyBinding::new("home", ToMin, Some(CONTEXT)),
        KeyBinding::new("end", ToMax, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Debug)]
pub struct DialConfig {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub unit: SharedString,
    /// Element size in px (it's square).
    pub size: f32,
    /// Arc sweep in degrees (the gap sits at the bottom).
    pub sweep: f32,
    pub thickness: f32,
    /// 0..=100; higher = stiffer, snappier spring.
    pub speed: f32,
    pub tap_bounce: f32,
    pub flick_bounce: f32,
    /// Scales how far a flick coasts.
    pub momentum: f32,
    /// Max comet length in degrees.
    pub comet_reach: f32,
    pub comet_width: f32,
    pub disabled: bool,
    /// Skip all easing (GPUI has no OS reduced-motion query, so you decide).
    pub reduce_motion: bool,
    /// `None` falls back to theme foreground.
    pub accent: Option<Hsla>,
    pub ink: Option<Hsla>,
}

impl Default for DialConfig {
    fn default() -> Self {
        Self {
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%".into(),
            size: 250.0,
            sweep: 320.0,
            thickness: 5.0,
            speed: 25.0,
            tap_bounce: 0.2,
            flick_bounce: 0.1,
            momentum: 1.0,
            comet_reach: 180.0,
            comet_width: 12.0,
            disabled: false,
            reduce_motion: false,
            accent: None,
            ink: None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum CometDialEvent {
    Change(f32),
    ChangeEnd { value: f32, velocity: f32, bounce: f32 },
}

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

pub struct CometDial {
    config: DialConfig,
    value: f32,
    reading: f32,
    vel: f32,
    spring: Option<Spring>,
    grip: Option<Grip>,
    smooth_v: f32,
    animating: bool,
    last_frame: Option<Instant>,
    text: SharedString,
    text_key: i64,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    focus_handle: FocusHandle,
}

impl EventEmitter<CometDialEvent> for CometDial {}

impl Focusable for CometDial {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

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

impl CometDial {
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
            bounds: Rc::default(),
            focus_handle: cx.focus_handle(),
        };
        this.value = this.snap(value);
        this.reading = this.value;
        this
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    /// Controlled-mode setter. Ignored mid-drag.
    pub fn set_value(&mut self, v: f32, cx: &mut Context<Self>) {
        if self.grip.is_some() {
            return;
        }
        let s = self.snap(v);
        if s != self.value {
            self.value = s;
            self.launch(s, self.config.tap_bounce, self.vel, cx);
        }
    }

    pub fn set_config(&mut self, config: DialConfig, cx: &mut Context<Self>) {
        self.config = config;
        self.text_key = i64::MIN;
        cx.notify();
    }


    fn range(&self) -> f32 {
        (self.config.max - self.config.min).max(1e-9)
    }

    fn gap(&self) -> f32 {
        360.0 - self.config.sweep
    }

    fn start(&self) -> f32 {
        90.0 + self.gap() / 2.0
    }

    fn snap(&self, v: f32) -> f32 {
        let c = &self.config;
        if c.step > 0.0 {
            (((v - c.min) / c.step).round() * c.step + c.min).clamp(c.min, c.max)
        } else {
            v.clamp(c.min, c.max)
        }
    }

    fn key_step(&self) -> f32 {
        if self.config.step > 0.0 { self.config.step } else { self.range() / 100.0 }
    }

    fn local(&self, pos: Point<Pixels>) -> (f32, f32) {
        let b = self.bounds.get();
        let scale = (f32::from(b.size.width).min(f32::from(b.size.height)) / 200.0).max(1e-6);
        let c = b.center();
        (f32::from(pos.x - c.x) / scale, f32::from(pos.y - c.y) / scale)
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
            *side = Some(if rel > sweep / 2.0 { Edge::Hi } else { Edge::Lo });
        }
        self.config.min + rel / sweep * self.range()
    }


    fn commit(&mut self, v: f32, finished: Option<(f32, f32)>, cx: &mut Context<Self>) {
        let s = self.snap(v);
        if s != self.value {
            self.value = s;
            cx.emit(CometDialEvent::Change(s));
        }
        if let Some((velocity, bounce)) = finished {
            cx.emit(CometDialEvent::ChangeEnd { value: s, velocity, bounce });
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

    fn on_down(&mut self, pos: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
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
        self.grip = Some(Grip { at: None, hist, side, moved: false, down: pos });

        self.commit(at, None, cx);
        let (to, bounce, v) = (self.snap(at), self.config.tap_bounce, self.vel);
        self.launch(to, bounce, v, cx);
    }

    fn on_move(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(g) = self.grip.as_ref() else { return };
        if !g.moved && dist(pos, g.down) < DRAG_PX {
            return;
        }
        let mut side = g.side;
        let at = self.angle_at(pos, &mut side);
        if let Some(g) = self.grip.as_mut() {
            if !g.moved {
                g.moved = true;
                self.spring = None; // hand control from the tap-spring to pointer-follow
            }
            g.side = side;
            g.at = Some(at);
            if g.hist.len() == 4 {
                g.hist.pop_front();
            }
            g.hist.push_back((Instant::now(), at));
        }
        self.commit(at, None, cx);
        self.wake(cx);
    }

    fn on_up(&mut self, cx: &mut Context<Self>) {
        let Some(g) = self.grip.take() else { return };
        let c = &self.config;

        if !g.moved {
            let tap = c.tap_bounce;
            self.commit(self.value, Some((0.0, tap)), cx);
            return;
        }

        let stale = g.hist.back().is_none_or(|(t, _)| t.elapsed() > STALE);
        let v = if stale { 0.0 } else { velocity_of(&g.hist) };
        let flick = (v.abs() / self.range() / V_FLICK).clamp(0.0, 1.0);
        let bounce = c.tap_bounce + (c.flick_bounce - c.tap_bounce) * flick;
        let coast = v / 1000.0 * (DECEL / (1.0 - DECEL)) * c.momentum;
        let to = self.snap(g.at.unwrap_or(self.value) + coast);

        self.commit(to, Some((v, bounce)), cx);
        self.launch(to, bounce, v, cx);
    }

    fn nudge(&mut self, to: f32, cx: &mut Context<Self>) {
        if self.config.disabled {
            return;
        }
        let to = self.snap(to);
        self.spring = None;
        self.reading = to;
        self.vel = 0.0;
        let tap = self.config.tap_bounce;
        self.commit(to, Some((0.0, tap)), cx);
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

    fn step(&mut self, window: &mut Window) {
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
                self.vel = if dt > 0.0 { (self.reading - prev) / dt } else { 0.0 };
            }
            None => self.step_spring(dt),
        }

        let v_raw = if dt == 0.0 || reduce { 0.0 } else { (self.reading - prev) / dt / self.range() };
        self.smooth_v += (v_raw - self.smooth_v) * (1.0 - (-dt / TAU_V).exp());

        let moving = self.grip.is_some() || self.spring.is_some() || self.smooth_v.abs() > 0.002;
        if moving {
            window.request_animation_frame();
        } else {
            self.animating = false;
            self.last_frame = None;
        }
    }

    fn refresh_text(&mut self) {
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
struct Frame {
    start: f32,
    end: f32,
    ang: f32,
    f: f32,
    s: f32,
    dir: f32,
    thickness: f32,
    comet_width: f32,
    comet_reach: f32,
    accent: Hsla,
    ink: Hsla,
    dragging: bool,
}

fn stroke_arc(window: &mut Window, c: Point<Pixels>, r: f32, a0: f32, a1: f32, w: f32, color: Hsla) {
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
    window.paint_quad(PaintQuad { corner_radii: Corners::all(px(r)), ..fill(bounds, color) });
}

fn paint_dial(window: &mut Window, bounds: Bounds<Pixels>, fr: &Frame) {
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

    dot(window, point_at(c, r, fr.ang), fr.thickness * 1.8 * scale, fr.accent);
}

impl Render for CometDial {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.animating {
            self.step(window);
        }
        self.refresh_text();

        let c = &self.config;
        let theme = cx.theme();
        let accent = c.accent.unwrap_or(theme.foreground);
        let ink = c.ink.unwrap_or(theme.foreground);

        let f = ((self.reading - c.min) / self.range()).clamp(0.0, 1.0);
        let start = self.start();
        let frame = Frame {
            start,
            end: start + c.sweep,
            ang: start + f * c.sweep,
            f,
            s: (self.smooth_v.abs() / V_FULL).clamp(0.0, 1.0),
            dir: if self.smooth_v < 0.0 { -1.0 } else { 1.0 },
            thickness: c.thickness,
            comet_width: c.comet_width,
            comet_reach: c.comet_reach,
            accent,
            ink,
            dragging: self.grip.is_some(),
        };

        let bounds_cell = self.bounds.clone();
        let entity = cx.entity();
        let (size_px, disabled) = (c.size, c.disabled);
        let unit = c.unit.clone();

        div()
            .id("comet-dial")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .relative()
            .size(px(size_px))
            .when(disabled, |d| d.opacity(0.55))
            .when(!disabled, |d| d.cursor_grab())
            .on_action(cx.listener(|this, _: &Increment, _, cx| {
                this.nudge(this.value + this.key_step(), cx)
            }))
            .on_action(cx.listener(|this, _: &Decrement, _, cx| {
                this.nudge(this.value - this.key_step(), cx)
            }))
            .on_action(cx.listener(|this, _: &IncrementLarge, _, cx| {
                this.nudge(this.value + this.key_step() * 10.0, cx)
            }))
            .on_action(cx.listener(|this, _: &DecrementLarge, _, cx| {
                this.nudge(this.value - this.key_step() * 10.0, cx)
            }))
            .on_action(cx.listener(|this, _: &ToMin, _, cx| this.nudge(this.config.min, cx)))
            .on_action(cx.listener(|this, _: &ToMax, _, cx| this.nudge(this.config.max, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                    this.on_down(ev.position, window, cx);
                }),
            )
            .child(
                canvas(
                    move |bounds, _, _| bounds_cell.set(bounds),
                    move |bounds, (), window, _| {
                        paint_dial(window, bounds, &frame);

                        if frame.dragging {
                            let e = entity.clone();
                            window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                                if phase != DispatchPhase::Capture {
                                    return;
                                }
                                e.update(cx, |this, cx| {
                                    if ev.pressed_button == Some(MouseButton::Left) {
                                        this.on_move(ev.position, cx);
                                    } else {
                                        this.on_up(cx); // mouse-up happened off-window
                                    }
                                });
                            });
                            let e = entity.clone();
                            window.on_mouse_event(move |ev: &MouseUpEvent, phase, _, cx| {
                                if phase == DispatchPhase::Capture && ev.button == MouseButton::Left {
                                    e.update(cx, |this, cx| this.on_up(cx));
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
                            .text_color(ink)
                            .child(self.text.clone())
                            .when(!unit.is_empty(), |d| {
                                d.child(
                                    div().text_size(px(size_px * 0.08)).opacity(0.5).child(unit),
                                )
                            }),
                    ),
            )
    }
}
