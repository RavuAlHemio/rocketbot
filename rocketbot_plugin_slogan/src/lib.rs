use std::sync::Weak;

use async_trait::async_trait;
use log::{debug, error};
use regex::Regex;
use rocketbot_interface::{ResultExtensions, send_channel_message, send_private_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{ChannelMessage, PrivateMessage};
use rocketbot_interface::sync::RwLock;
use serde_json;
use sxd_document;
use sxd_document::dom::Element;
use sxd_xpath;


#[derive(Clone, Debug)]
struct CleanupRegex {
    pub regex: Regex,
    pub replacement: String,
}


#[derive(Clone, Debug)]
struct Config {
    slogan_url: String,
    cleanup_regexes: Vec<CleanupRegex>,
    slogan_xpath: String,
    subject_placeholder: String,
}


pub struct SloganPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
}
impl SloganPlugin {
    async fn generate_slogan(&self, config: &Config, subject: &str) -> Option<String> {
        // obtain URL content
        let response = match reqwest::get(&config.slogan_url).await {
            Ok(r) => r,
            Err(e) => {
                error!("failed to obtain {} response: {}", config.slogan_url, e);
                return None;
            },
        };
        if response.status() != 200 {
            error!("response from {} is {}", config.slogan_url, response.status());
            return None;
        }
        let mut response_text = match response.text().await {
            Ok(rt) => rt,
            Err(e) => {
                error!("failed to open {} response text: {}", config.slogan_url, e);
                return None;
            },
        };

        // apply cleanup regexes
        for clean_regex in &config.cleanup_regexes {
            response_text = clean_regex.regex
                .replace_all(&response_text, &clean_regex.replacement)
                .into_owned();
        }

        // parse
        let doc_package = match sxd_document::parser::parse(&response_text) {
            Ok(dp) => dp,
            Err(e) => {
                error!("failed to parse {} response: {}", config.slogan_url, e);
                debug!("document content is: {:?}", response_text);
                return None;
            },
        };

        // apply xpath
        let xpath_factory = sxd_xpath::Factory::new();
        let xpath = match xpath_factory.build(&config.slogan_xpath) {
            Ok(Some(xp)) => xp,
            Ok(None) => {
                error!("XPath {:?} generated a None value", config.slogan_xpath);
                return None;
            },
            Err(e) => {
                error!("failed to parse XPath {:?}: {}", config.slogan_xpath, e);
                return None;
            },
        };
        let mut xpath_ctx = sxd_xpath::Context::new();
        xpath_ctx.set_namespace("h", "http://www.w3.org/1999/xhtml");
        let xpath_result = match xpath.evaluate(&xpath_ctx, doc_package.as_document().root()) {
            Ok(r) => r,
            Err(e) => {
                error!("failed to evaluate XPath {:?}: {}", config.slogan_xpath, e);
                return None;
            },
        };
        let xpath_string = match xpath_result {
            sxd_xpath::Value::String(s) => {
                s
            },
            sxd_xpath::Value::Nodeset(nodeset) => {
                let mut total_text = String::new();
                for node in nodeset.document_order() {
                    if let Some(t) = node.text() {
                        total_text.push_str(t.text());
                    } else if let Some(elem) = node.element() {
                        let s = collect_element_strings(&elem);
                        total_text.push_str(&s);
                    }
                }
                total_text
            },
            other => {
                error!("XPath {:?} returned {:?}, not a string value", config.slogan_xpath, other);
                return None;
            },
        };

        let response_string = xpath_string
            .replace(&config.subject_placeholder, &format!("*{}*", subject));

        Some(response_string)
    }

    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let slogan_url = config["slogan_url"]
            .as_str().ok_or("slogan_url is not a string")?
            .to_owned();

        let mut cleanup_regexes = Vec::new();
        for cleanup_regex_obj in config["cleanup_regexes"].as_array().ok_or("cleanup_regexes not an array")?.iter() {
            let regex_str = cleanup_regex_obj["regex"]
                .as_str().ok_or("cleanup_regexes[...].regex not a string")?;
            let regex = Regex::new(regex_str)
                .or_msg("failed to parse cleanup_regexes[...].regex")?;

            let replacement = cleanup_regex_obj["replacement"]
                .as_str().ok_or("cleanup_regexes[...].replacement not a string")?
                .to_owned();

            cleanup_regexes.push(CleanupRegex {
                regex,
                replacement,
            })
        }

        let slogan_xpath = config["slogan_xpath"]
            .as_str().ok_or("slogan_xpath is not a string")?
            .to_owned();
        let subject_placeholder = config["subject_placeholder"]
            .as_str().ok_or("subject_placeholder is not a string")?
            .to_owned();

        Ok(Config {
            slogan_url,
            cleanup_regexes,
            slogan_xpath,
            subject_placeholder,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for SloganPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "SloganPlugin::config",
            config_object,
        );

        let slogan_command = CommandDefinitionBuilder::new(
            "slogan".to_owned(),
            "slogan".to_owned(),
            "{cpfx}slogan [SUBJECT]".to_owned(),
            "Generates and outputs a generic marketing slogan about SUBJECT.".to_owned(),
        )
            .build();
        my_interface.register_channel_command(&slogan_command).await;
        my_interface.register_private_message_command(&slogan_command).await;

        Self {
            interface,
            config: config_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "slogan".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if command.name != "slogan" {
            return;
        }

        let subject = command.rest.trim();
        if subject.len() == 0 {
            return;
        }

        let config_guard = self.config.read().await;

        let response_string = match self.generate_slogan(&config_guard, subject).await {
            Some(rs) => rs,
            None => return,
        };

        // send it
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response_string,
        ).await;
    }

    async fn private_command(&self, private_message: &PrivateMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if command.name != "slogan" {
            return;
        }

        let subject = command.rest.trim();
        if subject.len() == 0 {
            return;
        }

        let config_guard = self.config.read().await;

        let response_string = match self.generate_slogan(&config_guard, subject).await {
            Some(rs) => rs,
            None => return,
        };

        // send it
        send_private_message!(
            interface,
            &private_message.conversation.id,
            &response_string,
        ).await;
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        match Self::try_get_config(new_config) {
            Ok(c) => {
                let mut config_guard = self.config.write().await;
                *config_guard = c;
                true
            },
            Err(e) => {
                error!("failed to load new config: {}", e);
                false
            },
        }
    }
}

fn collect_element_strings(element: &Element) -> String {
    if element.name().local_part() == "br" {
        return " ".to_owned();
    }

    let mut total_text = String::new();
    for child in element.children() {
        if let Some(t) = child.text() {
            total_text.push_str(t.text());
        } else if let Some(elem) = child.element() {
            let s = collect_element_strings(&elem);
            total_text.push_str(&s);
        }
    }
    total_text
}
