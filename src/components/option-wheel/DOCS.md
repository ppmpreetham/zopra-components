# Option Wheel

A 3D cylindrical vertical option wheel component with smooth physics-based exponential easing, mouse drag, scroll wheel, and keyboard navigation.

Gated behind `#![cfg(feature = "option-wheel")]`.

## Zopra usage

### Basic

```rust
use gpui_kit::SharedString;
use zopra_components::OptionWheel;
use zopra::view;

let items: Vec<SharedString> = vec![
    "January".into(),
    "February".into(),
    "March".into(),
    "April".into(),
    "May".into(),
];

view! {
    <OptionWheel items={items} />
}
```

### With selection callback and custom configuration

```rust
use std::sync::Arc;
use gpui_kit::SharedString;
use zopra_components::{OptionWheel, Side};
use zopra::{component, hooks::use_state, view};

#[component]
fn date_picker() {
    let (chosen, set_chosen) = use_state( SharedString::from("January"));

    let items = vec![
        "January".into(),
        "February".into(),
        "March".into(),
        "April".into(),
        "May".into(),
        "June".into(),
        "July".into(),
        "August".into(),
        "September".into(),
        "October".into(),
        "November".into(),
        "December".into(),
    ];

    let on_change = {
        let set_chosen = set_chosen.clone();
        Arc::new(move |_idx: usize, item: SharedString, _window, cx| {
            set_chosen.set(item, cx);
        })
    };

    view! {
        <div class="flex flex-col items-center gap-4 size-full">
            <div class="text-sm font-semibold">
                "Selected month: " {chosen.clone()}
            </div>
            <div class="w-72 h-80 relative overflow-hidden rounded-xl border_1 border-[#333]">
                <OptionWheel
                    items={items}
                    selected={Some(0)}
                    side={Some(Side::Left)}
                    font_size={Some(36.0)}
                    spacing={Some(1.5)}
                    curve={Some(1.2)}
                    tilt={Some(7.0)}
                    looping={Some(true)}
                    draggable={Some(true)}
                    on_change={on_change}
                />
            </div>
        </div>
    }
}
```

### Builder API

```rust
let items = vec!["Fast".into(), "Balanced".into(), "Quality".into()];

let wheel = OptionWheel::new(items)
    .font_size(32.0)
    .looping(true)
    .on_option_change(|index, item, _window, cx| {
        println!("Selected #{}: {}", index, item);
    })
    .render(window, cx);
```

### Tunable Parameters

| Prop | Type | Default | Description |
|---|---|---|---|
| `items` | `Vec<SharedString>` | **Required** | Options to display on the wheel |
| `selected` | `Option<usize>` | `Some(0)` | Initial selected option index |
| `config` | `Option<WheelConfig>` | `Default` | Full configuration bundle (overridable via individual props) |
| `side` | `Option<Side>` | `Side::Left` | Left or Right curvature orientation |
| `font_size` | `Option<f32>` | `48.0` | Font size in pixels |
| `spacing` | `Option<f32>` | `1.4` | Row height multiplier relative to font size |
| `curve` | `Option<f32>` | `1.0` | Horizontal bulge of the 3D arc (0 = flat list) |
| `tilt` | `Option<f32>` | `6.0` | Arc angle per row in degrees; lower = flatter arc |
| `fade` | `Option<f32>` | `0.25` | Opacity reduction factor per unit distance |
| `min_opacity` | `Option<f32>` | `0.05` | Minimum opacity clamp for distant rows |
| `smoothing_ms` | `Option<f32>` | `200.0` | Exponential easing time constant in ms |
| `inset` | `Option<f32>` | `80.0` | Horizontal offset in px from the anchored side |
| `looping` | `Option<bool>` | `false` | Infinite wrapping scroll |
| `draggable` | `Option<bool>` | `true` | Enable mouse click & drag inertia |
| `visible_radius` | `Option<usize>` | `6` | Number of visible rows above and below center |
| `text_color` | `Option<Hsla>` | `theme.muted_foreground` | Color of inactive/distant options |
| `active_color` | `Option<Hsla>` | `theme.foreground` | Highlight color of centered option |
| `on_change` | `Option<OptionWheelChangeCallback>` | `None` | Callback invoked on option change |
