// SPDX-License-Identifier: GPL-3.0-only

use crate::timers::TimerId;
use cosmic::iced::{
    Subscription,
    futures::{
        self, FutureExt, SinkExt, StreamExt,
        channel::{mpsc::Sender, oneshot},
        future::Either,
    },
    stream,
};
use notify_rust::{Hint, Notification, NotificationResponse, Timeout, Urgency};
use std::{
    any::TypeId,
    collections::HashMap,
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{LazyLock, Mutex},
};

const APP_ID: &str = "org.cosmic_utils.clock";
const ALARM_SOUND: &[u8] = include_bytes!("../resources/sounds/alarm.wav");
static TIMER_NOTIFICATIONS: LazyLock<Mutex<TimerNotificationRegistry>> =
    LazyLock::new(|| Mutex::new(TimerNotificationRegistry::default()));

#[derive(Default)]
struct TimerNotificationRegistry {
    close_senders: HashMap<TimerId, oneshot::Sender<()>>,
}

impl TimerNotificationRegistry {
    fn register(&mut self, timer_id: TimerId, close_sender: oneshot::Sender<()>) {
        if let Some(previous) = self.close_senders.insert(timer_id, close_sender) {
            let _ = previous.send(());
        }
    }

    fn close(&mut self, timer_id: TimerId) -> bool {
        self.close_senders
            .remove(&timer_id)
            .is_some_and(|sender| sender.send(()).is_ok())
    }
}

pub fn close_timer_notification(timer_id: TimerId) -> bool {
    TIMER_NOTIFICATIONS
        .lock()
        .is_ok_and(|mut registry| registry.close(timer_id))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AlertEvent {
    Alarm {
        label: String,
        time: String,
        snooze_minutes: Option<u16>,
    },
    Timer {
        id: TimerId,
        label: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlertAction {
    SnoozeAlarm,
    DismissAlarm,
    StopTimer(TimerId),
    ResetTimer(TimerId),
}

#[derive(Clone, Debug)]
pub enum AlertSubscriptionEvent {
    Ready(Sender<AlertAction>),
    Action(AlertAction),
}

impl AlertEvent {
    #[must_use]
    pub fn alarm(
        label: impl Into<String>,
        time: impl Into<String>,
        snooze_minutes: Option<u16>,
    ) -> Self {
        Self::Alarm {
            label: label.into(),
            time: time.into(),
            snooze_minutes,
        }
    }

    #[must_use]
    pub fn timer(id: TimerId, label: impl Into<String>) -> Self {
        Self::Timer {
            id,
            label: label.into(),
        }
    }

    #[must_use]
    pub const fn summary_key(&self) -> &'static str {
        match self {
            Self::Alarm { .. } => "notification-alarm-title",
            Self::Timer { .. } => "notification-timer-title",
        }
    }

    #[must_use]
    pub fn body(&self) -> String {
        match self {
            Self::Alarm { label, time, .. } if label.trim().is_empty() => time.clone(),
            Self::Alarm { label, time, .. } => format!("{time}: {label}"),
            Self::Timer { label, .. } => label.clone(),
        }
    }

    #[must_use]
    pub const fn timer_id(&self) -> Option<TimerId> {
        match self {
            Self::Alarm { .. } => None,
            Self::Timer { id, .. } => Some(*id),
        }
    }

    #[must_use]
    pub const fn notification_action_keys(&self) -> &'static [&'static str] {
        match self {
            Self::Alarm {
                snooze_minutes: Some(_),
                ..
            } => &["default", "snooze", "dismiss"],
            Self::Alarm {
                snooze_minutes: None,
                ..
            } => &["default", "dismiss"],
            Self::Timer { .. } => &["default", "stop"],
        }
    }

    #[must_use]
    pub fn action_for_notification_response(
        &self,
        response: &NotificationResponse,
    ) -> Option<AlertAction> {
        match (self, response) {
            (
                Self::Alarm {
                    snooze_minutes: Some(_),
                    ..
                },
                NotificationResponse::Default,
            ) => Some(AlertAction::SnoozeAlarm),
            (
                Self::Alarm {
                    snooze_minutes: Some(_),
                    ..
                },
                NotificationResponse::Action(action),
            ) if action == "snooze" => Some(AlertAction::SnoozeAlarm),
            (
                Self::Alarm {
                    snooze_minutes: None,
                    ..
                },
                NotificationResponse::Default,
            ) => Some(AlertAction::DismissAlarm),
            (Self::Alarm { .. }, NotificationResponse::Action(action)) if action == "dismiss" => {
                Some(AlertAction::DismissAlarm)
            }
            (Self::Timer { id, .. }, NotificationResponse::Default) => {
                Some(AlertAction::StopTimer(*id))
            }
            (Self::Timer { id, .. }, NotificationResponse::Action(action)) if action == "stop" => {
                Some(AlertAction::StopTimer(*id))
            }
            (Self::Timer { id, .. }, NotificationResponse::Closed(_)) => {
                Some(AlertAction::ResetTimer(*id))
            }
            (Self::Alarm { .. }, NotificationResponse::Closed(_))
            | (_, NotificationResponse::Action(_))
            | (_, NotificationResponse::Reply(_)) => None,
        }
    }
}

#[must_use]
pub const fn bundled_alarm_sound() -> &'static [u8] {
    ALARM_SOUND
}

