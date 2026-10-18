#![cfg(feature = "dock")]
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
    clippy::redundant_closure_for_method_calls
)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::{
    menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem},
    ActiveTheme as _,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use zopra::component;

pub use super::gpui::{DockConfig, DockEntry, DockItem, DockPosition, MenuEntry};

const TAU_MOUSE: f32 = 0.045;
const TAU_INTENSITY: f32 = 0.08;
const TAU_HIDE: f32 = 0.12;
const BOUNCE_TIME: Duration = Duration::from_millis(800);
const BOUNCE_HEIGHT: f32 = 12.0;
const HOP_ROOM: f32 = 16.0;
const PEEK: f32 = 6.0;

pub type DockClickCallback = Arc<dyn Fn(usize, &mut Window, &mut App)>;

pub struct DockState {
    pub mouse_s: f32,
    pub mouse_t: f32,
    pub tracking: bool,
    pub inside: bool,
    pub intensity: f32,
    pub hide: f32,
    pub hovered: Option<usize>,
    pub bounces: Vec<(usize, Instant)>,
    pub animating: bool,
    pub last_frame: Option<Instant>,
    pub item_bounds: Vec<Bounds<Pixels>>,
}

impl DockState {
    #[must_use]
    pub fn new(count: usize, auto_hide: bool) -> Self {
        Self {
            mouse_s: 0.0,
            mouse_t: 0.0,
            tracking: false,
            inside: false,
            intensity: 0.0,
            hide: if auto_hide { 1.0 } else { 0.0 },
            hovered: None,
            bounces: Vec::new(),
            animating: false,
            last_frame: None,
            item_bounds: vec![Bounds::default(); count],
        }
    }

    pub fn pointer(&mut self, pos: Point<Pixels>, pos_cfg: DockPosition, cx: &mut Context<Self>) {
        let m = if pos_cfg.horizontal() { f32::from(pos.x) } else { f32::from(pos.y) };
        self.mouse_t = m;
        if !self.tracking {
            self.tracking = true;
            self.mouse_s = m;
        }
        self.wake(cx);
    }

    pub fn set_inside(&mut self, inside: bool, cx: &mut Context<Self>) {
        self.inside = inside;
        if !inside {
            self.tracking = false;
        }
        self.wake(cx);
    }

    pub fn trigger_bounce(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.bounces.retain(|(i, _)| *i != ix);
        self.bounces.push((ix, Instant::now()));
        self.wake(cx);
    }

    fn wake(&mut self, cx: &mut Context<Self>) {
        if !self.animating {
            self.animating = true;
            self.last_frame = None;
        }
        cx.notify();
    }

    pub fn step(&mut self, auto_hide: bool, window: &mut Window) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .replace(now)
            .map_or(1.0 / 60.0, |p| (now - p).as_secs_f32().min(0.05));
        let ease = |tau: f32| 1.0 - (-dt / tau).exp();

        let want = if self.inside { 1.0 } else { 0.0 };
        let want_hide = if auto_hide && !self.inside { 1.0 } else { 0.0 };

        self.intensity += (want - self.intensity) * ease(TAU_INTENSITY);
        self.mouse_s += (self.mouse_t - self.mouse_s) * ease(TAU_MOUSE);
        self.hide += (want_hide - self.hide) * ease(TAU_HIDE);
        self.bounces.retain(|(_, t)| t.elapsed() < BOUNCE_TIME);

        let mouse_gap = if self.inside { (self.mouse_t - self.mouse_s).abs() } else { 0.0 };
        let moving = (want - self.intensity).abs() > 0.002
            || (want_hide - self.hide).abs() > 0.002
            || mouse_gap > 0.1
            || !self.bounces.is_empty();

