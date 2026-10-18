#[cfg(feature = "emoji-picker")]
#[path = "emoji-picker/mod.rs"]
pub mod emoji_picker;

#[cfg(feature = "emoji-picker")]
pub use emoji_picker::*;
