### GPUI Usage

```rust
mod color;
mod color_picker;
use color::ColorFormat::*;
use color_picker::*;

let picker = cx.new(|cx| {
    ColorPicker::new(
        ColorPickerConfig {
            mode: PickerMode::Ring,
            formats: vec![Hex, Rgb, Hsl, Oklch, Lab],
            ..Default::default()
        },
        window,
        cx,
    )
});

cx.subscribe(&picker, |this, _, ev: &ColorPickerEvent, cx| match ev {
    ColorPickerEvent::Change(hsla) => this.preview(*hsla),
    ColorPickerEvent::ChangeEnd(hsla) => this.commit(*hsla),
    ColorPickerEvent::SwatchesChanged(s) => this.save_swatches(s),
})
.detach();

picker.update(cx, |p, cx| { p.set_value("oklch(0.7 0.15 200)", cx); });
```
