#![cfg(feature = "discover-button")]
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
    clippy::manual_midpoint
)]

use std::sync::Arc;
use std::time::Instant;

use gpui_kit::component::{ActiveTheme as _, Icon, IconName};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use zopra::component;

pub use super::gpui::{DiscoverClicked, DiscoverConfig};

#[derive(Clone, Copy)]
struct Bezier(f32, f32, f32, f32);

impl Bezier {
    fn at(a: f32, b: f32, t: f32) -> f32 {
        let u = 1.0 - t;
        3.0 * a * u * u * t + 3.0 * b * u * t * t + t * t * t
    }

    fn slope(a: f32, b: f32, t: f32) -> f32 {
        let u = 1.0 - t;
        3.0 * u * u * a + 6.0 * u * t * (b - a) + 3.0 * t * t * (1.0 - b)
    }

    fn ease(self, x: f32) -> f32 {
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        let Self(x1, y1, x2, y2) = self;

        let mut t = x;
        for _ in 0..6 {
            let err = Self::at(x1, x2, t) - x;
            if err.abs() < 1e-5 {
                return Self::at(y1, y2, t);
            }
            let d = Self::slope(x1, x2, t);
            if d.abs() < 1e-6 {
                break;
            }
            t -= err / d;
        }

        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        t = x;
        for _ in 0..24 {
            let v = Self::at(x1, x2, t);
            if (v - x).abs() < 1e-5 {
                break;
            }
            if v < x {
                lo = t;
            } else {
                hi = t;
            }
            t = (lo + hi) / 2.0;
        }
        Self::at(y1, y2, t)
    }
}

const FILL_EASE: Bezier = Bezier(0.65, 0.0, 0.076, 1.0);
const COLOR_EASE: Bezier = Bezier(0.25, 0.1, 0.25, 1.0);
const ENTER_EASE: Bezier = Bezier(0.22, 1.0, 0.36, 1.0);

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

pub type DiscoverClickCallback = Arc<dyn Fn(&mut Window, &mut App)>;

pub struct DiscoverButtonState {
    pub config: DiscoverConfig,
    pub hovered: bool,
    pub focus_visible: bool,
    pub focused: bool,
    pub fill_t: f32,
    pub color_t: f32,
    pub enter_t: f32,
    pub animating: bool,
    pub last_frame: Option<Instant>,
    pub bounds: Bounds<Pixels>,
    pub focus_handle: FocusHandle,
}

impl DiscoverButtonState {
    #[must_use]
    pub fn new(config: DiscoverConfig, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle().tab_stop(true);
        let reduce = config.reduce_motion;
        Self {
            config,
            hovered: false,
            focus_visible: false,
            focused: false,
            fill_t: 0.0,
            color_t: 0.0,
            enter_t: if reduce { 1.0 } else { 0.0 },
            animating: false,
            last_frame: None,
            bounds: Bounds::default(),
            focus_handle,
        }
    }

    #[must_use]
    pub const fn expanded(&self) -> bool {
        self.hovered || (self.focused && self.focus_visible)
    }

    fn wake(&mut self, cx: &mut Context<Self>) {
        if !self.animating {
            self.animating = true;
            self.last_frame = None;
        }
        cx.notify();
    }

    pub fn set_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.hovered = hovered;
        self.wake(cx);
    }

    pub fn step(&mut self, window: &mut Window) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .replace(now)
            .map_or(1.0 / 60.0, |p| (now - p).as_secs_f32().min(0.05));
        let c = &self.config;

        let up = self.expanded();
        let dir = if up { 1.0 } else { -1.0 };
        let goal = if up { 1.0 } else { 0.0 };

        if c.reduce_motion {
            self.fill_t = goal;
            self.color_t = goal;
            self.enter_t = 1.0;
        } else {
            self.fill_t = (self.fill_t + dir * dt * 1000.0 / c.fill_ms.max(1.0)).clamp(0.0, 1.0);
            self.color_t = (self.color_t + dir * dt * 1000.0 / c.color_ms.max(1.0)).clamp(0.0, 1.0);
            self.enter_t = (self.enter_t + dt * 1000.0 / c.enter_ms.max(1.0)).min(1.0);
        }

        let moving = self.enter_t < 1.0
            || (self.fill_t - goal).abs() > 0.001
            || (self.color_t - goal).abs() > 0.001;
        if moving {
            window.request_animation_frame();
        } else {
            self.animating = false;
            self.last_frame = None;
        }
    }
}

