#![cfg(feature = "pixel-reveal")]
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
    clippy::struct_excessive_bools,
    clippy::manual_midpoint,
    clippy::missing_panics_doc
)]

use std::rc::Rc;
use std::sync::OnceLock;

use gpui_kit::*;
use zopra::component;

pub use super::gpui::{
    Direction, Ease, From as RevealFrom, Hover, Options as PixelRevealOptions, Pattern,
    PixelReveal as GpuiPixelReveal, PixelRevealEvent, Pos, Preset, RevealFont, Span,
    State as PixelRevealState, Sweep, TextAlign, TextStyle, TextTransform, Trigger,
};

static DEFAULT_FONT_BYTES: &[u8] = include_bytes!("../../../assets/font.ttf");

pub fn init_pixel_reveal(cx: &mut App) {
    if let Ok(font) = RevealFont::from_bytes(DEFAULT_FONT_BYTES) {
        _ = font.register(cx);
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
#[component]
pub fn pixel_reveal(
    text: Option<SharedString>,
    spans: Option<Vec<Span>>,
    font_family: Option<SharedString>,
    font: Option<Rc<RevealFont>>,
    style: Option<TextStyle>,
    options: Option<PixelRevealOptions>,
    preset: Option<Preset>,
    trigger: Option<Trigger>,
    font_size: Option<f32>,
    color: Option<Hsla>,
) {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    let font_rc = font.unwrap_or_else(|| {
        if let Some(family) = font_family {
            RevealFont::os_or_fallback(family, DEFAULT_FONT_BYTES)
        } else {
            let f = RevealFont::from_bytes(DEFAULT_FONT_BYTES).expect("valid default font");
            REGISTERED.get_or_init(|| {
                let _ = f.register(cx);
            });
            f
        }
    });

    let mut resolved_style = style.unwrap_or_else(|| {
        TextStyle::new(
            font_size.unwrap_or(36.0),
            color.unwrap_or_else(|| rgb(0xffffff).into()),
        )
    });
    if let Some(fs) = font_size {
        resolved_style.font_size = fs;
        resolved_style.line_height = fs * 1.2;
    }
    if let Some(c) = color {
        resolved_style.color = c;
    }

    let mut opts = options.unwrap_or_default();
    if let Some(p) = preset {
        opts = opts.preset(p);
    }

    let trig = trigger.unwrap_or(Trigger::Load);
    opts = opts.trigger(trig);

    let text_content = text.unwrap_or_else(|| SharedString::from("Pixels resolve into type."));
    let spans_list = spans.unwrap_or_else(|| vec![Span::new(text_content)]);

    let state_holder = window.use_state(cx, |_window, cx| {
        let spans_clone = spans_list.clone();
        let font_clone = font_rc.clone();
        let style_clone = resolved_style.clone();
        let opts_clone = opts.clone();
        cx.new(|_cx| {
            GpuiPixelReveal::with_spans(spans_clone, font_clone, style_clone, opts_clone)
        })
    });

    let state = state_holder.read(cx).clone();


    state.update(cx, |this, cx| {
        if this.font.family != font_rc.family {
            this.font = font_rc;
            this.stale = true;
            cx.notify();
        }
        if this.options != opts {
            this.set_options(opts, cx);
        }
        if this.style.font_size != resolved_style.font_size
            || this.style.color != resolved_style.color
        {
            this.set_style(resolved_style, cx);
        }
        if this.spans.len() != spans_list.len() {
            this.set_spans(spans_list, cx);
        }
    });

    div().relative().w_full().child(state)
}
