use std::borrow::Cow;
use std::sync::Weak;

use async_trait::async_trait;
use bitflags::bitflags;
use rocketbot_interface::{phrase_join, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;
use serde::{Deserialize, Serialize};
use serde_json;
use tokio_postgres::NoTls;
use tracing::error;


#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct Config {
    pub db_conn_string: String,
}

bitflags! {
    #[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub struct GenderFlags : i64 {
        const MASCULINE = (1 << 0);
        const FEMININE = (1 << 1);
        const NEUTER = (1 << 2);
        const SINGULARE_TANTUM = (1 << 3);
        const PLURALE_TANTUM = (1 << 4);
        const MALE_GIVEN = (1 << 5);
        const FEMALE_GIVEN = (1 << 6);
        const UNISEX_GIVEN = (1 << 7);
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GermanGender {
    pub word: String,
    pub gender_flags: GenderFlags,
}


pub struct LinguisticsPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
}
impl LinguisticsPlugin {
    async fn connect_db(&self, config: &Config) -> Result<tokio_postgres::Client, tokio_postgres::Error> {
        let (client, connection) = match tokio_postgres::connect(&config.db_conn_string, NoTls).await {
            Ok(cc) => cc,
            Err(e) => {
                error!("error connecting to database: {}", e);
                return Err(e);
            },
        };
        tokio::spawn(async move {
            connection.await
        });
        Ok(client)
    }

    async fn get_german_gender(
        &self,
        config: &Config,
        word: &str,
    ) -> Result<Option<GermanGender>, tokio_postgres::Error> {
        let db_client = self.connect_db(config).await?;
        let row_opt = db_client.query_opt(
            "
                SELECT  word
                    ,   gender_flags
                FROM    linguistics.german_genders
                WHERE   LOWER(word) = LOWER($1)
            ",
            &[&word],
        ).await?;
        let gender_opt = row_opt
            .map(|row| {
                let word: String = row.get(0);
                let flags_i64: i64 = row.get(1);

                let gender_flags = GenderFlags::from_bits_truncate(flags_i64);

                GermanGender {
                    word,
                    gender_flags,
                }
            });
        Ok(gender_opt)
    }

    async fn handle_gg(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let config_guard = self.config.read().await;
        let word = command.rest.trim();
        let message: Cow<'static, str> = match self.get_german_gender(&*config_guard, word).await {
            Ok(Some(gender)) => {
                let mut gender_words = Vec::with_capacity(3);
                if gender.gender_flags.contains(GenderFlags::MASCULINE) {
                    gender_words.push("masculine");
                }
                if gender.gender_flags.contains(GenderFlags::FEMININE) {
                    gender_words.push("feminine");
                }
                if gender.gender_flags.contains(GenderFlags::NEUTER) {
                    gender_words.push("neuter");
                }
                if gender.gender_flags.contains(GenderFlags::SINGULARE_TANTUM) {
                    gender_words.push("singular-only");
                }
                if gender.gender_flags.contains(GenderFlags::PLURALE_TANTUM) {
                    gender_words.push("plural-only");
                }
                if gender.gender_flags.contains(GenderFlags::MALE_GIVEN) {
                    gender_words.push("a male given name");
                }
                if gender.gender_flags.contains(GenderFlags::FEMALE_GIVEN) {
                    gender_words.push("a female given name");
                }
                if gender.gender_flags.contains(GenderFlags::UNISEX_GIVEN) {
                    gender_words.push("a unisex given name");
                }

                if gender_words.is_empty() {
                    Cow::Owned(format!("_{}_ is a German noun, but not much more is known", gender.word))
                } else {
                    Cow::Owned(format!("_{}_ is {}", gender.word, phrase_join(&gender_words, ", ", " and ")))
                }
            },
            Ok(None) => Cow::Borrowed("Wiktionary does not know this word. :disappointed:"),
            Err(e) => {
                error!("error querying German gender of {:?}: {}", word, e);
                Cow::Borrowed("A database error occurred. :disappointed:")
            },
        };

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &message,
        ).await;
    }

    fn try_get_config(config: serde_json::Value) -> Option<Config> {
        match serde_json::from_value(config) {
            Ok(c) => Some(c),
            Err(e) => {
                error!("error processing config: {}", e);
                None
            },
        }
    }
}
#[async_trait]
impl RocketBotPlugin for LinguisticsPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self where Self: Sized {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");

        let gg_command = CommandDefinitionBuilder::new(
            "gg",
            "linguistics",
            "{cpfx}gg WORD",
            "Looks up the German gender of WORD.",
        )
            .build();
        my_interface.register_channel_command(&gg_command).await;

        let config_lock = RwLock::new(
            "LinguisticsPlugin::config",
            config_object,
        );

        LinguisticsPlugin {
            interface,
            config: config_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "linguistics".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "gg" {
            self.handle_gg(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "gg" {
            Some(include_str!("../help/gg.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, new_config: serde_json::Value) -> bool {
        match Self::try_get_config(new_config) {
            Some(c) => {
                let mut config_guard = self.config.write().await;
                *config_guard = c;
                true
            },
            None => false,
        }
    }
}
