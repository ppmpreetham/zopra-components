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
    clippy::redundant_closure_for_method_calls,
    clippy::struct_excessive_bools,
    clippy::return_self_not_must_use,
    clippy::large_enum_variant
)]

use std::{cell::Cell, rc::Rc, time::{Duration, Instant}};

use gpui_kit::{
    canvas, div, prelude::*, px, red, relative, white, AnyElement, App, Bounds, ClickEvent,
    Context, Div, Hsla, MouseMoveEvent, Pixels, Point, Render, SharedString, Window,
};
use gpui_kit::component::{
    menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem},
    ActiveTheme as _, Icon,
};

const TAU_MOUSE: f32 = 0.045;
const TAU_INTENSITY: f32 = 0.08;
const TAU_HIDE: f32 = 0.12;
const BOUNCE_TIME: Duration = Duration::from_millis(800);
const BOUNCE_HEIGHT: f32 = 12.0;
const HOP_ROOM: f32 = 16.0;
const PEEK: f32 = 6.0;

type Callback = Rc<dyn Fn(&mut Window, &mut App)>;

// ---- public data model -----------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DockPosition {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

impl DockPosition {
    #[must_use]
    pub fn horizontal(self) -> bool {
        matches!(self, Self::Bottom | Self::Top)
    }
    /// Items hug the end side of the cross axis (bottom / right).
    #[must_use]
    pub fn at_end(self) -> bool {
        matches!(self, Self::Bottom | Self::Right)
    }
}

/// Tunable parameters. Override a few with `..Default::default()`.
#[derive(Clone, Copy, Debug)]
pub struct DockConfig {
    pub base_item_size: f32,
    pub magnification: f32,
    pub panel_height: f32,
    /// `None` falls back to the theme primary colour.
    pub accent: Option<Hsla>,
    pub badge_color: Hsla,
    pub position: DockPosition,
    /// Pointer distance (px) at which magnification fades to nothing.
    pub distance: f32,
    pub gap: f32,
    /// 0.0 = square corners, 1.0 = fully round.
    pub roundness: f32,
    pub bounce: bool,
    pub auto_hide: bool,
    pub tiles: bool,
    pub show_labels: bool,
    pub show_indicators: bool,
    pub show_badges: bool,
    /// Distance from the container edge.
    pub margin: f32,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            base_item_size: 50.0,
            magnification: 70.0,
            panel_height: 68.0,
            accent: None,
            badge_color: red(),
            position: DockPosition::Bottom,
            distance: 200.0,
            gap: 10.0,
            roundness: 0.5,
            bounce: true,
            auto_hide: false,
            tiles: true,
            show_labels: true,
            show_indicators: true,
            show_badges: true,
            margin: 12.0,
        }
    }
}

#[derive(Clone)]
pub enum MenuEntry {
    Item { label: SharedString, checked: bool, on_click: Option<Callback> },
    Separator,
    Submenu { label: SharedString, items: Rc<[MenuEntry]> },
}

impl MenuEntry {
    pub fn item(label: impl Into<SharedString>) -> Self {
        Self::Item { label: label.into(), checked: false, on_click: None }
    }
    pub fn separator() -> Self {
        Self::Separator
    }
    pub fn submenu(label: impl Into<SharedString>, items: impl IntoIterator<Item = MenuEntry>) -> Self {
        Self::Submenu { label: label.into(), items: items.into_iter().collect() }
    }
    pub fn checked(mut self, value: bool) -> Self {
        if let Self::Item { checked, .. } = &mut self {
            *checked = value;
        }
        self
    }
    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        if let Self::Item { on_click, .. } = &mut self {
            *on_click = Some(Rc::new(f));
        }
        self
    }
}

#[derive(Clone)]
pub struct DockItem {
    pub icon: Icon,
    pub label: SharedString,
    pub active: bool,
    pub badge: Option<u32>,
    pub on_click: Option<Callback>,
    pub menu: Rc<[MenuEntry]>,
    /// Written during prepaint, read next frame to find the item's live centre.
    pub bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl DockItem {
    pub fn new(icon: impl Into<Icon>, label: impl Into<SharedString>) -> Self {
        Self {
            icon: icon.into(),
            label: label.into(),
            active: false,
            badge: None,
            on_click: None,
            menu: Rc::from([]),
            bounds: Rc::default(),
        }
    }
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
    pub fn badge(mut self, count: u32) -> Self {
        self.badge = Some(count);
        self
    }
    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
    pub fn menu(mut self, entries: impl IntoIterator<Item = MenuEntry>) -> Self {
        self.menu = entries.into_iter().collect();
        self
    }
}

#[derive(Clone)]
pub enum DockEntry {
    Item(DockItem),
    Separator,
}

impl From<DockItem> for DockEntry {
    fn from(item: DockItem) -> Self {
        Self::Item(item)
    }
}

// ---- view ------------------------------------------------------------------

pub struct Dock {
    entries: Vec<DockEntry>,
    config: DockConfig,
    /// Smoothed pointer coordinate along the main axis (window space).
    mouse_s: f32,
    mouse_t: f32,
    /// True once a move event has seeded `mouse_s` for the current hover.
    tracking: bool,
    inside: bool,
    /// 0..1, ramps magnification in and out as the pointer enters and leaves.
    intensity: f32,
    /// 0..1, auto-hide slide.
    hide: f32,
    hovered: Option<usize>,
    bounces: Vec<(usize, Instant)>,
    animating: bool,
    last_frame: Option<Instant>,
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

impl Dock {
    pub fn new(entries: Vec<DockEntry>, config: DockConfig, _cx: &mut Context<Self>) -> Self {
        Self {
            entries,
            config,
            mouse_s: 0.0,
            mouse_t: 0.0,
            tracking: false,
            inside: false,
            intensity: 0.0,
            hide: if config.auto_hide { 1.0 } else { 0.0 },
            hovered: None,
            bounces: Vec::new(),
            animating: false,
            last_frame: None,
        }
    }

