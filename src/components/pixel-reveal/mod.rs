#![cfg(feature = "pixel-reveal")]

pub mod gpui;
pub mod zopra;

pub use gpui::{
    Direction, Ease, From as RevealFrom, Hover, Options as PixelRevealOptions, Pattern, Pos,
    Preset, RevealFont, Span, State as PixelRevealState, Sweep, TextAlign, TextStyle,
    TextTransform, Trigger,
};
pub use zopra::{PixelReveal, init_pixel_reveal, pixel_reveal};
