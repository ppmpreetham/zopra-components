#![cfg(feature = "emoji-picker")]

use std::ops::Range;

use emojis::{Emoji, Group, SkinTone};
use gpui_kit::{
    div, prelude::*, px, uniform_list, App, ClickEvent, Context, Entity, EventEmitter,
    FocusHandle, Focusable, IntoElement, Render, ScrollStrategy, Subscription,
    UniformListScrollHandle, Window,
};
use gpui_kit::component::{
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex, ActiveTheme as _,
};

const COLUMNS: usize = 8;
const CELL: f32 = 36.0;

#[derive(Clone, Debug)]
pub struct EmojiSelected(pub &'static Emoji);

pub struct EmojiPicker {
    search: Entity<InputState>,
    group: Option<Group>,
    results: Vec<&'static Emoji>,
    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
    _search_sub: Subscription,
}

impl EventEmitter<EmojiSelected> for EmojiPicker {}

impl Focusable for EmojiPicker {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EmojiPicker {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search emoji…"));

        let search_sub = cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.refilter(cx);
            }
        });

        let mut this = Self {
            search,
            group: None,
            results: Vec::new(),
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            _search_sub: search_sub,
        };
        this.refilter(cx);
        this
    }

    fn set_group(&mut self, group: Option<Group>, cx: &mut Context<Self>) {
        if self.group != group {
            self.group = group;
            self.refilter(cx);
        }
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().trim().to_lowercase();
        let group = self.group;

        self.results = emojis::iter()
            .filter(|e| e.skin_tone().is_none_or(|t| t == SkinTone::Default))
            .filter(|e| {
                if query.is_empty() {
                    group.is_none_or(|g| e.group() == g)
                } else {
                    e.name().contains(&query) || e.shortcodes().any(|s| s.contains(&query))
                }
            })
            .collect();

        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_bg = cx.theme().accent;

        h_flex().gap_1().children(
            std::iter::once(None)
                .chain(Group::iter().map(Some))
                .enumerate()
                .map(|(ix, group)| {
                    let label = group
                        .and_then(|g| g.emojis().next())
                        .map_or("✨", |e| e.as_str());

                    div()
                        .id(("emoji-group", ix))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .when(self.group == group, |d| d.bg(active_bg))
                        .child(label)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.set_group(group, cx);
                        }))
                }),
        )
    }

    #[allow(clippy::unused_self)]
    fn render_cell(
        &self,
        ix: usize,
        emoji: &'static Emoji,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let hover_bg = cx.theme().accent;

        div()
            .id(("emoji", ix))
            .size(px(CELL))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .text_xl()
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .child(emoji.as_str())
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.emit(EmojiSelected(emoji));
            }))
    }
}

impl Render for EmojiPicker {
    #[allow(clippy::cast_precision_loss)]
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let row_count = self.results.len().div_ceil(COLUMNS);

        v_flex()
            .key_context("EmojiPicker")
            .track_focus(&self.focus_handle)
            .w(px(COLUMNS as f32 * CELL + 16.0))
            .h(px(340.0))
            .p_2()
            .gap_2()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .child(Input::new(&self.search))
            .child(self.render_tabs(cx))
            .when(row_count == 0, |this| {
                this.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(cx.theme().muted_foreground)
                        .child("No emoji found"),
                )
            })
            .when(row_count > 0, |this| {
                this.child(
                    uniform_list(
                        "emoji-grid",
                        row_count,
                        cx.processor(|this, range: Range<usize>, _window, cx| {
                            range
                                .map(|row| {
                                    let start = row * COLUMNS;
                                    let end = (start + COLUMNS).min(this.results.len());
                                    h_flex().h(px(CELL)).children(
                                        this.results[start..end]
                                            .iter()
                                            .enumerate()
                                            .map(|(col, &emoji)| {
                                                this.render_cell(start + col, emoji, cx)
                                            })
                                            .collect::<Vec<_>>(),
                                    )
                                })
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&self.scroll)
                    .flex_1(),
                )
            })
    }
}
