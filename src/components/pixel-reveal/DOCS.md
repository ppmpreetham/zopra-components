# Pixel Reveal

A responsive typography component that resolves rasterized text through quadtree pixel mosaics, procedural noise, scanlines, and animated particle dissolves.

## Zopra RSX Usage

```rust
use zopra_components::{PixelReveal, Preset};

// Basic usage with preset
view! {
    <PixelReveal
        text={"CYBERNETIC INTERFACE"}
        preset={Preset::Signal}
        font_size={28.0}
    />
}

// Custom options & styled text
view! {
    <PixelReveal
        text={"ANOMALY DETECTED"}
        preset={Preset::Glitch}
        font_size={24.0}
        color={cx.theme().foreground}
    />
}
```

## Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `text` | `Option<SharedString>` | `"Pixels resolve into type."` | Text to display and animate. |
| `spans` | `Option<Vec<Span>>` | `None` | Optional rich-text spans if multi-styled text is needed. |
| `font` | `Option<Rc<RevealFont>>` | Bundled default font | Font metadata and rasterizer settings. |
| `style` | `Option<TextStyle>` | Default 36px white | Full `TextStyle` configuration (font size, line height, letter spacing, etc.). |
| `options` | `Option<PixelRevealOptions>` | `Default::default()` | Granular animation options (trigger, stagger, noise, pixel size, easing). |
| `preset` | `Option<Preset>` | `None` | Preconfigured visual preset (`Materialize`, `Signal`, `Typewriter`, `Dissolve`, `Rain`, `Radiate`, `Bitmap`, `Glitch`, `Flow`). |
| `font_size` | `Option<f32>` | `36.0` | Convenience prop overriding font size. |
| `color` | `Option<Hsla>` | White | Convenience prop overriding text color. |

---

## Low-level GPUI Usage

```rust
use zopra_components::pixel_reveal::{
    PixelReveal, PixelRevealEvent, Options, Preset, RevealFont, TextStyle,
};

// Startup: initialize custom font
let font = RevealFont::new("OpenSans", include_bytes!("../assets/font.ttf")).unwrap();

// Create entity
let reveal = cx.new(|cx| {
    PixelReveal::new(
        "Pixels resolve into type.",
        font.clone(),
        TextStyle::new(48.0, gpui_kit::white()),
        Options::new()
            .preset(Preset::Glitch)
            .trigger("inview")
            .stagger(0.05)
            .hover("lens"),
        cx,
    )
});

// Render into container
div().w(px(720.0)).child(reveal)
```
