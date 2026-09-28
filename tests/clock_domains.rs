// SPDX-License-Identifier: GPL-3.0-only

use chrono::{Datelike, TimeZone, Utc, Weekday};
use clock::{
    alarms::{Alarm, AlarmDay, AlarmDraft, alarm_should_ring, weekday_order_for_locale},
    alerts::{AlertAction, AlertEvent, bundled_alarm_sound},
    preview::preview_data,
    stopwatch::Stopwatch,
    timers::{Timer, TimerId, TimerStatus},
    world_clocks::{
        WorldClock, city_catalog, format_utc_offset, initial_world_clocks, preview_timestamp,
        preview_world_clocks, world_clock_for_timezone,
    },
};
use notify_rust::{CloseReason, NotificationResponse};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

#[test]
fn alert_events_provide_notification_content() {
    let alarm = AlertEvent::alarm("Morning", "07:30", Some(5));
    assert_eq!(alarm.summary_key(), "notification-alarm-title");
    assert_eq!(alarm.body(), "07:30: Morning");

    let unnamed_alarm = AlertEvent::alarm("", "07:30", Some(5));
    assert_eq!(unnamed_alarm.body(), "07:30");

    let timer_id = TimerId::new(7);
    let timer = AlertEvent::timer(timer_id, "Tea");
    assert_eq!(timer.summary_key(), "notification-timer-title");
    assert_eq!(timer.body(), "Tea");
    assert_eq!(timer.timer_id(), Some(timer_id));
}

#[test]
fn notifications_expose_their_default_actions() {
    let timer = AlertEvent::timer(TimerId::new(9), "Tea");
    assert_eq!(timer.notification_action_keys(), &["default", "stop"]);

    let alarm = AlertEvent::alarm("Morning", "07:30", Some(5));
    assert_eq!(
        alarm.notification_action_keys(),
        &["default", "snooze", "dismiss"]
    );
    assert_eq!(
        alarm.action_for_notification_response(&NotificationResponse::Default),
        Some(AlertAction::SnoozeAlarm)
    );
    assert_eq!(
        alarm.action_for_notification_response(&NotificationResponse::Action("snooze".to_owned())),
        Some(AlertAction::SnoozeAlarm)
    );
    assert_eq!(
        alarm.action_for_notification_response(&NotificationResponse::Action("dismiss".to_owned())),
        Some(AlertAction::DismissAlarm)
    );

    let alarm_without_snooze = AlertEvent::alarm("Early", "06:00", None);
    assert_eq!(
        alarm_without_snooze.notification_action_keys(),
        &["default", "dismiss"]
    );
    assert_eq!(
        alarm_without_snooze.action_for_notification_response(&NotificationResponse::Default),
        Some(AlertAction::DismissAlarm)
    );
}

#[test]
fn bundled_alarm_sound_is_a_non_silent_wave_file() {
    let bytes = bundled_alarm_sound();
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    assert!(bytes.len() > 44_100);
    assert!(bytes[44..].iter().any(|sample| *sample != 0));
}

#[test]
fn timer_notification_actions_apply_the_expected_timer_outcome() {
    let timer_id = TimerId::new(8);
    let timer = AlertEvent::timer(timer_id, "Laundry");
    assert_eq!(
        timer.action_for_notification_response(&NotificationResponse::Default),
        Some(AlertAction::StopTimer(timer_id))
    );
    assert_eq!(
        timer.action_for_notification_response(&NotificationResponse::Action("stop".to_owned())),
        Some(AlertAction::StopTimer(timer_id))
    );
    assert_eq!(
        timer.action_for_notification_response(&NotificationResponse::Closed(
            CloseReason::Dismissed
        )),
        Some(AlertAction::ResetTimer(timer_id))
    );
}

#[test]
fn world_clock_formats_local_time_and_offset() {
    let clock = WorldClock::new("Tokyo", "Asia/Tokyo").expect("valid timezone");
    let now = Utc.with_ymd_and_hms(2026, 1, 15, 12, 30, 0).unwrap();

    assert_eq!(clock.time_text(now), "21:30");
    assert_eq!(clock.date_text(now), "Thu, Jan 15");
    assert_eq!(format_utc_offset(clock.offset_seconds(now)), "UTC+09:00");
    assert!(WorldClock::new("Invalid", "Mars/Olympus").is_none());
}

#[test]
fn initial_world_clock_uses_the_current_timezone_city() {
    let clock = world_clock_for_timezone("America/Argentina/Buenos_Aires")
        .expect("valid timezone should produce a clock");

    assert_eq!(clock.name, "Buenos Aires");
    assert_eq!(clock.timezone.name(), "America/Argentina/Buenos_Aires");
}

#[test]
fn first_run_contains_only_the_current_timezone_city() {
    let clocks = initial_world_clocks(Some("Europe/Berlin"));

    assert_eq!(clocks.len(), 1);
    assert_eq!(clocks[0].name, "Berlin");
    assert_eq!(clocks[0].timezone.name(), "Europe/Berlin");
}

