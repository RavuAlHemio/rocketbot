use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;
use std::sync::Weak;

use async_trait::async_trait;
use chrono::{Datelike, Local};
use json::JsonValue;
use num_bigint::BigInt;
use once_cell::sync::Lazy;
use rand::{Rng, SeedableRng};
use rand::seq::SliceRandom;
use rand::rngs::StdRng;
use regex::{Captures, Regex};
use rocketbot_interface::commands::{CommandDefinition, CommandInstance, CommandValueType};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::Mutex;


static ROLL_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "(?i)",
    "(?P<dice>[1-9][0-9]*)?",
    "d",
    "(?P<sides>[1-9][0-9]*)",
    "(?:[*](?P<mul_value>[+-]?[1-9][0-9]*))?",
    "(?P<add_value>[+-][1-9][0-9]*)?",
)).expect("failed to compile roll regex"));

static SEPARATOR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(
    "(?:[,]|\\s)+"
).expect("failed to compile separator regex"));


fn strings_from_json<T: FromIterator<String>>(json: &JsonValue) -> T {
    if json.is_null() {
        std::iter::empty()
            .collect()
    } else {
        json.members()
            .filter_map(|m| m.as_str().map(|s| s.to_owned()))
            .collect()
    }
}
fn string_from_json(json: &JsonValue, default: &str) -> String {
    json.as_str().unwrap_or(default).to_owned()
}
fn u8_from_json(json: &JsonValue, default: u8) -> u8 {
    json.as_u8().unwrap_or(default)
}
fn u64_from_json(json: &JsonValue, default: u64) -> u64 {
    json.as_u64().unwrap_or(default)
}
fn opt_u64_from_json(upper_json: &JsonValue, key: &str, default: Option<u64>) -> Option<u64> {
    if !upper_json.has_key(key) {
        default
    } else {
        let json = &upper_json[key];
        if json.is_null() {
            None
        } else if let Some(n) = json.as_u64() {
            Some(n)
        } else {
            default
        }
    }
}
fn usize_from_json(json: &JsonValue, default: usize) -> usize {
    json.as_usize().unwrap_or(default)
}


