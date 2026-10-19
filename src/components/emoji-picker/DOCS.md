# Emoji Picker

A responsive, virtualized emoji picker component featuring live category tabs, search by name/shortcode, responsive grid layout, and customizable parameters.

Gated behind `#![cfg(feature = "emoji-picker")]`.

## Zopra usage

### Basic

```rust
use zopra_components::EmojiPicker;
use zopra::view;

view! {
    <EmojiPicker />
}
```

### With selection callback and custom tuning parameters

```rust
use std::sync::Arc;
use zopra_components::EmojiPicker;
use zopra::{component, hooks::use_state, view};

#[component]
fn my_chat() {
    let (message, set_message) = use_state( String::new());

    let on_select = {
        let set_message = set_message.clone();
        let message = message.clone();
        Arc::new(move |emoji: &'static emojis::Emoji, _window, cx| {
            set_message.set(format!("{}{}", *message, emoji.as_str()), cx);
        })
    };

    view! {
        <div class="flex flex-col gap-4">
            <div>"Current message: " {message.clone()}</div>
            <EmojiPicker
                on_select={on_select}
                columns={10}
                cell_size={32.0}
                height={300.0}
                placeholder={"Find an icon…"}
                show_tabs={true}
                show_search={true}
            />
        </div>
    }
}
```

### Builder API

```rust
let picker = EmojiPicker::new()
    .columns(8)
    .cell_size(36.0)
    .height(340.0)
    .on_pick(|emoji, _window, cx| {
        println!("Picked emoji: {}", emoji.as_str());
    })
    .render(window, cx);
```

### Tunable Parameters

| Prop | Type | Default | Description |
|---|---|---|---|
| `on_select` | `Option<EmojiSelectCallback>` | `None` | Callback invoked on emoji click: `Arc<dyn Fn(&'static Emoji, &mut Window, &mut App) + Send + Sync>` |
| `columns` | `Option<usize>` | `8` | Number of columns in grid |
| `cell_size` | `Option<f32>` | `36.0` | Size in px for each emoji cell |
| `width` | `Option<f32>` | `cols * cell_size + 16.0` | Explicit width override |
| `height` | `Option<f32>` | `340.0` | Explicit height override |
| `default_group` | `Option<Group>` | `None` | Default selected group (or `None` for all groups) |
| `skin_tone` | `Option<SkinTone>` | `SkinTone::Default` | Selected skin tone filter |
| `placeholder` | `Option<SharedString>` | `"Search emoji…"` | Search input placeholder text |
| `show_tabs` | `Option<bool>` | `true` | Whether to display category navigation tabs |
| `show_search` | `Option<bool>` | `true` | Whether to display search bar |

## GPUI usage

```rust
let picker = cx.new(|cx| EmojiPicker::new(window, cx));

let sub = cx.subscribe(&picker, |this, _picker, EmojiSelected(emoji), cx| {
    this.message.push_str(emoji.as_str());
    cx.notify();
});

// In render:
.child(picker.clone())
```
