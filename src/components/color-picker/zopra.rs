#![cfg(feature = "color-picker")]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    clippy::float_cmp,
    clippy::must_use_candidate,
    clippy::many_single_char_names,
    clippy::redundant_closure_for_method_calls,
    clippy::struct_excessive_bools
)]

use std::sync::Arc;

use gpui_kit::*;
use zopra::component;

pub use super::color::{Color, ColorFormat, Parsed};
pub use super::gpui::{
    ColorPicker as GpuiColorPicker, ColorPickerConfig, ColorPickerEvent, PickerMode,
};

pub type ColorChangeCallback = Arc<dyn Fn(Hsla, &mut App)>;
pub type SwatchesChangeCallback = Arc<dyn Fn(Vec<Option<Hsla>>, &mut App)>;

/// A reactive colour picker component with multiple surfaces, alpha/value sliders, format rows, and swatches.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[component]
pub fn color_picker(
    config: Option<ColorPickerConfig>,
    initial: Option<SharedString>,
    mode: Option<PickerMode>,
    show_mode_switcher: Option<bool>,
    formats: Option<Vec<ColorFormat>>,
    alpha: Option<bool>,
    hex_hash: Option<bool>,
    width: Option<f32>,
    swatch_cols: Option<usize>,
    swatch_rows: Option<usize>,
    on_change: Option<ColorChangeCallback>,
    on_change_end: Option<ColorChangeCallback>,
    on_swatches_changed: Option<SwatchesChangeCallback>,
) {
    let mut cfg = config.unwrap_or_default();
    if let Some(i) = initial {
        cfg.initial = i;
    }
    if let Some(m) = mode {
        cfg.mode = m;
    }
    if let Some(s) = show_mode_switcher {
        cfg.show_mode_switcher = s;
    }
    if let Some(f) = formats {
        cfg.formats = f;
    }
    if let Some(a) = alpha {
        cfg.alpha = a;
    }
    if let Some(h) = hex_hash {
        cfg.hex_hash = h;
    }
    if let Some(w) = width {
        cfg.width = w;
    }
    if let Some(sc) = swatch_cols {
        cfg.swatch_cols = sc;
    }
    if let Some(sr) = swatch_rows {
        cfg.swatch_rows = sr;
    }

    let state_holder = window.use_state(cx, |window, cx| {
        let cfg_clone = cfg.clone();
        cx.new(|cx| GpuiColorPicker::new(cfg_clone, window, cx))
    });

    let state = state_holder.read(cx).clone();

    // Subscribe to events when mounted
    let _sub = window.use_state(cx, |_window, cx| {
        let on_change = on_change.clone();
        let on_change_end = on_change_end.clone();
        let on_swatches_changed = on_swatches_changed.clone();

        cx.subscribe(
            &state,
            move |_this, _emitter, ev: &ColorPickerEvent, cx| match ev {
                ColorPickerEvent::Change(hsla) => {
                    if let Some(cb) = on_change.as_ref() {
                        cb(*hsla, cx);
                    }
                }
                ColorPickerEvent::ChangeEnd(hsla) => {
                    if let Some(cb) = on_change_end.as_ref() {
                        cb(*hsla, cx);
                    }
                }
                ColorPickerEvent::SwatchesChanged(swatches) => {
                    if let Some(cb) = on_swatches_changed.as_ref() {
                        cb(swatches.clone(), cx);
                    }
                }
            },
        )
    });

    div().relative().child(state)
}
