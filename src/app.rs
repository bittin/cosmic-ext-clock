// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    alarms::{
        Alarm, AlarmDay, AlarmDraft, AlarmPeriod, alarm_should_ring, weekday_order_for_locale,
    },
    alerts::{
        AlertAction, AlertEvent, AlertSound, AlertSubscriptionEvent, alert_action_subscription,
        close_timer_notification, send_notification,
    },
    fl,
    preview::preview_data,
    stopwatch::Stopwatch,
    timers::{Timer, TimerId, TimerStatus},
    world_clocks::{
        WorldClock, city_catalog, format_utc_offset, initial_world_clocks, preview_timestamp,
        relative_day_offset, relative_offset_minutes,
    },
};
use chrono::{
    DateTime, Duration as ChronoDuration, FixedOffset, Local, Timelike, Utc,
    format::Locale as ChronoLocale,
};
use cosmic::cosmic_config;
use cosmic::iced::futures::channel::mpsc::Sender;
use cosmic::{
    Application, Core, Element, Theme,
    app::context_drawer,
    cosmic_config::{
        Config, ConfigGet, ConfigSet, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry,
    },
    executor,
    iced::{
        Alignment, Background, Border, Length, Shadow, Subscription, mouse,
        widget::{column, row},
    },
    surface, theme,
    widget::{
        self, RcElementWrapper,
        about::About,
        button, icon,
        icon::from_name as symbolic,
        menu::{self, ItemHeight, ItemWidth, key_bind::KeyBind},
        nav_bar,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    time::{Duration, Instant},
};

const REPOSITORY_URL: &str = "https://github.com/cosmic-utils/cosmic-ext-clock";
const SUPPORT_URL: &str = "https://github.com/cosmic-utils/cosmic-ext-clock/issues";
const WEBSITE_URL: &str = "https://cosmic-utils.org";
const TIME_CONFIG_ID: &str = "com.system76.CosmicAppletTime";
const APP_ICON: &[u8] =
    include_bytes!("../resources/icons/hicolor/256x256/apps/org.cosmic_utils.clock.png");

#[derive(Clone, CosmicConfigEntry, Debug, Default, Eq, PartialEq)]
#[version = 1]
struct TimeConfig {
    military_time: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum AppTheme {
    #[default]
    System,
    Dark,
    Light,
}

impl AppTheme {
    fn from_dropdown_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::System),
            1 => Some(Self::Dark),
            2 => Some(Self::Light),
            _ => None,
        }
    }

    fn dropdown_index(self) -> usize {
        match self {
            Self::System => 0,
            Self::Dark => 1,
            Self::Light => 2,
        }
    }

    fn theme(self) -> Theme {
        if is_cosmic_desktop() {
            match self {
                Self::System => theme::system_preference(),
                Self::Dark => {
                    let mut theme = theme::system_dark();
                    theme.theme_type.prefer_dark(Some(true));
                    theme
                }
                Self::Light => {
                    let mut theme = theme::system_light();
                    theme.theme_type.prefer_dark(Some(false));
                    theme
                }
            }
        } else {
            match self {
                Self::System => theme::system_preference(),
                Self::Dark => Theme::dark(),
                Self::Light => Theme::light(),
            }
        }
    }
}

