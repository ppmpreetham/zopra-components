# Magnetic Button

An interactive magnetic button component that smoothly attracts toward the mouse cursor with spring physics and inertia.

Gated behind `#![cfg(feature = "magnetic-button")]`.

## Zopra usage

### Basic

```rust
use zopra_components::MagneticButton;
use zopra::view;

view! {
    <MagneticButton label="Hover Me" />
}
```

### With custom children and click callback

```rust
use std::sync::Arc;
use zopra_components::MagneticButton;
use zopra::{component, hooks::use_state, view};

#[component]
fn demo() {
    let (clicks, set_clicks) = use_state( 0usize);

    let on_click = {
        let set_clicks = set_clicks.clone();
        let clicks = clicks.clone();
        Arc::new(move |_ev, _window, cx| {
            set_clicks.set(*clicks + 1, cx);
        })
    };

    view! {
        <div class="flex flex-col items-center gap-4">
            <div>"Clicks: " {clicks.clone()}</div>
            <MagneticButton
                strength={0.5}
                radius={60.0}
                max_offset={20.0}
                responsiveness={18.0}
                width={140.0}
                height={50.0}
                on_click={on_click}
            >
                <div class="flex items-center gap-2 font-bold">
                    "🧲 Pull In"
                </div>
            </MagneticButton>
        </div>
    }
}
```

### Builder API

```rust
let button = MagneticButton::new()
    .label("Snappy")
    .strength(0.6)
    .radius(50.0)
    .on_click_fn(|_ev, _window, _cx| {
        println!("Clicked!");
    })
    .render(window, cx);
```

### Tunable Parameters

| Prop | Type | Default | Description |
|---|---|---|---|
| `label` | `Option<SharedString>` | `None` | Text label for the button |
| `children` | `Vec<AnyElement>` | `[]` | Arbitrary custom content rendered inside button |
| `config` | `Option<MagneticConfig>` | `Default` | Struct containing overall magnetic configuration |
| `width` | `Option<f32>` | `120.0` | Button width in px |
| `height` | `Option<f32>` | `44.0` | Button height in px |
| `radius` | `Option<f32>` | `40.0` | Magnetic pull radius beyond the button edge in px |
| `strength` | `Option<f32>` | `0.35` | Fraction of cursor offset the button follows (`0.0..=1.0`) |
| `max_offset` | `Option<f32>` | `14.0` | Maximum travel distance from rest position |
| `responsiveness` | `Option<f32>` | `14.0` | Easing speed constant (higher = snappier, lower = floatier) |
| `bg` | `Option<Hsla>` | `theme.primary` | Custom button background color |
| `text_color` | `Option<Hsla>` | `theme.primary_foreground` | Custom text color |
| `on_click` | `Option<MagneticClickCallback>` | `None` | Click event callback |
