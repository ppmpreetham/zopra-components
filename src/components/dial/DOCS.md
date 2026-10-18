# Dial (Comet Dial)

A physics-based circular dial featuring spring dynamics, coasting momentum on flick, trailing comet particle arcs, keyboard navigation, and customizable units.

## Using in Zopra

```rust
use zopra_components::Dial;

// Basic usage with reactive callback
<Dial
    value={*volume}
    min={0.0}
    max={100.0}
    step={1.0}
    unit={"%"}
    size={220.0}
    speed={35.0}
    on_change={on_volume_change}
/>
```

### Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `value` | `Option<f32>` | `min` | Controlled or initial value |
| `config` | `Option<DialConfig>` | `Default::default()` | Complete dial configuration struct |
| `size` | `Option<f32>` | `250.0` | Outer square diameter in pixels |
| `min` | `Option<f32>` | `0.0` | Minimum selectable value |
| `max` | `Option<f32>` | `100.0` | Maximum selectable value |
| `step` | `Option<f32>` | `1.0` | Snapping step increment (`0.0` for continuous) |
| `unit` | `Option<SharedString>` | `"%"` | Display unit suffix |
| `speed` | `Option<f32>` | `25.0` | Spring stiffness (0..=100) |
| `accent` | `Option<Hsla>` | `theme.foreground` | Active track and comet color |
| `ink` | `Option<Hsla>` | `theme.foreground` | Base track and text color |
| `disabled` | `Option<bool>` | `false` | Dim and disable interaction |
| `on_change` | `Option<DialChangeCallback>` | `None` | Fires on every value change |
| `on_end` | `Option<DialEndCallback>` | `None` | Fires when drag/flick completes |

---

## Using in Pure GPUI

```rust
// Startup
zopra_components::dial::init(cx);

// Create entity
let dial = cx.new(|cx| {
    CometDial::new(
        62.0,
        DialConfig { max: 100.0, unit: "%".into(), speed: 40.0, ..Default::default() },
        cx,
    )
});

// Subscribe to events
cx.subscribe(&dial, |this, _dial, ev: &CometDialEvent, cx| match ev {
    CometDialEvent::Change(v) => this.volume = *v,
    CometDialEvent::ChangeEnd { value, .. } => this.save(*value),
})
.detach();
```
