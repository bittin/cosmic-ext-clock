// SPDX-License-Identifier: GPL-3.0-only

use chrono::{DateTime, Offset, TimeZone, Utc};
use chrono_tz::{TZ_VARIANTS, Tz};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

const CITY_TIMEZONE_REGIONS: [&str; 10] = [
    "Africa/",
    "America/",
    "Antarctica/",
    "Arctic/",
    "Asia/",
    "Atlantic/",
    "Australia/",
    "Europe/",
    "Indian/",
    "Pacific/",
];

static CITY_CATALOG: LazyLock<Vec<(String, &'static str)>> = LazyLock::new(|| {
    let mut cities: Vec<_> = TZ_VARIANTS
        .iter()
        .map(|timezone| timezone.name())
        .filter(|timezone| {
            CITY_TIMEZONE_REGIONS
                .iter()
                .any(|region| timezone.starts_with(region))
        })
        .map(|timezone| {
            let city = timezone
                .rsplit('/')
                .next()
                .unwrap_or(timezone)
                .replace('_', " ");
            (city, timezone)
        })
        .collect();
    cities.sort_by_cached_key(|(city, timezone)| (city.to_lowercase(), *timezone));
    cities
});

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorldClock {
    pub name: String,
    pub timezone: Tz,
}

impl WorldClock {
    #[must_use]
    pub fn new(name: impl Into<String>, timezone: &str) -> Option<Self> {
        Some(Self {
            name: name.into(),
            timezone: timezone.parse().ok()?,
        })
    }

    #[must_use]
    pub fn time_text(&self, now: DateTime<Utc>) -> String {
        now.with_timezone(&self.timezone)
            .format("%H:%M")
            .to_string()
    }

    #[must_use]
    pub fn date_text(&self, now: DateTime<Utc>) -> String {
        now.with_timezone(&self.timezone)
            .format("%a, %b %-d")
            .to_string()
    }

    #[must_use]
    pub fn offset_seconds(&self, now: DateTime<Utc>) -> i32 {
        now.with_timezone(&self.timezone)
            .offset()
            .fix()
            .local_minus_utc()
    }
}

#[must_use]
pub fn format_utc_offset(seconds: i32) -> String {
    let sign = if seconds < 0 { '-' } else { '+' };
    let total_minutes = seconds.unsigned_abs() / 60;
    format!(
        "UTC{sign}{:02}:{:02}",
        total_minutes / 60,
        total_minutes % 60
    )
}

#[must_use]
pub fn world_clock_for_timezone(timezone: &str) -> Option<WorldClock> {
    let city = timezone
        .rsplit('/')
        .next()
        .unwrap_or(timezone)
        .replace('_', " ");
    WorldClock::new(city, timezone)
}

#[must_use]
pub fn initial_world_clocks(timezone: Option<&str>) -> Vec<WorldClock> {
    timezone
        .and_then(world_clock_for_timezone)
        .or_else(|| world_clock_for_timezone("UTC"))
        .into_iter()
        .collect()
}

#[must_use]
pub fn preview_world_clocks() -> Vec<WorldClock> {
    [
        "Europe/London",
        "America/New_York",
        "Asia/Tokyo",
        "Australia/Sydney",
    ]
    .into_iter()
    .map(|timezone| {
        world_clock_for_timezone(timezone)
            .expect("preview timezones must exist in the time zone database")
    })
    .collect()
}

#[must_use]
pub fn preview_timestamp() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0)
        .single()
        .expect("preview time must be valid")
}

#[must_use]
pub fn city_catalog() -> &'static [(String, &'static str)] {
    &CITY_CATALOG
}
