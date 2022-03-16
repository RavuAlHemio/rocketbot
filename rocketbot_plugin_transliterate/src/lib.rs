mod model;


use std::collections::HashMap;
use std::fs::{File, read_dir};
use std::ops::DerefMut;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use log::{debug, error};
use rand::{RngCore, Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_interface::{ResultExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, RwLock};
use serde_json;

use crate::model::{Language, Transformation};


#[derive(Clone, Debug)]
struct Config {
    languages: HashMap<String, Language>,
    command_to_lang_combo: HashMap<String, (String, String)>,
}


pub struct TransliteratePlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    rng: Mutex<StdRng>,
}
impl TransliteratePlugin {
    fn transliterate<R: RngCore>(
        rng: &mut R,
        transformations: &[Transformation],
        base_string: &str,
    ) -> String {
        let mut i: usize = 0;
        let mut ret = String::with_capacity(base_string.len());
        let base_string_lower = base_string.to_lowercase();

        while i < base_string_lower.len() {
            // find the first matching transformation
            let mut matched = false;
            for xform in transformations {
                let m = if let Some(m) = xform.matcher.find(&base_string_lower[i..]) {
                    if m.start() > 0 {
                        continue;
                    }

                    // leftmost match; process it
                    m
                } else {
                    // not found
                    continue;
                };

                debug!("found matcher {:?} for {:?}", xform.matcher.as_str(), &base_string_lower[i..]);

                // find a replacement!
                let total_weight = xform.replacements.iter()
                    .map(|r| r.weight)
                    .sum();
                let mut my_weight = rng.gen_range(0..total_weight);
                for replacement in &xform.replacements {
                    if my_weight >= replacement.weight {
                        my_weight -= replacement.weight;
                        continue;
                    }

                    debug!("chose replacement {:?}", replacement.replacement);

                    // apply the replacement!
                    let replaced = xform.matcher.replace(
                        &base_string_lower[i..][m.range()],
                        &replacement.replacement,
                    );
                    ret.push_str(&replaced);
                    debug!("result is now {:?}", ret);

                    // advance!
                    i += m.range().len();

                    break;
                }

                matched = true;
            }

            if !matched {
                // by default, just copy the character over and advance by one character
                // (might be multiple bytes)
                let c = base_string_lower[i..].chars().nth(0).unwrap();
                ret.push(c);
                i += c.len_utf8();
                debug!("unmatched {:?}; result is now {:?}", c, ret);
            }
        }

        ret
    }

    async fn channel_command_transliterate(
        &self,
        config: &Config,
        source_lang: &str,
        dest_lang: &str,
        channel_message: &ChannelMessage,
        command: &CommandInstance,
    ) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let source_language = match config.languages.get(source_lang) {
            Some(sl) => sl,
            None => {
                error!("source language {:?} not found", source_lang);
                return;
            },
        };
        let dest_language = match config.languages.get(dest_lang) {
            Some(dl) => dl,
            None => {
                error!("destination language {:?} not found", dest_lang);
                return;
            },
        };

