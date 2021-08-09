use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use rocketbot_interface::commands::{
    CommandBehaviors, CommandConfiguration, CommandDefinition, CommandInstance,
};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{ChannelMessage, PrivateMessage};
use serde_json;


fn replace_config_placeholders(original: &str, command_name: &str, command_config: &CommandConfiguration) -> String {
    original
        .replace("{cmd}", command_name)
        .replace("{cpfx}", &command_config.command_prefix)
        .replace("{sopfx}", &command_config.short_option_prefix)
        .replace("{lopfx}", &command_config.long_option_prefix)
}


async fn respond_channel_message<'a>(interface: Arc<dyn RocketBotInterface + 'a>, orig_message: &ChannelMessage, response: &str) {
    interface.send_channel_message(&orig_message.channel.name, &response).await
}

async fn respond_private_message<'a>(interface: Arc<dyn RocketBotInterface + 'a>, orig_message: &PrivateMessage, response: &str) {
    interface.send_private_message(&orig_message.conversation.id, &response).await
}

fn collect_command_usages(
    command_config: &CommandConfiguration,
    defined_commands: &[CommandDefinition],
    additional_commands: &HashMap<String, (String, String)>,
) -> Option<String> {
    let mut usage_to_descr: BTreeMap<String, (String, String)> = BTreeMap::new();

    // get all regular commands and usages
    for defn in defined_commands {
        usage_to_descr.insert(
            defn.name.clone(),
            (
                replace_config_placeholders(&defn.usage, &defn.name, &command_config),
                replace_config_placeholders(&defn.description, &defn.name, &command_config),
            ),
        );
    }

    // get special commands and usages
    for (name, (usage, description)) in additional_commands {
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


macro_rules! handle_command_func {
    (
        $func_name:ident,
        $message_name:ident,
        $message_type:ty,
        $respond_func_name:ident,
        $get_def_cmds_func_name:ident,
        $get_add_cmds_usages_func_name:ident,
    ) => {
        async fn $func_name(&self, $message_name: &$message_type, command: &CommandInstance) {
            let interface = match self.interface.upgrade() {
                None => return,
                Some(i) => i,
            };

            let command_config = interface.get_command_configuration().await;

            if command.name == "help" {
                let target_command_name = command.rest.trim();
                if target_command_name.len() > 0 {
                    let help = interface.get_command_help(target_command_name).await;
                    if let Some(h) = &help {
                        let h_replaced = replace_config_placeholders(&h, target_command_name, &command_config);
                        $respond_func_name(Arc::clone(&interface), $message_name, &h_replaced).await;
                    }

                    // output nothing if the command is not known
                    // (it might be handled by a different bot)
                } else {
                    let defined_commands = interface.$get_def_cmds_func_name(None).await;
                    let additional_usages = interface.$get_add_cmds_usages_func_name(None).await;
                    let all_usages = match collect_command_usages(&command_config, &defined_commands, &additional_usages) {
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

                            $respond_func_name(Arc::clone(&interface), $message_name, &help_message).await;
                            return;
                        }
                    }

                    $respond_func_name(Arc::clone(&interface), $message_name, &all_usages).await;
                }
            } else if command.name == "helpplug" {
                let command_config = interface.get_command_configuration().await;
                let plugin_name = &command.rest;
                let defined_commands = interface.$get_def_cmds_func_name(Some(plugin_name.as_str())).await;
                let additional_usages = interface.$get_add_cmds_usages_func_name(Some(plugin_name.as_str())).await;
                let plugin_usages = match collect_command_usages(&command_config, &defined_commands, &additional_usages) {
                    None => return,
                    Some(au) => au,
                };
                if plugin_usages.len() > 0 {
                    $respond_func_name(Arc::clone(&interface), $message_name, &plugin_usages).await;
                }
            } else if command.name == "usage" {
                let target_command_name = command.rest.trim();

                // try to find it as a regular command
                let mut usage: Option<String> = None;
                let mut description: Option<String> = None;

                for defn in interface.$get_def_cmds_func_name(None).await {
                    if defn.name == target_command_name {
                        usage = Some(replace_config_placeholders(&defn.usage, &defn.name, &command_config));
                        description = Some(replace_config_placeholders(&defn.description, &defn.name, &command_config));
                        break;
                    }
                }

                if usage.is_none() || description.is_none() {
                    let additionals = interface.$get_add_cmds_usages_func_name(None).await;
                    if let Some((u, d)) = additionals.get(target_command_name) {
                        usage = Some(replace_config_placeholders(&u, &target_command_name, &command_config));
                        description = Some(replace_config_placeholders(&d, &target_command_name, &command_config));
                    }
                }

                if !usage.is_none() && !description.is_none() {
                    $respond_func_name(
                        Arc::clone(&interface),
                        $message_name,
                        &format!("`{}` \u{2013} {}", usage.unwrap(), description.unwrap()),
                    ).await;
                }
            }
        }
    };
}


pub struct HelpPlugin {
    interface: Weak<dyn RocketBotInterface>,
}
impl HelpPlugin {
    handle_command_func!(
        handle_channel_command,
        channel_message,
        ChannelMessage,
        respond_channel_message,
        get_defined_channel_commands,
        get_additional_channel_commands_usages,
    );
    handle_command_func!(
        handle_private_command,
        private_message,
        PrivateMessage,
        respond_private_message,
        get_defined_private_message_commands,
        get_additional_private_message_commands_usages,
    );
}
#[async_trait]
impl RocketBotPlugin for HelpPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let help_command = CommandDefinition::new(
            "help".to_owned(),
            "help".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}help [COMMAND]".to_owned(),
            "Shows help about the given command, or lists all available commands.".to_owned(),
        );
        my_interface.register_channel_command(&help_command).await;
        my_interface.register_private_message_command(&help_command).await;

        let helpplug_command = CommandDefinition::new(
            "helpplug".to_owned(),
            "help".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}helpplug PLUGIN".to_owned(),
            "Lists all available commands, including usage information, provided by the given plugin.".to_owned(),
        );
        my_interface.register_channel_command(&helpplug_command).await;
        my_interface.register_private_message_command(&helpplug_command).await;

        let usage_command = CommandDefinition::new(
            "usage".to_owned(),
            "help".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}usage COMMAND".to_owned(),
            "Shows usage information for the given command.".to_owned(),
        );
        my_interface.register_channel_command(&usage_command).await;
        my_interface.register_private_message_command(&usage_command).await;

        HelpPlugin {
            interface,
        }
    }

    async fn plugin_name(&self) -> String {
        "help".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        self.handle_channel_command(channel_message, command).await
    }

    async fn private_command(&self, private_message: &PrivateMessage, command: &CommandInstance) {
        self.handle_private_command(private_message, command).await
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
