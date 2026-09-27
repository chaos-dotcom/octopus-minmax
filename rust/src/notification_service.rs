//! `src/notification_service.py`

use std::sync::Mutex;

use crate::apprise::Apprise;
use crate::config;
use crate::clock;
use crate::{logd, loge, logi, logw};

/// Discord's limit, minus room for the code fence Apprise adds.
pub const DISCORD_CHAR_LIMIT: usize = 1900;

struct State {
    notification_urls: String,
    batch_enabled: bool,
    batch: Vec<String>,
    apprise: Option<Apprise>,
}

pub struct NotificationService {
    state: Mutex<State>,
}

impl NotificationService {
    pub fn new(notification_urls: &str, batch_enabled: bool) -> NotificationService {
        NotificationService {
            state: Mutex::new(State {
                notification_urls: notification_urls.to_string(),
                batch_enabled,
                batch: Vec::new(),
                apprise: None,
            }),
        }
    }

    fn refresh_from_config(&self, state: &mut State) {
        let settings = config::get();
        if state.notification_urls != settings.notification_urls {
            state.notification_urls = settings.notification_urls.clone();
            state.apprise = None;
        }
        if state.batch_enabled != settings.batch_notifications {
            state.batch_enabled = settings.batch_notifications;
        }
    }

    fn build_apprise(&self, state: &mut State) {
        if state.apprise.is_none() {
            let mut apprise = Apprise::new();
            let urls = state.notification_urls.clone();
            for url in urls.split(',') {
                let trimmed = url.trim();
                if trimmed.is_empty() {
                    continue;
                }
                // Apprise logs "Unparseable URL ..." from its own logger, which no handler
                // is attached to, so nothing reaches the application log.
                let _ = apprise.add(trimmed);
            }
            state.apprise = Some(apprise);
        }
    }

    /// `NotificationService.send_notification`
    pub fn send_notification(&self, message: &str, title: &str, is_error: bool, batchable: bool) -> bool {
        let mut state = self.state.lock().unwrap();
        self.refresh_from_config(&mut state);
        self.build_apprise(&mut state);

        let mut message = message.to_string();
        if is_error {
            // Apprise truncates at the Discord limit, so long errors are sent in pieces.
            while message.chars().count() > DISCORD_CHAR_LIMIT {
                let split = message.chars().count() - DISCORD_CHAR_LIMIT;
                let head: String = message.chars().take(split).collect();
                let tail: String = message.chars().skip(split).collect();
                drop(state);
                self.send_notification(&head, "", is_error, batchable);
                state = self.state.lock().unwrap();
                self.refresh_from_config(&mut state);
                self.build_apprise(&mut state);
                message = tail;
            }
            message = format!("```py\n{}\n```", message);
        }

        if state.batch_enabled && batchable {
            state.batch.push(message);
            logd!(
                "octobot.notification_service",
                "notification_service.send_notification",
                "Added message to batch. Current batch size: {}",
                state.batch.len()
            );
            return true;
        }

        let apprise = state.apprise.as_ref().unwrap();
        if apprise.is_empty() {
            logw!(
                "octobot.notification_service",
                "notification_service.send_notification",
                "No notification services configured. Check config.NOTIFICATION_URLS."
            );
            logi!(
                "octobot.notification_service",
                "notification_service.send_notification",
                "{}",
                message
            );
            return false;
        }
        let success = apprise.notify(&message, title);
        logi!(
            "octobot.notification_service",
            "notification_service.send_notification",
            "Successfuly sent notification: {}",
            message
        );
        if !success {
            loge!(
                "octobot.notification_service",
                "notification_service.send_notification",
                "Failed to send notification: {}",
                title
            );
        }
        success
    }

    /// `NotificationService.send_batch_notification`
    pub fn send_batch_notification(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        self.refresh_from_config(&mut state);
        if state.batch.is_empty() {
            logd!(
                "octobot.notification_service",
                "notification_service.send_batch_notification",
                "No notifications in batch to send"
            );
            return true;
        }
        self.build_apprise(&mut state);
        let settings = config::get();

        let title = if settings.one_off_run {
            format!(
                "Octopus MinMax Results - {} {}",
                clock::weekday_day_month(),
                clock::hour_minute_second()
            )
        } else {
            format!(
                "Octopus MinMax Results - {} {}",
                clock::weekday_day_month(),
                settings.execution_time
            )
        };
        let body = state.batch.join("\n");

        let apprise = state.apprise.as_ref().unwrap();
        if apprise.is_empty() {
            logw!(
                "octobot.notification_service",
                "notification_service.send_batch_notification",
                "Cannot send batch - no notification services configured. Check config.NOTIFICATION_URLS."
            );
            logi!(
                "octobot.notification_service",
                "notification_service.send_batch_notification",
                "{}",
                body
            );
            return false;
        }
        let success = apprise.notify(&body, &title);
        if success {
            logi!(
                "octobot.notification_service",
                "notification_service.send_batch_notification",
                "Sent batch notification with {} messages",
                state.batch.len()
            );
            state.batch.clear();
        } else {
            loge!(
                "octobot.notification_service",
                "notification_service.send_batch_notification",
                "Failed to send batch notification"
            );
        }
        success
    }
}
