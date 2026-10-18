#![cfg(feature = "status-bars")]

use gpui_kit::actions;
actions!(status_bars, [Prev, Next, PrevWeek, NextWeek, First, Last, Dismiss]);

pub mod gpui;
pub mod zopra;

pub use gpui::{
    day_status, day_uptime, fill_status_days, init as init_status_bars, status_today,
    StatusBars as GpuiStatusBars, StatusBarsConfig, StatusDay, StatusIncident, StatusLevel,
};
pub use zopra::*;