    pub fn set_entries(&mut self, entries: Vec<DockEntry>, cx: &mut Context<Self>) {
        self.entries = entries;
        cx.notify();
    }

    pub fn set_config(&mut self, config: DockConfig, cx: &mut Context<Self>) {
        self.config = config;
        self.wake(cx);
        cx.notify();
    }

    pub fn set_active(&mut self, index: usize, active: bool, cx: &mut Context<Self>) {
        if let Some(DockEntry::Item(item)) = self.entries.get_mut(index) {
            item.active = active;
            cx.notify();
        }
    }

    pub fn set_badge(&mut self, index: usize, badge: Option<u32>, cx: &mut Context<Self>) {
        if let Some(DockEntry::Item(item)) = self.entries.get_mut(index) {
            item.badge = badge;
            cx.notify();
        }
    }

    // ---- interaction -------------------------------------------------------

    fn wake(&mut self, cx: &mut Context<Self>) {
        if !self.animating {
            self.animating = true;
            self.last_frame = None;
            cx.notify();
        }
    }

    fn pointer(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let m = if self.config.position.horizontal() { f32::from(pos.x) } else { f32::from(pos.y) };
        self.mouse_t = m;
        if !self.tracking {
            self.tracking = true;
            self.mouse_s = m; // don't sweep in from a stale position
        }
        self.wake(cx);
    }

    fn set_inside(&mut self, inside: bool, cx: &mut Context<Self>) {
        self.inside = inside;
        if !inside {
            self.tracking = false;
        }
        self.wake(cx);
    }

    fn clicked(&mut self, ix: usize, cb: Option<Callback>, window: &mut Window, cx: &mut Context<Self>) {
        if self.config.bounce {
            self.bounces.retain(|(i, _)| *i != ix);
            self.bounces.push((ix, Instant::now()));
            self.wake(cx);
        }
        if let Some(cb) = cb {
            cb(window, cx);
        }
    }

    // ---- per-frame simulation ---------------------------------------------

    fn step(&mut self, window: &mut Window) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .replace(now)
            .map_or(1.0 / 60.0, |p| (now - p).as_secs_f32().min(0.05));
        let ease = |tau: f32| 1.0 - (-dt / tau).exp();

        let want = if self.inside { 1.0 } else { 0.0 };
        let want_hide = if self.config.auto_hide && !self.inside { 1.0 } else { 0.0 };

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

    fn size_for(&self, center: f32) -> f32 {
        let c = &self.config;
        let t = (1.0 - (center - self.mouse_s).abs() / c.distance.max(1.0)).clamp(0.0, 1.0);
        let eased = t * t * (3.0 - 2.0 * t); // smoothstep
        c.base_item_size + (c.magnification - c.base_item_size) * eased * self.intensity
    }

    fn hop(&self, ix: usize) -> f32 {
        self.bounces.iter().find(|(i, _)| *i == ix).map_or(0.0, |(_, t0)| {
            let t = t0.elapsed().as_secs_f32();
            BOUNCE_HEIGHT * (-t * 5.0).exp() * (t * 14.0).sin().abs()
        })
    }

    // ---- element builders --------------------------------------------------

