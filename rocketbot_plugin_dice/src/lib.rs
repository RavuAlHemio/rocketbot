use std::collections::{HashMap, HashSet};
use std::ops::DerefMut;
use std::sync::Weak;

use async_trait::async_trait;
use chrono::{Datelike, Local};
use log::error;
use num_bigint::BigInt;
use once_cell::sync::Lazy;
use rand::{Rng, SeedableRng};
use rand::distributions::Bernoulli;
use rand::seq::SliceRandom;
use rand::rngs::StdRng;
use regex::{Captures, Regex};
use rocketbot_interface::{ResultExtensions, send_channel_message};
use rocketbot_interface::commands::{
    CommandBehaviors, CommandDefinition, CommandDefinitionBuilder, CommandInstance,
    CommandValueType,
};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json;


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


#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct DiceConfig {
    #[serde(default)] pub channels: HashSet<String>,
    #[serde(default)] pub obstinate_answers: Vec<String>,
    #[serde(default)] pub yes_no_answers: Vec<String>,
    #[serde(default)] pub decision_splitters: Vec<String>,
    #[serde(default)] pub special_decision_answers: Vec<String>,
    #[serde(default)] pub cooldown_answers: Vec<String>,
    #[serde(default = "DiceConfig::default_special_decision_answer_percent")] pub special_decision_answer_percent: u8,
    #[serde(default = "DiceConfig::default_max_roll_count")] pub max_roll_count: usize,
    #[serde(default = "DiceConfig::default_max_dice_count")] pub max_dice_count: usize,
    #[serde(default = "DiceConfig::default_max_side_count")] pub max_side_count: u64,
    #[serde(default = "DiceConfig::default_cooldown_per_command_usage")] pub cooldown_per_command_usage: u64,
    #[serde(default = "DiceConfig::default_cooldown_upper_boundary")] pub cooldown_upper_boundary: Option<u64>,
    #[serde(default = "DiceConfig::default_default_wikipedia_language")] pub default_wikipedia_language: String,
}
impl DiceConfig {
    fn default_special_decision_answer_percent() -> u8 { 10 }
    fn default_max_roll_count() -> usize { 16 }
    fn default_max_dice_count() -> usize { 1024 }
    fn default_max_side_count() -> u64 { 1048576 }
    fn default_cooldown_per_command_usage() -> u64 { 4 }
    fn default_cooldown_upper_boundary() -> Option<u64> { Some(32) }
    fn default_default_wikipedia_language() -> String { "en".to_owned() }
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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum WordleRating {
    Wrong,
    Misplaced,
    Correct,
}
impl Default for WordleRating {
    fn default() -> Self { Self::Wrong }
}
impl WordleRating {
    pub fn as_char(&self, dark: bool, purple: bool) -> char {
        match self {
            Self::Wrong => if dark { '\u{2B1B}' } else { '\u{2B1C}' }, // black square, white square
            Self::Misplaced => if purple { '\u{1F7EA}' } else { '\u{1F7E8}' }, // purple square, yellow square
            Self::Correct => '\u{1F7E9}', // green square
        }
    }
}


pub struct DicePlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<DiceConfig>,
    rng: Mutex<StdRng>,
    channel_name_to_cooldown_state: Mutex<HashMap<String, CooldownState>>,
}
impl DicePlugin {
    async fn obtain_dice_group(&self, config: &DiceConfig, roll_match_captures: &Captures<'_>, channel_name: &str, sender_username: &str) -> Option<DiceGroup> {
        let interface = match self.interface.upgrade() {
            None => return None,
            Some(i) => i,
        };

        let die_count: Option<usize> = roll_match_captures.name("dice")
            .map(|dcm| dcm.as_str())
            .or(Some("1"))
            .and_then(|dcs| dcs.parse().ok())
            .and_then(|dc| if dc > config.max_dice_count { None } else { Some(dc) });
        let side_count: Option<u64> = roll_match_captures.name("sides")
            .and_then(|sm| sm.as_str().parse().ok())
            .and_then(|s| if s > config.max_side_count { None } else { Some(s) });
        let multiply_value: Option<i64> = roll_match_captures.name("mul_value")
            .map(|mm| mm.as_str())
            .or(Some("1"))
            .and_then(|ms| ms.parse().ok());
        let add_value: Option<i64> = roll_match_captures.name("add_value")
            .map(|am| am.as_str())
            .or(Some("0"))
            .and_then(|as_| as_.parse().ok());

        if die_count.is_none() {
            send_channel_message!(
                interface,
                channel_name,
                &format!("@{} Too many dice.", sender_username),
            ).await;
            return None;
        }
        if side_count.is_none() {
            send_channel_message!(
                interface,
                channel_name,
                &format!("@{} Too many sides.", sender_username),
            ).await;
            return None;
        }
        if multiply_value.is_none() {
            send_channel_message!(
                interface,
                channel_name,
                &format!("@{} Value to multiply too large.", sender_username),
            ).await;
            return None;
        }
        if add_value.is_none() {
            send_channel_message!(
                interface,
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

        let config_guard = self.config.read().await;

        let rolls = SEPARATOR_REGEX.split(&command.rest);
        let mut dice_groups = Vec::new();
        for roll in rolls {
            let roll_match_captures = match ROLL_REGEX.captures(roll) {
                Some(rmc) => rmc,
                None => {
                    send_channel_message!(
                        interface,
                        channel_name,
                        &format!("@{} Failed to parse roll {:?}", sender_username, roll),
                    ).await;
                    return;
                },
            };
            let dice_group = match self.obtain_dice_group(&config_guard, &roll_match_captures, channel_name, sender_username).await {
                Some(dg) => dg,
                None => {
                    // error message already output; bail out
                    return;
                },
            };
            dice_groups.push(dice_group);
        }

        if dice_groups.len() > config_guard.max_roll_count {
            send_channel_message!(
                interface,
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
            send_channel_message!(
                interface,
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
                    if dice_group.side_count == 1 && config_guard.obstinate_answers.len() > 0 {
                        // special case: give an obstinate answer instead
                        // since a 1-sided toss has an obvious result
                        let obstinate_answer = config_guard.obstinate_answers
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
        send_channel_message!(
            interface,
            channel_name,
            &all_rolls_string,
        ).await;
    }

    async fn is_on_cooldown(&self, config: &DiceConfig, sender_username: &str, channel_name: &str) -> bool {
        let interface = match self.interface.upgrade() {
            None => return false,
            Some(i) => i,
        };

        if config.cooldown_upper_boundary.is_none() {
            // the cooldown feature is not being used
            return false;
        }

        let mut rng_guard = self.rng.lock().await;
        let mut cooldown_guard = self.channel_name_to_cooldown_state.lock().await;
        let cooldown_state = cooldown_guard.entry(channel_name.to_string())
            .or_insert_with(|| CooldownState::new(0, false));

        cooldown_state.cooldown_value += config.cooldown_per_command_usage;

        let cooling_down = if cooldown_state.cooldown_triggered {
            cooldown_state.cooldown_value > 0
        } else {
            cooldown_state.cooldown_value > config.cooldown_upper_boundary.unwrap()
        };

        if cooling_down {
            cooldown_state.cooldown_triggered = true;
            if let Some(cooldown_answer) = config.cooldown_answers.choose(&mut *rng_guard) {
                send_channel_message!(
                    interface,
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

        let config_guard = self.config.read().await;

        if self.is_on_cooldown(&config_guard, sender_username, channel_name).await {
            return;
        }

        let mut rng_guard = self.rng.lock().await;
        let yes_no_answer = config_guard.yes_no_answers.choose(&mut *rng_guard);
        if let Some(yna) = yes_no_answer {
            send_channel_message!(
                interface,
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

        let config_guard = self.config.read().await;

        let splitter_opt = config_guard.decision_splitters.iter()
            .filter(|s| decision_string.contains(*s))
            .nth(0);
        let splitter = match splitter_opt {
            None => {
                send_channel_message!(
                    interface,
                    channel_name,
                    &format!("@{} Uhh... that looks like only one option to choose from.", sender_username),
                ).await;
                return;
            },
            Some(s) => s,
        };

        let mut rng_guard = self.rng.lock().await;
        if config_guard.special_decision_answers.len() > 0 {
            let percent = rng_guard.gen_range(0..100);
            if percent < config_guard.special_decision_answer_percent {
                // special answer instead!
                let special_answer = config_guard.special_decision_answers.choose(&mut *rng_guard);
                if let Some(sa) = special_answer {
                    send_channel_message!(
                        interface,
                        channel_name,
                        &format!("@{} {}", sender_username, sa),
                    ).await;
                }
                return;
            }
        }

        let options: Vec<&str> = decision_string.split(splitter).collect();
        if let Some(option) = options.choose(&mut *rng_guard) {
            send_channel_message!(
                interface,
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

        let config_guard = self.config.read().await;

        let splitter_opt = config_guard.decision_splitters.iter()
            .filter(|s| decision_string.contains(*s))
            .nth(0);
        let splitter = match splitter_opt {
            None => {
                send_channel_message!(
                    interface,
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
        send_channel_message!(
            interface,
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

        let config_guard = self.config.read().await;

        let wikipedia = command.options.iter()
            .filter(|(it, _cv)| *it == "w" || *it == "wikipedia")
            .map(|(_it, cv)| cv.as_str().unwrap().to_owned())
            .last()
            .unwrap_or(config_guard.default_wikipedia_language.clone());

        let wikipedia_invalid = wikipedia
            .chars()
            .any(|c|
                (c < '0' || c > '9')
                && (c < 'a' || c > 'z')
                && (c != '-')
                && (c < 'A' || c > 'Z')
            );
        if wikipedia_invalid {
            send_channel_message!(
                interface,
                channel_name,
                &format!("@{} That does not look like a valid Wikipedia to me.", sender_username),
            ).await;
            return;
        }

        let mut rng_guard = self.rng.lock().await;
        let current_year = Local::now().year();
        let year = rng_guard.gen_range(1..=current_year);
        send_channel_message!(
            interface,
            channel_name,
            &format!("@{} https://{}.wikipedia.org/wiki/{}", sender_username, wikipedia, year),
        ).await;
    }

    async fn handle_randwordle(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let dark_mode = command.flags.contains("d") || command.flags.contains("dark");
        let purple_mode = command.flags.contains("p") || command.flags.contains("purple");
        let square_count_i64 = command.options.get("squares")
            .or_else(|| command.options.get("s"))
            .map(|opt| opt.as_i64().unwrap())
            .unwrap_or(5);
        let max_length_i64 = command.options.get("length")
            .or_else(|| command.options.get("l"))
            .map(|opt| opt.as_i64().unwrap())
            .unwrap_or(6);

        let square_count: usize = match square_count_i64.try_into() {
            Ok(n) => n,
            Err(_) => return,
        };
        let max_length: usize = match max_length_i64.try_into() {
            Ok(n) => n,
            Err(_) => return,
        };

        let mut guesses: Vec<Vec<WordleRating>> = Vec::with_capacity(max_length);

        {
            let mut rng_guard = self.rng.lock().await;
            let half_boolean = Bernoulli::new(0.5)
                .expect("failed to create half Bernoulli distribution");
            let quarter_boolean = Bernoulli::new(0.25)
                .expect("failed to create quarter Bernoulli distribution");
            let three_quarters_boolean = Bernoulli::new(0.75)
                .expect("failed to create three-quarters Bernoulli distribution");

            // start with a pretty shitty first guess
            let mut current_guess = vec![WordleRating::Wrong; square_count];
            while rng_guard.sample(&half_boolean) {
                let square_index = rng_guard.gen_range(0..square_count);
                let make_correct = rng_guard.sample(&quarter_boolean);
                current_guess[square_index] = if make_correct { WordleRating::Correct } else { WordleRating::Misplaced };
            }
            guesses.push(current_guess.clone());

            loop {
                if guesses.len() >= max_length {
                    break;
                }

                if current_guess.iter().all(|g| *g == WordleRating::Correct) {
                    // we won!
                    break;
                }

                // improve the guess
                let mut victory = false;
                while rng_guard.sample(&three_quarters_boolean) {
                    // pick a wrong or misplaced guess
                    let imperfect_guesses: Vec<usize> = current_guess.iter().enumerate()
                        .filter(|(_i, g)| **g != WordleRating::Correct)
                        .map(|(i, _g)| i)
                        .collect();
                    let imperfect_index = *imperfect_guesses.choose(rng_guard.deref_mut())
                        .expect("no imperfect guess found");

                    // decide on its improvement
                    if current_guess[imperfect_index] == WordleRating::Misplaced {
                        // improve misplaced to correct
                        current_guess[imperfect_index] = WordleRating::Correct;
                    } else {
                        // improve wrong to either misplaced (more likely) or correct (less likely)
                        assert_eq!(current_guess[imperfect_index], WordleRating::Wrong);
                        let make_correct = rng_guard.sample(&quarter_boolean);
                        current_guess[imperfect_index] = if make_correct { WordleRating::Correct } else { WordleRating::Misplaced };
                    }

                    if current_guess.iter().all(|g| *g == WordleRating::Correct) {
                        // we won!
                        guesses.push(current_guess.clone());
                        victory = true;
                        break;
                    }
                }

                if victory {
                    break;
                }

                // shuffle some squares around
                while rng_guard.sample(&half_boolean) {
                    let from_square = rng_guard.gen_range(0..current_guess.len());
                    let to_square = rng_guard.gen_range(0..current_guess.len());
                    if from_square != to_square {
                        current_guess.swap(from_square, to_square);
                    }
                }

                // protect against absurd "one misplaced, all others correct" constellation
                // by just turning it green
                // (this also protects from "misplaced" when we only have one square per guess)
                let none_wrong = current_guess.iter()
                    .all(|g| *g != WordleRating::Wrong);
                if none_wrong {
                    let misplaced_indexes: Vec<usize> = current_guess.iter().enumerate()
                        .filter(|(_i, g)| **g == WordleRating::Misplaced)
                        .map(|(i, _g)| i)
                        .collect();
                    if misplaced_indexes.len() == 1 {
                        current_guess[misplaced_indexes[0]] = WordleRating::Correct;
                    }
                }

                // remember this
                guesses.push(current_guess.clone());
            }
        }

        // output the guesses
        let mut output = String::new();
        for guess in &guesses {
            if output.len() > 0 {
                output.push('\n');
            }
            for rating in guess {
                output.push(rating.as_char(dark_mode, purple_mode));
            }
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &output,
        ).await;
    }

    fn try_get_config(config: serde_json::Value) -> Result<DiceConfig, &'static str> {
        serde_json::from_value(config)
            .or_msg("failed to load config")
    }
}
#[async_trait]
impl RocketBotPlugin for DicePlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> DicePlugin {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "DicePlugin::config",
            config_object,
        );

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
            "dice".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}roll DICE [DICE ...]".to_owned(),
            "Rolls one or more dice.".to_owned(),
        );
        my_interface.register_channel_command(&roll_command).await;

        let yn_command = CommandDefinition::new(
            "yn".to_string(),
            "dice".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}yn [QUESTION]".to_owned(),
            "Helps you make a decision (or not) by answering a yes/no question.".to_owned(),
        );
        my_interface.register_channel_command(&yn_command).await;

        let decide_command = CommandDefinition::new(
            "decide".to_string(),
            "dice".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}decide OPTION or OPTION [or OPTION...]".to_owned(),
            "Helps you make a decision (or not) by choosing one of multiple options.".to_owned(),
        );
        my_interface.register_channel_command(&decide_command).await;

        let shuffle_command = CommandDefinition::new(
            "shuffle".to_string(),
            "dice".to_owned(),
            Some(HashSet::new()),
            HashMap::new(),
            0,
            CommandBehaviors::empty(),
            "{cpfx}shuffle OPTION or OPTION [or OPTION...]".to_owned(),
            "Helps you prioritize by shuffling the options.".to_owned(),
        );
        my_interface.register_channel_command(&shuffle_command).await;

        let mut wikipedia_options = HashMap::new();
        wikipedia_options.insert("w".to_string(), CommandValueType::String);
        wikipedia_options.insert("wikipedia".to_string(), CommandValueType::String);
        let some_year_command = CommandDefinition::new(
            "someyear".to_string(),
            "dice".to_owned(),
            Some(HashSet::new()),
            wikipedia_options,
            0,
            CommandBehaviors::empty(),
            "{cpfx}someyear [{lopfx}wikipedia WP]".to_owned(),
            "Selects a random year and links to its Wikipedia article.".to_owned(),
        );
        my_interface.register_channel_command(&some_year_command).await;

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "randwordle".to_owned(),
                "dice".to_owned(),
                "{cpfx}randwordle [OPTIONS]".to_owned(),
                "Generates a random Wordle solution pattern.".to_owned(),
            )
                .add_option("s", CommandValueType::Integer)
                .add_option("squares", CommandValueType::Integer)
                .add_option("l", CommandValueType::Integer)
                .add_option("length", CommandValueType::Integer)
                .add_flag("d").add_flag("dark")
                .add_flag("p").add_flag("purple")
                .build()
        ).await;

        DicePlugin {
            interface,
            config: config_lock,
            rng,
            channel_name_to_cooldown_state,
        }
    }

    async fn plugin_name(&self) -> String {
        "dice".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let channel_name = &channel_message.channel.name;

        let config_guard = self.config.read().await;

        if config_guard.cooldown_upper_boundary.is_none() {
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
        } else if command.name == "randwordle" {
            self.handle_randwordle(channel_message, command).await;
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "roll" {
            Some(include_str!("../help/roll.md").to_owned())
        } else if command_name == "yn" {
            Some(include_str!("../help/yn.md").to_owned())
        } else if command_name == "decide" || command_name == "shuffle" {
            let config_guard = self.config.read().await;
            let separator_lines: String = config_guard.decision_splitters.iter()
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
            let config_guard = self.config.read().await;
            Some(
                include_str!("../help/someyear.md")
                    .replace("{defwiki}", &config_guard.default_wikipedia_language)
            )
        } else if command_name == "randwordle" {
            Some(include_str!("../help/randwordle.md").to_owned())
        } else {
            None
        }
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
