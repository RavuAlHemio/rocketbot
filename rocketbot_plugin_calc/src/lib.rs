mod ast;
mod grimoire;
mod parsing;


use std::collections::{HashMap, HashSet};
use std::sync::Weak;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use json::JsonValue;
use log::error;
use rocketbot_interface::commands::{CommandDefinition, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;

use crate::ast::{AstNode, SimplificationState};
use crate::grimoire::{get_canonical_constants, get_canonical_functions};
use crate::parsing::parse_full_expression;


pub struct CalcPlugin {
    interface: Weak<dyn RocketBotInterface>,
    timeout_seconds: f64,
    max_result_string_length: usize,
}
impl CalcPlugin {
    async fn handle_calc(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let channel_name = &channel_message.channel.name;
        let sender_username = &channel_message.message.sender.username;

        let top_node = match parse_full_expression(&command.rest) {
            Ok(node) => node,
            Err(e) => {
                interface.send_channel_message(
                    channel_name,
                    &format!("@{} Failed to parse message:\n```\n{}\n```", sender_username, e),
                ).await;
                return;
            },
        };

        let simplified_res = {
            let mut state = SimplificationState {
                constants: get_canonical_constants(),
                functions: get_canonical_functions(),
                start_time: Instant::now(),
                timeout: Duration::from_secs_f64(self.timeout_seconds),
            };
            top_node.simplify(&mut state)
        };
        match simplified_res {
            Ok(ntn) => {
                let result_string = match &ntn.node {
                    AstNode::Int(i) => {
                        i.to_string()
                    },
                    AstNode::Float(f) => {
                        f.to_string()
                    },
                    other => {
                        error!("simplification produced invalid value: {:?}", other);
                        interface.send_channel_message(
                            channel_name,
                            &format!("@{} Simplification produced an invalid value!", sender_username),
                        ).await;
                        return;
                    },
                };

                if result_string.len() > self.max_result_string_length {
                    interface.send_channel_message(
                        channel_name,
                        &format!("@{} NUMBER TOO GROSS", sender_username),
                    ).await;
                }

                interface.send_channel_message(
                    channel_name,
                    &format!("@{} {}", sender_username, result_string),
                ).await;
            },
            Err(e) => {
                interface.send_channel_message(
                    channel_name,
                    &format!("@{} Simplification failed: {}", sender_username, e),
                ).await;
            },
        };
    }
}
#[async_trait]
impl RocketBotPlugin for CalcPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: JsonValue) -> CalcPlugin {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let timeout_seconds = config["timeout_seconds"].as_f64()
            .expect("timeout_seconds missing or not representable as f64");
        let max_result_string_length = config["max_result_string_length"].as_usize()
            .expect("max_result_string_length missing or not representable as usize");

        my_interface.register_channel_command(&CommandDefinition::new(
            "calc".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            "{cpfx}calc EXPRESSION".to_owned(),
            "Calculates the given mathematical expression and outputs the result.".to_owned(),
        )).await;

        CalcPlugin {
            interface,
            timeout_seconds,
            max_result_string_length,
        }
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "calc" {
            self.handle_calc(channel_message, command).await
        }
    }
}
