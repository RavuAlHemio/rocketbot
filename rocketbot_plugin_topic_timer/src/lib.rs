use std::sync::{Arc, Weak};

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Timelike, Utc};
use log::{debug, warn};
use regex::Regex;
use rocketbot_interface::JsonValueExtensions;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelTextType;
use serde_json;


#[derive(Debug)]
struct Counter {
    pub base_timestamp: DateTime<Utc>,
    pub channel_name: String,
    pub topic_matcher: Regex,
    pub replacement: String,
}
impl Counter {
    pub fn new(
        base_timestamp: DateTime<Utc>,
        channel_name: String,
        topic_matcher: Regex,
        replacement: String,
    ) -> Self {
        Self {
            base_timestamp,
            channel_name,
            topic_matcher,
            replacement,
        }
    }
}


async fn register_topic_timer(interface: Arc<dyn RocketBotInterface>, base_timestamp: &DateTime<Utc>, index: usize) {
    let now = Utc::now();
    let mut next_occurrence = now.date()
        .and_hms(base_timestamp.hour(), base_timestamp.minute(), base_timestamp.second());
    if next_occurrence < now {
        next_occurrence = next_occurrence + chrono::Duration::days(1);
    }
    debug!("next occurrence of timer with index {}: {:?}", index, next_occurrence);
    let custom_data = serde_json::json!(["topic_timer", index]);
    interface.register_timer(next_occurrence, custom_data).await;
}


pub struct TopicTimerPlugin {
    interface: Weak<dyn RocketBotInterface>,
    counters: Vec<Counter>,
}
#[async_trait]
impl RocketBotPlugin for TopicTimerPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let mut counters = Vec::new();
        for counter_json in config["counters"].members().expect("counters is not a list") {
            let base_timestamp_str = counter_json["base_timestamp_utc"].as_str()
                .expect("base_timestamp_utc not representable as a string");
            let base_timestamp = Utc.datetime_from_str(base_timestamp_str, "%Y-%m-%d %H:%M:%S")
                .expect("base_timestamp_utc not in \"YYYY-MM-DD hh:mm:ss\" format");

            let channel_name = counter_json["channel_name"].as_str()
                .expect("channel_name not representable as a string");

            let topic_matcher_str = counter_json["topic_matcher"].as_str()
                .expect("topic_matcher not representable as a string");
            let topic_matcher = Regex::new(topic_matcher_str)
                .expect("failed to compile topic_matcher regex");

            let replacement = counter_json["replacement"].as_str()
                .expect("replacement not representable as string");

            counters.push(Counter::new(
                base_timestamp,
                channel_name.to_owned(),
                topic_matcher,
                replacement.to_owned(),
            ))
        }

        for (i, counter) in counters.iter().enumerate() {
            register_topic_timer(Arc::clone(&my_interface), &counter.base_timestamp, i).await;
        }

        Self {
            interface,
            counters,
        }
    }

    async fn plugin_name(&self) -> String {
        "topic_timer".to_owned()
    }

    async fn timer_elapsed(&self, custom_data: &serde_json::Value) {
        if !custom_data.is_array() {
            return;
        }
        if custom_data[0] != "topic_timer" {
            return;
        }
        let index = match custom_data[1].as_usize() {
            Some(y) => y,
            None => return,
        };
        if index >= self.counters.len() {
            return;
        }
        let counter = &self.counters[index];

        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if let Some(topic) = interface.get_channel_text(&counter.channel_name, ChannelTextType::Topic).await {
            let now = Utc::now();
            let duration_since = now.signed_duration_since(counter.base_timestamp);
            let replacement_fill = counter.replacement
                .replace("{days}", &duration_since.num_days().to_string());

            let replaced = counter.topic_matcher.replace(&topic, &replacement_fill);
            if replaced.as_ref() != &topic {
                interface.set_channel_text(
                    &counter.channel_name,
                    ChannelTextType::Topic,
                    replaced.as_ref(),
                ).await;
            }
        }

        register_topic_timer(Arc::clone(&interface), &counter.base_timestamp, index).await;
    }

    async fn configuration_updated(&self, _new_config: serde_json::Value) -> bool {
        warn!("configuration updates are not yet supported for the topic_timer plugin");
        false
    }
}
