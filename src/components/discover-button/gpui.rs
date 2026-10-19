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
    clippy::manual_midpoint,
    clippy::unusual_byte_groupings
)]

use std::{cell::Cell, rc::Rc, time::Instant};

use gpui_kit::component::{ActiveTheme as _, Icon, IconName};
use gpui_kit::{
    App, Bounds, ClickEvent, Context, EventEmitter, FocusHandle, Focusable, FontWeight, Hsla,
    Pixels, Render, Rgba, SharedString, Subscription, Window, canvas, div, prelude::*, px,
    relative, rgb, rgba, white,
};

/// Emitted on click or keyboard activation.
#[derive(Clone, Debug)]
pub struct DiscoverClicked;

// ---- cubic-bezier easing -----------------------------------------------------

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

    /// CSS-style timing function: maps progress `x` to eased progress.
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

        // Newton wandered off or stalled: bisect.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        t = x;
        for _ in 0..24 {
            let v = Self::at(x1, x2, t);
            if (v - x).abs() < 1e-5 {
                break;
            }
            if v < x {
                lo = t
            } else {
                hi = t
            }
            t = (lo + hi) / 2.0;
        }
        Self::at(y1, y2, t)
    }
}

const FILL_EASE: Bezier = Bezier(0.65, 0.0, 0.076, 1.0);
const COLOR_EASE: Bezier = Bezier(0.25, 0.1, 0.25, 1.0); // CSS `ease`
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

// ---- config ----------------------------------------------------------------

/// Tunable parameters. Override a few with `..Default::default()`.
#[derive(Clone, Debug)]
pub struct DiscoverConfig {
    pub label: SharedString,
    /// Opened with the system handler on click (in addition to the event).
    pub href: Option<SharedString>,
    pub height: f32,
    /// Width of the dark circle at rest.
    pub fill_size: f32,
    pub icon_size: f32,
    /// Icon nudge to the right when expanded.
    pub icon_shift: f32,
    pub fill_ms: f32,
    pub color_ms: f32,
    pub enter_ms: f32,
    pub reduce_motion: bool,
    pub surface: Hsla,
    pub text: Hsla,
    pub fill: Hsla,
    pub active_text: Hsla,
}

impl Default for DiscoverConfig {
    fn default() -> Self {
        Self {
            label: "Discover Components".into(),
            href: None,
            height: 66.0,
            fill_size: 60.0,
            icon_size: 25.0,
            icon_shift: 7.0,
            fill_ms: 480.0,
            color_ms: 320.0,
            enter_ms: 500.0,
            reduce_motion: false,
            surface: rgba(0xededed_eb).into(),
            text: rgb(0x171717).into(),
            fill: rgb(0x171717).into(),
            active_text: rgb(0xffffff).into(),
        }
    }
}

// ---- view ------------------------------------------------------------------

pub struct DiscoverButton {
    config: DiscoverConfig,
    hovered: bool,
    /// Keyboard focus ring (not set by mouse focus).
    focus_visible: bool,
    focused: bool,
    /// Linear progress values; curves are applied at render time.
    fill_t: f32,
    color_t: f32,
    enter_t: f32,
    animating: bool,
    last_frame: Option<Instant>,
    /// Written during prepaint; the fill needs the final button width.
    bounds: Rc<Cell<Bounds<Pixels>>>,
    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}

impl EventEmitter<DiscoverClicked> for DiscoverButton {}

impl Focusable for DiscoverButton {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl DiscoverButton {
    pub fn new(config: DiscoverConfig, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle().tab_stop(true);
        let subs = vec![
            cx.on_focus(&focus_handle, window, |this, window, cx| {
                this.focused = true;
                // If your gpui build lacks this, treat every focus as visible.
                this.focus_visible = window.last_input_was_keyboard();
                this.wake(cx);
            }),
            cx.on_blur(&focus_handle, window, |this, _, cx| {
                this.focused = false;
                this.focus_visible = false;
                this.wake(cx);
            }),
        ];

        let reduce = config.reduce_motion;
        let mut this = Self {
            config,
            hovered: false,
            focus_visible: false,
            focused: false,
            fill_t: 0.0,
            color_t: 0.0,
            enter_t: if reduce { 1.0 } else { 0.0 },
            animating: false,
            last_frame: None,
            bounds: Rc::default(),
            focus_handle,
            _subs: subs,
        };
        this.wake(cx);
        this
    }

    pub fn set_config(&mut self, config: DiscoverConfig, cx: &mut Context<Self>) {
        self.config = config;
        cx.notify();
    }

    fn wake(&mut self, cx: &mut Context<Self>) {
        if !self.animating {
            self.animating = true;
            self.last_frame = None;
            cx.notify();
        }
    }

    const fn expanded(&self) -> bool {
        self.hovered || (self.focused && self.focus_visible)
    }

    fn clicked(&mut self, cx: &mut Context<Self>) {
        cx.emit(DiscoverClicked);
        if let Some(href) = &self.config.href {
            cx.open_url(href);
        }
    }

    fn step(&mut self, window: &mut Window) {
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

        let moving = self.enter_t < 1.0 || self.fill_t != goal || self.color_t != goal;
        if moving {
            window.request_animation_frame();
        } else {
            self.animating = false;
            self.last_frame = None;
        }
    }
}

impl Render for DiscoverButton {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.animating {
            self.step(window);
        }

        let c = &self.config;
        let fill_e = FILL_EASE.ease(self.fill_t);
        let color_e = COLOR_EASE.ease(self.color_t);
        let enter_e = ENTER_EASE.ease(self.enter_t);

        // Fill grows from a circle to the full pill (minus the 2px inset on both sides).
        let button_w = f32::from(self.bounds.get().size.width);
        let full = (button_w - 4.0).max(c.fill_size);
        let fill_w = c.fill_size + (full - c.fill_size) * fill_e;

        let text_color = mix(c.text, c.active_text, color_e);
        let ring = cx.theme().ring;
        let probe = self.bounds.clone();
        let icon_box = c.height - 2.0;

        div()
            .relative()
            .flex()
            .flex_none()
            .max_w_full()
            .opacity(enter_e)
            .when(self.focus_visible, |d| {
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
                    .track_focus(&self.focus_handle)
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
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        this.hovered = *hovered;
                        this.wake(cx);
                    }))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.clicked(cx)))
                    // Bounds probe: the fill needs the button's final width.
                    .child(
                        canvas(move |bounds, _, _| probe.set(bounds), |_, (), _, _| {})
                            .absolute()
                            .size_full(),
                    )
                    // Fill.
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
                    // Icon.
                    .child(
                        div()
                            .relative()
                            .left(px(c.icon_shift * fill_e)) // visual shift only; layout unchanged
                            .flex_none()
                            .size(px(icon_box))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(c.active_text)
                            .child(Icon::new(IconName::ArrowRight).size(px(c.icon_size))),
                    )
                    // Label.
                    .child(
                        div()
                            .relative()
                            .pl(px(15.0))
                            .pr(px(27.0))
                            .text_center()
                            .text_color(text_color)
                            .child(c.label.clone()),
                    ),
            )
    }
}
