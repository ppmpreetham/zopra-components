# Status Bars

A responsive uptime and system status component with interactive daily bars, animated ping rings for degraded/outage states, rich tooltip details with downtime and incident breakdowns, full keyboard navigation, and smooth counter animations.

## Using in Zopra

```rust
use chrono::Duration;
use zopra_components::{
    fill_status_days, status_today, StatusBars, StatusBarsConfig, StatusDay, StatusIncident,
};

// Generate daily history up to today
let today = status_today();
let from = today - Duration::days(89);
let status_history = fill_status_days(from, today, [
    StatusDay::new(today - Duration::days(12))
        .downtime(95)
        .incident(StatusIncident::new("Database failover").minutes(95)),
    StatusDay::new(today - Duration::days(3)).degraded(40),
]);

<StatusBars
    label={"Cloud API"}
    days={status_history}
    bar_height={32.0}
    pulse={true}
/>
```

### Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `label` | `Option<SharedString>` | `"System Status"` | Header service title |
| `days` | `Option<Vec<StatusDay>>` | `vec![]` | Daily records (`StatusDay`) from past to today |
| `config` | `Option<StatusBarsConfig>` | `Default::default()` | Complete `StatusBarsConfig` override |
| `current` | `Option<StatusLevel>` | derived from today | Header operational status override |
| `max_days` | `Option<usize>` | `90` | Maximum visible days (steps down responsively to 60, 30) |
| `min_bar_width` | `Option<f32>` | `3.0` | Minimum bar width before downscaling visible days |
| `compact` | `Option<bool>` | `false` | When true, hides header and footer for dense lists |
| `loading` | `Option<bool>` | `false` | Pulsing skeleton loading state |
| `bar_height` | `Option<f32>` | `32.0` | Bar height in pixels |
| `gap` | `Option<f32>` | `2.0` | Gap between bars in pixels |
| `tooltip_width` | `Option<f32>` | `200.0` | Hover tooltip width in pixels |
| `pulse` | `Option<bool>` | `true` | Pulse ring animation on degraded or outage states |
| `up_color` | `Option<Hsla>` | `#22C55E` | Color for operational days |
| `degraded_color` | `Option<Hsla>` | `#F59E0B` | Color for degraded days |
| `down_color` | `Option<Hsla>` | `#EF4444` | Color for outage days |
| `empty_color` | `Option<Hsla>` | theme muted | Color for days with no data / front padding |

---

## Keyboard Navigation

When focused:
- `Left` / `Right`: Navigate day by day
- `PageUp` / `PageDown`: Jump by 7 days (1 week)
- `Home` / `End`: Jump to first or last day
- `Escape`: Dismiss tooltip

---

## Using in Pure GPUI

```rust
// In your app initialization:
zopra_components::status_bars::init_status_bars(cx);

let bars = cx.new(|cx| {
    GpuiStatusBars::new("API", days, StatusBarsConfig::default(), window, cx)
});
```