        if moving {
            window.request_animation_frame();
        } else {
            self.intensity = want;
            self.hide = want_hide;
            self.mouse_s = self.mouse_t;
            self.animating = false;
            self.last_frame = None;
        }
    }

    #[must_use]
    pub fn size_for(&self, center: f32, config: &DockConfig) -> f32 {
        let t = (1.0 - (center - self.mouse_s).abs() / config.distance.max(1.0)).clamp(0.0, 1.0);
        let eased = t * t * (3.0 - 2.0 * t);
        config.base_item_size + (config.magnification - config.base_item_size) * eased * self.intensity
    }

    #[must_use]
    pub fn hop(&self, ix: usize) -> f32 {
        self.bounces.iter().find(|(i, _)| *i == ix).map_or(0.0, |(_, t0)| {
            let t = t0.elapsed().as_secs_f32();
            BOUNCE_HEIGHT * (-t * 5.0).exp() * (t * 14.0).sin().abs()
        })
    }
}

struct Palette {
    accent: Hsla,
    badge: Hsla,
    tile_bg: Hsla,
    border: Hsla,
    fg: Hsla,
    label_bg: Hsla,
    label_fg: Hsla,
}

fn edge_margin<E: Styled>(el: E, pos: DockPosition, v: f32) -> E {
    match pos {
        DockPosition::Bottom => el.mb(px(v)),
        DockPosition::Top => el.mt(px(v)),
        DockPosition::Left => el.ml(px(v)),
        DockPosition::Right => el.mr(px(v)),
    }
}

fn edge_slot(pos: DockPosition, off: f32) -> Div {
    let d = div().absolute().flex();
    match pos {
        DockPosition::Bottom => d.left_0().right_0().bottom(px(-off)).justify_center(),
        DockPosition::Top => d.left_0().right_0().top(px(-off)).justify_center(),
        DockPosition::Left => d.top_0().bottom_0().left(px(-off)).items_center(),
        DockPosition::Right => d.top_0().bottom_0().right(px(-off)).items_center(),
    }
}

fn label_slot(pos: DockPosition, gap: f32) -> Div {
    let d = div().absolute().flex();
    match pos {
        DockPosition::Bottom => d.left_0().right_0().bottom(relative(1.0)).mb(px(gap)).justify_center(),
        DockPosition::Top => d.left_0().right_0().top(relative(1.0)).mt(px(gap)).justify_center(),
        DockPosition::Left => d.top_0().bottom_0().left(relative(1.0)).ml(px(gap)).items_center(),
        DockPosition::Right => d.top_0().bottom_0().right(relative(1.0)).mr(px(gap)).items_center(),
    }
}

fn build_menu(
    menu: PopupMenu,
    entries: &[MenuEntry],
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    entries.iter().fold(menu, |menu, entry| match entry {
        MenuEntry::Separator => menu.separator(),
        MenuEntry::Item { label, checked, on_click } => {
            let cb = on_click.clone();
            menu.item(
                PopupMenuItem::new(label.clone()).checked(*checked).on_click(move |_, window, cx| {
                    if let Some(cb) = &cb {
                        cb(window, cx);
                    }
                }),
            )
        }
        MenuEntry::Submenu { label, items } => {
            let items = items.clone();
            menu.submenu(label.clone(), window, cx, move |m, window, cx| {
                build_menu(m, &items, window, cx)
            })
        }
    })
}