        let transliterated = {
            let mut rng_lock = self.rng.lock().await;

            // transliterate from source language
            let intermediate = Self::transliterate(
                rng_lock.deref_mut(),
                &source_language.from_lang,
                &command.rest,
            );

            // transliterate to target language
            let target = Self::transliterate(
                rng_lock.deref_mut(),
                &dest_language.to_lang,
                &intermediate,
            );

            target
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &transliterated,
        ).await;
    }

    async fn channel_command_languages(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let mut lang_abbrs: Vec<String> = config_guard.languages.keys()
            .map(|ln| format!("`{}`", ln))
            .collect();
        lang_abbrs.sort_unstable();
        let lang_abbrs_string = lang_abbrs.join(", ");

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &format!("Available languages: {}", lang_abbrs_string),
        ).await;
    }

    async fn channel_command_onestep(
        &self,
        detransliterate: bool,
        channel_message: &ChannelMessage,
        command: &CommandInstance,
    ) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let language_abbr = command.args.get(0).unwrap();
        let language = match config_guard.languages.get(language_abbr) {
            Some(l) => l,
            None => {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &format!("Unknown language `{}`.", language_abbr),
                ).await;
                return;
            },
        };

        let result = {
            let mut rng_lock = self.rng.lock().await;
            Self::transliterate(
                rng_lock.deref_mut(),
                if detransliterate { &language.from_lang } else { &language.to_lang },
                &command.rest,
            )
        };
        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &result,
        ).await;
    }

    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let language_dir = config["language_dir"]
            .as_str().ok_or("language_dir not a string")?;

        let mut languages_vec: Vec<Language> = Vec::new();
        for entry_res in read_dir(language_dir).or_msg("failed to read language_dir")? {
            let entry = match entry_res {
                Ok(e) => e,
                Err(e) => {
                    error!("failed to read language_dir ({:?}) entry: {}", language_dir, e);
                    continue;
                },
            };

            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                // not a file (or failed to read its type)
                continue;
            }
            if !entry.file_name().to_string_lossy().to_lowercase().ends_with(".json") {
                // not JSON
                continue;
            }

            // read the file as a language
            let file = File::open(entry.path())
                .or_msg("failed to open language file")?;
            let language: Language = serde_json::from_reader(file)
                .or_msg("failed to parse language file")?;
            languages_vec.push(language);
        }
        let languages: HashMap<String, Language> = languages_vec
            .drain(..)
            .map(|l| (l.abbrev.clone(), l))
            .collect();

        let default_language = config["default_language"]
            .as_str().ok_or("default_language not a string")?
            .to_owned();
        if !languages.contains_key(&default_language) {
            error!("default language {} not found", default_language);
            return Err("default language not found");
        }

        let mut command_to_lang_combo: HashMap<String, (String, String)> = HashMap::new();
        for source_lang in languages.values() {
            for dest_lang in languages.values() {
                if source_lang.abbrev == dest_lang.abbrev {
                    continue;
                }

                let command = format!("{}{}", source_lang.abbrev, dest_lang.abbrev);
                command_to_lang_combo.insert(
                    command.clone(),
                    (source_lang.abbrev.clone(), dest_lang.abbrev.clone()),
                );

                if source_lang.abbrev == default_language {
                    // transliterating from the default language
                    command_to_lang_combo.insert(
                        dest_lang.abbrev.clone(),
                        (source_lang.abbrev.clone(), dest_lang.abbrev.clone()),
                    );
                }
            }
        }

        Ok(Config {
            languages,
            command_to_lang_combo,
        })
    }

    async fn register_commands(interface: Arc<dyn RocketBotInterface>, config: &Config) {
        for (command, (source_lang_abbr, dest_lang_abbr)) in &config.command_to_lang_combo {
            let source_lang = config.languages.get(source_lang_abbr)
                .expect("unknown language");
            let dest_lang = config.languages.get(dest_lang_abbr)
                .expect("unknown language");

            interface.register_channel_command(
                &CommandDefinitionBuilder::new(
                    command.clone(),
                    "transliterate",
                    "{cpfx}{cmd} PHRASE",
                    format!(
                        "Transliterates text from {} to {}.",
                        source_lang.name, dest_lang.name,
                    ),
                )
                    .build(),
            ).await;
        }
    }
}
#[async_trait]
impl RocketBotPlugin for TransliteratePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "languages",
                "transliterate",
                "{cpfx}languages",
                "Transliterates text from a specific language to the intermediate language.",
            )
                .build(),
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "detransliterate",
                "transliterate",
                "{cpfx}detransliterate LANG PHRASE",
                "Transliterates text from a specific language to the intermediate language.",
            )
                .arg_count(1)
                .build(),
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "entransliterate",
                "transliterate",
                "{cpfx}entransliterate LANG PHRASE",
                "Transliterates text from the intermediate language to a specific language.",
            )
                .arg_count(1)
                .build(),
        ).await;

        Self::register_commands(Arc::clone(&my_interface), &config_object).await;

        let config_lock = RwLock::new(
            "TransliteratePlugin::config",
            config_object,
        );

        let rng = Mutex::new(
            "TransliteratePlugin::rng",
            StdRng::from_entropy(),
        );

        Self {
            interface,
            config: config_lock,
            rng,
        }
    }

    async fn plugin_name(&self) -> String {
        "transliterate".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "languages" {
            self.channel_command_languages(channel_message, command).await;
            return;
        } else if command.name == "detransliterate" {
            self.channel_command_onestep(true, channel_message, command).await;
            return;
        } else if command.name == "entransliterate" {
            self.channel_command_onestep(false, channel_message, command).await;
            return;
        }

        let config_guard = self.config.read().await;

        let (source_lang, dest_lang) = match config_guard.command_to_lang_combo.get(&command.name) {
            Some(s_d) => s_d,
            None => return,
        };

        self.channel_command_transliterate(&config_guard, source_lang, dest_lang, channel_message, command).await
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "languages" {
            return Some(include_str!("../help/languages.md").to_owned());
        } else if command_name == "detransliterate" {
            return Some(include_str!("../help/detransliterate.md").to_owned());
        } else if command_name == "entransliterate" {
            return Some(include_str!("../help/entransliterate.md").to_owned());
        }

        let config_guard = self.config.read().await;

        let (source_lang, dest_lang) = match config_guard.command_to_lang_combo.get(command_name) {
            Some(s_d) => s_d,
            None => return None,
        };

        let help_text = include_str!("../help/transliterate.md")
            .replace("{source_lang}", source_lang)
            .replace("{target_lang}", dest_lang);
        Some(help_text)
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        let interface = match self.interface.upgrade() {
            None => {
                error!("interface is gone");
                return false;
            },
            Some(i) => i,
        };

        match Self::try_get_config(new_config) {
            Ok(c) => {
                let mut config_guard = self.config.write().await;

                // remove old language commands
                for command_name in config_guard.command_to_lang_combo.keys() {
                    interface.unregister_channel_command(command_name).await;
                }

                // replace config
                *config_guard = c;

                // register new language commands
                Self::register_commands(Arc::clone(&interface), &config_guard).await;

                true
            },
            Err(e) => {
                error!("failed to load new config: {}", e);
                false
            },
        }
    }
}
