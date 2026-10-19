#![cfg(feature = "magnetic-button")]
#![allow(
    clippy::cast_precision_loss,
    clippy::similar_names,
    clippy::redundant_closure_for_method_calls
)]

use std::{cell::Cell, rc::Rc, time::Instant};

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    Bounds, ClickEvent, Context, EventEmitter, IntoElement, MouseMoveEvent, Pixels, Point, Render,
    SharedString, Size, Window, canvas, div, point, prelude::*, px,
};

#[derive(Clone, Copy, Debug)]
pub struct MagneticConfig {
    /// Button size.
    pub size: Size<Pixels>,
    /// How far beyond the button's edges the magnet reaches.
    pub radius: Pixels,
    /// Fraction of the cursor offset the button follows (0.0..=1.0).
    pub strength: f32,
    /// Maximum travel distance from the resting position.
    pub max_offset: Pixels,
    /// Easing speed in 1/seconds. Higher is snappier, lower is floatier.
    pub responsiveness: f32,
}

impl Default for MagneticConfig {
    fn default() -> Self {
        Self {
            size: Size {
                width: px(120.),
                height: px(44.),
            },
            radius: px(40.),
            strength: 0.35,
            max_offset: px(14.),
            responsiveness: 14.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MagneticClicked;

pub struct MagneticButton {
    label: SharedString,
    config: MagneticConfig,
    zone_bounds: Rc<Cell<Bounds<Pixels>>>,
    current: Point<f32>,
    target: Point<f32>,
    animating: bool,
    last_frame: Option<Instant>,
}

impl EventEmitter<MagneticClicked> for MagneticButton {}

impl MagneticButton {
    pub fn new(label: impl Into<SharedString>, config: MagneticConfig) -> Self {
        Self {
            label: label.into(),
            config,
            zone_bounds: Rc::default(),
            current: Point::default(),
            target: Point::default(),
            animating: false,
            last_frame: None,
        }
    }

    pub fn set_config(&mut self, config: MagneticConfig, cx: &mut Context<Self>) {
        self.config = config;
        cx.notify();
    }

    fn follow(&mut self, cursor: Point<Pixels>, cx: &mut Context<Self>) {
        let center = self.zone_bounds.get().center();
        let mut dx = f32::from(cursor.x - center.x) * self.config.strength;
        let mut dy = f32::from(cursor.y - center.y) * self.config.strength;

        let max = f32::from(self.config.max_offset);
        let len = dx.hypot(dy);
        if len > max {
            let scale = max / len;
            dx *= scale;
            dy *= scale;
        }
        self.set_target(point(dx, dy), cx);
    }

    fn release(&mut self, cx: &mut Context<Self>) {
        self.set_target(Point::default(), cx);
    }

    fn set_target(&mut self, target: Point<f32>, cx: &mut Context<Self>) {
        self.target = target;
        if !self.animating {
            self.animating = true;
            self.last_frame = None;
            cx.notify();
        }
    }

    fn step(&mut self, window: &mut Window) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .replace(now)
            .map_or(1.0 / 60.0, |prev| (now - prev).as_secs_f32().min(0.05));

        let k = 1.0 - (-self.config.responsiveness * dt).exp();
        self.current.x += (self.target.x - self.current.x) * k;
        self.current.y += (self.target.y - self.current.y) * k;

        let settled = (self.target.x - self.current.x).abs() < 0.05
            && (self.target.y - self.current.y).abs() < 0.05;
        if settled {
            self.current = self.target;
            self.animating = false;
            self.last_frame = None;
        } else {
            window.request_animation_frame();
        }
    }
}

impl Render for MagneticButton {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.animating {
            self.step(window);
        }

        let c = self.config;
        let theme = cx.theme();
        let (bg, fg, hover_bg) = (theme.primary, theme.primary_foreground, theme.primary_hover);
        let bounds_cell = self.zone_bounds.clone();

        div()
            .id("magnetic-zone")
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .w(c.size.width + c.radius * 2.0)
            .h(c.size.height + c.radius * 2.0)
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                this.follow(ev.position, cx);
            }))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !*hovered {
                    this.release(cx);
                }
            }))
            .child(
                canvas(
                    move |bounds, _, _| bounds_cell.set(bounds),
                    |_, (), _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .id("magnetic-button")
                    .relative()
                    .left(px(self.current.x))
                    .top(px(self.current.y))
                    .w(c.size.width)
                    .h(c.size.height)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(bg)
                    .text_color(fg)
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover_bg))
                    .child(self.label.clone())
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                        cx.emit(MagneticClicked);
                    })),
            )
    }
}