/// A reactive macOS-inspired application dock component with magnification, hopping, and badges.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[component]
pub fn dock(
    entries: Option<Vec<DockEntry>>,
    config: Option<DockConfig>,
    position: Option<DockPosition>,
    base_item_size: Option<f32>,
    magnification: Option<f32>,
    gap: Option<f32>,
    roundness: Option<f32>,
    bounce: Option<bool>,
    auto_hide: Option<bool>,
    tiles: Option<bool>,
    show_labels: Option<bool>,
    show_indicators: Option<bool>,
    show_badges: Option<bool>,
    accent: Option<Hsla>,
    on_item_click: Option<DockClickCallback>,
) {
    let mut cfg = config.unwrap_or_default();
    if let Some(p) = position { cfg.position = p; }
    if let Some(b) = base_item_size { cfg.base_item_size = b; }
    if let Some(m) = magnification { cfg.magnification = m; }
    if let Some(g) = gap { cfg.gap = g; }
    if let Some(r) = roundness { cfg.roundness = r; }
    if let Some(b) = bounce { cfg.bounce = b; }
    if let Some(h) = auto_hide { cfg.auto_hide = h; }
    if let Some(t) = tiles { cfg.tiles = t; }
    if let Some(l) = show_labels { cfg.show_labels = l; }
    if let Some(i) = show_indicators { cfg.show_indicators = i; }
    if let Some(b) = show_badges { cfg.show_badges = b; }
    if let Some(a) = accent { cfg.accent = Some(a); }

    let all_entries = entries.unwrap_or_default();
    let entries_count = all_entries.len();
    let is_auto_hide = cfg.auto_hide;

    let state_holder = window.use_state(cx, |_window, cx| {
        cx.new(|_cx| DockState::new(entries_count, is_auto_hide))
    });

    let state = state_holder.read(cx).clone();

    // Resize state bounds buffer if entries count changed
    state.update(cx, |this, _cx| {
        if this.item_bounds.len() < entries_count {
            this.item_bounds.resize(entries_count, Bounds::default());
        }
    });

    if state.read(cx).animating {
        state.update(cx, |this, _cx| {
            this.step(cfg.auto_hide, window);
        });
    }

    let theme = cx.theme();
    let st = Palette {
        accent: cfg.accent.unwrap_or(theme.primary),
        badge: cfg.badge_color,
        tile_bg: theme.secondary,
        border: theme.border,
        fg: theme.foreground,
        label_bg: theme.popover,
        label_fg: theme.popover_foreground,
    };
    let panel_bg = theme.background.opacity(0.75);

    let pos = cfg.position;
    let horiz = pos.horizontal();
    let pad = ((cfg.panel_height - cfg.base_item_size) / 2.0).max(0.0);
    let stage_cross = cfg.magnification + pad * 2.0 + HOP_ROOM;
    let cross = stage_cross + (cfg.panel_height - stage_cross) * state.read(cx).hide;
    let edge = cfg.margin - state.read(cx).hide * (cfg.panel_height + cfg.margin - PEEK);
    let radius = px(cfg.roundness.clamp(0.0, 1.0) * cfg.panel_height / 2.0);

    let background = {
        let d = div()
            .absolute()
            .rounded(radius)
            .bg(panel_bg)
            .border_1()
            .border_color(st.border)
            .shadow_lg();
        match pos {
            DockPosition::Bottom => d.left_0().right_0().bottom_0().h(px(cfg.panel_height)),
            DockPosition::Top => d.left_0().right_0().top_0().h(px(cfg.panel_height)),
            DockPosition::Left => d.top_0().bottom_0().left_0().w(px(cfg.panel_height)),
            DockPosition::Right => d.top_0().bottom_0().right_0().w(px(cfg.panel_height)),
        }
    };

    let items: Vec<AnyElement> = all_entries
        .into_iter()
        .enumerate()
        .map(|(ix, entry)| match entry {
            DockEntry::Item(item) => {
                let state_item = state.clone();
                let state_probe = state.clone();
                let state_hover = state.clone();
                let on_item_click_cb = on_item_click.clone();
                let item_on_click = item.on_click.clone();
                let bounce_enabled = cfg.bounce;

                let b = state.read(cx).item_bounds.get(ix).copied().unwrap_or_default();
                let center = if pos.horizontal() { f32::from(b.center().x) } else { f32::from(b.center().y) };
                let item_size = state.read(cx).size_for(center, &cfg);
                let hop_val = state.read(cx).hop(ix);
                let is_hovered = state.read(cx).hovered == Some(ix);

                let el = div()
                    .id(("dock-item", ix))
                    .relative()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(item_size))
                    .rounded(px(cfg.roundness.clamp(0.0, 1.0) * item_size / 2.0))
                    .when(cfg.tiles, |d| d.bg(st.tile_bg).border_1().border_color(st.border).shadow_sm())
                    .text_color(st.fg)
                    .cursor_pointer()
                    .map(|d| edge_margin(d, pos, hop_val))
                    .child(
                        canvas(
                            move |bounds, _window, cx| {
                                state_probe.update(cx, |this, _cx| {
                                    if ix < this.item_bounds.len() {
                                        this.item_bounds[ix] = bounds;
                                    }
                                });
                            },
                            |_, (), _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(item.icon.clone().size(px(item_size * 0.5)))
                    .when(cfg.show_indicators && item.active, |d| {
                        d.child(edge_slot(pos, 8.0).child(
                            div().size(px(4.0)).rounded_full().bg(st.accent),
                        ))
                    })
                    .when(cfg.show_badges, |d| match item.badge.filter(|n| *n > 0) {
                        Some(n) => d.child(
                            div()
                                .absolute()
                                .top(px(-4.0))
                                .right(px(-4.0))
                                .min_w(px(16.0))
                                .h(px(16.0))
                                .px(px(4.0))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(st.badge)
                                .text_color(white())
                                .text_size(px(10.0))
                                .child(if n > 99 { "99+".to_string() } else { n.to_string() }),
                        ),
                        None => d,
                    })
                    .when(cfg.show_labels && is_hovered, |d| {
                        d.child(label_slot(pos, 8.0).child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .whitespace_nowrap()
                                .text_xs()
                                .bg(st.label_bg)
                                .text_color(st.label_fg)
                                .border_1()
                                .border_color(st.border)
                                .child(item.label.clone()),
                        ))
                    })
                    .on_hover(move |hovered: &bool, _window, cx| {
                        state_hover.update(cx, |this, cx| {
                            if *hovered {
                                this.hovered = Some(ix);
                            } else if this.hovered == Some(ix) {
                                this.hovered = None;
                            }
                            cx.notify();
                        });
                    })
                    .on_click(move |_: &ClickEvent, window, cx| {
                        if bounce_enabled {
                            state_item.update(cx, |this, cx| {
                                this.trigger_bounce(ix, cx);
                            });
                        }
                        if let Some(cb) = &item_on_click {
                            cb(window, cx);
                        }
                        if let Some(cb) = &on_item_click_cb {
                            cb(ix, window, cx);
                        }
                    });

                if item.menu.is_empty() {
                    el.into_any_element()
                } else {
                    let menu = item.menu.clone();
                    el.context_menu(move |m, window, cx| build_menu(m, &menu, window, cx)).into_any_element()
                }
            }
            DockEntry::Separator => {
                let len = cfg.base_item_size * 0.7;
                let line = div().flex_none().bg(st.border);
                let line = if cfg.position.horizontal() { line.w(px(1.0)).h(px(len)) } else { line.h(px(1.0)).w(px(len)) };
                edge_margin(line, cfg.position, (cfg.base_item_size - len) / 2.0).into_any_element()
            }
        })
        .collect();

    let state_move = state.clone();
    let state_inside = state;

    let panel = div()
        .id("dock-panel")
        .relative()
        .flex_none()
        .flex()
        .gap(px(cfg.gap))
        .map(|d| if horiz { d.flex_row().h(px(cross)).px(px(pad)) } else { d.flex_col().w(px(cross)).py(px(pad)) })
        .map(|d| match pos {
            DockPosition::Bottom => d.items_end().pb(px(pad)),
            DockPosition::Top => d.items_start().pt(px(pad)),
            DockPosition::Left => d.items_start().pl(px(pad)),
            DockPosition::Right => d.items_end().pr(px(pad)),
        })
        .on_mouse_move(move |ev: &MouseMoveEvent, _window, cx| {
            state_move.update(cx, |this, cx| {
                this.pointer(ev.position, pos, cx);
            });
        })
        .on_hover(move |hovered: &bool, _window, cx| {
            state_inside.update(cx, |this, cx| {
                this.set_inside(*hovered, cx);
            });
        })
        .child(background)
        .children(items);

    let stage = div().absolute().flex();
    let stage = match pos {
        DockPosition::Bottom => stage.left_0().right_0().bottom(px(edge)).justify_center(),
        DockPosition::Top => stage.left_0().right_0().top(px(edge)).justify_center(),
        DockPosition::Left => stage.top_0().bottom_0().left(px(edge)).items_center(),
        DockPosition::Right => stage.top_0().bottom_0().right(px(edge)).items_center(),
    };

    div().relative().size_full().child(stage.child(panel))
}
