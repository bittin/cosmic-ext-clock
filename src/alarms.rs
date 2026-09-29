// SPDX-License-Identifier: GPL-3.0-only

use chrono::{DateTime, Datelike, TimeZone, Weekday};
use icu_calendar::{types::Weekday as IcuWeekday, week::WeekInformation};
use icu_locale::Locale;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const DEFAULT_SNOOZE_MINUTES: u16 = 5;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum AlarmDay {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AlarmPeriod {
    #[default]
    Am,
    Pm,
}

impl AlarmPeriod {
    #[must_use]
    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Am),
            1 => Some(Self::Pm),
            _ => None,
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Am => 0,
            Self::Pm => 1,
        }
    }
}

impl AlarmDay {
    pub const ALL_MONDAY_FIRST: [Self; 7] = [
        Self::Monday,
        Self::Tuesday,
        Self::Wednesday,
        Self::Thursday,
        Self::Friday,
        Self::Saturday,
        Self::Sunday,
    ];

    const fn from_chrono(day: Weekday) -> Self {
        match day {
            Weekday::Mon => Self::Monday,
            Weekday::Tue => Self::Tuesday,
            Weekday::Wed => Self::Wednesday,
            Weekday::Thu => Self::Thursday,
            Weekday::Fri => Self::Friday,
            Weekday::Sat => Self::Saturday,
            Weekday::Sun => Self::Sunday,
        }
    }

