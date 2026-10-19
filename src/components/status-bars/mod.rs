#![cfg(feature = "status-bars")]

use gpui_kit::actions;
actions!(
    status_bars,
    [Prev, Next, PrevWeek, NextWeek, First, Last, Dismiss]
);

pub mod gpui;
pub mod zopra;

pub use gpui::{
    StatusBars as GpuiStatusBars, StatusBarsConfig, StatusDay, StatusIncident, StatusLevel,
    day_status, day_uptime, fill_status_days, init as init_status_bars, status_today,
};
pub use zopra::*;
