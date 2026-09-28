// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    alarms::{Alarm, AlarmDay},
    stopwatch::Stopwatch,
    timers::{Timer, TimerId},
    world_clocks::{WorldClock, preview_world_clocks},
};
use std::time::{Duration, Instant};

pub struct PreviewData {
    pub now_instant: Instant,
    pub world_clocks: Vec<WorldClock>,
    pub alarms: Vec<Alarm>,
    pub timers: Vec<Timer>,
    pub stopwatch: Stopwatch,
}

#[must_use]
pub fn preview_data(anchor: Instant) -> PreviewData {
    let mut morning = Alarm::new("Morning", 7, 0).expect("preview alarm must be valid");
    for day in [
        AlarmDay::Monday,
        AlarmDay::Tuesday,
        AlarmDay::Wednesday,
        AlarmDay::Thursday,
        AlarmDay::Friday,
    ] {
        morning.set_repeat_day(day, true);
    }
    let lunch = Alarm::new("Lunch", 12, 30).expect("preview alarm must be valid");
    let mut wind_down = Alarm::new("Wind down", 21, 45).expect("preview alarm must be valid");
    wind_down.enabled = false;

    let tea = Timer::new(TimerId::new(1), "Tea", Duration::from_secs(5 * 60))
        .expect("preview timer must be valid");
    let mut laundry = Timer::new(TimerId::new(2), "Laundry", Duration::from_secs(45 * 60))
        .expect("preview timer must be valid");
    laundry.start(anchor);
    laundry.pause(anchor + Duration::from_secs(12 * 60));

    let mut focus = Timer::new(
        TimerId::new(3),
        "Focus session",
        Duration::from_secs(25 * 60),
    )
    .expect("preview timer must be valid");
    let focus_start = anchor + Duration::from_secs(12 * 60);
    focus.start(focus_start);
    let now_instant = focus_start + Duration::from_secs(7 * 60 + 30);

    let mut stopwatch = Stopwatch::default();
    stopwatch.start(anchor);
    stopwatch.lap(anchor + Duration::from_millis(42_180));
    stopwatch.lap(anchor + Duration::from_millis(85_640));
    stopwatch.pause(anchor + Duration::from_millis(132_470));

    PreviewData {
        now_instant,
        world_clocks: preview_world_clocks(),
        alarms: vec![morning, lunch, wind_down],
        timers: vec![tea, laundry, focus],
        stopwatch,
    }
}
