use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Weak;

use async_trait::async_trait;
use rocketbot_interface::commands::{CommandConfiguration, CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde_json;


fn replace_config_placeholders(original: &str, command_name: &str, command_config: &CommandConfiguration) -> String {
    original
        .replace("{cmd}", command_name)
        .replace("{cpfx}", &command_config.command_prefix)
        .replace("{sopfx}", &command_config.short_option_prefix)
        .replace("{lopfx}", &command_config.long_option_prefix)
}


pub struct HelpPlugin {
    interface: Weak<dyn RocketBotInterface>,
}
impl HelpPlugin {
    async fn get_command_usages(&self, command_config: &CommandConfiguration, plugin: Option<&str>) -> Option<String> {
        let interface = match self.interface.upgrade() {
            None => return None,
            Some(i) => i,
        };

        let mut usage_to_descr: BTreeMap<String, (String, String)> = BTreeMap::new();

        // get all regular commands and usages
        for defn in &interface.get_defined_commands(plugin).await {
            usage_to_descr.insert(
                defn.name.clone(),
                (
                    replace_config_placeholders(&defn.usage, &defn.name, &command_config),
                    replace_config_placeholders(&defn.description, &defn.name, &command_config),
                ),
            );
        }

        // get special commands and usages
        for (name, (usage, description)) in &interface.get_additional_commands_usages(plugin).await {
            usage_to_descr.insert(
                name.clone(),
                (
                    replace_config_placeholders(usage, name, &command_config),
                    replace_config_placeholders(description, name, &command_config),
                ),
            );
        }

        let commands: Vec<String> = usage_to_descr
            .values()
            .map(|(u, d)| format!("`{}` \u{2013} {}", u, d))
            .collect();
        let commands_str = commands.join("\n");

        Some(commands_str)
    }
}
#[async_trait]
impl RocketBotPlugin for HelpPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        my_interface.register_channel_command(&CommandDefinition::new(
            "help".to_owned(),
            "help".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            "{cpfx}help [COMMAND]".to_owned(),
            "Shows help about the given command, or lists all available commands.".to_owned(),
        )).await;
        my_interface.register_channel_command(&CommandDefinition::new(
            "helpplug".to_owned(),
            "help".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            "{cpfx}helpplug PLUGIN".to_owned(),
            "Lists all available commands, including usage information, provided by the given plugin.".to_owned(),
        )).await;
        my_interface.register_channel_command(&CommandDefinition::new(
            "usage".to_owned(),
            "help".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            "{cpfx}usage COMMAND".to_owned(),
            "Shows usage information for the given command.".to_owned(),
        )).await;

        HelpPlugin {
            interface,
        }
    }

    async fn plugin_name(&self) -> String {
        "help".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let channel_name = &channel_message.channel.name;
        let command_config = interface.get_command_configuration().await;

        if command.name == "help" {
            let target_command_name = command.rest.trim();
            if target_command_name.len() > 0 {
                let help = interface.get_command_help(target_command_name).await;
                if let Some(h) = &help {
                    let h_replaced = replace_config_placeholders(&h, target_command_name, &command_config);
                    interface.send_channel_message(channel_name, &h_replaced).await;
                }

                // output nothing if the command is not known
                // (it might be handled by a different bot)
            } else {
                let all_usages = match self.get_command_usages(&command_config, None).await {
                    None => return,
                    Some(au) => au,
                };

                if let Some(max_len) = interface.get_maximum_message_length().await {
                    if all_usages.len() > max_len {
                        // full list is too long
                        let mut plugin_names: Vec<String> = interface.get_plugin_names().await
                            .iter()
                            .map(|pn| format!("`{}`", pn))
                            .collect();
                        plugin_names.sort_unstable();
                        let plugin_name_string = plugin_names.join(", ");

                        let help_message = format!(
                            "The exhaustive list of commands is too long for this server. Please request help information per plugin using `!helpplug PLUGIN`. Available plugins are: {}",
                            plugin_name_string,
                        );

                        interface.send_channel_message(channel_name, &help_message).await;
                        return;
                    }
                }

                interface.send_channel_message(channel_name, &all_usages).await;
            }
        } else if command.name == "helpplug" {
            let command_config = interface.get_command_configuration().await;
            let plugin_name = &command.rest;
            let all_usages = match self.get_command_usages(&command_config, Some(plugin_name.as_str())).await {
                None => return,
                Some(au) => au,
            };
            if all_usages.len() > 0 {
                interface.send_channel_message(channel_name, &all_usages).await;
            }
        } else if command.name == "usage" {
            let target_command_name = command.rest.trim();

            // try to find it as a regular command
            let mut usage: Option<String> = None;
            let mut description: Option<String> = None;

            for defn in &interface.get_defined_commands(None).await {
                if defn.name == target_command_name {
                    usage = Some(replace_config_placeholders(&defn.usage, &defn.name, &command_config));
                    description = Some(replace_config_placeholders(&defn.description, &defn.name, &command_config));
                    break;
                }
            }

            if usage.is_none() || description.is_none() {
                let additionals = interface.get_additional_commands_usages(None).await;
                if let Some((u, d)) = additionals.get(target_command_name) {
                    usage = Some(replace_config_placeholders(&u, &target_command_name, &command_config));
                    description = Some(replace_config_placeholders(&d, &target_command_name, &command_config));
                }
            }

            if !usage.is_none() && !description.is_none() {
                interface.send_channel_message(
                    channel_name,
                    &format!("`{}` \u{2013} {}", usage.unwrap(), description.unwrap()),
                ).await;
            }
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "help" {
            Some(include_str!("../help/help.md").to_owned())
        } else if command_name == "usage" {
            Some(include_str!("../help/usage.md").to_owned())
        } else {
            None
        }
    }
}