/// A reactive pill button with expanding circular fill on hover and cubic-bezier easing.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[component]
pub fn discover_button(
    label: Option<SharedString>,
    href: Option<SharedString>,
    config: Option<DiscoverConfig>,
    height: Option<f32>,
    fill_size: Option<f32>,
    icon_size: Option<f32>,
    surface: Option<Hsla>,
    fill: Option<Hsla>,
    text_color: Option<Hsla>,
    active_text: Option<Hsla>,
    on_click: Option<DiscoverClickCallback>,
) {
    let mut cfg = config.unwrap_or_default();
    if let Some(l) = label {
        cfg.label = l;
    }
    if let Some(h) = href {
        cfg.href = Some(h);
    }
    if let Some(h) = height {
        cfg.height = h;
    }
    if let Some(fs) = fill_size {
        cfg.fill_size = fs;
    }
    if let Some(is) = icon_size {
        cfg.icon_size = is;
    }
    if let Some(s) = surface {
        cfg.surface = s;
    }
    if let Some(f) = fill {
        cfg.fill = f;
    }
    if let Some(tc) = text_color {
        cfg.text = tc;
    }
    if let Some(at) = active_text {
        cfg.active_text = at;
    }

    let state_holder = window.use_state(cx, |_window, cx| {
        cx.new(|cx| DiscoverButtonState::new(cfg.clone(), cx))
    });

    let state = state_holder.read(cx).clone();

    if state.read(cx).animating {
        state.update(cx, |this, _cx| {
            this.step(window);
        });
    }

    let c = state.read(cx).config.clone();
    let fill_e = FILL_EASE.ease(state.read(cx).fill_t);
    let color_e = COLOR_EASE.ease(state.read(cx).color_t);
    let enter_e = ENTER_EASE.ease(state.read(cx).enter_t);

    let button_w = f32::from(state.read(cx).bounds.size.width);
    let full = (button_w - 4.0).max(c.fill_size);
    let fill_w = c.fill_size + (full - c.fill_size) * fill_e;

    let text_clr = mix(c.text, c.active_text, color_e);
    let ring = cx.theme().ring;
    let icon_box = c.height - 2.0;

    let state_probe = state.clone();
    let state_hover = state.clone();
    let state_click = state;

    let on_click_cb = on_click;
    let href_opt = c.href.clone();
    let label_str = c.label.clone();
    let focus_handle = state_probe.read(cx).focus_handle.clone();
    let focus_visible = state_probe.read(cx).focus_visible;

    div()
        .relative()
        .flex()
        .flex_none()
        .max_w_full()
        .opacity(enter_e)
        .when(focus_visible, |d| {
            d.child(
                div()
                    .absolute()
                    .top(px(-6.0))
                    .left(px(-6.0))
                    .right(px(-6.0))
                    .bottom(px(-6.0))
                    .rounded_full()
                    .border_2()
                    .border_color(ring),
            )
        })
        .child(
            div()
                .id("discover-button")
                .track_focus(&focus_handle)
                .relative()
                .flex()
                .items_center()
                .min_h(px(c.height))
                .rounded_full()
                .border_1()
                .border_color(white().opacity(0.3))
                .bg(c.surface)
                .cursor_pointer()
                .text_size(px(16.0))
                .font_weight(FontWeight::SEMIBOLD)
                .line_height(relative(1.1))
                .whitespace_nowrap()
                .on_hover(move |hovered: &bool, _window, cx| {
                    state_hover.update(cx, |this, cx| {
                        this.set_hovered(*hovered, cx);
                    });
                })
                .on_click(move |_: &ClickEvent, window, cx| {
                    if let Some(href) = &href_opt {
                        cx.open_url(href);
                    }
                    if let Some(cb) = &on_click_cb {
                        cb(window, cx);
                    }
                    state_click.update(cx, |this, cx| {
                        this.wake(cx);
                    });
                })
                .child(
                    canvas(
                        move |bounds, _window, cx| {
                            state_probe.update(cx, |this, _cx| {
                                this.bounds = bounds;
                            });
                        },
                        |_, (), _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                // Expanding background fill
                .child(
                    div()
                        .absolute()
                        .top(px(2.0))
                        .bottom(px(2.0))
                        .left(px(2.0))
                        .w(px(fill_w))
                        .rounded_full()
                        .bg(c.fill)
                        .border_1()
                        .border_color(white().opacity(0.12)),
                )
                // Left circular icon box
                .child(
                    div()
                        .relative()
                        .left(px(c.icon_shift * fill_e))
                        .flex_none()
                        .size(px(icon_box))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(c.active_text)
                        .child(Icon::new(IconName::ArrowRight).size(px(c.icon_size))),
                )
                // Label
                .child(
                    div()
                        .relative()
                        .pl(px(15.0))
                        .pr(px(27.0))
                        .text_center()
                        .text_color(text_clr)
                        .child(label_str),
                ),
        )
}
