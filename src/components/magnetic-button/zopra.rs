#![cfg(feature = "magnetic-button")]
#![allow(
    clippy::cast_precision_loss,
    clippy::similar_names,
    clippy::needless_pass_by_value
)]

use std::sync::Arc;
use std::time::Instant;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use zopra::component;

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
                width: px(120.0),
                height: px(44.0),
            },
            radius: px(40.0),
            strength: 0.35,
            max_offset: px(14.0),
            responsiveness: 14.0,
        }
    }
}

pub type MagneticClickCallback = Arc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

pub struct MagneticButtonState {
    pub zone_bounds: Bounds<Pixels>,
    pub current: Point<f32>,
    pub target: Point<f32>,
    pub animating: bool,
    pub last_frame: Option<Instant>,
}

impl MagneticButtonState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            zone_bounds: Bounds::default(),
            current: Point::default(),
            target: Point::default(),
            animating: false,
            last_frame: None,
        }
    }

    pub fn follow(&mut self, cursor: Point<Pixels>, strength: f32, max_offset: f32) {
        let center = self.zone_bounds.center();
        let mut dx = f32::from(cursor.x - center.x) * strength;
        let mut dy = f32::from(cursor.y - center.y) * strength;

        let len = dx.hypot(dy);
        if len > max_offset && len > 0.0 {
            let scale = max_offset / len;
            dx *= scale;
            dy *= scale;
        }
        self.target = point(dx, dy);
    }

    pub fn release(&mut self) {
        self.target = Point::default();
    }

    pub fn step(&mut self, responsiveness: f32, window: &mut Window) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .replace(now)
            .map_or(1.0 / 60.0, |prev| (now - prev).as_secs_f32().min(0.05));

        let k = 1.0 - (-responsiveness * dt).exp();
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

impl Default for MagneticButtonState {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[component]
pub fn magnetic_button(
    label: Option<SharedString>,
    config: Option<MagneticConfig>,
    width: Option<f32>,
    height: Option<f32>,
    radius: Option<f32>,
    strength: Option<f32>,
    max_offset: Option<f32>,
    responsiveness: Option<f32>,
    bg: Option<Hsla>,
    text_color: Option<Hsla>,
    on_click: Option<MagneticClickCallback>,
) {
    let mut cfg = config.unwrap_or_default();
    if let Some(w) = width {
        cfg.size.width = px(w);
    }
    if let Some(h) = height {
        cfg.size.height = px(h);
    }
    if let Some(r) = radius {
        cfg.radius = px(r);
    }
    if let Some(s) = strength {
        cfg.strength = s;
    }
    if let Some(m) = max_offset {
        cfg.max_offset = px(m);
    }
    if let Some(resp) = responsiveness {
        cfg.responsiveness = resp;
    }

    let state_holder = window.use_state(cx, |_window, cx| cx.new(|_cx| MagneticButtonState::new()));

    let state = state_holder.read(cx).clone();

    if state.read(cx).animating {
        state.update(cx, |this, _cx| {
            this.step(cfg.responsiveness, window);
        });
    }

    let theme = cx.theme();
    let btn_bg = bg.unwrap_or(theme.primary);
    let btn_fg = text_color.unwrap_or(theme.primary_foreground);
    let hover_bg = theme.primary_hover;

    let current_x = state.read(cx).current.x;
    let current_y = state.read(cx).current.y;

    let state_probe = state.clone();
    let state_move = state.clone();
    let state_hover = state;

    let strn = cfg.strength;
    let max_off = f32::from(cfg.max_offset);

    let inner_content: AnyElement = if let Some(lbl) = label {
        div().child(lbl).into_any_element()
    } else {
        div().child("Magnetic").into_any_element()
    };

    let el = div()
        .id("magnetic-zone")
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .w(cfg.size.width + cfg.radius * 2.0)
        .h(cfg.size.height + cfg.radius * 2.0)
        .on_mouse_move(move |ev: &MouseMoveEvent, _window, cx| {
            state_move.update(cx, |this, cx| {
                this.follow(ev.position, strn, max_off);
                if !this.animating {
                    this.animating = true;
                    this.last_frame = None;
                    cx.notify();
                }
            });
        })
        .on_hover(move |hovered: &bool, _window, cx| {
            if !*hovered {
                state_hover.update(cx, |this, cx| {
                    this.release();
                    if !this.animating {
                        this.animating = true;
                        this.last_frame = None;
                        cx.notify();
                    }
                });
            }
        })
        .child(
            canvas(
                move |bounds, _window, cx| {
                    state_probe.update(cx, |this, _cx| {
                        this.zone_bounds = bounds;
                    });
                },
                |_, (), _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .child(
            div()
                .id("magnetic-button")
                .relative()
                .left(px(current_x))
                .top(px(current_y))
                .w(cfg.size.width)
                .h(cfg.size.height)
                .flex()
                .items_center()
                .justify_center()
                .rounded_lg()
                .bg(btn_bg)
                .text_color(btn_fg)
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .child(inner_content)
                .on_click(move |ev: &ClickEvent, window, cx| {
                    if let Some(cb) = &on_click {
                        cb(ev, window, cx);
                    }
                }),
        );

    el.into_any_element()
}

impl MagneticButtonProps {
    #[must_use]
    pub fn on_click_fn(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Arc::new(handler));
        self
    }
}
