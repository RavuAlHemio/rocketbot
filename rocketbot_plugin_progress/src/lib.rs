use std::sync::Weak;

use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use rocketbot_interface::{JsonValueExtensions, send_channel_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde_json;


static PROGRESS_INDICATOR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "(?P<minus>-?)",
    "(?P<number>0|[1-9][0-9]?[0-9]?|200)",
    "%",
    "(?:",
        "(?P<start>\\S)",
        "(?P<end>\\S)?",
    ")?",
)).expect("failed to parse progress indicator regex"));


pub struct ProgressPlugin {
    interface: Weak<dyn RocketBotInterface>,
    bar_length: usize,
}
#[async_trait]
impl RocketBotPlugin for ProgressPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let bar_length = if config["bar_length"].is_null() {
            20
        } else {
            config["bar_length"].as_usize()
                .expect("bar_length not representable as a usize")
        };

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "progress".to_owned(),
                "progress".to_owned(),
                "{cpfx}progress TEXT".to_owned(),
                "Annotates percentages in the text with progress bars.".to_owned(),
            )
                .build()
        ).await;

        Self {
            interface,
            bar_length,
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

        let replaced = PROGRESS_INDICATOR_RE.replace_all(
            &command.rest,
            |caps: &Captures| regex_replacement_func(caps, self.bar_length),
        );
        if replaced != command.rest {
            send_channel_message!(
                interface,
                &channel_message.channel.name,
                &replaced,
            );
        }
    }
}


fn regex_replacement_func(caps: &Captures, bar_length: usize) -> String {
    let has_minus = caps
        .name("minus").expect("minus not captured")
        .as_str() == "-";

    let number: usize = caps
        .name("number").expect("number not captured")
        .as_str()
        .parse().expect("failed to parse number");

    let start_str = caps
        .name("start")
        .map(|s| s.as_str())
        .unwrap_or("");
    let end_str = caps
        .name("end")
        .map(|s| s.as_str())
        .unwrap_or("");

    let foreground_str = "=";
    let background_str = " ";
    let left_box = "[";
    let right_box = "]";

    progress_replace(
        bar_length,
        has_minus,
        number,
        start_str,
        end_str,
        foreground_str,
        background_str,
        left_box,
        right_box,
    )
}

fn progress_replace(
    bar_length: usize,
    has_minus: bool,
    number: usize,
    start_str: &str,
    end_str: &str,
    foreground_str: &str,
    background_str: &str,
    left_box: &str,
    right_box: &str,
) -> String {
    let full_foreground = repeat_string(foreground_str, 2*bar_length);
    let full_background = repeat_string(background_str, bar_length);

    // calculate number of segments (before sign!)
    let segment_count = number * bar_length / 100;

    // assemble the segments
    let mut segs = String::new();
    if segment_count < start_str.len() + end_str.len() {
        segs.push_str(&full_foreground[0..segment_count]);
    } else {
        segs.push_str(start_str);
        segs.push_str(&full_foreground[0..segment_count-(start_str.len()+end_str.len())]);
        segs.push_str(end_str);
    }

    if has_minus {
        // segments come before the box
        format!(
            "{}{}{}{} -{}%",
            segs, left_box, full_background, right_box, number,
        )
    } else {
        // segments are inside and sometimes outside the box
        let seg_char_count = segs.chars().count();
        let (segs_inside_box, segs_outside_box, bg_inside_box): (String, String, String) = if seg_char_count <= bar_length {
            // bar fits inside box
            (
                segs.chars().collect(),
                String::new(),
                full_background.chars().skip(seg_char_count).collect(),
            )
        } else {
            // bar juts outside of box
            (
                segs.chars().take(bar_length).collect(),
                segs.chars().skip(bar_length).collect(),
                String::new(),
            )
        };

        format!(
            "{}{}{}{}{} {}%",
            left_box, segs_inside_box, bg_inside_box, right_box, segs_outside_box, number,
        )
    }
}

fn repeat_string(string: &str, length: usize) -> String {
    let mut ret = String::new();
    while ret.len() < length {
        ret.push_str(string);
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
            "",
            "",
            "=",
            " ",
            "[",
            "]"
        );
        assert_eq!(expected, obtained);
    }

    fn run_yolo_test(expected: &str, has_minus: bool, number: usize) {
        let obtained = super::progress_replace(
            50,
            has_minus,
            number,
            "Y",
            "OLO",
            "rofl",
            "lol",
            ">>>",
            "<<<"
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
}
