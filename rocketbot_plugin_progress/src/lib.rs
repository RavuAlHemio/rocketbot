use std::fmt::Write;
use std::sync::Weak;

use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance, CommandValueType};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;
use serde_json;
use tracing::error;


static PROGRESS_INDICATOR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "(?P<minus>-?)",
    "(?P<number>0|[1-9][0-9]?[0-9]?|200)",
    "%",
    "(?:",
        "(?P<start>\\S)",
        "(?P<end>\\S)?",
    ")?",
)).expect("failed to parse progress indicator regex"));


#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Config {
    bar_length: usize,
}


pub struct ProgressPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
}
impl ProgressPlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let bar_length = if config["bar_length"].is_null() {
            20
        } else {
            config["bar_length"].as_usize()
                .ok_or("bar_length not representable as a usize")?
        };

        Ok(Config {
            bar_length,
        })
    }
}
#[async_trait]
impl RocketBotPlugin for ProgressPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");
        let config_lock = RwLock::new(
            "ProgressPlugin::config",
            config_object,
        );

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "progress",
                "progress",
                "{cpfx}progress TEXT",
                "Annotates percentages in the text with progress bars.",
            )
                .add_option("f", CommandValueType::String)
                .add_option("foreground", CommandValueType::String)
                .add_option("b", CommandValueType::String)
                .add_option("background", CommandValueType::String)
                .add_option("s", CommandValueType::String)
                .add_option("start-bar", CommandValueType::String)
                .add_option("e", CommandValueType::String)
                .add_option("end-bar", CommandValueType::String)
                .add_option("S", CommandValueType::String)
                .add_option("start-box", CommandValueType::String)
                .add_option("E", CommandValueType::String)
                .add_option("end-box", CommandValueType::String)
                .build()
        ).await;

        Self {
            interface,
            config: config_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "progress".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name != "progress" {
            return;
        }
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let config_guard = self.config.read().await;

        let foreground = command.options.get("foreground")
            .or_else(|| command.options.get("f"))
            .map(|v| v.as_str())
            .flatten()
            .unwrap_or("=");
        let background = command.options.get("background")
            .or_else(|| command.options.get("b"))
            .map(|v| v.as_str())
            .flatten()
            .unwrap_or(" ");
        let start_bar = command.options.get("start-bar")
            .or_else(|| command.options.get("s"))
            .map(|v| v.as_str())
            .flatten()
            .unwrap_or("");
        let end_bar = command.options.get("end-bar")
            .or_else(|| command.options.get("e"))
            .map(|v| v.as_str())
            .flatten()
            .unwrap_or("");
        let start_box = command.options.get("start-box")
            .or_else(|| command.options.get("S"))
            .map(|v| v.as_str())
            .flatten()
            .unwrap_or("[");
        let end_box = command.options.get("end-box")
            .or_else(|| command.options.get("E"))
            .map(|v| v.as_str())
            .flatten()
            .unwrap_or("]");

        let replaced = PROGRESS_INDICATOR_RE.replace_all(
            &command.rest,
            |caps: &Captures| regex_replacement_func(
                caps, config_guard.bar_length,
                foreground, background, start_bar, end_bar, start_box, end_box,
            ),
        );
        if replaced != command.rest {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &replaced,
            ).await;
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "progress" {
            Some(include_str!("../help/progress.md").to_owned())
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


fn regex_replacement_func(
    caps: &Captures, bar_length: usize,
    foreground_str: &str, background_str: &str,
    default_start_bar: &str, default_end_bar: &str,
    start_box: &str, end_box: &str,
) -> String {
    let has_minus = caps
        .name("minus").expect("minus not captured")
        .as_str() == "-";

    let number: usize = caps
        .name("number").expect("number not captured")
        .as_str()
        .parse().expect("failed to parse number");

    let start_chars: Vec<char> = caps
        .name("start")
        .map(|s| s.as_str())
        .unwrap_or(default_start_bar)
        .chars()
        .collect();
    let end_chars: Vec<char> = caps
        .name("end")
        .map(|s| s.as_str())
        .unwrap_or(default_end_bar)
        .chars()
        .collect();

    fn s2v(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    let foreground_chars = s2v(foreground_str);
    let background_chars = s2v(background_str);
    let start_box_chars = s2v(start_box);
    let end_box_chars = s2v(end_box);

    let rendered_bar = progress_replace(
        bar_length,
        has_minus,
        number,
        &start_chars,
        &end_chars,
        &foreground_chars,
        &background_chars,
        &start_box_chars,
        &end_box_chars,
    );
    format!("`{}`", rendered_bar)
}

fn progress_replace(
    bar_length: usize,
    has_minus: bool,
    number: usize,
    start_str: &[char],
    end_str: &[char],
    foreground_str: &[char],
    background_str: &[char],
    start_box: &[char],
    end_box: &[char],
) -> String {
    let full_foreground = repeat_string(foreground_str, 2*bar_length);
    let full_background = repeat_string(background_str, bar_length);

    // calculate number of segments (before sign!)
    let segment_count = number * bar_length / 100;

    // assemble the segments
    let mut segs = Vec::new();
    if segment_count < start_str.len() + end_str.len() {
        segs.extend_from_slice(&full_foreground[0..segment_count]);
    } else {
        segs.extend_from_slice(start_str);
        segs.extend_from_slice(&full_foreground[0..segment_count-(start_str.len()+end_str.len())]);
        segs.extend_from_slice(end_str);
    }

    if has_minus {
        // segments come before the box
        let mut ret = String::new();
        ret.extend(segs);
        ret.extend(start_box);
        ret.extend(full_background);
        ret.extend(end_box);
        write!(&mut ret, " -{}%", number).expect("write failed");
        ret
    } else {
        // segments are inside and sometimes outside the box
        let seg_char_count = segs.len();
        let (segs_inside_box, segs_outside_box, bg_inside_box): (Vec<char>, Vec<char>, Vec<char>) = if seg_char_count <= bar_length {
            // bar fits inside box
            (
                segs,
                Vec::new(),
                full_background.iter().skip(seg_char_count).map(|c| *c).collect(),
            )
        } else {
            // bar juts outside of box
            (
                segs.iter().take(bar_length).map(|c| *c).collect(),
                segs.iter().skip(bar_length).map(|c| *c).collect(),
                Vec::new(),
            )
        };

        let mut ret = String::new();
        ret.extend(start_box);
        ret.extend(segs_inside_box);
        ret.extend(bg_inside_box);
        ret.extend(end_box);
        ret.extend(segs_outside_box);
        write!(&mut ret, " {}%", number).expect("write failed");
        ret
    }
}

fn repeat_string(string: &[char], length: usize) -> Vec<char> {
    let mut ret = Vec::new();
    while ret.len() < length {
        for &c in string {
            ret.push(c);
        }
    }
    ret.truncate(length);
    ret
}


#[cfg(test)]
mod tests {
    fn run_canonical_test(expected: &str, has_minus: bool, number: usize) {
        let obtained = super::progress_replace(
            20,
            has_minus,
            number,
            &[],
            &[],
            &['='],
            &[' '],
            &['['],
            &[']'],
        );
        assert_eq!(expected, obtained);
    }

    fn run_yolo_test(expected: &str, has_minus: bool, number: usize) {
        let obtained = super::progress_replace(
            50,
            has_minus,
            number,
            &['Y'],
            &['O', 'L', 'O'],
            &['r', 'o', 'f', 'l'],
            &['l', 'o', 'l'],
            &['>', '>', '>'],
            &['<', '<', '<'],
        );
        assert_eq!(expected, obtained);
    }

    fn run_straightwave_test(expected: &str, has_minus: bool, number: usize) {
        let obtained = super::progress_replace(
            20,
            has_minus,
            number,
            &[],
            &[],
            &['=', '\u{2248}'],
            &[' '],
            &['['],
            &[']'],
        );
        assert_eq!(expected, obtained);
    }

    #[test]
    fn test_canonical_within() {
        run_canonical_test(
            "[                    ] 0%",
            false,
            0,
        );
        run_canonical_test(
            "[=                   ] 5%",
            false,
            5,
        );
        run_canonical_test(
            "[====                ] 24%",
            false,
            24,
        );
        run_canonical_test(
            "[=====               ] 25%",
            false,
            25,
        );
        run_canonical_test(
            "[=================== ] 99%",
            false,
            99,
        );
        run_canonical_test(
            "[====================] 100%",
            false,
            100,
        );
    }

    #[test]
    fn test_canonical_overshoot() {
        run_canonical_test(
            "[====================] 104%",
            false,
            104,
        );
        run_canonical_test(
            "[====================]= 105%",
            false,
            105,
        );
        run_canonical_test(
            "[====================]========== 150%",
            false,
            150,
        );
        run_canonical_test(
            "[====================]==================== 200%",
            false,
            200,
        );
    }

    #[test]
    fn test_canonical_negative() {
        run_canonical_test(
            "=[                    ] -5%",
            true,
            5,
        );
        run_canonical_test(
            "=====[                    ] -25%",
            true,
            25,
        );
        run_canonical_test(
            "==========[                    ] -50%",
            true,
            50,
        );
        run_canonical_test(
            "====================[                    ] -100%",
            true,
            100,
        );
        run_canonical_test(
            "========================================[                    ] -200%",
            true,
            200,
        );
    }

    #[test]
    fn test_yolo_within() {
        run_yolo_test(
            ">>>lollollollollollollollollollollollollollollollollo<<< 0%",
            false,
            0,
        );
        run_yolo_test(
            ">>>rollollollollollollollollollollollollollollollollo<<< 2%",
            false,
            2,
        );
        run_yolo_test(
            ">>>rollollollollollollollollollollollollollollollollo<<< 4%",
            false,
            4,
        );
        run_yolo_test(
            ">>>YOLOollollollollollollollollollollollollollollollo<<< 8%",
            false,
            8,
        );
        run_yolo_test(
            ">>>YrofOLOollollollollollollollollollollollollollollo<<< 15%",
            false,
            15,
        );
        run_yolo_test(
            ">>>YroflOLOllollollollollollollollollollollollollollo<<< 16%",
            false,
            16,
        );
        run_yolo_test(
            ">>>YroflrOLOlollollollollollollollollollollollollollo<<< 19%",
            false,
            19,
        );
        run_yolo_test(
            ">>>YroflroOLOollollollollollollollollollollollollollo<<< 20%",
            false,
            20,
        );
        run_yolo_test(
            ">>>YroflroflroflroflroflrOLOollollollollollollollollo<<< 50%",
            false,
            50,
        );
        run_yolo_test(
            ">>>YroflroflroflroflroflroflroflroflroflroflroflrOLOo<<< 99%",
            false,
            99,
        );
        run_yolo_test(
            ">>>YroflroflroflroflroflroflroflroflroflroflroflroOLO<<< 100%",
            false,
            100,
        );
    }

    #[test]
    fn test_yolo_overshoot() {
        run_yolo_test(
            ">>>YroflroflroflroflroflroflroflroflroflroflroflroOLO<<< 101%",
            false,
            101,
        );
        run_yolo_test(
            ">>>YroflroflroflroflroflroflroflroflroflroflroflrofOL<<<O 102%",
            false,
            102,
        );
        run_yolo_test(
            ">>>YroflroflroflroflroflroflroflroflroflroflroflroflO<<<LO 105%",
            false,
            105,
        );
        run_yolo_test(
            ">>>Yroflroflroflroflroflroflroflroflroflroflroflroflr<<<oflroflroflroflroflrofOLO 150%",
            false,
            150,
        );
        run_yolo_test(
            ">>>Yroflroflroflroflroflroflroflroflroflroflroflroflr<<<oflroflroflroflroflroflroflroflroflroflroflroflOLO 200%",
            false,
            200,
        );
    }

    #[test]
    fn test_yolo_negative() {
        run_yolo_test(
            "r>>>lollollollollollollollollollollollollollollollollo<<< -2%",
            true,
            2,
        );
        run_yolo_test(
            "ro>>>lollollollollollollollollollollollollollollollollo<<< -4%",
            true,
            4,
        );
        run_yolo_test(
            "YOLO>>>lollollollollollollollollollollollollollollollollo<<< -8%",
            true,
            8,
        );
        run_yolo_test(
            "YrOLO>>>lollollollollollollollollollollollollollollollollo<<< -10%",
            true,
            10,
        );
        run_yolo_test(
            "YroOLO>>>lollollollollollollollollollollollollollollollollo<<< -12%",
            true,
            12,
        );
        run_yolo_test(
            "YroflroflroflroflroflrOLO>>>lollollollollollollollollollollollollollollollollo<<< -50%",
            true,
            50,
        );
        run_yolo_test(
            "YroflroflroflroflroflroflroflroflroflroflroflroOLO>>>lollollollollollollollollollollollollollollollollo<<< -100%",
            true,
            100,
        );
        run_yolo_test(
            "YroflroflroflroflroflroflroflroflroflroflroflroflroflroflroflroflroflroflroflroflroflroflroflroflOLO>>>lollollollollollollollollollollollollollollollollo<<< -200%",
            true,
            200,
        );
    }

    #[test]
    fn test_straightwave_within() {
        run_straightwave_test(
            "[                    ] 0%",
            false,
            0,
        );
        run_straightwave_test(
            "[=                   ] 5%",
            false,
            5,
        );
        run_straightwave_test(
            "[=\u{2248}=\u{2248}                ] 24%",
            false,
            24,
        );
        run_straightwave_test(
            "[=\u{2248}=\u{2248}=               ] 25%",
            false,
            25,
        );
        run_straightwave_test(
            "[=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}= ] 99%",
            false,
            99,
        );
        run_straightwave_test(
            "[=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}] 100%",
            false,
            100,
        );
    }

    #[test]
    fn test_straightwave_overshoot() {
        run_straightwave_test(
            "[=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}] 104%",
            false,
            104,
        );
        run_straightwave_test(
            "[=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}]= 105%",
            false,
            105,
        );
        run_straightwave_test(
            "[=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}]=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248} 150%",
            false,
            150,
        );
        run_straightwave_test(
            "[=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}]=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248} 200%",
            false,
            200,
        );
    }

    #[test]
    fn test_straightwave_negative() {
        run_straightwave_test(
            "=[                    ] -5%",
            true,
            5,
        );
        run_straightwave_test(
            "=\u{2248}=\u{2248}=[                    ] -25%",
            true,
            25,
        );
        run_straightwave_test(
            "=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}[                    ] -50%",
            true,
            50,
        );
        run_straightwave_test(
            "=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}[                    ] -100%",
            true,
            100,
        );
        run_straightwave_test(
            "=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}=\u{2248}[                    ] -200%",
            true,
            200,
        );
    }
}
