#![cfg(feature = "option-wheel")]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::needless_pass_by_value
)]

use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use zopra::component;

pub use super::{Next, Prev};

pub const CONTEXT: &str = "OptionWheel";
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionChanged {
    pub index: usize,
    pub item: SharedString,
}

pub type OptionWheelChangeCallback = Arc<dyn Fn(usize, SharedString, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub start_y: Pixels,
    pub start_target: f32,
}

pub struct OptionWheelState {
    pub items: Vec<SharedString>,
    pub config: WheelConfig,
    pub selected: usize,
    pub current: f32,
    pub target: f32,
    pub animating: bool,
    pub last_frame: Option<Instant>,
    pub drag: Option<Drag>,
    pub dragged: bool,
    pub snap_task: Option<Task<()>>,
    pub focus_handle: FocusHandle,
}

impl EventEmitter<OptionChanged> for OptionWheelState {}

impl OptionWheelState {
    #[allow(clippy::cast_precision_loss)]
    pub fn new(
        items: Vec<SharedString>,
        selected: usize,
        config: WheelConfig,
        cx: &mut Context<Self>,
    ) -> Self {
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
    pub fn row_h(&self) -> f32 {
        (self.config.font_size * self.config.spacing).max(1.0)
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    pub fn set_target(&mut self, value: f32, snap: bool, cx: &mut Context<Self>) {
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

    pub fn schedule_snap(&mut self, cx: &mut Context<Self>) {
        self.snap_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SNAP_DELAY).await;
            this.update(cx, |this, cx| {
                let target = this.target;
                this.set_target(target, true, cx);
            })
            .ok();
        }));
    }

    pub fn end_drag(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() && self.dragged {
            let target = self.target;
            self.set_target(target, true, cx);
        }
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn step(&mut self, window: &mut Window) {
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

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    pub fn visible(&self) -> impl Iterator<Item = (usize, isize)> + '_ {
        let n = self.items.len() as isize;
        let mut r = self.config.visible_radius as isize;
        if self.config.looping {
            r = r.min((n - 1) / 2);
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

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]
#[component]
pub fn option_wheel(
    items: Vec<SharedString>,
    selected: Option<usize>,
    config: Option<WheelConfig>,
    side: Option<Side>,
    font_size: Option<f32>,
    spacing: Option<f32>,
    curve: Option<f32>,
    tilt: Option<f32>,
    fade: Option<f32>,
    min_opacity: Option<f32>,
    smoothing_ms: Option<f32>,
    inset: Option<f32>,
    looping: Option<bool>,
    draggable: Option<bool>,
    visible_radius: Option<usize>,
    text_color: Option<Hsla>,
    active_color: Option<Hsla>,
    on_change: Option<OptionWheelChangeCallback>,
) {
    let mut cfg = config.unwrap_or_default();
    if let Some(val) = side {
        cfg.side = val;
    }
    if let Some(val) = font_size {
        cfg.font_size = val;
    }
    if let Some(val) = spacing {
        cfg.spacing = val;
    }
    if let Some(val) = curve {
        cfg.curve = val;
    }
    if let Some(val) = tilt {
        cfg.tilt = val;
    }
    if let Some(val) = fade {
        cfg.fade = val;
    }
    if let Some(val) = min_opacity {
        cfg.min_opacity = val;
    }
    if let Some(val) = smoothing_ms {
        cfg.smoothing_ms = val;
    }
    if let Some(val) = inset {
        cfg.inset = val;
    }
    if let Some(val) = looping {
        cfg.looping = val;
    }
    if let Some(val) = draggable {
        cfg.draggable = val;
    }
    if let Some(val) = visible_radius {
        cfg.visible_radius = val;
    }
    if let Some(val) = text_color {
        cfg.text_color = Some(val);
    }
    if let Some(val) = active_color {
        cfg.active_color = Some(val);
    }

    let init_selected = selected.unwrap_or(0);
    let init_items = items;

    let state_holder = window.use_state(cx, |_window, cx| {
        cx.new(|cx| OptionWheelState::new(init_items, init_selected, cfg, cx))
    });

    let state = state_holder.read(cx).clone();

    if state.read(cx).animating {
        state.update(cx, |this, _cx| {
            this.step(window);
        });
    }

    let c = state.read(cx).config;
    let theme = cx.theme();
    let text = c.text_color.unwrap_or(theme.muted_foreground);
    let active = c.active_color.unwrap_or(theme.foreground);

    let row_h = state.read(cx).row_h();
    let tilt_rad = c.tilt.to_radians();
    let radius = if tilt_rad > 0.0005 {
        row_h / tilt_rad
    } else {
        0.0
    };
    let mirror = if c.side == Side::Right { -1.0 } else { 1.0 };
    let cur = state.read(cx).current;
    let sel = state.read(cx).selected;
    let focus = state.read(cx).focus_handle.clone();
    let dragging = state.read(cx).drag.is_some();

    let on_change_click = on_change.clone();
    let state_click = state.clone();

    let rows: Vec<_> = state
        .read(cx)
        .visible()
        .map(|(idx, k)| {
            let d = k as f32 - cur;
            let dist = d.abs();

            let (x, y) = if radius > 0.0 {
                let ang = (d * tilt_rad).clamp(-FRAC_PI_2, FRAC_PI_2);
                (
                    -mirror * radius * (1.0 - ang.cos()) * c.curve,
                    radius * ang.sin(),
                )
            } else {
                (0.0, d * row_h)
            };

            let opacity = (1.0 - dist * c.fade).max(c.min_opacity);
            let color = mix(text, active, 1.0 - dist.min(1.0));
            let weight = if idx == sel {
                FontWeight::MEDIUM
            } else {
                FontWeight::EXTRA_LIGHT
            };

            let label = state.read(cx).items[idx].clone();
            let state_cell = state_click.clone();
            let on_change_cell = on_change_click.clone();

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
                .cursor_pointer()
                .child(label)
                .on_click(move |_event, window, cx| {
                    state_cell.update(cx, |this, cx| {
                        if !this.dragged {
                            this.set_target(k as f32, true, cx);
                            if let Some(cb) = &on_change_cell {
                                cb(this.selected, this.items[this.selected].clone(), window, cx);
                            }
                        }
                    });
                })
        })
        .collect();

    let state_prev = state.clone();
    let state_next = state.clone();
    let state_scroll = state.clone();
    let state_down = state.clone();
    let state_move = state.clone();
    let state_up = state.clone();
    let state_up_out = state.clone();
    let on_change_prev = on_change.clone();
    let on_change_next = on_change.clone();
    let on_change_scroll = on_change.clone();
    let on_change_up = on_change.clone();
    let on_change_up_out = on_change;

    let el = div()
        .id("option-wheel")
        .key_context(CONTEXT)
        .track_focus(&focus)
        .relative()
        .size_full()
        .overflow_hidden()
        .when(c.draggable, |d| {
            if dragging {
                d.cursor_grabbing()
            } else {
                d.cursor_grab()
            }
        })
        .on_action(move |_: &Prev, window, cx| {
            state_prev.update(cx, |this, cx| {
                let target = this.target.round() - 1.0;
                this.set_target(target, true, cx);
                if let Some(cb) = &on_change_prev {
                    cb(this.selected, this.items[this.selected].clone(), window, cx);
                }
            });
        })
        .on_action(move |_: &Next, window, cx| {
            state_next.update(cx, |this, cx| {
                let target = this.target.round() + 1.0;
                this.set_target(target, true, cx);
                if let Some(cb) = &on_change_next {
                    cb(this.selected, this.items[this.selected].clone(), window, cx);
                }
            });
        })
        .on_scroll_wheel(move |ev: &ScrollWheelEvent, window, cx| {
            state_scroll.update(cx, |this, cx| {
                let dy = f32::from(ev.delta.pixel_delta(px(24.0)).y);
                let step = (dy / this.row_h()).clamp(-1.0, 1.0);
                let target = this.target - step;
                this.set_target(target, false, cx);
                this.schedule_snap(cx);
                cx.stop_propagation();
                if let Some(cb) = &on_change_scroll {
                    cb(this.selected, this.items[this.selected].clone(), window, cx);
                }
            });
        })
        .on_mouse_down(
            MouseButton::Left,
            move |ev: &MouseDownEvent, _window, cx| {
                state_down.update(cx, |this, _cx| {
                    if this.config.draggable {
                        this.dragged = false;
                        this.drag = Some(Drag {
                            start_y: ev.position.y,
                            start_target: this.target,
                        });
                    }
                });
            },
        )
        .on_mouse_move(move |ev: &MouseMoveEvent, _window, cx| {
            state_move.update(cx, |this, cx| {
                let Some(drag) = this.drag else { return };
                let dy = f32::from(ev.position.y - drag.start_y);
                let start = drag.start_target;
                if dy.abs() > DRAG_THRESHOLD {
                    this.dragged = true;
                }
                if this.dragged {
                    let target = start - dy / this.row_h();
                    this.set_target(target, false, cx);
                }
            });
        })
        .on_mouse_up(MouseButton::Left, move |_ev: &MouseUpEvent, window, cx| {
            state_up.update(cx, |this, cx| {
                this.end_drag(cx);
                if let Some(cb) = &on_change_up {
                    cb(this.selected, this.items[this.selected].clone(), window, cx);
                }
            });
        })
        .on_mouse_up_out(MouseButton::Left, move |_ev: &MouseUpEvent, window, cx| {
            state_up_out.update(cx, |this, cx| {
                this.end_drag(cx);
                if let Some(cb) = &on_change_up_out {
                    cb(this.selected, this.items[this.selected].clone(), window, cx);
                }
            });
        })
        .children(rows);

    el.into_any_element()
}

impl OptionWheelProps {
    #[must_use]
    pub fn on_option_change(
        mut self,
        handler: impl Fn(usize, SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Arc::new(handler));
        self
    }
}
