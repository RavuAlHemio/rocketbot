use std::sync::Weak;

use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use serde_json;

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
            let inner_config: serde_json::Value = plugin_config.config.clone().into();

            let plugin: Box<dyn RocketBotPlugin> = if plugin_config.name == "allograph" {
                Box::new(rocketbot_plugin_allograph::AllographPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "belch" {
                Box::new(rocketbot_plugin_belch::BelchPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "calc" {
                Box::new(rocketbot_plugin_calc::CalcPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "catchword" {
                Box::new(rocketbot_plugin_catchword::CatchwordPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "config_user_alias" {
                Box::new(rocketbot_plugin_config_user_alias::ConfigUserAliasPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "dice" {
                Box::new(rocketbot_plugin_dice::DicePlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "fact" {
                Box::new(rocketbot_plugin_fact::FactPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "fortune" {
                Box::new(rocketbot_plugin_fortune::FortunePlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "grammargen" {
                Box::new(rocketbot_plugin_grammargen::GrammarGenPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "group_pressure" {
                Box::new(rocketbot_plugin_group_pressure::GroupPressurePlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "help" {
                Box::new(rocketbot_plugin_help::HelpPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "logger" {
                Box::new(rocketbot_plugin_logger::LoggerPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "new_year" {
                Box::new(rocketbot_plugin_new_year::NewYearPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "paper" {
                Box::new(rocketbot_plugin_paper::PaperPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "quotes" {
                Box::new(rocketbot_plugin_quotes::QuotesPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "randreact" {
                Box::new(rocketbot_plugin_randreact::RandReactPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "sed" {
                Box::new(rocketbot_plugin_sed::SedPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "sockpuppet" {
                Box::new(rocketbot_plugin_sockpuppet::SockpuppetPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "text_commands" {
                Box::new(rocketbot_plugin_text_commands::TextCommandsPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "thanks" {
                Box::new(rocketbot_plugin_thanks::ThanksPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "topic_timer" {
                Box::new(rocketbot_plugin_topic_timer::TopicTimerPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "vaccine" {
                Box::new(rocketbot_plugin_vaccine::VaccinePlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "version" {
                Box::new(rocketbot_plugin_version::VersionPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "vitals" {
                Box::new(rocketbot_plugin_vitals::VitalsPlugin::new(iface_weak, inner_config).await)
            } else if plugin_config.name == "weather" {
                Box::new(rocketbot_plugin_weather::WeatherPlugin::new(iface_weak, inner_config).await)
            } else {
                panic!("unknown plugin {}", plugin_config.name);
            };

            let self_reported_name = plugin.plugin_name().await;
            if !self_reported_name.starts_with(&plugin_config.name) {
                panic!(
                    "plugin {:?} claims to be {:?}; self-reported name must start with config name",
                    plugin_config.name,
                    self_reported_name,
                );
            }

            plugins.push(plugin);
        }

        plugins
    }
}