    fn render_item(&self, ix: usize, item: &DockItem, st: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.config;
        let pos = c.position;

        let b = item.bounds.get();
        let center = if pos.horizontal() { f32::from(b.center().x) } else { f32::from(b.center().y) };
        let size = self.size_for(center);
        let hop = self.hop(ix);
        let slot = item.bounds.clone();
        let on_click = item.on_click.clone();

        let el = div()
            .id(("dock-item", ix))
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(px(size))
            .rounded(px(c.roundness.clamp(0.0, 1.0) * size / 2.0))
            .when(c.tiles, |d| d.bg(st.tile_bg).border_1().border_color(st.border).shadow_sm())
            .text_color(st.fg)
            .cursor_pointer()
            .map(|d| edge_margin(d, pos, hop))
            .child(canvas(move |bounds, _, _| slot.set(bounds), |_, (), _, _| {}).absolute().size_full())
            .child(item.icon.clone().size(px(size * 0.5)))
            .when(c.show_indicators && item.active, |d| {
                d.child(edge_slot(pos, 8.0).child(
                    div().size(px(4.0)).rounded_full().bg(st.accent),
                ))
            })
            .when(c.show_badges, |d| match item.badge.filter(|n| *n > 0) {
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
            .when(c.show_labels && self.hovered == Some(ix), |d| {
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
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.hovered = Some(ix);
                } else if this.hovered == Some(ix) {
                    this.hovered = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.clicked(ix, on_click.clone(), window, cx);
            }));

        if item.menu.is_empty() {
            el.into_any_element()
        } else {
            let menu = item.menu.clone();
            el.context_menu(move |m, window, cx| build_menu(m, &menu, window, cx)).into_any_element()
        }
    }

    fn render_separator(&self, st: &Palette) -> AnyElement {
        let c = &self.config;
        let len = c.base_item_size * 0.7;
        let line = div().flex_none().bg(st.border);
        let line = if c.position.horizontal() { line.w(px(1.0)).h(px(len)) } else { line.h(px(1.0)).w(px(len)) };
        edge_margin(line, c.position, (c.base_item_size - len) / 2.0).into_any_element()
    }
}

fn edge_margin<E: Styled>(el: E, pos: DockPosition, v: f32) -> E {
    match pos {
        DockPosition::Bottom => el.mb(px(v)),
        DockPosition::Top => el.mt(px(v)),
        DockPosition::Left => el.ml(px(v)),
        DockPosition::Right => el.mr(px(v)),
    }
}

/// Strip just outside the item on the dock-edge side (active indicator).
fn edge_slot(pos: DockPosition, off: f32) -> Div {
    let d = div().absolute().flex();
    match pos {
        DockPosition::Bottom => d.left_0().right_0().bottom(px(-off)).justify_center(),
        DockPosition::Top => d.left_0().right_0().top(px(-off)).justify_center(),
        DockPosition::Left => d.top_0().bottom_0().left(px(-off)).items_center(),
        DockPosition::Right => d.top_0().bottom_0().right(px(-off)).items_center(),
    }
}

/// Strip just outside the item on the open side (hover label).
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

impl Render for Dock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.animating {
            self.step(window);
        }

        let c = self.config;
        let theme = cx.theme();
        let st = Palette {
            accent: c.accent.unwrap_or(theme.primary),
            badge: c.badge_color,
            tile_bg: theme.secondary,
            border: theme.border,
            fg: theme.foreground,
            label_bg: theme.popover,
            label_fg: theme.popover_foreground,
        };
        let panel_bg = theme.background.opacity(0.75);

        let pos = c.position;
        let horiz = pos.horizontal();
        let pad = ((c.panel_height - c.base_item_size) / 2.0).max(0.0);
        // The hover stage covers the magnified growth; it collapses to the bare
        // panel while hidden so the invisible zone can't trigger a reveal.
        let stage_cross = c.magnification + pad * 2.0 + HOP_ROOM;
        let cross = stage_cross + (c.panel_height - stage_cross) * self.hide;
        let edge = c.margin - self.hide * (c.panel_height + c.margin - PEEK);
        let radius = px(c.roundness.clamp(0.0, 1.0) * c.panel_height / 2.0);

        let background = {
            let d = div()
                .absolute()
                .rounded(radius)
                .bg(panel_bg)
                .border_1()
                .border_color(st.border)
                .shadow_lg();
            match pos {
                DockPosition::Bottom => d.left_0().right_0().bottom_0().h(px(c.panel_height)),
                DockPosition::Top => d.left_0().right_0().top_0().h(px(c.panel_height)),
                DockPosition::Left => d.top_0().bottom_0().left_0().w(px(c.panel_height)),
                DockPosition::Right => d.top_0().bottom_0().right_0().w(px(c.panel_height)),
            }
        };

        let items: Vec<AnyElement> = self
            .entries
            .iter()
            .enumerate()
            .map(|(ix, entry)| match entry {
                DockEntry::Item(item) => self.render_item(ix, item, &st, cx),
                DockEntry::Separator => self.render_separator(&st),
            })
            .collect();

        let panel = div()
            .id("dock-panel")
            .relative()
            .flex_none()
            .flex()
            .gap(px(c.gap))
            .map(|d| if horiz { d.flex_row().h(px(cross)).px(px(pad)) } else { d.flex_col().w(px(cross)).py(px(pad)) })
            .map(|d| match pos {
                DockPosition::Bottom => d.items_end().pb(px(pad)),
                DockPosition::Top => d.items_start().pt(px(pad)),
                DockPosition::Left => d.items_start().pl(px(pad)),
                DockPosition::Right => d.items_end().pr(px(pad)),
            })
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| this.pointer(ev.position, cx)))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| this.set_inside(*hovered, cx)))
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
}
