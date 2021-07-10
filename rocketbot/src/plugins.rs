use std::sync::Weak;

use json::JsonValue;
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};

use crate::config::CONFIG;


pub(crate) async fn load_plugins(iface: Weak<dyn RocketBotInterface>) -> Vec<Box<dyn RocketBotPlugin>> {
    let mut plugins: Vec<Box<dyn RocketBotPlugin>> = Vec::new();

    {
        let config_guard = CONFIG
            .get().expect("initial config not set")
            .read().await;

        for plugin_config in &config_guard.plugins {
            if !plugin_config.enabled {
                continue;
            }

            let iface_weak = Weak::clone(&iface);
            let inner_config: JsonValue = plugin_config.config.clone().into();

            let plugin: Box<dyn RocketBotPlugin> = if plugin_config.name == "belch" {
                Box::new(rocketbot_plugin_belch::BelchPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "config_user_alias" {
                Box::new(rocketbot_plugin_config_user_alias::ConfigUserAliasPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "fortune" {
                Box::new(rocketbot_plugin_fortune::FortunePlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "grammargen" {
                Box::new(rocketbot_plugin_grammargen::GrammarGenPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "sed" {
                Box::new(rocketbot_plugin_sed::SedPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "text_commands" {
                Box::new(rocketbot_plugin_text_commands::TextCommandsPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "thanks" {
                Box::new(rocketbot_plugin_thanks::ThanksPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "vaccine" {
                Box::new(rocketbot_plugin_vaccine::VaccinePlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "version" {
                Box::new(rocketbot_plugin_version::VersionPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "weather" {
                Box::new(rocketbot_plugin_weather::WeatherPlugin::new(iface_weak, inner_config).await)
            } else {
                panic!("unknown plugin {}", plugin_config.name);
            };

            plugins.push(plugin);
        }

        plugins
    }
}
