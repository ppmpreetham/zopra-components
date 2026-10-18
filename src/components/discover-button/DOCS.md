# Discover Button

A pill button featuring an expanding circular fill on hover, cubic-bezier easing, icon shift animation, optional external link opening, and full keyboard/focus ring accessibility.

## Using in Zopra

```rust
use zopra_components::DiscoverButton;

// Basic usage
<DiscoverButton
    label={"Discover More"}
    href={"https://github.com"}
/>

// Fully customized with colors & click callback
let on_discover = std::sync::Arc::new(|window: &mut Window, cx: &mut App| {
    println!("Discover button clicked!");
});

<DiscoverButton
    label={"Explore Projects"}
    height={48.0}
    fill_size={44.0}
    icon_size={18.0}
    surface={gpui_kit::rgb(0x18181b).into()}
    fill={gpui_kit::rgb(0x6366f1).into()}
    text_color={gpui_kit::rgb(0xf4f4f5).into()}
    active_text={gpui_kit::rgb(0xffffff).into()}
    on_click={on_discover}
/>
```

### Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `label` | `Option<SharedString>` | `"Discover"` | Button text label |
| `href` | `Option<SharedString>` | `None` | Optional external URL to open on click |
| `config` | `Option<DiscoverConfig>` | `Default::default()` | Complete DiscoverConfig object |
| `height` | `Option<f32>` | `44.0` | Total button height in px |
| `fill_size` | `Option<f32>` | `40.0` | Initial circle diameter on the right side in px |
| `icon_size` | `Option<f32>` | `16.0` | Arrow icon size in px |
| `surface` | `Option<Hsla>` | `#0F172A` | Background surface color |
| `fill` | `Option<Hsla>` | `#F8FAFC` | Fill circle accent color |
| `text_color` | `Option<Hsla>` | `#F8FAFC` | Default resting text color |
| `active_text` | `Option<Hsla>` | `#0F172A` | Inverted text color when fill expands |
| `on_click` | `Option<DiscoverClickCallback>` | `None` | Custom callback invoked on click / Enter / Space |

---

## Using in Pure GPUI

```rust
let button = cx.new(|cx| {
    DiscoverButton::new(
        DiscoverConfig {
            label: "Explore".into(),
            href: Some("https://example.com".into()),
            ..Default::default()
        },
        cx,
    )
});
```