#[derive(Clone, Debug, Eq, PartialEq)]
struct DiceConfig {
    pub channels: HashSet<String>,
    pub obstinate_answers: Vec<String>,
    pub yes_no_answers: Vec<String>,
    pub decision_splitters: Vec<String>,
    pub special_decision_answers: Vec<String>,
    pub cooldown_answers: Vec<String>,
    pub special_decision_answer_percent: u8,
    pub max_roll_count: usize,
    pub max_dice_count: usize,
    pub max_side_count: u64,
    pub cooldown_per_command_usage: u64,
    pub cooldown_upper_boundary: Option<u64>,
    pub default_wikipedia_language: String,
}
impl From<&JsonValue> for DiceConfig {
    fn from(jv: &JsonValue) -> Self {
        let channels: HashSet<String> = strings_from_json(&jv["channels"]);
        let obstinate_answers: Vec<String> = strings_from_json(&jv["obstinate_answers"]);
        let yes_no_answers: Vec<String> = strings_from_json(&jv["yes_no_answers"]);
        let decision_splitters: Vec<String> = strings_from_json(&jv["decision_splitters"]);
        let special_decision_answers: Vec<String> = strings_from_json(&jv["special_decision_answers"]);
        let cooldown_answers: Vec<String> = strings_from_json(&jv["cooldown_answers"]);
        let special_decision_answer_percent = u8_from_json(&jv["special_decision_answer_percent"], 10);
        let max_roll_count = usize_from_json(&jv["max_roll_count"], 16);
        let max_dice_count = usize_from_json(&jv["max_dice_count"], 1024);
        let max_side_count = u64_from_json(&jv["max_side_count"], 1048576);
        let cooldown_per_command_usage = u64_from_json(&jv["cooldown_per_command_usage"], 4);
        let cooldown_upper_boundary = opt_u64_from_json(&jv, "cooldown_upper_boundary", Some(32));
        let default_wikipedia_language = string_from_json(&jv["default_wikipedia_language"], "en");

        DiceConfig {
            channels,
            obstinate_answers,
            yes_no_answers,
            decision_splitters,
            special_decision_answers,
            cooldown_answers,
            special_decision_answer_percent,
            max_roll_count,
            max_dice_count,
            max_side_count,
            cooldown_per_command_usage,
            cooldown_upper_boundary,
            default_wikipedia_language,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CooldownState {
    pub cooldown_value: u64,
    pub cooldown_triggered: bool,
}
impl CooldownState {
    pub fn new(
        cooldown_value: u64,
        cooldown_triggered: bool,
    ) -> CooldownState {
        CooldownState {
            cooldown_value,
            cooldown_triggered,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct DiceGroup {
    pub die_count: usize,
    pub side_count: u64,
    pub multiply_value: i64,
    pub add_value: i64,
}
impl DiceGroup {
    pub fn new(
        die_count: usize,
        side_count: u64,
        multiply_value: i64,
        add_value: i64,
    ) -> DiceGroup {
        DiceGroup {
            die_count,
            side_count,
            multiply_value,
            add_value,
        }
    }
}


pub struct DicePlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: DiceConfig,
    rng: Mutex<StdRng>,
    channel_name_to_cooldown_state: Mutex<HashMap<String, CooldownState>>,
}
impl DicePlugin {
    async fn obtain_dice_group(&self, roll_match_captures: &Captures<'_>, channel_name: &str, sender_username: &str) -> Option<DiceGroup> {
        let interface = match self.interface.upgrade() {
            None => return None,
            Some(i) => i,
        };

        let die_count: Option<usize> = roll_match_captures.name("dice")
            .map(|dcm| dcm.as_str())
            .or(Some("1"))
            .and_then(|dcs| dcs.parse().ok())
            .and_then(|dc| if dc > self.config.max_dice_count { None } else { Some(dc) });
        let side_count: Option<u64> = roll_match_captures.name("sides")
            .and_then(|sm| sm.as_str().parse().ok())
            .and_then(|s| if s > self.config.max_side_count { None } else { Some(s) });
        let multiply_value: Option<i64> = roll_match_captures.name("mul_value")
            .map(|mm| mm.as_str())
            .or(Some("1"))
            .and_then(|ms| ms.parse().ok());
        let add_value: Option<i64> = roll_match_captures.name("add_value")
            .map(|am| am.as_str())
            .or(Some("0"))
            .and_then(|as_| as_.parse().ok());

        if die_count.is_none() {
            interface.send_channel_message(
                channel_name,
                &format!("@{} Too many dice.", sender_username),
            ).await;
            return None;
        }
        if side_count.is_none() {
            interface.send_channel_message(
                channel_name,
                &format!("@{} Too many sides.", sender_username),
            ).await;
            return None;
        }
        if multiply_value.is_none() {
            interface.send_channel_message(
                channel_name,
                &format!("@{} Value to multiply too large.", sender_username),
            ).await;
            return None;
        }
        if add_value.is_none() {
            interface.send_channel_message(
                channel_name,
                &format!("@{} Value to add too large.", sender_username),
            ).await;
            return None;
        }
        Some(DiceGroup::new(
            die_count.unwrap(),
            side_count.unwrap(),
            multiply_value.unwrap(),
            add_value.unwrap(),
        ))
    }

    async fn handle_roll(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };
        let channel_name = &channel_message.channel.name;
        let sender_username = &channel_message.message.sender.username;

        let rolls = SEPARATOR_REGEX.split(&command.rest);
        let mut dice_groups = Vec::new();
        for roll in rolls {
            let roll_match_captures = match ROLL_REGEX.captures(roll) {
                Some(rmc) => rmc,
                None => {
                    interface.send_channel_message(
                        channel_name,
                        &format!("@{} Failed to parse roll {:?}", sender_username, roll),
                    ).await;
                    return;
                },
            };
            let dice_group = match self.obtain_dice_group(&roll_match_captures, channel_name, sender_username).await {
                Some(dg) => dg,
                None => {
                    // error message already output; bail out
                    return;
                },
            };
            dice_groups.push(dice_group);
        }

        if dice_groups.len() > self.config.max_roll_count {
            interface.send_channel_message(
                channel_name,
                &format!("@{} Too many rolls.", sender_username),
            ).await;
            return;
        }

        // special-case 2d1
        if
            dice_groups.len() == 1
            && dice_groups[0].die_count == 2
            && dice_groups[0].side_count == 1
            && dice_groups[0].multiply_value == 1
            && dice_groups[0].add_value == 0
        {
            interface.send_channel_message(
                channel_name,
                "_rolls its eyes_",
            ).await;
            return;
        }

        let mut all_rolls = Vec::with_capacity(dice_groups.len());

        {
            let mut rng_guard = self.rng.lock().await;
            for dice_group in &dice_groups {
                let mut these_rolls = Vec::with_capacity(dice_group.die_count);
                for _ in 0..dice_group.die_count {
                    if dice_group.side_count == 1 && self.config.obstinate_answers.len() > 0 {
                        // special case: give an obstinate answer instead
                        // since a 1-sided toss has an obvious result
                        let obstinate_answer = self.config.obstinate_answers
                            .choose(&mut *rng_guard).unwrap();
                        these_rolls.push(obstinate_answer.clone());
                    } else {
                        let mut roll = BigInt::from(rng_guard.gen_range(0..dice_group.side_count));
                        roll += 1; // 6-sided dice are normally numbered 1..=6, not 0..=5
                        roll *= dice_group.multiply_value;
                        roll += dice_group.add_value;
                        these_rolls.push(roll.to_string());
                    }
                }
                all_rolls.push(these_rolls.join(" "));
            }
        }

        let all_rolls_string = format!(
            "@{} {}",
            sender_username,
            all_rolls.join("; "),
        );
        interface.send_channel_message(
            channel_name,
            &all_rolls_string,
        ).await;
    }

    async fn is_on_cooldown(&self, sender_username: &str, channel_name: &str) -> bool {
        let interface = match self.interface.upgrade() {
            None => return false,
            Some(i) => i,
        };

        if self.config.cooldown_upper_boundary.is_none() {
            // the cooldown feature is not being used
            return false;
        }

        let mut rng_guard = self.rng.lock().await;
        let mut cooldown_guard = self.channel_name_to_cooldown_state.lock().await;
        let cooldown_state = cooldown_guard.entry(channel_name.to_string())
            .or_insert_with(|| CooldownState::new(0, false));

        cooldown_state.cooldown_value += self.config.cooldown_per_command_usage;

        let cooling_down = if cooldown_state.cooldown_triggered {
            cooldown_state.cooldown_value > 0
        } else {
            cooldown_state.cooldown_value > self.config.cooldown_upper_boundary.unwrap()
        };

        if cooling_down {
            cooldown_state.cooldown_triggered = true;
            if let Some(cooldown_answer) = self.config.cooldown_answers.choose(&mut *rng_guard) {
                interface.send_channel_message(
                    channel_name,
                    &format!("@{} {}", sender_username, cooldown_answer),
                ).await;
            }
            true
        } else {
            false
        }
    }

    async fn handle_yes_no(&self, channel_message: &ChannelMessage, _command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let sender_username = &channel_message.message.sender.username;
        let channel_name = &channel_message.channel.name;

        if self.is_on_cooldown(sender_username, channel_name).await {
            return;
        }

        let mut rng_guard = self.rng.lock().await;
        let yes_no_answer = self.config.yes_no_answers.choose(&mut *rng_guard);
        if let Some(yna) = yes_no_answer {
            interface.send_channel_message(
                channel_name,
                &format!("@{} {}", sender_username, yna),
            ).await;
        }
    }

    async fn handle_decide(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let sender_username = &channel_message.message.sender.username;
        let channel_name = &channel_message.channel.name;
        let decision_string = &command.rest;

        let splitter_opt = self.config.decision_splitters.iter()
            .filter(|s| decision_string.contains(*s))
            .nth(0);
        let splitter = match splitter_opt {
            None => {
                interface.send_channel_message(
                    channel_name,
                    &format!("@{} Uhh... that looks like only one option to choose from.", sender_username),
                ).await;
                return;
            },
            Some(s) => s,
        };

        let mut rng_guard = self.rng.lock().await;
        if self.config.special_decision_answers.len() > 0 {
            let percent = rng_guard.gen_range(0..100);
            if percent < self.config.special_decision_answer_percent {
                // special answer instead!
                let special_answer = self.config.special_decision_answers.choose(&mut *rng_guard);
                if let Some(sa) = special_answer {
                    interface.send_channel_message(
                        channel_name,
                        &format!("@{} {}", sender_username, sa),
                    ).await;
                }
                return;
            }
        }

        let options: Vec<&str> = decision_string.split(splitter).collect();
        if let Some(option) = options.choose(&mut *rng_guard) {
            interface.send_channel_message(
                channel_name,
                &format!("@{} {}", sender_username, option),
            ).await;
        }
    }

    async fn handle_shuffle(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let sender_username = &channel_message.message.sender.username;
        let channel_name = &channel_message.channel.name;
        let decision_string = &command.rest;

        let splitter_opt = self.config.decision_splitters.iter()
            .filter(|s| decision_string.contains(*s))
            .nth(0);
        let splitter = match splitter_opt {
            None => {
                interface.send_channel_message(
                    channel_name,
                    &format!("@{} Uhh... that looks like only one option to shuffle from.", sender_username),
                ).await;
                return;
            },
            Some(s) => s,
        };

        let mut rng_guard = self.rng.lock().await;
        let mut options: Vec<&str> = decision_string.split(splitter).collect();
        options.shuffle(&mut *rng_guard);
        let new_string = options.join(splitter);
        interface.send_channel_message(
            channel_name,
            &format!("@{} {}", sender_username, new_string),
        ).await;
    }

    async fn handle_some_year(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let sender_username = &channel_message.message.sender.username;
        let channel_name = &channel_message.channel.name;

        let wikipedia = command.options.iter()
            .filter(|(it, _cv)| *it == "w" || *it == "wikipedia")
            .map(|(_it, cv)| cv.as_str().unwrap().to_owned())
            .last()
            .unwrap_or(self.config.default_wikipedia_language.clone());

        let wikipedia_invalid = wikipedia
            .chars()
            .any(|c|
                (c < '0' || c > '9')
                && (c < 'a' || c > 'z')
                && (c != '-')
                && (c < 'A' || c > 'Z')
            );
        if wikipedia_invalid {
            interface.send_channel_message(
                channel_name,
                &format!("@{} That does not look like a valid Wikipedia to me.", sender_username),
            ).await;
            return;
        }

        let mut rng_guard = self.rng.lock().await;
        let current_year = Local::now().year();
        let year = rng_guard.gen_range(1..=current_year);
        interface.send_channel_message(
            channel_name,
            &format!("@{} https://{}.wikipedia.org/wiki/{}", sender_username, wikipedia, year),
        ).await;
    }
}
#[async_trait]
impl RocketBotPlugin for DicePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: JsonValue) -> DicePlugin {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let dice_config = DiceConfig::from(&config);
        let rng = Mutex::new(
            "DicePlugin::rng",
            StdRng::from_entropy(),
        );
        let channel_name_to_cooldown_state = Mutex::new(
            "DicePlugin::channel_name_to_cooldown_state",
            HashMap::new(),
        );

        let roll_command = CommandDefinition::new(
            "roll".to_string(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            "{cpfx}roll DICE [DICE ...]".to_owned(),
            "Rolls one or more dice.".to_owned(),
        );
        my_interface.register_channel_command(&roll_command).await;

        let yn_command = CommandDefinition::new(
            "yn".to_string(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            "{cpfx}yn [QUESTION]".to_owned(),
            "Helps you make a decision (or not) by answering a yes/no question.".to_owned(),
        );
        my_interface.register_channel_command(&yn_command).await;

        let decide_command = CommandDefinition::new(
            "decide".to_string(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            "{cpfx}decide OPTION or OPTION [or OPTION...]".to_owned(),
            "Helps you make a decision (or not) by choosing one of multiple options.".to_owned(),
        );
        my_interface.register_channel_command(&decide_command).await;

        let shuffle_command = CommandDefinition::new(
            "shuffle".to_string(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            "{cpfx}shuffle OPTION or OPTION [or OPTION...]".to_owned(),
            "Helps you prioritize by shuffling the options.".to_owned(),
        );
        my_interface.register_channel_command(&shuffle_command).await;

        let mut wikipedia_options = HashMap::new();
        wikipedia_options.insert("w".to_string(), CommandValueType::String);
        wikipedia_options.insert("wikipedia".to_string(), CommandValueType::String);
        let some_year_command = CommandDefinition::new(
            "someyear".to_string(),
            Some(HashSet::new()),
            wikipedia_options,
            0,
            "{cpfx}someyear [{lopfx}wikipedia WP]".to_owned(),
            "Selects a random year and links to its Wikipedia article.".to_owned(),
        );
        my_interface.register_channel_command(&some_year_command).await;

        DicePlugin {
            interface,
            config: dice_config,
            rng,
            channel_name_to_cooldown_state,
        }
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let channel_name = &channel_message.channel.name;

        if self.config.cooldown_upper_boundary.is_none() {
            // the cooldown feature is not being used
            return;
        }

        let mut cooldown_guard = self.channel_name_to_cooldown_state.lock().await;
        let cooldown_state = cooldown_guard.entry(channel_name.to_string())
            .or_insert_with(|| CooldownState::new(0, false));

        if cooldown_state.cooldown_value > 0 {
            cooldown_state.cooldown_value -= 1;
            if cooldown_state.cooldown_value == 0 {
                cooldown_state.cooldown_triggered = false;
            }
        }
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "roll" {
            self.handle_roll(channel_message, command).await;
        } else if command.name == "yn" {
            self.handle_yes_no(channel_message, command).await;
        } else if command.name == "decide" {
            self.handle_decide(channel_message, command).await;
        } else if command.name == "shuffle" {
            self.handle_shuffle(channel_message, command).await;
        } else if command.name == "someyear" {
            self.handle_some_year(channel_message, command).await;
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "roll" {
            Some(include_str!("../help/roll.md").to_owned())
        } else if command_name == "yn" {
            Some(include_str!("../help/yn.md").to_owned())
        } else if command_name == "decide" || command_name == "shuffle" {
            let separator_lines: String = self.config.decision_splitters.iter()
                .map(|ds| format!("* `{}`", ds))
                .collect::<Vec<String>>()
                .join("\n");

            let base_help = if command_name == "decide" {
                include_str!("../help/decide.md")
            } else if command_name == "shuffle" {
                include_str!("../help/shuffle.md")
            } else {
                unreachable!()
            };

            Some(base_help.replace("{separators}", &separator_lines))
        } else if command_name == "someyear" {
            Some(
                include_str!("../help/someyear.md")
                    .replace("{defwiki}", &self.config.default_wikipedia_language)
            )
        } else {
            None
        }
    }
}
