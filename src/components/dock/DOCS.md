# Dock

A macOS-inspired application dock component with smooth pointer-proximity magnification, spring bounce hopping on click, context menus, notification badges, active app indicators, and edge anchoring (`Bottom`, `Top`, `Left`, `Right`).

## Using in Zopra

```rust
use zopra_components::{Dock, DockEntry, DockItem, DockPosition};

let dock_entries = vec![
    DockEntry::Item(DockItem::new(Icon::new(IconName::Bot), "Bot").active(true)),
    DockEntry::Item(DockItem::new(Icon::new(IconName::Folder), "Files").badge(3)),
    DockEntry::Item(DockItem::new(Icon::new(IconName::Search), "Search")),
    DockEntry::Separator,
    DockEntry::Item(DockItem::new(Icon::new(IconName::Settings), "Settings")),
];

<Dock
    entries={dock_entries}
    position={DockPosition::Bottom}
    base_item_size={48.0}
    magnification={72.0}
    gap={10.0}
    roundness={0.5}
    bounce={true}
    tiles={true}
    show_labels={true}
    show_indicators={true}
    show_badges={true}
    on_item_click={on_dock_click}
/>
```

### Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `entries` | `Option<Vec<DockEntry>>` | `vec![]` | List of items and separators |
| `config` | `Option<DockConfig>` | `Default::default()` | Full dock configuration struct |
| `position` | `Option<DockPosition>` | `DockPosition::Bottom` | Screen edge anchoring (`Bottom`, `Top`, `Left`, `Right`) |
| `base_item_size` | `Option<f32>` | `50.0` | Default resting item size in px |
| `magnification` | `Option<f32>` | `70.0` | Peak magnified item size in px |
| `gap` | `Option<f32>` | `10.0` | Gap between dock items in px |
| `roundness` | `Option<f32>` | `0.5` | Corner roundness (0.0 = square, 1.0 = fully round) |
| `bounce` | `Option<bool>` | `true` | Spring hop animation on item click |
| `auto_hide` | `bool` | `false` | Slide panel off-screen when pointer leaves |
| `tiles` | `Option<bool>` | `true` | Show frosted/bordered background tile per icon |
| `show_labels` | `Option<bool>` | `true` | Show floating tooltip label on hover |
| `show_indicators` | `Option<bool>` | `true` | Show dot indicator for active applications |
| `show_badges` | `Option<bool>` | `true` | Show notification pill badges |
| `accent` | `Option<Hsla>` | `theme.primary` | Indicator and active accent color |
| `on_item_click` | `Option<DockClickCallback>` | `None` | Callback invoked with clicked item index |

---

## Using in Pure GPUI

```rust
let dock = cx.new(|cx| {
    Dock::new(dock_entries, DockConfig::default(), cx)
});

// In render:
.child(dock.clone())
```
