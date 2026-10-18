#![cfg(feature = "option-wheel")]

use gpui_kit::actions;
actions!(option_wheel, [Prev, Next]);

pub mod gpui;
pub mod zopra;

pub use zopra::*;