#[test]
fn preview_uses_four_popular_world_clocks() {
    let clocks = preview_world_clocks();
    let cities: Vec<_> = clocks.iter().map(|clock| clock.name.as_str()).collect();
    let timezones: Vec<_> = clocks.iter().map(|clock| clock.timezone.name()).collect();

    assert_eq!(cities, ["London", "New York", "Tokyo", "Sydney"]);
    assert_eq!(
        timezones,
        [
            "Europe/London",
            "America/New_York",
            "Asia/Tokyo",
            "Australia/Sydney"
        ]
    );
}

#[test]
fn preview_timestamp_is_fixed() {
    assert_eq!(
        preview_timestamp(),
        Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap()
    );
}

#[test]
fn preview_data_populates_every_clock_page() {
    let anchor = Instant::now();
    let data = preview_data(anchor);

    assert_eq!(data.world_clocks.len(), 4);
    assert_eq!(
        data.alarms.iter().map(Alarm::time_text).collect::<Vec<_>>(),
        ["07:00", "12:30", "21:45"]
    );
    assert_eq!(
        data.timers
            .iter()
            .map(|timer| timer.label.as_str())
            .collect::<Vec<_>>(),
        ["Tea", "Laundry", "Focus session"]
    );
    assert_eq!(data.timers[0].status(), TimerStatus::Ready);
    assert_eq!(data.timers[1].status(), TimerStatus::Paused);
    assert_eq!(data.timers[2].status(), TimerStatus::Running);
    assert_eq!(
        data.stopwatch.elapsed_at(data.now_instant),
        Duration::from_millis(132_470)
    );
    assert_eq!(data.stopwatch.laps().len(), 2);
}

#[test]
fn city_catalog_contains_every_geographic_timezone_in_alphabetical_order() {
    const REGIONS: [&str; 10] = [
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
    let expected: BTreeSet<_> = chrono_tz::TZ_VARIANTS
        .iter()
        .map(|timezone| timezone.name())
        .filter(|timezone| REGIONS.iter().any(|region| timezone.starts_with(region)))
        .collect();
    let actual: BTreeSet<_> = city_catalog()
        .iter()
        .map(|(_, timezone)| *timezone)
        .collect();

    assert_eq!(actual, expected);

    let keys: Vec<_> = city_catalog()
        .iter()
        .map(|(city, timezone)| (city.to_lowercase(), *timezone))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted);
}

#[test]
fn alarm_rings_when_its_minute_is_crossed() {
    let alarm = Alarm::new("Morning", 7, 30).expect("valid alarm");
    let previous = Utc.with_ymd_and_hms(2026, 9, 27, 7, 29, 50).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 9, 27, 7, 30, 2).unwrap();

    assert!(alarm_should_ring(&alarm, previous, now));
    assert!(!alarm_should_ring(&alarm, now, now));
    assert!(Alarm::new("Invalid", 24, 0).is_none());
}

#[test]
fn new_alarm_has_no_repeat_days_and_five_minute_snooze() {
    let alarm = Alarm::new("Morning", 7, 30).expect("valid alarm");

    assert!(alarm.repeat_days().is_empty());
    assert!(alarm.snooze_enabled);
    assert_eq!(alarm.snooze_minutes, 5);
}

#[test]
fn beginning_to_ring_does_not_disable_an_alarm() {
    let alarm = Alarm::new("Morning", 7, 30).expect("valid alarm");

    let ringing = alarm.begin_ringing();

    assert!(alarm.enabled);
    assert_eq!(ringing.label, "Morning");
    assert_eq!(ringing.time, "07:30");
    assert_eq!(ringing.snooze_minutes, Some(5));
}

#[test]
fn repeating_alarm_only_rings_on_selected_weekdays() {
    let mut alarm = Alarm::new("Work", 7, 30).expect("valid alarm");
    alarm.set_repeat_day(AlarmDay::Monday, true);

    let sunday_before = Utc.with_ymd_and_hms(2026, 9, 27, 7, 29, 50).unwrap();
    let sunday_after = Utc.with_ymd_and_hms(2026, 9, 27, 7, 30, 2).unwrap();
    assert_eq!(sunday_after.weekday(), Weekday::Sun);
    assert!(!alarm_should_ring(&alarm, sunday_before, sunday_after));

    let monday_before = Utc.with_ymd_and_hms(2026, 9, 28, 7, 29, 50).unwrap();
    let monday_after = Utc.with_ymd_and_hms(2026, 9, 28, 7, 30, 2).unwrap();
    assert!(alarm_should_ring(&alarm, monday_before, monday_after));
}

#[test]
fn weekday_order_uses_the_locales_first_weekday() {
    assert_eq!(weekday_order_for_locale("en-US")[0], AlarmDay::Sunday);
    assert_eq!(weekday_order_for_locale("de-DE")[0], AlarmDay::Monday);
}

#[test]
fn alarm_draft_builds_repeat_and_snooze_settings() {
    let mut draft = AlarmDraft::default();
    draft.label = "Gym".to_owned();
    draft.hour = "08".to_owned();
    draft.minute = "15".to_owned();
    draft.set_repeat_day(AlarmDay::Wednesday, true);
    draft.snooze_enabled = false;
    draft.snooze_minutes = "10".to_owned();

    let alarm = draft.build().expect("valid alarm draft");
    assert!(alarm.repeat_days().contains(&AlarmDay::Wednesday));
    assert!(!alarm.snooze_enabled);
    assert_eq!(alarm.snooze_minutes, 10);
}

