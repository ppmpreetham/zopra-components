use std::sync::Arc;
use zopra_components::EmojiPicker;
use gpui_kit::component::{ActiveTheme as _, Root};
use gpui_kit::*;
use zopra::{component, hooks::use_state, view};

#[component]
fn demo_app() {
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

    let bg_color = cx.theme().background;
    let card_bg = cx.theme().popover;
    let border_color = cx.theme().border;
    let text_muted = cx.theme().muted_foreground;

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

    view! {
        <div class="flex flex-col items-center justify-center size-full p-6" bg={bg_color}>
            <div class="flex flex-col items-center gap-5">
                // Selected Emoji Display
                <div class="flex flex-col items-center gap-2">
                    <div class="text-xs font-semibold" text_color={text_muted}>
                        "Zopra Emoji Picker"
                    </div>
                    <div
                        class="size-16 flex items-center justify-center rounded-2xl text-3xl shadow-sm border_1"
                        bg={card_bg}
                        border_color={border_color}
                    >
                        {selected_emoji.clone()}
                    </div>
                    <div class="text-xs font-medium" text_color={text_muted}>
                        "Click an emoji below to select"
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

                // The Tunable EmojiPicker Component
                <EmojiPicker
                    on_select={on_select}
                    columns={8}
                    cell_size={36.0}
                    height={340.0}
                />
            </div>
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

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(460.0), px(600.0)),
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
