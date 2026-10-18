#![cfg(feature = "emoji-picker")]

use std::sync::Arc;

use emojis::{Emoji, Group, SkinTone};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{h_flex, ActiveTheme as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use zopra::{component, view};

pub const DEFAULT_COLUMNS: usize = 8;
pub const DEFAULT_CELL_SIZE: f32 = 36.0;
pub const DEFAULT_HEIGHT: f32 = 340.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmojiSelected(pub &'static Emoji);

pub type EmojiSelectCallback = Arc<dyn Fn(&'static Emoji, &mut Window, &mut App)>;

pub struct EmojiPickerState {
    pub search: Entity<InputState>,
    pub scroll: UniformListScrollHandle,
    pub focus_handle: FocusHandle,
    pub group: Option<Group>,
    pub skin_tone: SkinTone,
    pub results: Arc<[&'static Emoji]>,
    _search_sub: Subscription,
}

impl EmojiPickerState {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        default_group: Option<Group>,
        skin_tone: SkinTone,
        placeholder: SharedString,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let search_sub = cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.refilter(cx);
            }
        });

        let mut state = Self {
            search,
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            group: default_group,
            skin_tone,
            results: Arc::new([]),
            _search_sub: search_sub,
        };
        state.refilter(cx);
        state
    }

    pub fn set_group(&mut self, group: Option<Group>, cx: &mut Context<Self>) {
        if self.group != group {
            self.group = group;
            self.refilter(cx);
        }
    }

    pub fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().trim().to_lowercase();
        let group = self.group;
        let skin_tone = self.skin_tone;

        let filtered: Vec<&'static Emoji> = emojis::iter()
            .filter(|e| e.skin_tone().is_none_or(|t| t == skin_tone))
            .filter(|e| {
                if query.is_empty() {
                    group.is_none_or(|g| e.group() == g)
                } else {
                    e.name().contains(&query) || e.shortcodes().any(|s| s.contains(&query))
                }
            })
            .collect();

        self.results = filtered.into();
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }
}

/// A reactive, customizable, virtualized emoji picker component.
#[allow(clippy::too_many_arguments, clippy::too_many_lines, clippy::cast_precision_loss)]
#[component]
pub fn emoji_picker(
    on_select: Option<EmojiSelectCallback>,
    columns: Option<usize>,
    cell_size: Option<f32>,
    width: Option<f32>,
    height: Option<f32>,
    default_group: Option<Group>,
    skin_tone: Option<SkinTone>,
    placeholder: Option<SharedString>,
    show_tabs: Option<bool>,
    show_search: Option<bool>,
) {
    let cols = columns.unwrap_or(DEFAULT_COLUMNS);
    let cell_dim = cell_size.unwrap_or(DEFAULT_CELL_SIZE);
    let picker_w = width.unwrap_or(cols as f32 * cell_dim + 16.0);
    let picker_h = height.unwrap_or(DEFAULT_HEIGHT);
    let show_category_tabs = show_tabs.unwrap_or(true);
    let show_search_input = show_search.unwrap_or(true);
    let placeholder_text = placeholder.unwrap_or_else(|| SharedString::from("Search emoji…"));
    let chosen_skin_tone = skin_tone.unwrap_or(SkinTone::Default);

    let state_holder = window.use_state(cx, |window, cx| {
        cx.new(|cx| {
            EmojiPickerState::new(
                window,
                cx,
                default_group,
                chosen_skin_tone,
                placeholder_text,
            )
        })
    });

    let state = state_holder.read(cx).clone();

    let current_group = state.read(cx).group;
    let results = state.read(cx).results.clone();
    let row_count = results.len().div_ceil(cols);
    let scroll = state.read(cx).scroll.clone();
    let search_state = state.read(cx).search.clone();
    let focus = state.read(cx).focus_handle.clone();

    let hover_bg = cx.theme().accent;
    let theme_border = cx.theme().border;
    let theme_popover = cx.theme().popover;
    let theme_muted = cx.theme().muted_foreground;

    let tabs: Vec<(Option<Group>, &'static str, bool)> = if show_category_tabs {
        std::iter::once((None, "✨", current_group.is_none()))
            .chain(Group::iter().map(|g| {
                let is_active = current_group == Some(g);
                let icon = g.emojis().next().map_or("✨", |e| e.as_str());
                (Some(g), icon, is_active)
            }))
            .collect()
    } else {
        Vec::new()
    };

    view! {
        <div
            class="flex flex-col p-2 gap-2 rounded-lg"
            key_context="EmojiPicker"
            track_focus={&focus}
            w={px(picker_w)}
            h={px(picker_h)}
            border_1
            border_color={theme_border}
            bg={theme_popover}
        >
            { if show_search_input {
                view! {
                    <input state={&search_state} />
                }.into_any_element()
            } else {
                div().into_any_element()
            } }

            { if show_category_tabs {
                view! {
                    <div class="flex flex-row gap-1 items-center overflow-x-hidden">
                        { for (ix, (group, label, is_active)) in tabs.into_iter().enumerate() {
                            <div
                                id={("emoji-group", ix)}
                                class="px-2 py-1 rounded-md cursor-pointer flex items-center justify-center text-sm"
                                when={(is_active, {
                                    let active_bg = hover_bg;
                                    move |d| d.bg(active_bg)
                                })}
                                onClick={{
                                    let state = state.clone();
                                    move |_event, _window, cx| {
                                        state.update(cx, |this, cx| {
                                            this.set_group(group, cx);
                                        });
                                    }
                                }}
                            >
                                {label}
                            </div>
                        } }
                    </div>
                }.into_any_element()
            } else {
                div().into_any_element()
            } }

            {
                if row_count == 0 {
                    view! {
                        <div class="flex-1 flex items-center justify-center text-sm" text_color={theme_muted}>
                            "No emoji found"
                        </div>
                    }.into_any_element()
                } else {
                    uniform_list(
                        "emoji-grid",
                        row_count,
                        move |range, _window, _cx| {
                            let results = results.clone();
                            let on_select = on_select.clone();
                            range
                                .map(|row| {
                                    let start = row * cols;
                                    let end = (start + cols).min(results.len());
                                    let row_emojis = &results[start..end];
                                    h_flex().h(px(cell_dim)).children(
                                        row_emojis.iter().enumerate().map(|(col, &emoji)| {
                                            let on_select = on_select.clone();
                                            let cell_hover_bg = hover_bg;
                                            let cell = div()
                                                .id(("emoji", start + col))
                                                .size(px(cell_dim))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_md()
                                                .text_xl()
                                                .cursor_pointer()
                                                .hover(move |s| s.bg(cell_hover_bg))
                                                .child(emoji.as_str());

                                            if let Some(on_select) = on_select {
                                                cell.on_click(move |_event, window, cx| {
                                                    on_select(emoji, window, cx);
                                                })
                                            } else {
                                                cell
                                            }
                                        })
                                    )
                                })
                                .collect::<Vec<_>>()
                        },
                    )
                    .track_scroll(&scroll)
                    .flex_1()
                    .into_any_element()
                }
            }
        </div>
    }
}

impl EmojiPickerProps {
    #[must_use]
    pub fn on_pick(
        mut self,
        handler: impl Fn(&'static Emoji, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Arc::new(handler));
        self
    }
}