#[test]
fn alarm_draft_does_not_require_duration_when_snooze_is_disabled() {
    let mut draft = AlarmDraft::default();
    draft.hour = "08".to_owned();
    draft.minute = "15".to_owned();
    draft.snooze_enabled = false;
    draft.snooze_minutes.clear();

    let alarm = draft.build().expect("disabled snooze needs no duration");
    assert!(!alarm.snooze_enabled);
    assert_eq!(alarm.snooze_minutes, 5);
}

#[test]
fn alarm_can_be_loaded_into_a_draft_and_edited_without_enabling_it() {
    let mut alarm = Alarm::new("Work", 7, 30).expect("valid alarm");
    alarm.enabled = false;
    alarm.set_repeat_day(AlarmDay::Monday, true);
    alarm.snooze_enabled = false;

    let mut draft = AlarmDraft::from_alarm(&alarm);
    assert_eq!(draft.label, "Work");
    assert_eq!(draft.hour, "07");
    assert_eq!(draft.minute, "30");
    assert!(draft.repeat_days().contains(&AlarmDay::Monday));
    assert!(!draft.snooze_enabled);

    draft.label = "Office".to_owned();
    draft.hour = "08".to_owned();
    draft.minute = "15".to_owned();
    assert!(alarm.update_from_draft(&draft));
    assert_eq!(alarm.label, "Office");
    assert_eq!(alarm.time_text(), "08:15");
    assert!(!alarm.enabled);
}

#[test]
fn timer_uses_monotonic_elapsed_time_and_can_pause() {
    let start = Instant::now();
    let mut timer =
        Timer::new(TimerId::new(1), "Tea", Duration::from_secs(300)).expect("valid timer");

    timer.start(start);
    assert_eq!(
        timer.remaining_at(start + Duration::from_secs(65)),
        Duration::from_secs(235)
    );
    timer.pause(start + Duration::from_secs(65));
    assert_eq!(timer.status(), TimerStatus::Paused);
    assert_eq!(
        timer.remaining_at(start + Duration::from_secs(200)),
        Duration::from_secs(235)
    );
}

#[test]
fn timer_finishes_without_tick_accumulation_drift() {
    let start = Instant::now();
    let timer_id = TimerId::new(2);
    let mut timer = Timer::new(timer_id, "Short", Duration::from_secs(2)).expect("valid timer");

    timer.start(start);
    assert!(timer.update(start + Duration::from_millis(2_500)));
    assert_eq!(timer.id(), timer_id);
    assert_eq!(timer.status(), TimerStatus::Finished);
    assert!(timer.is_ringing());
    assert_eq!(
        timer.remaining_at(start + Duration::from_secs(20)),
        Duration::ZERO
    );
}

#[test]
fn finished_timer_can_be_acknowledged_without_resetting_it() {
    let start = Instant::now();
    let mut timer =
        Timer::new(TimerId::new(3), "Laundry", Duration::from_secs(1)).expect("valid timer");

    timer.start(start);
    assert!(timer.update(start + Duration::from_secs(1)));
    assert!(timer.acknowledge());
    assert!(!timer.is_ringing());
    assert_eq!(timer.status(), TimerStatus::Finished);
    assert!(!timer.acknowledge());
}

#[test]
fn timer_can_be_reconfigured_without_changing_its_identity() {
    let start = Instant::now();
    let timer_id = TimerId::new(4);
    let mut timer = Timer::new(timer_id, "Tea", Duration::from_secs(60)).expect("valid timer");
    timer.start(start);

    assert!(timer.reconfigure("Coffee", Duration::from_secs(90)));
    assert_eq!(timer.id(), timer_id);
    assert_eq!(timer.label, "Coffee");
    assert_eq!(timer.duration(), Duration::from_secs(90));
    assert_eq!(timer.status(), TimerStatus::Ready);
    assert_eq!(
        timer.remaining_at(start + Duration::from_secs(120)),
        Duration::from_secs(90)
    );
    assert!(!timer.is_ringing());

    assert!(!timer.reconfigure("Invalid", Duration::ZERO));
    assert_eq!(timer.label, "Coffee");
    assert_eq!(timer.duration(), Duration::from_secs(90));
}

#[test]
fn stopwatch_tracks_elapsed_time_and_laps_monotonically() {
    let start = Instant::now();
    let mut stopwatch = Stopwatch::default();

    stopwatch.start(start);
    assert_eq!(
        stopwatch.elapsed_at(start + Duration::from_millis(1_250)),
        Duration::from_millis(1_250)
    );
    stopwatch.lap(start + Duration::from_millis(1_250));
    stopwatch.pause(start + Duration::from_millis(2_000));

    assert_eq!(
        stopwatch.elapsed_at(start + Duration::from_secs(10)),
        Duration::from_secs(2)
    );
    assert_eq!(stopwatch.laps(), &[Duration::from_millis(1_250)]);
}
