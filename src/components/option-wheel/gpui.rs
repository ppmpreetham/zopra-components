#![cfg(feature = "option-wheel")]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::redundant_closure_for_method_calls
)]

use std::{
    f32::consts::FRAC_PI_2,
    time::{Duration, Instant},
};

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    App, ClickEvent, Context, EventEmitter, FocusHandle, Focusable, FontWeight, Hsla, KeyBinding,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Render, Rgba,
    ScrollWheelEvent, SharedString, Task, Window, div, prelude::*, px, relative,
};

use super::{Next, Prev};

const CONTEXT: &str = "OptionWheel";
const DRAG_THRESHOLD: f32 = 4.0;
const SNAP_DELAY: Duration = Duration::from_millis(140);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", Prev, Some(CONTEXT)),
        KeyBinding::new("left", Prev, Some(CONTEXT)),
        KeyBinding::new("down", Next, Some(CONTEXT)),
        KeyBinding::new("right", Next, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    #[default]
    Left,
    Right,
}

/// Tunable parameters. Override a few with `..Default::default()`.
#[derive(Clone, Copy, Debug)]
pub struct WheelConfig {
    pub side: Side,
    /// Font size in px.
    pub font_size: f32,
    /// Row height as a multiple of `font_size`.
    pub spacing: f32,
    /// Horizontal bulge of the arc (0 = straight list).
    pub curve: f32,
    /// Arc angle per row in degrees; lower = flatter arc.
    pub tilt: f32,
    /// Opacity lost per row of distance from the centre.
    pub fade: f32,
    pub min_opacity: f32,
    /// Easing time constant in ms; higher = floatier.
    pub smoothing_ms: f32,
    /// Distance from the anchored edge in px.
    pub inset: f32,
    pub looping: bool,
    pub draggable: bool,
    /// Rows rendered on each side of the centre.
    pub visible_radius: usize,
    /// `None` falls back to theme colours.
    pub text_color: Option<Hsla>,
    pub active_color: Option<Hsla>,
}

impl Default for WheelConfig {
    fn default() -> Self {
        Self {
            side: Side::Left,
            font_size: 48.0,
            spacing: 1.4,
            curve: 1.0,
            tilt: 6.0,
            fade: 0.25,
            min_opacity: 0.05,
            smoothing_ms: 200.0,
            inset: 80.0,
            looping: false,
            draggable: true,
            visible_radius: 6,
            text_color: None,
            active_color: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OptionChanged {
    pub index: usize,
    pub item: SharedString,
}

struct Drag {
    start_y: Pixels,
    start_target: f32,
}

pub struct OptionWheel {
    items: Vec<SharedString>,
    config: WheelConfig,
    selected: usize,
    current: f32,
    target: f32,
    animating: bool,
    last_frame: Option<Instant>,
    drag: Option<Drag>,
    dragged: bool,
    snap_task: Option<Task<()>>,
    focus_handle: FocusHandle,
}

impl EventEmitter<OptionChanged> for OptionWheel {}

impl Focusable for OptionWheel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl OptionWheel {
    pub fn new(
        items: impl IntoIterator<Item = impl Into<SharedString>>,
        selected: usize,
        config: WheelConfig,
        cx: &mut Context<Self>,
    ) -> Self {
        let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
        let selected = selected.min(items.len().saturating_sub(1));
        Self {
            items,
            config,
            selected,
            current: selected as f32,
            target: selected as f32,
            animating: false,
            last_frame: None,
            drag: None,
            dragged: false,
            snap_task: None,
            focus_handle: cx.focus_handle(),
        }
    }

    #[must_use]
    pub const fn selected(&self) -> usize {
        self.selected
    }

    pub fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        self.set_target(index as f32, true, cx);
    }

    pub fn set_config(&mut self, config: WheelConfig, cx: &mut Context<Self>) {
        self.config = config;
        cx.notify();
    }

    fn row_h(&self) -> f32 {
        (self.config.font_size * self.config.spacing).max(1.0)
    }

    fn set_target(&mut self, value: f32, snap: bool, cx: &mut Context<Self>) {
        let n = self.items.len();
        if n == 0 {
            return;
        }
        let mut v = if self.config.looping {
            value
        } else {
            value.clamp(0.0, (n - 1) as f32)
        };
        if snap {
            v = v.round();
        }
        self.target = v;

        let idx = (v.round() as isize).rem_euclid(n as isize) as usize;
        if idx != self.selected {
            self.selected = idx;
            cx.emit(OptionChanged {
                index: idx,
                item: self.items[idx].clone(),
            });
        }

        if !self.animating {
            self.animating = true;
            self.last_frame = None;
            cx.notify();
        }
    }

    fn schedule_snap(&mut self, cx: &mut Context<Self>) {
        self.snap_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SNAP_DELAY).await;
            this.update(cx, |this, cx| {
                let target = this.target;
                this.set_target(target, true, cx);
            })
            .ok();
        }));
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() && self.dragged {
            let target = self.target;
            self.set_target(target, true, cx);
        }
    }

    /// Frame-rate independent exponential easing; stops requesting frames once settled.
    fn step(&mut self, window: &mut Window) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .replace(now)
            .map_or(1.0 / 60.0, |prev| (now - prev).as_secs_f32().min(0.05));

        let tau = (self.config.smoothing_ms / 1000.0).max(0.001);
        let k = 1.0 - (-dt / tau).exp();
        self.current += (self.target - self.current) * k;

        if (self.target - self.current).abs() < 0.001 {
            self.current = self.target;
            self.animating = false;
            self.last_frame = None;

            let n = self.items.len() as f32;
            if self.config.looping && n > 0.0 {
                let shift = (self.target / n).floor() * n;
                self.target -= shift;
                self.current -= shift;
            }
        } else {
            window.request_animation_frame();
        }
    }

    fn visible(&self) -> impl Iterator<Item = (usize, isize)> + '_ {
        let n = self.items.len() as isize;
        let mut r = self.config.visible_radius as isize;
        if self.config.looping {
            r = r.min((n - 1) / 2); // guarantees no duplicate indices
        }
        let base = self.current.round() as isize;
        let looping = self.config.looping;

        (base - r..=base + r).filter_map(move |k| {
            if looping {
                Some((k.rem_euclid(n.max(1)) as usize, k))
            } else if (0..n).contains(&k) {
                Some((k as usize, k))
            } else {
                None
            }
        })
    }
}

fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b) = (Rgba::from(a), Rgba::from(b));
    Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
    .into()
}

impl Render for OptionWheel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.animating {
            self.step(window);
        }

        let c = self.config;
        let theme = cx.theme();
        let text = c.text_color.unwrap_or(theme.muted_foreground);
        let active = c.active_color.unwrap_or(theme.foreground);

        let row_h = self.row_h();
        let tilt = c.tilt.to_radians();
        let radius = if tilt > 0.0005 { row_h / tilt } else { 0.0 };
        let mirror = if c.side == Side::Right { -1.0 } else { 1.0 };
        let cur = self.current;

        let rows: Vec<_> = self
            .visible()
            .map(|(idx, k)| {
                let d = k as f32 - cur;
                let dist = d.abs();

                let (x, y) = if radius > 0.0 {
                    let ang = (d * tilt).clamp(-FRAC_PI_2, FRAC_PI_2);
                    (
                        -mirror * radius * (1.0 - ang.cos()) * c.curve,
                        radius * ang.sin(),
                    )
                } else {
                    (0.0, d * row_h)
                };

                let opacity = (1.0 - dist * c.fade).max(c.min_opacity);
                let color = mix(text, active, 1.0 - dist.min(1.0));
                let weight = if idx == self.selected {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::EXTRA_LIGHT
                };

                div()
                    .id(("option", idx))
                    .absolute()
                    .top(relative(0.5))
                    .mt(px(y - row_h / 2.0))
                    .map(|d| match c.side {
                        Side::Left => d.left(px(c.inset + x)),
                        Side::Right => d.right(px(c.inset - x)),
                    })
                    .h(px(row_h))
                    .flex()
                    .items_center()
                    .whitespace_nowrap()
                    .text_size(px(c.font_size))
                    .font_weight(weight)
                    .text_color(color)
                    .opacity(opacity)
                    .child(self.items[idx].clone())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if !this.dragged {
                            this.set_target(k as f32, true, cx);
                        }
                    }))
            })
            .collect();

        let dragging = self.drag.is_some();

        div()
            .id("option-wheel")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .overflow_hidden()
            .when(c.draggable, |d| {
                d.when(dragging, |d| d.cursor_grabbing())
                    .when(!dragging, |d| d.cursor_grab())
            })
            .on_action(cx.listener(|this, _: &Prev, _, cx| {
                this.set_target(this.target.round() - 1.0, true, cx);
            }))
            .on_action(cx.listener(|this, _: &Next, _, cx| {
                this.set_target(this.target.round() + 1.0, true, cx);
            }))
            .on_scroll_wheel(cx.listener(|this, ev: &ScrollWheelEvent, _, cx| {
                let dy = f32::from(ev.delta.pixel_delta(px(24.0)).y);
                let step = (dy / this.row_h()).clamp(-1.0, 1.0);
                this.set_target(this.target - step, false, cx);
                this.schedule_snap(cx);
                cx.stop_propagation();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, _, _| {
                    if this.config.draggable {
                        this.dragged = false;
                        this.drag = Some(Drag {
                            start_y: ev.position.y,
                            start_target: this.target,
                        });
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                let Some(drag) = this.drag.as_ref() else {
                    return;
                };
                let dy = f32::from(ev.position.y - drag.start_y);
                let start = drag.start_target;
                if dy.abs() > DRAG_THRESHOLD {
                    this.dragged = true;
                }
                if this.dragged {
                    let target = start - dy / this.row_h();
                    this.set_target(target, false, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.end_drag(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.end_drag(cx)),
            )
            .children(rows)
    }
}