    const fn from_icu(day: IcuWeekday) -> Self {
        match day {
            IcuWeekday::Monday => Self::Monday,
            IcuWeekday::Tuesday => Self::Tuesday,
            IcuWeekday::Wednesday => Self::Wednesday,
            IcuWeekday::Thursday => Self::Thursday,
            IcuWeekday::Friday => Self::Friday,
            IcuWeekday::Saturday => Self::Saturday,
            IcuWeekday::Sunday => Self::Sunday,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Alarm {
    pub label: String,
    pub hour: u8,
    pub minute: u8,
    pub enabled: bool,
    #[serde(default)]
    repeat_days: BTreeSet<AlarmDay>,
    #[serde(default = "default_snooze_enabled")]
    pub snooze_enabled: bool,
    #[serde(default = "default_snooze_minutes")]
    pub snooze_minutes: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RingingAlarm {
    pub label: String,
    pub snooze_minutes: Option<u16>,
}

impl Alarm {
    #[must_use]
    pub fn new(label: impl Into<String>, hour: u8, minute: u8) -> Option<Self> {
        (hour < 24 && minute < 60).then(|| Self {
            label: label.into(),
            hour,
            minute,
            enabled: true,
            repeat_days: BTreeSet::new(),
            snooze_enabled: true,
            snooze_minutes: DEFAULT_SNOOZE_MINUTES,
        })
    }

    #[must_use]
    pub fn time_text(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    #[must_use]
    pub fn repeat_days(&self) -> &BTreeSet<AlarmDay> {
        &self.repeat_days
    }

    pub fn set_repeat_day(&mut self, day: AlarmDay, selected: bool) {
        if selected {
            self.repeat_days.insert(day);
        } else {
            self.repeat_days.remove(&day);
        }
    }

    #[must_use]
    pub fn repeats_on(&self, weekday: Weekday) -> bool {
        self.repeat_days.is_empty() || self.repeat_days.contains(&AlarmDay::from_chrono(weekday))
    }

    #[must_use]
    pub fn begin_ringing(&self) -> RingingAlarm {
        RingingAlarm {
            label: self.label.clone(),
            snooze_minutes: self.snooze_enabled.then_some(self.snooze_minutes.max(1)),
        }
    }

    pub fn update_from_draft(&mut self, draft: &AlarmDraft) -> bool {
        self.update_from_draft_with_format(draft, true)
    }

    pub fn update_from_draft_with_format(
        &mut self,
        draft: &AlarmDraft,
        military_time: bool,
    ) -> bool {
        let Some(mut updated) = draft.build_with_format(military_time) else {
            return false;
        };
        updated.enabled = self.enabled;
        *self = updated;
        true
    }
}

#[derive(Clone, Debug)]
pub struct AlarmDraft {
    pub label: String,
    pub hour: String,
    pub minute: String,
    pub period: AlarmPeriod,
    repeat_days: BTreeSet<AlarmDay>,
    pub snooze_enabled: bool,
    pub snooze_minutes: String,
}

impl Default for AlarmDraft {
    fn default() -> Self {
        Self {
            label: String::new(),
            hour: String::new(),
            minute: String::new(),
            period: AlarmPeriod::Am,
            repeat_days: BTreeSet::new(),
            snooze_enabled: true,
            snooze_minutes: DEFAULT_SNOOZE_MINUTES.to_string(),
        }
    }
}

impl AlarmDraft {
    #[must_use]
    pub fn from_alarm(alarm: &Alarm) -> Self {
        Self::from_alarm_with_format(alarm, true)
    }

    #[must_use]
    pub fn from_alarm_with_format(alarm: &Alarm, military_time: bool) -> Self {
        let (hour, period) = display_hour(alarm.hour, military_time);
        Self {
            label: alarm.label.clone(),
            hour: format_display_hour(hour, military_time),
            minute: format!("{:02}", alarm.minute),
            period,
            repeat_days: alarm.repeat_days.clone(),
            snooze_enabled: alarm.snooze_enabled,
            snooze_minutes: alarm.snooze_minutes.to_string(),
        }
    }

    #[must_use]
    pub fn repeat_days(&self) -> &BTreeSet<AlarmDay> {
        &self.repeat_days
    }

    pub fn set_repeat_day(&mut self, day: AlarmDay, selected: bool) {
        if selected {
            self.repeat_days.insert(day);
        } else {
            self.repeat_days.remove(&day);
        }
    }

    #[must_use]
    pub fn build(&self) -> Option<Alarm> {
        self.build_with_format(true)
    }

    #[must_use]
    pub fn build_with_format(&self, military_time: bool) -> Option<Alarm> {
        let hour = canonical_hour(self.hour.parse::<u8>().ok()?, self.period, military_time)?;
        let mut alarm = Alarm::new(self.label.trim(), hour, self.minute.parse::<u8>().ok()?)?;
        alarm.repeat_days.clone_from(&self.repeat_days);
        alarm.snooze_enabled = self.snooze_enabled;
        if let Ok(minutes) = self.snooze_minutes.parse::<u16>() {
            alarm.snooze_minutes = minutes.clamp(1, 60);
        } else if self.snooze_enabled {
            return None;
        }
        Some(alarm)
    }

    pub fn reformat_hour(&mut self, previous_military_time: bool, military_time: bool) {
        if previous_military_time == military_time {
            return;
        }
        let Ok(hour) = self.hour.parse::<u8>() else {
            return;
        };
        let Some(hour) = canonical_hour(hour, self.period, previous_military_time) else {
            return;
        };
        let (hour, period) = display_hour(hour, military_time);
        self.hour = format_display_hour(hour, military_time);
        self.period = period;
    }
}

fn format_display_hour(hour: u8, military_time: bool) -> String {
    if military_time {
        format!("{hour:02}")
    } else {
        hour.to_string()
    }
}

fn canonical_hour(hour: u8, period: AlarmPeriod, military_time: bool) -> Option<u8> {
    if military_time {
        return (hour < 24).then_some(hour);
    }
    if !(1..=12).contains(&hour) {
        return None;
    }
    Some(match (hour, period) {
        (12, AlarmPeriod::Am) => 0,
        (12, AlarmPeriod::Pm) => 12,
        (_, AlarmPeriod::Am) => hour,
        (_, AlarmPeriod::Pm) => hour + 12,
    })
}

const fn display_hour(hour: u8, military_time: bool) -> (u8, AlarmPeriod) {
    if military_time {
        (hour, AlarmPeriod::Am)
    } else if hour == 0 {
        (12, AlarmPeriod::Am)
    } else if hour < 12 {
        (hour, AlarmPeriod::Am)
    } else if hour == 12 {
        (12, AlarmPeriod::Pm)
    } else {
        (hour - 12, AlarmPeriod::Pm)
    }
}

#[must_use]
pub fn weekday_order_for_locale(locale: &str) -> [AlarmDay; 7] {
    let first = locale
        .parse::<Locale>()
        .ok()
        .and_then(|locale| WeekInformation::try_new(locale.into()).ok())
        .map_or(AlarmDay::Monday, |week| {
            AlarmDay::from_icu(week.first_weekday)
        });
    let start = AlarmDay::ALL_MONDAY_FIRST
        .iter()
        .position(|day| *day == first)
        .unwrap_or(0);
    std::array::from_fn(|offset| AlarmDay::ALL_MONDAY_FIRST[(start + offset) % 7])
}

#[must_use]
pub fn alarm_should_ring<Tz>(alarm: &Alarm, previous: DateTime<Tz>, now: DateTime<Tz>) -> bool
where
    Tz: TimeZone,
{
    if !alarm.enabled || now <= previous || !alarm.repeats_on(now.weekday()) {
        return false;
    }

    let Some(target) =
        now.naive_local()
            .date()
            .and_hms_opt(u32::from(alarm.hour), u32::from(alarm.minute), 0)
    else {
        return false;
    };

    previous.naive_local() < target && now.naive_local() >= target
}

const fn default_snooze_enabled() -> bool {
    true
}

const fn default_snooze_minutes() -> u16 {
    DEFAULT_SNOOZE_MINUTES
}