pub fn alert_action_subscription() -> Subscription<AlertSubscriptionEvent> {
    struct AlertActionSubscription;

    Subscription::run_with(TypeId::of::<AlertActionSubscription>(), |_| {
        stream::channel(8, |mut output: Sender<AlertSubscriptionEvent>| async move {
            let (sender, mut receiver) = cosmic::iced::futures::channel::mpsc::channel(8);
            if output
                .send(AlertSubscriptionEvent::Ready(sender))
                .await
                .is_err()
            {
                return;
            }

            while let Some(action) = receiver.next().await {
                if output
                    .send(AlertSubscriptionEvent::Action(action))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        })
    })
}

#[derive(Default)]
pub struct AlertSound {
    child: Option<Child>,
    unavailable: bool,
}

impl AlertSound {
    pub fn start(&mut self) -> Result<(), String> {
        if self.unavailable {
            return Ok(());
        }
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(None) => return Ok(()),
                Ok(Some(_)) | Err(_) => self.child = None,
            }
        }

        let path = write_alarm_sound()?;
        let mut last_error = None;
        for program in ["paplay", "pw-play", "aplay"] {
            match Command::new(program)
                .arg(&path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    self.child = Some(child);
                    return Ok(());
                }
                Err(error) => last_error = Some(format!("{program}: {error}")),
            }
        }
        self.unavailable = true;
        Err(last_error.unwrap_or_else(|| "no supported audio player is available".to_owned()))
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for AlertSound {
    fn drop(&mut self) {
        self.stop();
    }
}

fn write_alarm_sound() -> Result<PathBuf, String> {
    let path = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(
        || std::env::temp_dir().join(format!("{APP_ID}-alarm.wav")),
        |runtime_dir| PathBuf::from(runtime_dir).join(APP_ID).join("alarm.wav"),
    );
    if fs::read(&path).ok().as_deref() != Some(ALARM_SOUND) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&path, ALARM_SOUND).map_err(|error| error.to_string())?;
    }
    Ok(path)
}

pub fn send_notification(
    event: AlertEvent,
    summary: String,
    stop_label: Option<String>,
    snooze_label: Option<String>,
    dismiss_label: Option<String>,
    action_sender: Option<Sender<AlertAction>>,
) {
    let timer_id = event.timer_id();
    let close_receiver = timer_id.and_then(|timer_id| {
        let (close_sender, close_receiver) = oneshot::channel();
        TIMER_NOTIFICATIONS.lock().ok().map(|mut registry| {
            registry.register(timer_id, close_sender);
            close_receiver
        })
    });
    std::thread::spawn(move || {
        let mut close_receiver = close_receiver;
        if close_receiver
            .as_mut()
            .is_some_and(|receiver| receiver.try_recv().is_ok_and(|value| value.is_some()))
        {
            return;
        }
        let mut notification = Notification::new();
        notification
            .appname("Clock")
            .summary(&summary)
            .body(&event.body())
            .icon(APP_ID)
            .hint(Hint::DesktopEntry(APP_ID.to_owned()))
            .hint(Hint::Category("alarm".to_owned()))
            .hint(Hint::SuppressSound(true))
            .urgency(Urgency::Critical)
            .timeout(Timeout::Never);
        for action in event.notification_action_keys() {
            let label = match *action {
                "stop" => stop_label.as_deref().unwrap_or("Stop ringing"),
                "snooze" => snooze_label.as_deref().unwrap_or("Snooze"),
                "dismiss" => dismiss_label.as_deref().unwrap_or("Dismiss"),
                _ => "",
            };
            notification.action(action, label);
        }

        match notification.show() {
            Ok(handle) => {
                if let (Some(_), Some(close_receiver)) = (timer_id, close_receiver) {
                    let mut action_sender = action_sender;
                    futures::executor::block_on(async {
                        let should_close = {
                            let response = handle
                                .wait_for_action_async(|response: &NotificationResponse| {
                                    if let Some(action) =
                                        event.action_for_notification_response(response)
                                        && let Some(sender) = &mut action_sender
                                    {
                                        let _ = sender.try_send(action);
                                    }
                                })
                                .fuse();
                            let close = close_receiver.fuse();
                            futures::pin_mut!(response, close);
                            matches!(
                                futures::future::select(response, close).await,
                                Either::Right(_)
                            )
                        };
                        if should_close {
                            handle.close_async().await;
                        }
                    });
                } else {
                    let mut action_sender = action_sender;
                    futures::executor::block_on(handle.wait_for_action_async(
                        |response: &NotificationResponse| {
                            if let Some(action) = event.action_for_notification_response(response)
                                && let Some(sender) = &mut action_sender
                            {
                                let _ = sender.try_send(action);
                            }
                        },
                    ));
                }
            }
            Err(error) => {
                if let Some(timer_id) = timer_id {
                    close_timer_notification(timer_id);
                }
                tracing::warn!(%error, "failed to show alert notification");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_notification_registry_closes_only_the_matching_timer() {
        let mut registry = TimerNotificationRegistry::default();
        let timer_id = TimerId::new(42);
        let (close_sender, close_receiver) = futures::channel::oneshot::channel();
        registry.register(timer_id, close_sender);

        assert!(!registry.close(TimerId::new(7)));
        assert!(registry.close(timer_id));
        assert_eq!(futures::executor::block_on(close_receiver), Ok(()));
        assert!(!registry.close(timer_id));
    }
}
