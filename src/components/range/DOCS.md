# Range Slider

A reactive dual-thumb range slider featuring:
- Ghost hover preview with smoothed expansion/contraction
- Smooth numeric easing for header values
- Non-overlapping dual thumbs with keyboard navigation (`Left`, `Right`, `Up`, `Down`, `PageUp`, `PageDown`, `Home`, `End`)
- Clear button to quickly reset range
- Discrete snapping ticks and customizable labels

## Using in Zopra

```rust
use zopra_components::RangeSlider;

<RangeSlider
    low={*low}
    high={*high}
    min={0.0}
    max={1000.0}
    step={10.0}
    prefix={"$"}
    ticks={5}
    clear_button={true}
    hover_preview={true}
    on_change={on_range_change}
/>
```

### Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `low` | `Option<f32>` | `config.default_low` | Lower thumb value |
| `high` | `Option<f32>` | `config.default_high` | Upper thumb value |
| `config` | `Option<RangeSliderConfig>` | `Default::default()` | Complete configuration struct |
| `min` | `Option<f32>` | `0.0` | Minimum selectable range value |
| `max` | `Option<f32>` | `1000.0` | Maximum selectable range value |
| `step` | `Option<f32>` | `10.0` | Snapping step increment |
| `label` | `Option<SharedString>` | `"Price Range"` | Header category label |
| `prefix` | `Option<SharedString>` | `"$"}` | Prefix currency or unit symbol |
| `ticks` | `Option<usize>` | `5` | Number of tick label marks beneath track |
| `clear_button` | `Option<bool>` | `true` | Show header button to reset to default |
| `hover_preview` | `Option<bool>` | `true` | Show ghost highlight while hovering |
| `on_change` | `Option<RangeChangeCallback>` | `None` | Fires on every value change |
| `on_end` | `Option<RangeChangeCallback>` | `None` | Fires when drag finishes |

---

## Using in Pure GPUI

```rust
// Startup
zopra_components::range::init(cx);

// Construct
let slider = cx.new(|cx| RangeSlider::new(RangeSliderConfig::default(), window, cx));

// Event subscription
cx.subscribe(&slider, |this, _s, ev: &RangeSliderEvent, cx| match ev {
    RangeSliderEvent::Change { low, high } => this.preview_filter(*low, *high),
    RangeSliderEvent::ChangeEnd { low, high } => this.apply_filter(*low, *high),
})
.detach();
```
