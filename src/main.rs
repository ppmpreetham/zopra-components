use std::sync::Arc;
use gpui_kit::component::{ActiveTheme as _, Root};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use zopra::{component, hooks::use_state, view};
use zopra_components::{EmojiPicker, MagneticButton, OptionWheel, Side};

#[allow(clippy::too_many_lines)]
#[component]
fn demo_app() {
    let (tab, set_tab) = use_state(|| 0usize);

    // Emoji Picker state
    let (selected_emoji, set_selected_emoji) = use_state(|| SharedString::from("✨"));
    let (recent_emojis, set_recent_emojis) = use_state(|| {
        vec![
            SharedString::from("✨"),
            SharedString::from("🚀"),
            SharedString::from("🔥"),
            SharedString::from("🎉"),
            SharedString::from("💡"),
        ]
    });

    // Option Wheel state
    let (selected_option, set_selected_option) = use_state(|| SharedString::from("Rust"));
    let wheel_items = vec![
        "Rust".into(),
        "TypeScript".into(),
        "Swift".into(),
        "Kotlin".into(),
        "Python".into(),
        "Go".into(),
        "C++".into(),
        "Zig".into(),
        "Haskell".into(),
        "Elixir".into(),
    ];

    // Magnetic Button state
    let (magnetic_clicks, set_magnetic_clicks) = use_state(|| 0usize);

    let bg_color = cx.theme().background;
    let card_bg = cx.theme().popover;
    let border_color = cx.theme().border;
    let text_muted = cx.theme().muted_foreground;
    let accent_color = cx.theme().accent;

    let on_select = {
        let set_selected = set_selected_emoji.clone();
        let set_recent = set_recent_emojis.clone();
        Arc::new(move |emoji: &'static emojis::Emoji, _window: &mut Window, cx: &mut App| {
            let str_val = SharedString::from(emoji.as_str());
            set_selected.set(str_val.clone(), cx);

            set_recent.update(|recent| {
                recent.retain(|e| e != &str_val);
                recent.insert(0, str_val);
                if recent.len() > 8 {
                    recent.truncate(8);
                }
            }, cx);
        })
    };

    let on_wheel_change = {
        let set_selected = set_selected_option.clone();
        Arc::new(move |_idx: usize, item: SharedString, _window: &mut Window, cx: &mut App| {
            set_selected.set(item, cx);
        })
    };

    #[allow(clippy::arc_with_non_send_sync)]
    let on_magnetic_click = {
        let set_clicks = set_magnetic_clicks.clone();
        let clicks = magnetic_clicks.clone();
        Arc::new(move |_ev: &ClickEvent, _window: &mut Window, cx: &mut App| {
            set_clicks.set(*clicks + 1, cx);
        })
    };

    let is_emoji_tab = *tab == 0;
    let is_wheel_tab = *tab == 1;
    let is_magnetic_tab = *tab == 2;

    view! {
        <div class="flex flex-col items-center size-full p-6" bg={bg_color}>
            // Header navigation switcher
            <div class="flex flex-row gap-2 p-1 rounded-lg border_1 mb-6" border_color={border_color} bg={card_bg}>
                <div
                    class="px-4 py-1.5 rounded-md text-sm font-medium cursor-pointer"
                    when={(is_emoji_tab, move |d| d.bg(accent_color))}
                    onClick={move |_event, _window, cx| set_tab.set(0usize, cx)}
                >
                    "Emoji Picker"
                </div>
                <div
                    class="px-4 py-1.5 rounded-md text-sm font-medium cursor-pointer"
                    when={(is_wheel_tab, move |d| d.bg(accent_color))}
                    onClick={move |_event, _window, cx| set_tab.set(1usize, cx)}
                >
                    "Option Wheel"
                </div>
                <div
                    class="px-4 py-1.5 rounded-md text-sm font-medium cursor-pointer"
                    when={(is_magnetic_tab, move |d| d.bg(accent_color))}
                    onClick={move |_event, _window, cx| set_tab.set(2usize, cx)}
                >
                    "Magnetic Button"
                </div>
            </div>

            // Emoji Picker view
            { if is_emoji_tab {
                view! {
                    <div class="flex flex-col items-center gap-5">
                        <div class="flex flex-col items-center gap-2">
                            <div class="text-xs font-semibold" text_color={text_muted}>
                                "Selected Emoji"
                            </div>
                            <div
                                class="size-16 flex items-center justify-center rounded-2xl text-3xl shadow-sm border_1"
                                bg={card_bg}
                                border_color={border_color}
                            >
                                {selected_emoji.clone()}
                            </div>
                        </div>

                        // Recent Row
                        <div class="flex flex-row items-center gap-1.5">
                            <span class="text-xs font-medium mr-1" text_color={text_muted}>"Recent:"</span>
                            { for (ix, emoji) in recent_emojis.iter().cloned().enumerate() {
                                <div
                                    key={ix}
                                    class="size-7 flex items-center justify-center rounded-md cursor-pointer text-base hover:bg-neutral-200"
                                    onClick={{
                                        let emoji_str = emoji.clone();
                                        let set_selected = set_selected_emoji.clone();
                                        move |_event, _window, cx| {
                                            set_selected.set(emoji_str.clone(), cx);
                                        }
                                    }}
                                >
                                    {emoji}
                                </div>
                            } }
                        </div>

                        <EmojiPicker
                            on_select={on_select}
                            columns={8}
                            cell_size={36.0}
                            height={320.0}
                        />
                    </div>
                }.into_any_element()
            } else {
                div().into_any_element()
            } }

            // Option Wheel view
            { if is_wheel_tab {
                view! {
                    <div class="flex flex-col items-center gap-4 size-full">
                        <div class="flex flex-col items-center gap-1">
                            <div class="text-xs font-semibold" text_color={text_muted}>
                                "Selected Option"
                            </div>
                            <div class="text-lg font-bold">
                                {selected_option.clone()}
                            </div>
                            <div class="text-xs" text_color={text_muted}>
                                "Use mouse drag, scroll wheel, or arrow keys"
                            </div>
                        </div>

                        <div
                            class="w-80 h-96 relative overflow-hidden rounded-xl border_1 shadow-sm"
                            border_color={border_color}
                            bg={card_bg}
                        >
                            <OptionWheel
                                items={wheel_items}
                                selected={0}
                                side={Side::Left}
                                font_size={38.0}
                                spacing={1.5}
                                curve={1.1}
                                tilt={6.5}
                                looping={true}
                                draggable={true}
                                on_change={on_wheel_change}
                            />
                        </div>
                    </div>
                }.into_any_element()
            } else {
                div().into_any_element()
            } }

            // Magnetic Button view
            { if is_magnetic_tab {
                view! {
                    <div class="flex flex-col items-center justify-center gap-6 size-full mt-6">
                        <div class="flex flex-col items-center gap-1">
                            <div class="text-xs font-semibold" text_color={text_muted}>
                                "Interactive Magnetic Playground"
                            </div>
                            <div class="text-xs" text_color={text_muted}>
                                "Move cursor near the buttons to feel magnetic attraction"
                            </div>
                            <div class="text-sm font-semibold mt-2">
                                "Clicks: " {magnetic_clicks.clone()}
                            </div>
                        </div>

                        <div class="flex flex-row items-center gap-12 mt-4">
                            // Snappy magnetic button
                            <div class="flex flex-col items-center gap-2">
                                <span class="text-xs font-medium" text_color={text_muted}>"Snappy (High Pull)"</span>
                                <MagneticButton
                                    label={"🧲 Snappy"}
                                    width={140.0}
                                    height={48.0}
                                    strength={0.55}
                                    radius={55.0}
                                    max_offset={18.0}
                                    responsiveness={20.0}
                                    on_click={on_magnetic_click.clone()}
                                />
                            </div>

                            // Floaty magnetic button
                            <div class="flex flex-col items-center gap-2">
                                <span class="text-xs font-medium" text_color={text_muted}>"Floaty (Soft Spring)"</span>
                                <MagneticButton
                                    label={"✨ Floaty"}
                                    width={140.0}
                                    height={48.0}
                                    strength={0.3}
                                    radius={60.0}
                                    max_offset={22.0}
                                    responsiveness={8.0}
                                    on_click={on_magnetic_click}
                                />
                            </div>
                        </div>
                    </div>
                }.into_any_element()
            } else {
                div().into_any_element()
            } }
        </div>
    }
}

struct AppView;

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        demo_app(window, cx)
    }
}

fn main() {
    let app = gpui_kit::application();

    app.run(|cx| {
        gpui_kit::init(cx);
        zopra_components::option_wheel::zopra::init(cx);

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(560.0), px(680.0)),
                cx,
            ))),
            ..Default::default()
        };

        cx.open_window(window_options, |window, cx| {
            let view = cx.new(|_| AppView);
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("failed to open window");

        cx.activate(true);
    });
}