fn is_cosmic_desktop() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .is_ok_and(|desktop| desktop.to_ascii_uppercase().contains("COSMIC"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Page {
    WorldClocks,
    Alarms,
    Timers,
    Stopwatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextPage {
    About,
    Settings,
    CityPicker,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PersistentState {
    world_clocks: Vec<WorldClock>,
    alarms: Vec<Alarm>,
    app_theme: AppTheme,
}

impl Default for PersistentState {
    fn default() -> Self {
        let timezone = iana_time_zone::get_timezone().ok();
        Self {
            world_clocks: initial_world_clocks(timezone.as_deref()),
            alarms: Vec::new(),
            app_theme: AppTheme::System,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Flags {
    pub initial_theme: Option<AppTheme>,
    pub initial_page: u16,
    pub locale: String,
    pub preview: bool,
}

#[derive(Clone, Debug)]
pub enum Message {
    Tick,
    Surface(surface::Action<Message>),
    OpenUrl(String),
    ToggleAbout,
    ToggleSettings,
    SetAppTheme(usize),
    CosmicThemeChanged,
    TimeFormatChanged(bool),
    ResetAllSettings,
    OpenCityPicker,
    CloseCityPicker,
    CitySearchChanged(String),
    ClearCitySearch,
    AddWorldClock(usize),
    EditWorldClock(usize),
    CancelWorldClockEdit,
    RemoveWorldClock(usize),
    OpenAlarmForm,
    EditAlarm(usize),
    CancelAlarmForm,
    AlarmLabelChanged(String),
    AlarmHourChanged(String),
    AlarmMinuteChanged(String),
    AlarmPeriodChanged(usize),
    ToggleAlarmRepeat(AlarmDay),
    ToggleAlarmSnooze(bool),
    AlarmSnoozeMinutesChanged(String),
    AddAlarm,
    ToggleAlarm(usize, bool),
    RemoveAlarm(usize),
    SnoozeAlarm,
    DismissAlarm,
    AlertSubscription(AlertSubscriptionEvent),
    TimerLabelChanged(String),
    TimerMinutesChanged(String),
    TimerSecondsChanged(String),
    OpenTimerForm,
    AddTimer,
    EditTimer(usize),
    CancelTimerForm,
    ToggleTimer(usize),
    ResetTimer(usize),
    RemoveTimer(usize),
    CollectionTileHover(CollectionTile, bool),
    ToggleStopwatch,
    ResetStopwatch,
    AddLap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MenuItemAction {
    Settings,
    About,
}

impl menu::action::MenuAction for MenuItemAction {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            Self::Settings => Message::ToggleSettings,
            Self::About => Message::ToggleAbout,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AlarmRepeatAction {
    Toggle(AlarmDay),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollectionTile {
    WorldClock(usize),
    Alarm(usize),
    Timer(TimerId),
}

impl menu::action::MenuAction for AlarmRepeatAction {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            Self::Toggle(day) => Message::ToggleAlarmRepeat(*day),
        }
    }
}

fn about_widget() -> About {
    About::default()
        .name(fl!("clock"))
        .icon(icon::from_raster_bytes(APP_ICON))
        .version(env!("GIT_VERSION"))
        .author("Frederic Laing")
        .comments(fl!("desktop-comment"))
        .license("GPL-3.0-only")
        .developers([("Frederic Laing", "frederic.laing.development@gmail.com")])
        .links([
            (fl!("website"), WEBSITE_URL),
            (fl!("repository"), REPOSITORY_URL),
            (fl!("support"), SUPPORT_URL),
        ])
}

pub struct ClockApp {
    core: Core,
    nav_model: nav_bar::Model,
    about: About,
    menu_key_binds: HashMap<KeyBind, MenuItemAction>,
    context_page: ContextPage,
    config: Option<Config>,
    app_theme: AppTheme,
    preview: bool,
    now_utc: DateTime<Utc>,
    previous_local: DateTime<Local>,
    now_instant: Instant,
    world_clocks: Vec<WorldClock>,
    editing_world_clock: Option<usize>,
    city_search: String,
    city_search_id: widget::Id,
    alarms: Vec<Alarm>,
    alarm_form_open: bool,
    editing_alarm: Option<usize>,
    alarm_draft: AlarmDraft,
    alarm_hour_id: widget::Id,
    alarm_minute_id: widget::Id,
    locale: String,
    military_time: bool,
    ringing_alarm: Option<usize>,
    snoozed_alarm: Option<(usize, DateTime<Local>)>,
    alert_sound: AlertSound,
    alert_action_sender: Option<Sender<AlertAction>>,
    timers: Vec<Timer>,
    timer_label: String,
    timer_minutes: String,
    timer_seconds: String,
    timer_form_open: bool,
    editing_timer: Option<TimerId>,
    next_timer_id: u64,
    hovered_collection_tile: Option<CollectionTile>,
    stopwatch: Stopwatch,
}

impl ClockApp {
    fn toggle_context_page(&mut self, page: ContextPage) {
        clear_collection_tile_hover(&mut self.hovered_collection_tile);
        if self.core.window.show_context && self.context_page == page {
            self.core.window.show_context = false;
        } else {
            self.context_page = page;
            self.core.window.show_context = true;
        }
    }

    fn save(&self) {
        let Some(config) = &self.config else {
            return;
        };
        let state = PersistentState {
            world_clocks: self.world_clocks.clone(),
            alarms: self.alarms.clone(),
            app_theme: self.app_theme,
        };
        if let Err(error) = config.set("state", state) {
            tracing::warn!(%error, "failed to save clock settings");
        }
    }

    fn acknowledge_timer(&mut self, timer_id: TimerId) {
        if let Some(timer) = self.timers.iter_mut().find(|timer| timer.id() == timer_id) {
            timer.acknowledge();
        }
    }

    fn reset_timer(&mut self, timer_id: TimerId) {
        close_timer_notification(timer_id);
        if let Some(timer) = self.timers.iter_mut().find(|timer| timer.id() == timer_id) {
            timer.reset();
        }
    }

    fn snooze_ringing_alarm(&mut self) {
        if let Some(index) = self.ringing_alarm.take()
            && let Some(alarm) = self.alarms.get(index)
            && alarm.snooze_enabled
        {
            let minutes = i64::from(alarm.snooze_minutes.max(1));
            self.snoozed_alarm = Some((index, Local::now() + ChronoDuration::minutes(minutes)));
        }
    }

    fn dismiss_ringing_alarm(&mut self) {
        self.ringing_alarm = None;
        self.snoozed_alarm = None;
    }

    fn settings_drawer(&self) -> context_drawer::ContextDrawer<'_, Message> {
        let theme_options = vec![fl!("match-desktop"), fl!("dark"), fl!("light")];
        let appearance = widget::settings::section()
            .title(fl!("settings-appearance"))
            .add(
                widget::settings::item::builder(fl!("settings-theme")).control(widget::dropdown(
                    theme_options,
                    Some(self.app_theme.dropdown_index()),
                    Message::SetAppTheme,
                )),
            );
        let reset =
            widget::button::standard(fl!("settings-reset-all")).on_press(Message::ResetAllSettings);
        let content: Element<'_, Message> =
            widget::settings::view_column(vec![appearance.into(), reset.into()]).into();
        context_drawer::context_drawer(content, Message::ToggleSettings)
            .title(fl!("settings-title"))
    }

    fn city_picker_drawer(&self) -> context_drawer::ContextDrawer<'_, Message> {
        let filtered_cities = filtered_city_indices(&self.city_search);
        let mut cities = widget::list_column();
        for city_index in &filtered_cities {
            let (name, timezone) = &city_catalog()[*city_index];
            let already_added = self
                .world_clocks
                .iter()
                .any(|clock| clock.timezone.name() == *timezone);
            let action = widget::button::text(if already_added {
                fl!("city-added")
            } else {
                fl!("add")
            })
            .on_press_maybe((!already_added).then_some(Message::AddWorldClock(*city_index)));
            cities = cities.add(
                widget::settings::item::builder(name)
                    .description(*timezone)
                    .control(action),
            );
        }

        let content: Element<'_, Message> = if filtered_cities.is_empty() {
            widget::text(fl!("city-search-empty")).into()
        } else {
            cities.into()
        };
        let search = widget::text_input::search_input(fl!("search-cities"), &self.city_search)
            .on_input(Message::CitySearchChanged)
            .on_clear(Message::ClearCitySearch)
            .id(self.city_search_id.clone());

        context_drawer::context_drawer(content, Message::CloseCityPicker)
            .title(fl!("choose-city"))
            .header(search)
    }

    fn page(&self) -> Page {
        self.nav_model
            .active_data::<Page>()
            .copied()
            .unwrap_or(Page::WorldClocks)
    }

    fn world_clocks_view(&self) -> Element<'_, Message> {
        let now = self.now_utc;
        let local_now = reference_time(now, self.preview);
        let local_offset_seconds = local_now.offset().local_minus_utc();
        let local_date = local_now.date_naive();
        let mut clocks = column![].spacing(12);
        for (index, clock) in self.world_clocks.iter().enumerate() {
            let tile = CollectionTile::WorldClock(index);
            let mut details = vec![relative_offset_text(relative_offset_minutes(
                now,
                clock.timezone,
                local_offset_seconds,
            ))];
            if let Some(relative_day) =
                relative_day_text(relative_day_offset(now, clock.timezone, local_date))
            {
                details.push(relative_day);
            }
            details.push(format_utc_offset(clock.offset_seconds(now)));
            let card = row![
                column![
                    widget::text(&clock.name).size(20),
                    widget::text(details.join(", ")),
                ]
                .spacing(4)
                .width(Length::Fill),
                widget::text(world_clock_time_text(
                    clock,
                    now,
                    &self.locale,
                    self.military_time,
                ))
                .size(38),
            ]
            .spacing(16)
            .align_y(Alignment::Center);
            clocks = clocks.push(
                widget::mouse_area(
                    widget::container(
                        widget::button::custom(card)
                            .padding(0)
                            .width(Length::Fill)
                            .class(collection_tile_edit_button())
                            .on_press(Message::EditWorldClock(index)),
                    )
                    .padding(16)
                    .width(Length::Fill)
                    .class(collection_tile_container(
                        collection_tile_is_hovered(self.hovered_collection_tile, tile),
                    )),
                )
                .on_enter(Message::CollectionTileHover(tile, true))
                .on_exit(Message::CollectionTileHover(tile, false))
                .on_release(Message::EditWorldClock(index))
                .interaction(mouse::Interaction::Pointer),
            );
        }
        if self.world_clocks.is_empty() {
            clocks = clocks.push(widget::text(fl!("world-clock-empty")));
        }

        let header = row![
            widget::text::heading(fl!("world-clocks")).width(Length::Fill),
            widget::button::suggested(fl!("add-city")).on_press(Message::OpenCityPicker),
        ]
        .align_y(Alignment::Center);

        let local_time = widget::container(
            column![
                widget::text(local_time_text(
                    &local_now,
                    &self.locale,
                    self.military_time,
                ))
                .size(56),
                widget::text(local_date_text(&local_now, &self.locale)).size(20),
            ]
            .spacing(4)
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .center_x(Length::Fill)
        .padding([16, 0]);

        let mut page = column![header, local_time].spacing(20);
        if let Some(index) = self.editing_world_clock
            && let Some(clock) = self.world_clocks.get(index)
        {
            page = page.push(
                widget::container(
                    column![
                        widget::text::heading(fl!("world-clock-edit")),
                        widget::text(&clock.name).size(24),
                        widget::text(clock.timezone.name()),
                        row![
                            widget::container(
                                widget::button::standard(fl!("cancel"))
                                    .on_press(Message::CancelWorldClockEdit),
                            )
                            .width(Length::Fill)
                            .align_x(Alignment::End),
                            widget::button::destructive(fl!("remove"))
                                .on_press(Message::RemoveWorldClock(index)),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    ]
                    .spacing(16),
                )
                .padding(20)
                .width(Length::Fill)
                .class(theme::Container::Card),
            );
        }

        if show_collection_list(self.world_clocks.len(), self.editing_world_clock.is_some()) {
            page = page.push(clocks);
        }
        page_container(page)
    }

    fn alarm_repeat_menu(&self) -> Element<'_, Message> {
        let weekday_order = weekday_order_for_locale(&self.locale);
        let items = weekday_order
            .into_iter()
            .map(|day| {
                menu::Item::CheckBox(
                    alarm_day_label(day),
                    None,
                    self.alarm_draft.repeat_days().contains(&day),
                    AlarmRepeatAction::Toggle(day),
                )
            })
            .collect();
        let root = RcElementWrapper::new(Element::from(menu::root(repeat_summary(
            self.alarm_draft.repeat_days(),
            &weekday_order,
        ))));
        menu::bar(vec![menu::Tree::with_children(
            root,
            menu::items(&HashMap::new(), items),
        )])
        .item_height(ItemHeight::Dynamic(40))
        .item_width(ItemWidth::Uniform(220))
        .spacing(4.0)
        .into()
    }

    fn alarm_form_view(&self) -> Element<'_, Message> {
        let mut time = row![
            widget::text_input(fl!("alarm-hour"), &self.alarm_draft.hour)
                .on_input(Message::AlarmHourChanged)
                .id(self.alarm_hour_id.clone())
                .size(20)
                .width(90),
            widget::text(":").size(24),
            widget::text_input(fl!("alarm-minute"), &self.alarm_draft.minute)
                .on_input(Message::AlarmMinuteChanged)
                .id(self.alarm_minute_id.clone())
                .size(20)
                .width(90),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if !self.military_time {
            time = time.push(
                widget::dropdown(
                    alarm_period_options(&self.locale),
                    Some(self.alarm_draft.period.index()),
                    Message::AlarmPeriodChanged,
                )
                .width(90),
            );
        }

        let repeat = row![
            widget::text(fl!("alarm-repeat")).width(Length::Fill),
            self.alarm_repeat_menu(),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let snooze = row![
            widget::text(fl!("alarm-snooze")).width(Length::Fill),
            widget::toggler(self.alarm_draft.snooze_enabled).on_toggle(Message::ToggleAlarmSnooze),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let form_title = if self.editing_alarm.is_some() {
            fl!("alarm-edit")
        } else {
            fl!("alarm-new")
        };
        let mut form = column![
            widget::text::heading(form_title),
            time,
            widget::text_input(fl!("alarm-label"), &self.alarm_draft.label)
                .on_input(Message::AlarmLabelChanged),
            repeat,
            snooze,
        ]
        .spacing(16);
        if self.alarm_draft.snooze_enabled {
            form = form.push(
                row![
                    widget::text(fl!("alarm-snooze-duration")).width(Length::Fill),
                    widget::spin_button(
                        &self.alarm_draft.snooze_minutes,
                        fl!("alarm-snooze-duration"),
                        self.alarm_draft.snooze_minutes.parse::<u16>().unwrap_or(5),
                        1,
                        1,
                        60,
                        |minutes| Message::AlarmSnoozeMinutesChanged(minutes.to_string()),
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        let mut right_actions = row![].spacing(8).align_y(Alignment::Center);
        if let Some(index) = self.editing_alarm {
            right_actions = right_actions.push(
                widget::button::destructive(fl!("remove")).on_press(Message::RemoveAlarm(index)),
            );
        }
        right_actions =
            right_actions.push(widget::button::suggested(fl!("save")).on_press(Message::AddAlarm));
        let actions = row![
            widget::container(
                widget::button::standard(fl!("cancel")).on_press(Message::CancelAlarmForm),
            )
            .width(Length::Fill),
            right_actions,
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        form = form.push(actions);

        widget::container(form)
            .padding(20)
            .width(Length::Fill)
            .class(theme::Container::Card)
            .into()
    }

    fn alarms_view(&self) -> Element<'_, Message> {
        let add_button =
            widget::button::suggested(fl!("add-alarm")).on_press(Message::OpenAlarmForm);
        if self.alarms.is_empty() && !self.alarm_form_open && self.ringing_alarm.is_none() {
            return page_container(
                widget::container(add_button)
                    .width(Length::Fill)
                    .align_x(Alignment::End)
                    .padding(32),
            );
        }

        let mut list = column![].spacing(12);
        for (index, alarm) in self.alarms.iter().enumerate() {
            let tile = CollectionTile::Alarm(index);
            let is_snoozed = self
                .snoozed_alarm
                .is_some_and(|(snoozed_index, _)| snoozed_index == index);
            let details = column![
                widget::text(alarm_entry_summary(alarm, &self.locale, self.military_time,))
                    .size(36)
            ]
            .width(Length::Fill);
            let mut controls = row![
                widget::toggler(alarm.enabled)
                    .on_toggle(move |enabled| Message::ToggleAlarm(index, enabled))
            ]
            .spacing(16)
            .align_y(Alignment::Center);
            if is_snoozed {
                controls = controls.push(
                    widget::button::suggested(fl!("alarm-dismiss")).on_press(Message::DismissAlarm),
                );
            }
            let alarm_row = row![
                widget::button::custom(details)
                    .padding(0)
                    .width(Length::Fill)
                    .class(collection_tile_edit_button())
                    .on_press(Message::EditAlarm(index)),
                controls,
            ]
            .spacing(16)
            .align_y(Alignment::Center);
            list = list.push(
                widget::mouse_area(
                    widget::container(alarm_row)
                        .padding(16)
                        .width(Length::Fill)
                        .class(collection_tile_container(collection_tile_is_hovered(
                            self.hovered_collection_tile,
                            tile,
                        ))),
                )
                .on_enter(Message::CollectionTileHover(tile, true))
                .on_exit(Message::CollectionTileHover(tile, false))
                .on_release(Message::EditAlarm(index))
                .interaction(mouse::Interaction::Pointer),
            );
        }

        let mut page = column![].spacing(20);
        if self.alarm_form_open {
            page = page.push(self.alarm_form_view());
        } else {
            page = page.push(
                row![
                    widget::text::heading(fl!("alarms")).width(Length::Fill),
                    add_button,
                ]
                .align_y(Alignment::Center),
            );
        }

        if let Some(index) = self.ringing_alarm
            && let Some(alarm) = self.alarms.get(index)
        {
            let mut actions = row![].spacing(8);
            if alarm.snooze_enabled {
                actions = actions.push(
                    widget::button::standard(fl!(
                        "alarm-snooze-button",
                        minutes = alarm.snooze_minutes
                    ))
                    .on_press(Message::SnoozeAlarm),
                );
            }
            actions = actions.push(
                widget::button::suggested(fl!("alarm-dismiss")).on_press(Message::DismissAlarm),
            );
            page = page.push(
                widget::container(
                    column![
                        widget::text(fl!("alarm-ringing")).size(20),
                        widget::text(format!(
                            "{}: {}",
                            alarm_entry_summary(alarm, &self.locale, self.military_time),
                            alarm.label
                        )),
                        actions,
                    ]
                    .spacing(10),
                )
                .padding(16)
                .width(Length::Fill)
                .class(theme::Container::Card),
            );
        }
        if show_collection_list(self.alarms.len(), self.editing_alarm.is_some()) {
            page = page.push(list);
        }
        page_container(page)
    }

    fn timers_view(&self) -> Element<'_, Message> {
        let mut list = column![].spacing(12);
        for (index, timer) in self.timers.iter().enumerate() {
            let tile = CollectionTile::Timer(timer.id());
            let remaining = timer.remaining_at(self.now_instant);
            let status = if timer.is_ringing() {
                fl!("timer-ringing")
            } else {
                match timer.status() {
                    TimerStatus::Ready => fl!("ready"),
                    TimerStatus::Running => fl!("running"),
                    TimerStatus::Paused => fl!("paused"),
                    TimerStatus::Finished => fl!("finished"),
                }
            };
            let primary_action: Element<'_, Message> = if timer.is_ringing() {
                widget::button::suggested(fl!("timer-stop-ringing"))
                    .on_press(timer_primary_message(timer, index))
                    .into()
            } else {
                let toggle_label = if timer.status() == TimerStatus::Running {
                    fl!("pause")
                } else {
                    fl!("start")
                };
                widget::button::suggested(toggle_label)
                    .on_press(timer_primary_message(timer, index))
                    .into()
            };
            let mut controls = row![primary_action].spacing(8);
            if !timer.is_ringing() {
                controls = controls.push(
                    widget::button::standard(fl!("reset")).on_press(Message::ResetTimer(index)),
                );
            }
            let row = column![
                widget::button::custom(
                    row![
                        column![widget::text(&timer.label).size(20), widget::text(status)]
                            .spacing(4)
                            .width(Length::Fill),
                        widget::text(format_duration(remaining, false)).size(34),
                    ]
                    .align_y(Alignment::Center)
                )
                .padding(0)
                .width(Length::Fill)
                .class(collection_tile_edit_button())
                .on_press(Message::EditTimer(index)),
                widget::container(controls)
                    .width(Length::Fill)
                    .align_x(Alignment::End),
            ]
            .spacing(12);
            list = list.push(
                widget::mouse_area(
                    widget::container(row)
                        .padding(16)
                        .width(Length::Fill)
                        .class(collection_tile_container(collection_tile_is_hovered(
                            self.hovered_collection_tile,
                            tile,
                        ))),
                )
                .on_enter(Message::CollectionTileHover(tile, true))
                .on_exit(Message::CollectionTileHover(tile, false))
                .on_release(Message::EditTimer(index))
                .interaction(mouse::Interaction::Pointer),
            );
        }
        if self.timers.is_empty() {
            list = list.push(widget::text(fl!("timer-empty")));
        }

        let add_button =
            widget::button::suggested(fl!("add-timer")).on_press(Message::OpenTimerForm);
        if self.timers.is_empty() && !self.timer_form_open {
            return page_container(
                widget::container(add_button)
                    .width(Length::Fill)
                    .align_x(Alignment::End)
                    .padding(32),
            );
        }

        let mut content = column![].spacing(20);
        if let Some(mode) = timer_form_mode(self.timer_form_open, self.editing_timer.is_some()) {
            let title = match mode {
                TimerFormMode::New => fl!("timer-new"),
                TimerFormMode::Edit => fl!("timer-edit"),
            };
            let inputs = row![
                widget::text_input(fl!("timer-label"), &self.timer_label)
                    .on_input(Message::TimerLabelChanged)
                    .width(Length::Fill),
                widget::text_input(fl!("timer-minutes"), &self.timer_minutes)
                    .on_input(Message::TimerMinutesChanged)
                    .width(90),
                widget::text_input(fl!("timer-seconds"), &self.timer_seconds)
                    .on_input(Message::TimerSecondsChanged)
                    .width(90),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            let mut right_actions = row![].spacing(8).align_y(Alignment::Center);
            if let Some(timer_id) = self.editing_timer
                && let Some(index) = self.timers.iter().position(|timer| timer.id() == timer_id)
            {
                right_actions = right_actions.push(
                    widget::button::destructive(fl!("remove"))
                        .on_press(Message::RemoveTimer(index)),
                );
            }
            right_actions = right_actions
                .push(widget::button::suggested(fl!("save")).on_press(Message::AddTimer));
            let actions = row![
                widget::container(
                    widget::button::standard(fl!("cancel")).on_press(Message::CancelTimerForm),
                )
                .width(Length::Fill),
                right_actions,
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            let form = column![widget::text::heading(title), inputs, actions,].spacing(16);
            content = content.push(
                widget::container(form)
                    .padding(20)
                    .width(Length::Fill)
                    .class(theme::Container::Card),
            );
        } else {
            content = content.push(
                row![
                    widget::text::heading(fl!("timers")).width(Length::Fill),
                    add_button
                ]
                .align_y(Alignment::Center),
            );
        }
        if show_collection_list(self.timers.len(), self.editing_timer.is_some()) {
            content = content.push(list);
        }
        page_container(content)
    }

    fn stopwatch_view(&self) -> Element<'_, Message> {
        let elapsed = self.stopwatch.elapsed_at(self.now_instant);
        let toggle_label = if self.stopwatch.is_running() {
            fl!("pause")
        } else {
            fl!("start")
        };
        let mut laps = column![widget::text::heading(fl!("stopwatch-laps"))].spacing(8);
        if self.stopwatch.laps().is_empty() {
            laps = laps.push(widget::text(fl!("stopwatch-empty")));
        } else {
            for (index, lap) in self.stopwatch.laps().iter().enumerate().rev() {
                laps = laps.push(
                    row![
                        widget::text(format!("Lap {}", index + 1)).width(Length::Fill),
                        widget::text(format_duration(*lap, true)),
                    ]
                    .spacing(12),
                );
            }
        }

        let controls = row![
            widget::button::suggested(toggle_label).on_press(Message::ToggleStopwatch),
            widget::button::standard(fl!("lap")).on_press(Message::AddLap),
            widget::button::standard(fl!("reset")).on_press(Message::ResetStopwatch),
        ]
        .spacing(10);
        page_container(
            column![
                widget::text::heading(fl!("stopwatch")),
                widget::container(widget::text(format_duration(elapsed, true)).size(56))
                    .width(Length::Fill)
                    .center_x(Length::Fill)
                    .padding(24)
                    .class(theme::Container::Card),
                controls,
                laps,
            ]
            .spacing(20),
        )
    }
}

impl Application for ClockApp {
    type Executor = executor::Default;
    type Flags = Flags;
    type Message = Message;

    const APP_ID: &'static str = "org.cosmic_utils.clock";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(mut core: Core, flags: Self::Flags) -> (Self, cosmic::app::Task<Self::Message>) {
        core.window.header_title.clear();
        core.window.show_headerbar = true;
        core.window.content_container = true;
        core.window.use_template = true;

        let config = (!flags.preview)
            .then(|| Config::new(Self::APP_ID, 1).ok())
            .flatten();
        let mut state = config
            .as_ref()
            .and_then(|config| config.get::<PersistentState>("state").ok())
            .unwrap_or_default();
        if let Some(theme) = flags.initial_theme {
            state.app_theme = theme;
        }
        let preview_data = flags.preview.then(|| preview_data(Instant::now()));
        if let Some(preview) = &preview_data {
            state.world_clocks.clone_from(&preview.world_clocks);
            state.alarms.clone_from(&preview.alarms);
        }

        let mut nav_model = nav_bar::Model::default();
        nav_model
            .insert()
            .icon(symbolic("preferences-time-and-language-symbolic"))
            .text(fl!("world-clocks"))
            .data(Page::WorldClocks);
        nav_model
            .insert()
            .icon(symbolic("alarm-symbolic"))
            .text(fl!("alarms"))
            .data(Page::Alarms);
        nav_model
            .insert()
            .icon(symbolic("accessories-clock-symbolic"))
            .text(fl!("timers"))
            .data(Page::Timers);
        nav_model
            .insert()
            .icon(symbolic("appointment-soon-symbolic"))
            .text(fl!("stopwatch"))
            .data(Page::Stopwatch);
        nav_model.activate_position(flags.initial_page.min(3));

        let now_local = Local::now();
        let now_utc = if flags.preview {
            preview_timestamp()
        } else {
            Utc::now()
        };
        let now_instant = preview_data
            .as_ref()
            .map_or_else(Instant::now, |preview| preview.now_instant);
        let time_config = if flags.preview {
            TimeConfig::default()
        } else {
            match Config::new(TIME_CONFIG_ID, TimeConfig::VERSION) {
                Ok(config) => TimeConfig::get_entry(&config).unwrap_or_else(|(errors, config)| {
                    tracing::warn!(?errors, "failed to load system time settings");
                    config
                }),
                Err(error) => {
                    tracing::warn!(%error, "failed to open system time settings");
                    TimeConfig::default()
                }
            }
        };
        let theme_task = if flags.initial_theme.is_some() {
            cosmic::command::set_theme(state.app_theme.theme())
        } else {
            cosmic::app::Task::none()
        };
        (
            Self {
                core,
                nav_model,
                about: about_widget(),
                menu_key_binds: HashMap::new(),
                context_page: ContextPage::About,
                config,
                app_theme: state.app_theme,
                preview: flags.preview,
                now_utc,
                previous_local: now_local,
                now_instant,
                world_clocks: state.world_clocks,
                editing_world_clock: None,
                city_search: String::new(),
                city_search_id: widget::Id::unique(),
                alarms: state.alarms,
                alarm_form_open: false,
                editing_alarm: None,
                alarm_draft: AlarmDraft::default(),
                alarm_hour_id: widget::Id::unique(),
                alarm_minute_id: widget::Id::unique(),
                locale: flags.locale,
                military_time: time_config.military_time,
                ringing_alarm: None,
                snoozed_alarm: None,
                alert_sound: AlertSound::default(),
                alert_action_sender: None,
                timers: preview_data
                    .as_ref()
                    .map_or_else(Vec::new, |preview| preview.timers.clone()),
                timer_label: String::new(),
                timer_minutes: String::new(),
                timer_seconds: String::new(),
                timer_form_open: false,
                editing_timer: None,
                next_timer_id: if flags.preview { 4 } else { 1 },
                hovered_collection_tile: None,
                stopwatch: preview_data
                    .map_or_else(Stopwatch::default, |preview| preview.stopwatch),
            },
            theme_task,
        )
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav_model)
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> cosmic::app::Task<Self::Message> {
        clear_collection_tile_hover(&mut self.hovered_collection_tile);
        if self.context_page == ContextPage::CityPicker {
            self.core.window.show_context = false;
            self.city_search.clear();
        }
        self.nav_model.activate(id);
        cosmic::app::Task::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        if self.preview {
            return Subscription::none();
        }
        let timer_running = self
            .timers
            .iter()
            .any(|timer| timer.status() == TimerStatus::Running);
        let interval = active_tick_interval(self.stopwatch.is_running(), timer_running);
        let mut subscriptions = vec![cosmic::iced::time::every(interval).map(|_| Message::Tick)];
        subscriptions.push(alert_action_subscription().map(Message::AlertSubscription));
        subscriptions.push(
            self.core()
                .watch_config::<TimeConfig>(TIME_CONFIG_ID)
                .map(|update| {
                    if !update.errors.is_empty() {
                        tracing::warn!(?update.errors, "failed to reload system time settings");
                    }
                    Message::TimeFormatChanged(update.config.military_time)
                }),
        );
        if is_cosmic_desktop() {
            subscriptions.push(Subscription::batch([
                self.core()
                    .watch_config::<cosmic::cosmic_theme::Theme>(
                        cosmic::cosmic_theme::DARK_THEME_ID,
                    )
                    .map(|_| Message::CosmicThemeChanged),
                self.core()
                    .watch_config::<cosmic::cosmic_theme::Theme>(
                        cosmic::cosmic_theme::LIGHT_THEME_ID,
                    )
                    .map(|_| Message::CosmicThemeChanged),
                self.core()
                    .watch_config::<cosmic::cosmic_theme::ThemeMode>(
                        cosmic::cosmic_theme::THEME_MODE_ID,
                    )
                    .map(|_| Message::CosmicThemeChanged),
            ]));
        }
        Subscription::batch(subscriptions)
    }

    fn update(&mut self, message: Self::Message) -> cosmic::app::Task<Self::Message> {
        match message {
            Message::Tick => {
                let now_local = Local::now();
                let mut alert_events = Vec::new();
                if self.ringing_alarm.is_none()
                    && let Some((index, due)) = self.snoozed_alarm
                    && now_local >= due
                {
                    self.snoozed_alarm = None;
                    if let Some(alarm) = self.alarms.get(index) {
                        self.ringing_alarm = Some(index);
                        let ringing = alarm.begin_ringing();
                        alert_events.push(AlertEvent::alarm(
                            ringing.label,
                            alarm_entry_summary(alarm, &self.locale, self.military_time),
                            ringing.snooze_minutes,
                        ));
                    }
                }
                if self.ringing_alarm.is_none() {
                    let due_alarm = self
                        .alarms
                        .iter()
                        .position(|alarm| alarm_should_ring(alarm, self.previous_local, now_local));
                    if let Some(index) = due_alarm {
                        self.ringing_alarm = Some(index);
                        let alarm = &self.alarms[index];
                        let ringing = alarm.begin_ringing();
                        alert_events.push(AlertEvent::alarm(
                            ringing.label,
                            alarm_entry_summary(alarm, &self.locale, self.military_time),
                            ringing.snooze_minutes,
                        ));
                    }
                }
                self.previous_local = now_local;
                self.now_utc = Utc::now();
                self.now_instant = Instant::now();
                for timer in &mut self.timers {
                    if timer.update(self.now_instant) {
                        tracing::info!(label = %timer.label, "timer finished");
                        alert_events.push(AlertEvent::timer(timer.id(), &timer.label));
                    }
                }
                if !alert_events.is_empty() {
                    for event in alert_events {
                        let summary = match event {
                            AlertEvent::Alarm { .. } => fl!("notification-alarm-title"),
                            AlertEvent::Timer { .. } => fl!("notification-timer-title"),
                        };
                        let (stop_label, snooze_label, dismiss_label) = match &event {
                            AlertEvent::Alarm { snooze_minutes, .. } => (
                                None,
                                snooze_minutes
                                    .map(|minutes| fl!("alarm-snooze-button", minutes = minutes)),
                                Some(fl!("alarm-dismiss")),
                            ),
                            AlertEvent::Timer { .. } => {
                                (Some(fl!("timer-stop-ringing")), None, None)
                            }
                        };
                        send_notification(
                            event,
                            summary,
                            stop_label,
                            snooze_label,
                            dismiss_label,
                            self.alert_action_sender.clone(),
                        );
                    }
                }
            }
            Message::Surface(action) => {
                return cosmic::task::message(cosmic::Action::Surface(action));
            }
            Message::OpenUrl(url) => {
                if let Err(error) = open::that_detached(&url) {
                    tracing::warn!(%error, %url, "failed to open URL");
                }
            }
            Message::ToggleAbout => self.toggle_context_page(ContextPage::About),
            Message::ToggleSettings => self.toggle_context_page(ContextPage::Settings),
            Message::SetAppTheme(index) => {
                let Some(app_theme) = AppTheme::from_dropdown_index(index) else {
                    return cosmic::app::Task::none();
                };
                self.app_theme = app_theme;
                self.save();
                return cosmic::command::set_theme(app_theme.theme());
            }
            Message::CosmicThemeChanged => {
                return cosmic::command::set_theme(self.app_theme.theme());
            }
            Message::TimeFormatChanged(military_time) => {
                if self.alarm_form_open {
                    self.alarm_draft
                        .reformat_hour(self.military_time, military_time);
                }
                self.military_time = military_time;
            }
            Message::ResetAllSettings => {
                self.hovered_collection_tile = None;
                let defaults = PersistentState::default();
                self.world_clocks = defaults.world_clocks;
                self.editing_world_clock = None;
                self.alarms = defaults.alarms;
                self.alarm_form_open = false;
                self.editing_alarm = None;
                self.alarm_draft = AlarmDraft::default();
                self.app_theme = defaults.app_theme;
                self.save();
                return cosmic::command::set_theme(self.app_theme.theme());
            }
            Message::OpenCityPicker => {
                clear_collection_tile_hover(&mut self.hovered_collection_tile);
                self.editing_world_clock = None;
                self.context_page = ContextPage::CityPicker;
                self.core.window.show_context = true;
                self.city_search.clear();
                return widget::text_input::focus(self.city_search_id.clone());
            }
            Message::CloseCityPicker => {
                clear_collection_tile_hover(&mut self.hovered_collection_tile);
                if self.context_page == ContextPage::CityPicker {
                    self.core.window.show_context = false;
                }
                self.city_search.clear();
            }
            Message::CitySearchChanged(value) => {
                clear_collection_tile_hover(&mut self.hovered_collection_tile);
                self.city_search = value;
            }
            Message::ClearCitySearch => {
                clear_collection_tile_hover(&mut self.hovered_collection_tile);
                self.city_search.clear();
            }
            Message::AddWorldClock(city_index) => {
                if let Some((name, timezone)) = city_catalog().get(city_index)
                    && !self
                        .world_clocks
                        .iter()
                        .any(|clock| clock.timezone.name() == *timezone)
                    && let Some(clock) = WorldClock::new(name.clone(), timezone)
                {
                    clear_collection_tile_hover(&mut self.hovered_collection_tile);
                    self.world_clocks.push(clock);
                    self.save();
                }
            }
            Message::EditWorldClock(index) => {
                if index < self.world_clocks.len() {
                    clear_collection_tile_hover(&mut self.hovered_collection_tile);
                    self.editing_world_clock = Some(index);
                    if self.context_page == ContextPage::CityPicker {
                        self.core.window.show_context = false;
                        self.city_search.clear();
                    }
                }
            }
            Message::CancelWorldClockEdit => self.editing_world_clock = None,
            Message::RemoveWorldClock(index) => {
                if index < self.world_clocks.len() {
                    clear_collection_tile_hover(&mut self.hovered_collection_tile);
                    self.world_clocks.remove(index);
                    self.editing_world_clock = None;
                    self.save();
                }
            }
            Message::OpenAlarmForm => {
                clear_collection_tile_hover(&mut self.hovered_collection_tile);
                self.editing_alarm = None;
                self.alarm_draft = AlarmDraft::default();
                self.alarm_form_open = true;
                return widget::text_input::focus(self.alarm_hour_id.clone());
            }
            Message::EditAlarm(index) => {
                if let Some(alarm) = self.alarms.get(index) {
                    clear_collection_tile_hover(&mut self.hovered_collection_tile);
                    self.editing_alarm = Some(index);
                    self.alarm_draft =
                        AlarmDraft::from_alarm_with_format(alarm, self.military_time);
                    self.alarm_form_open = true;
                    return widget::text_input::focus(self.alarm_hour_id.clone());
                }
            }
            Message::CancelAlarmForm => {
                clear_collection_tile_hover(&mut self.hovered_collection_tile);
                self.alarm_form_open = false;
                self.editing_alarm = None;
                self.alarm_draft = AlarmDraft::default();
            }
            Message::AlarmLabelChanged(value) => self.alarm_draft.label = value,
            Message::AlarmHourChanged(value) => {
                self.alarm_draft.hour = digits_only(value, 2);
                if self.alarm_draft.hour.len() == 2 {
                    return widget::text_input::focus(self.alarm_minute_id.clone());
                }
            }
            Message::AlarmMinuteChanged(value) => {
                self.alarm_draft.minute = digits_only(value, 2);
            }
            Message::AlarmPeriodChanged(index) => {
                if let Some(period) = AlarmPeriod::from_index(index) {
                    self.alarm_draft.period = period;
                }
            }
            Message::ToggleAlarmRepeat(day) => {
                let selected = !self.alarm_draft.repeat_days().contains(&day);
                self.alarm_draft.set_repeat_day(day, selected);
            }
            Message::ToggleAlarmSnooze(enabled) => {
                self.alarm_draft.snooze_enabled = enabled;
            }
            Message::AlarmSnoozeMinutesChanged(value) => {
                self.alarm_draft.snooze_minutes = digits_only(value, 2);
            }
            Message::AddAlarm => {
                let saved = if let Some(index) = self.editing_alarm {
                    self.alarms.get_mut(index).is_some_and(|alarm| {
                        alarm.update_from_draft_with_format(&self.alarm_draft, self.military_time)
                    })
                } else if let Some(alarm) = self.alarm_draft.build_with_format(self.military_time) {
                    self.alarms.push(alarm);
                    true
                } else {
                    false
                };
                if saved {
                    clear_collection_tile_hover(&mut self.hovered_collection_tile);
                    self.alarm_form_open = false;
                    self.editing_alarm = None;
                    self.alarm_draft = AlarmDraft::default();
                    self.save();
                }
            }
            Message::ToggleAlarm(index, enabled) => {
                if let Some(alarm) = self.alarms.get_mut(index) {
                    alarm.enabled = enabled;
                    if !enabled && self.ringing_alarm == Some(index) {
                        self.ringing_alarm = None;
                    }
                    self.save();
                }
            }
            Message::RemoveAlarm(index) => {
                if index < self.alarms.len() {
                    clear_collection_tile_hover(&mut self.hovered_collection_tile);
                    self.alarms.remove(index);
                    match self.editing_alarm {
                        Some(editing) if editing == index => {
                            self.alarm_form_open = false;
                            self.editing_alarm = None;
                            self.alarm_draft = AlarmDraft::default();
                        }
                        Some(editing) if editing > index => {
                            self.editing_alarm = Some(editing - 1);
                        }
                        _ => {}
                    }
                    self.ringing_alarm = None;
                    self.snoozed_alarm = None;
                    self.save();
                }
            }
            Message::SnoozeAlarm => {
                self.snooze_ringing_alarm();
            }
            Message::DismissAlarm => self.dismiss_ringing_alarm(),
            Message::AlertSubscription(event) => match event {
                AlertSubscriptionEvent::Ready(sender) => {
                    self.alert_action_sender = Some(sender);
                }
                AlertSubscriptionEvent::Action(AlertAction::SnoozeAlarm) => {
                    self.snooze_ringing_alarm();
                }
                AlertSubscriptionEvent::Action(AlertAction::DismissAlarm) => {
                    self.dismiss_ringing_alarm();
                }
                AlertSubscriptionEvent::Action(AlertAction::StopTimer(timer_id)) => {
                    self.acknowledge_timer(timer_id);
                }
                AlertSubscriptionEvent::Action(AlertAction::ResetTimer(timer_id)) => {
                    self.reset_timer(timer_id);
                }
            },
            Message::TimerLabelChanged(value) => self.timer_label = value,
            Message::TimerMinutesChanged(value) => self.timer_minutes = digits_only(value, 3),
            Message::TimerSecondsChanged(value) => self.timer_seconds = digits_only(value, 2),
            Message::OpenTimerForm => {
                clear_collection_tile_hover(&mut self.hovered_collection_tile);
                self.timer_form_open = true;
                self.editing_timer = None;
                self.timer_label.clear();
                self.timer_minutes.clear();
                self.timer_seconds.clear();
            }
            Message::AddTimer => {
                let minutes = self.timer_minutes.parse::<u64>().unwrap_or(0);
                let seconds = self.timer_seconds.parse::<u64>().unwrap_or(0).min(59);
                let duration = Duration::from_secs(minutes.saturating_mul(60) + seconds);
                let label = normalized_timer_label(&self.timer_label);
                let saved = if let Some(timer_id) = self.editing_timer {
                    let saved = self
                        .timers
                        .iter_mut()
                        .find(|timer| timer.id() == timer_id)
                        .is_some_and(|timer| timer.reconfigure(label, duration));
                    if saved {
                        close_timer_notification(timer_id);
                    }
                    saved
                } else {
                    let timer_id = TimerId::new(self.next_timer_id);
                    if let Some(timer) = Timer::new(timer_id, label, duration) {
                        self.timers.push(timer);
                        self.next_timer_id = self.next_timer_id.saturating_add(1);
                        true
                    } else {
                        false
                    }
                };
                if saved {
                    clear_collection_tile_hover(&mut self.hovered_collection_tile);
                    self.timer_form_open = false;
                    self.editing_timer = None;
                    self.timer_label.clear();
                    self.timer_minutes.clear();
                    self.timer_seconds.clear();
                }
            }
            Message::EditTimer(index) => {
                if let Some(timer) = self.timers.get(index) {
                    clear_collection_tile_hover(&mut self.hovered_collection_tile);
                    let total_seconds = timer.duration().as_secs();
                    self.timer_form_open = true;
                    self.editing_timer = Some(timer.id());
                    self.timer_label.clone_from(&timer.label);
                    self.timer_minutes = (total_seconds / 60).to_string();
                    self.timer_seconds = (total_seconds % 60).to_string();
                }
            }
            Message::CancelTimerForm => {
                clear_collection_tile_hover(&mut self.hovered_collection_tile);
                self.timer_form_open = false;
                self.editing_timer = None;
                self.timer_label.clear();
                self.timer_minutes.clear();
                self.timer_seconds.clear();
            }
            Message::ToggleTimer(index) => {
                if let Some(timer) = self.timers.get_mut(index) {
                    if timer.status() == TimerStatus::Running {
                        timer.pause(self.now_instant);
                    } else if timer.status() == TimerStatus::Finished {
                        timer.reset();
                        timer.start(self.now_instant);
                    } else {
                        timer.start(self.now_instant);
                    }
                }
            }
            Message::ResetTimer(index) => {
                if let Some(timer) = self.timers.get_mut(index) {
                    close_timer_notification(timer.id());
                    timer.reset();
                }
            }
            Message::RemoveTimer(index) => {
                if index < self.timers.len() {
                    clear_collection_tile_hover(&mut self.hovered_collection_tile);
                    let timer_id = self.timers[index].id();
                    close_timer_notification(timer_id);
                    self.timers.remove(index);
                    if self.editing_timer == Some(timer_id) {
                        self.timer_form_open = false;
                        self.editing_timer = None;
                        self.timer_label.clear();
                        self.timer_minutes.clear();
                        self.timer_seconds.clear();
                    }
                }
            }
            Message::CollectionTileHover(tile, hovered) => {
                self.hovered_collection_tile =
                    update_collection_tile_hover(self.hovered_collection_tile, tile, hovered);
            }
            Message::ToggleStopwatch => {
                if self.stopwatch.is_running() {
                    self.stopwatch.pause(self.now_instant);
                } else {
                    self.stopwatch.start(self.now_instant);
                }
            }
            Message::ResetStopwatch => self.stopwatch.reset(),
            Message::AddLap => self.stopwatch.lap(self.now_instant),
        }
        let alert_active =
            self.ringing_alarm.is_some() || self.timers.iter().any(Timer::is_ringing);
        if alert_active {
            if let Err(error) = self.alert_sound.start() {
                tracing::warn!(%error, "failed to play alert sound");
            }
        } else {
            self.alert_sound.stop();
        }
        cosmic::app::Task::none()
    }

    fn context_drawer(&self) -> Option<context_drawer::ContextDrawer<'_, Message>> {
        if !self.core.window.show_context {
            return None;
        }
        Some(match self.context_page {
            ContextPage::About => context_drawer::about(
                &self.about,
                |url| Message::OpenUrl(url.to_owned()),
                Message::ToggleAbout,
            ),
            ContextPage::Settings => self.settings_drawer(),
            ContextPage::CityPicker => self.city_picker_drawer(),
        })
    }

    fn header_start(&self) -> Vec<Element<'_, Message>> {
        vec![
            menu::bar(vec![menu::Tree::with_children(
                RcElementWrapper::new(Element::from(
                    button::icon(icon::from_name("open-menu-symbolic"))
                        .padding([4, 12])
                        .class(theme::Button::MenuRoot),
                )),
                menu::items(
                    &self.menu_key_binds,
                    vec![
                        menu::Item::Button(fl!("menu-settings"), None, MenuItemAction::Settings),
                        menu::Item::Divider,
                        menu::Item::Button(fl!("menu-about"), None, MenuItemAction::About),
                    ],
                ),
            )])
            .item_height(ItemHeight::Dynamic(40))
            .item_width(ItemWidth::Uniform(320))
            .spacing(4.0)
            .into(),
        ]
    }

    fn view(&self) -> Element<'_, Self::Message> {
        match self.page() {
            Page::WorldClocks => self.world_clocks_view(),
            Page::Alarms => self.alarms_view(),
            Page::Timers => self.timers_view(),
            Page::Stopwatch => self.stopwatch_view(),
        }
    }
}

fn page_container<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    widget::scrollable(
        widget::container(content)
            .padding(24)
            .width(Length::Fill)
            .max_width(960),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn alarm_day_label(day: AlarmDay) -> String {
    match day {
        AlarmDay::Monday => fl!("weekday-monday"),
        AlarmDay::Tuesday => fl!("weekday-tuesday"),
        AlarmDay::Wednesday => fl!("weekday-wednesday"),
        AlarmDay::Thursday => fl!("weekday-thursday"),
        AlarmDay::Friday => fl!("weekday-friday"),
        AlarmDay::Saturday => fl!("weekday-saturday"),
        AlarmDay::Sunday => fl!("weekday-sunday"),
    }
}

fn alarm_day_short_label(day: AlarmDay) -> String {
    match day {
        AlarmDay::Monday => fl!("weekday-short-monday"),
        AlarmDay::Tuesday => fl!("weekday-short-tuesday"),
        AlarmDay::Wednesday => fl!("weekday-short-wednesday"),
        AlarmDay::Thursday => fl!("weekday-short-thursday"),
        AlarmDay::Friday => fl!("weekday-short-friday"),
        AlarmDay::Saturday => fl!("weekday-short-saturday"),
        AlarmDay::Sunday => fl!("weekday-short-sunday"),
    }
}

fn repeat_summary(days: &BTreeSet<AlarmDay>, order: &[AlarmDay; 7]) -> String {
    if days.is_empty() {
        return fl!("alarm-repeat-never");
    }
    if days.len() == 7 {
        return fl!("alarm-repeat-every-day");
    }
    order
        .iter()
        .filter(|day| days.contains(day))
        .map(|day| alarm_day_short_label(*day))
        .collect::<Vec<_>>()
        .join(", ")
}

fn digits_only(value: String, maximum_length: usize) -> String {
    value
        .chars()
        .filter(char::is_ascii_digit)
        .take(maximum_length)
        .collect()
}

fn active_tick_interval(stopwatch_running: bool, timer_running: bool) -> Duration {
    if stopwatch_running {
        Duration::from_millis(10)
    } else if timer_running {
        Duration::from_millis(100)
    } else {
        Duration::from_secs(1)
    }
}

fn timer_primary_message(timer: &Timer, index: usize) -> Message {
    if timer.is_ringing() {
        Message::ResetTimer(index)
    } else {
        Message::ToggleTimer(index)
    }
}

fn normalized_timer_label(label: &str) -> String {
    let label = label.trim();
    if label.is_empty() {
        fl!("timer")
    } else {
        label.to_owned()
    }
}

fn filtered_city_indices(query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    city_catalog()
        .iter()
        .enumerate()
        .filter_map(|(index, (name, timezone))| {
            (query.is_empty()
                || name.to_lowercase().contains(&query)
                || timezone.to_lowercase().contains(&query))
            .then_some(index)
        })
        .collect()
}

fn reference_time(now: DateTime<Utc>, preview: bool) -> DateTime<FixedOffset> {
    if preview {
        now.with_timezone(&chrono_tz::Asia::Kolkata).fixed_offset()
    } else {
        now.with_timezone(&Local).fixed_offset()
    }
}

fn chrono_locale(locale: &str) -> ChronoLocale {
    let normalized = locale
        .split(['.', '@'])
        .next()
        .unwrap_or(locale)
        .replace('-', "_");
    normalized.parse().unwrap_or(ChronoLocale::POSIX)
}

fn local_time_text(now: &DateTime<FixedOffset>, locale: &str, military_time: bool) -> String {
    format_clock_time(now, locale, military_time, true)
}

fn world_clock_time_text(
    clock: &WorldClock,
    now: DateTime<Utc>,
    locale: &str,
    military_time: bool,
) -> String {
    format_clock_time(
        &now.with_timezone(&clock.timezone).fixed_offset(),
        locale,
        military_time,
        false,
    )
}

fn format_clock_time(
    now: &DateTime<FixedOffset>,
    locale: &str,
    military_time: bool,
    show_seconds: bool,
) -> String {
    let locale = chrono_locale(locale);
    if military_time {
        let format = if show_seconds { "%H:%M:%S" } else { "%H:%M" };
        return now.format_localized(format, locale).to_string();
    }

    let format = if show_seconds { "%I:%M:%S" } else { "%I:%M" };
    let period = alarm_period_text(
        if now.hour() < 12 {
            AlarmPeriod::Am
        } else {
            AlarmPeriod::Pm
        },
        locale,
    );
    format!("{} {period}", now.format_localized(format, locale))
}

fn alarm_period_options(locale: &str) -> Vec<String> {
    let locale = chrono_locale(locale);
    vec![
        alarm_period_text(AlarmPeriod::Am, locale),
        alarm_period_text(AlarmPeriod::Pm, locale),
    ]
}

fn alarm_period_text(period: AlarmPeriod, locale: ChronoLocale) -> String {
    let hour = match period {
        AlarmPeriod::Am => 1,
        AlarmPeriod::Pm => 13,
    };
    let time = DateTime::<Utc>::from_timestamp(i64::from(hour) * 3_600, 0)
        .expect("day-period hour must be valid")
        .fixed_offset();
    let localized = time.format_localized("%p", locale).to_string();
    match localized.trim() {
        "" if period == AlarmPeriod::Am => "AM".to_owned(),
        "" => "PM".to_owned(),
        period => period.to_owned(),
    }
}

fn local_date_text(now: &DateTime<FixedOffset>, locale: &str) -> String {
    now.format_localized("%x, %A", chrono_locale(locale))
        .to_string()
}

fn relative_offset_text(minutes: i32) -> String {
    if minutes == 0 {
        return fl!("world-clock-same-time");
    }

    let sign = if minutes < 0 { "-" } else { "+" };
    let total_minutes = minutes.unsigned_abs();
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    if minutes == 0 {
        fl!("world-clock-relative-hours", sign = sign, hours = hours)
    } else {
        fl!(
            "world-clock-relative-hours-minutes",
            sign = sign,
            hours = hours,
            minutes = minutes
        )
    }
}

fn relative_day_text(day_offset: i64) -> Option<String> {
    match day_offset {
        -1 => Some(fl!("yesterday")),
        0 => None,
        1 => Some(fl!("tomorrow")),
        days if days < 0 => Some(fl!("world-clock-days-earlier", days = days.unsigned_abs())),
        days => Some(fl!("world-clock-days-later", days = days.unsigned_abs())),
    }
}

#[must_use]
pub fn format_duration(duration: Duration, show_millis: bool) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds / 60) % 60;
    let seconds = total_seconds % 60;
    if show_millis {
        format!(
            "{hours:02}:{minutes:02}:{seconds:02}.{:02}",
            duration.subsec_millis() / 10
        )
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    }
}

fn alarm_entry_summary(alarm: &Alarm, locale: &str, military_time: bool) -> String {
    if military_time {
        return alarm.time_text();
    }

    let period = if alarm.hour < 12 {
        AlarmPeriod::Am
    } else {
        AlarmPeriod::Pm
    };
    let hour = match alarm.hour % 12 {
        0 => 12,
        hour => hour,
    };
    format!(
        "{hour}:{:02} {}",
        alarm.minute,
        alarm_period_text(period, chrono_locale(locale))
    )
}

fn collection_tile_container(hovered: bool) -> theme::Container<'static> {
    theme::Container::custom(move |theme| {
        let cosmic = theme.cosmic();
        let component = &theme.current_container().component;
        cosmic::iced::widget::container::Style {
            icon_color: Some(component.on.into()),
            text_color: Some(component.on.into()),
            background: Some(Background::Color(
                if hovered {
                    component.hover
                } else {
                    component.base
                }
                .into(),
            )),
            border: Border {
                radius: cosmic.corner_radii.radius_s.into(),
                ..Default::default()
            },
            shadow: Shadow::default(),
            snap: true,
        }
    })
}

fn collection_tile_edit_button() -> theme::Button {
    theme::Button::Custom {
        active: Box::new(|focused, theme| collection_tile_edit_button_style(theme, focused, false)),
        disabled: Box::new(|theme| collection_tile_edit_button_style(theme, false, true)),
        hovered: Box::new(|focused, theme| {
            collection_tile_edit_button_style(theme, focused, false)
        }),
        pressed: Box::new(|focused, theme| {
            collection_tile_edit_button_style(theme, focused, false)
        }),
    }
}

fn collection_tile_edit_button_style(
    theme: &Theme,
    focused: bool,
    disabled: bool,
) -> cosmic::widget::button::Style {
    use cosmic::widget::button::Catalog;

    let mut style = if disabled {
        theme.disabled(&theme::Button::Text)
    } else {
        theme.active(focused, false, &theme::Button::Text)
    };
    style.background = None;
    style.overlay = None;
    style
}

fn collection_tile_is_hovered(hovered_tile: Option<CollectionTile>, tile: CollectionTile) -> bool {
    matches!(hovered_tile, Some(hovered) if hovered == tile)
}

fn update_collection_tile_hover(
    hovered_tile: Option<CollectionTile>,
    tile: CollectionTile,
    hovered: bool,
) -> Option<CollectionTile> {
    if hovered {
        Some(tile)
    } else if collection_tile_is_hovered(hovered_tile, tile) {
        None
    } else {
        hovered_tile
    }
}

fn clear_collection_tile_hover(hovered_tile: &mut Option<CollectionTile>) {
    *hovered_tile = None;
}

const fn show_collection_list(item_count: usize, editing: bool) -> bool {
    item_count > 0 && !editing
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TimerFormMode {
    New,
    Edit,
}

const fn timer_form_mode(form_open: bool, editing: bool) -> Option<TimerFormMode> {
    if !form_open {
        None
    } else if editing {
        Some(TimerFormMode::Edit)
    } else {
        Some(TimerFormMode::New)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_format_supports_long_timers_and_stopwatch_precision() {
        let duration = Duration::from_millis(3_723_450);
        assert_eq!(format_duration(duration, false), "01:02:03");
        assert_eq!(format_duration(duration, true), "01:02:03.45");
    }

    #[test]
    fn alarm_entry_summary_uses_the_system_hour_cycle() {
        let morning = Alarm::new("Morning", 7, 30).expect("valid alarm");
        let evening = Alarm::new("Evening", 19, 30).expect("valid alarm");

        assert_eq!(alarm_entry_summary(&morning, "en-US", true), "07:30");
        assert_eq!(alarm_entry_summary(&morning, "en-US", false), "7:30 AM");
        assert_eq!(alarm_entry_summary(&evening, "en-US", false), "7:30 PM");
        assert_eq!(alarm_entry_summary(&evening, "fr-BE", false), "7:30 PM");
    }

    #[test]
    fn collection_lists_are_hidden_while_editing() {
        assert!(show_collection_list(2, false));
        assert!(!show_collection_list(2, true));
        assert!(!show_collection_list(0, false));
    }

    #[test]
    fn timer_form_has_closed_new_and_edit_modes() {
        assert_eq!(timer_form_mode(false, false), None);
        assert_eq!(timer_form_mode(true, false), Some(TimerFormMode::New));
        assert_eq!(timer_form_mode(true, true), Some(TimerFormMode::Edit));
    }

    #[test]
    fn numeric_inputs_reject_non_digits_and_are_bounded() {
        assert_eq!(digits_only("1a234".to_owned(), 3), "123");
    }

    #[test]
    fn stopwatch_refreshes_at_centisecond_precision() {
        assert_eq!(active_tick_interval(true, false), Duration::from_millis(10));
        assert_eq!(
            active_tick_interval(false, true),
            Duration::from_millis(100)
        );
        assert_eq!(active_tick_interval(false, false), Duration::from_secs(1));
    }

    #[test]
    fn city_search_filters_names_and_timezones_case_insensitively() {
        let all = filtered_city_indices("");
        assert_eq!(all.len(), city_catalog().len());

        let berlin = filtered_city_indices("BERLIN");
        assert_eq!(berlin.len(), 1);
        assert_eq!(city_catalog()[berlin[0]].0, "Berlin");

        let america = filtered_city_indices("america/");
        assert!(america.len() >= 4);
    }

    #[test]
    fn world_clock_offset_text_is_relative_to_local_time() {
        let visible = |text: String| text.replace(['\u{2068}', '\u{2069}'], "");

        assert_eq!(visible(relative_offset_text(-690)), "-11h 30m");
        assert_eq!(visible(relative_offset_text(-180)), "-3h");
        assert_eq!(relative_offset_text(0), "Same time");
        assert_eq!(visible(relative_offset_text(210)), "+3h 30m");
    }

    #[test]
    fn world_clock_day_text_only_appears_when_the_calendar_day_differs() {
        assert_eq!(relative_day_text(-1), Some("yesterday".to_owned()));
        assert_eq!(relative_day_text(0), None);
        assert_eq!(relative_day_text(1), Some("tomorrow".to_owned()));
    }

    #[test]
    fn local_clock_text_uses_the_system_hour_cycle_and_locale_date() {
        use chrono::TimeZone;

        let now = chrono::FixedOffset::east_opt(5 * 60 * 60 + 30 * 60)
            .unwrap()
            .with_ymd_and_hms(2026, 9, 28, 17, 30, 4)
            .unwrap();
        let morning = now.with_hour(5).unwrap();

        assert_eq!(local_time_text(&now, "en-US", false), "05:30:04 PM");
        assert_eq!(local_time_text(&now, "en-US", true), "17:30:04");
        assert_eq!(local_time_text(&now, "fr-BE", false), "05:30:04 PM");
        assert_eq!(local_time_text(&morning, "fr-BE", false), "05:30:04 AM");
        assert_eq!(local_date_text(&now, "en-US"), "09/28/2026, Monday");
        assert_eq!(local_date_text(&now, "de-DE"), "28.09.2026, Montag");
    }

    #[test]
    fn world_clock_text_uses_the_system_hour_cycle() {
        use chrono::TimeZone;

        let clock = WorldClock::new("Tokyo", "Asia/Tokyo").unwrap();
        let now = Utc.with_ymd_and_hms(2026, 1, 15, 12, 30, 0).unwrap();

        assert_eq!(
            world_clock_time_text(&clock, now, "en-US", false),
            "09:30 PM"
        );
        assert_eq!(world_clock_time_text(&clock, now, "en-US", true), "21:30");
        assert_eq!(
            world_clock_time_text(&clock, now, "fr-BE", false),
            "09:30 PM"
        );
    }

    #[test]
    fn ringing_timer_primary_action_resets_it() {
        let start = Instant::now();
        let mut timer =
            Timer::new(TimerId::new(1), "Tea", Duration::from_secs(1)).expect("valid timer");
        timer.start(start);
        assert!(timer.update(start + Duration::from_secs(1)));

        assert!(matches!(
            timer_primary_message(&timer, 4),
            Message::ResetTimer(4)
        ));
    }

    #[test]
    fn blank_timer_label_uses_singular_timer_name() {
        assert_eq!(normalized_timer_label("   "), "Timer");
        assert_eq!(normalized_timer_label(" Tea "), "Tea");
    }
}
