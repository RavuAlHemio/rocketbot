use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::Weak;

use async_trait::async_trait;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use serde_json;
use strsim;


pub struct TextPlugin {
    interface: Weak<dyn RocketBotInterface>,
}
impl TextPlugin {
    fn same_char_set(a: &str, b: &str) -> bool {
        let a_set: HashSet<char> = a.chars().collect();
        let b_set: HashSet<char> = b.chars().collect();
        a_set == b_set
    }

    fn yes_no(b: bool) -> &'static str {
        if b {
            "yes"
        } else {
            "no"
        }
    }
}
#[async_trait]
impl RocketBotPlugin for TextPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: serde_json::Value) -> Self where Self: Sized {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "compare".to_owned(),
                "text".to_owned(),
                "{cpfx}compare STRING1 STRING2".to_owned(),
                "Outputs information about how much two strings differ.".to_owned(),
            )
                .arg_count(2)
                .build()
        ).await;

        Self {
            interface,
        }
    }

    async fn plugin_name(&self) -> String {
        "text".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let string1 = &command.args[0];
        let string2 = &command.args[1];

        let comparison_str = match string1.cmp(string2) {
            Ordering::Greater => ">",
            Ordering::Equal => "==",
            Ordering::Less => "<",
        };

        let lev = strsim::levenshtein(string1, string2);
        let dam_lev = strsim::damerau_levenshtein(string1, string2);
        let optimal_string_align_dist = strsim::osa_distance(string1, string2);

        let hamming = strsim::hamming(string1, string2);
        let hamming_string = match hamming {
            Ok(dist) => dist.to_string(),
            Err(e) => format!("({})", e),
        };

        let jaro = strsim::jaro(string1, string2);
        let jaro_winkler = strsim::jaro_winkler(string1, string2);
        let sorensen_dice = strsim::sorensen_dice(string1, string2);

        let same_set_str = Self::yes_no(Self::same_char_set(string1, string2));

        let response = format!(
            concat!(
                "{:?} {} {:?}:",
                "\nLevenshtein distance: {}",
                "\nDamerau-Levenshtein distance: {}",
                "\nOptimal String Alignment distance: {}",
                "\nHamming distance: {}",
                "\nJaro similarity: {:.3}%",
                "\nJaro-Winkler similarity: {:.3}%",
                "\nS\u{F8}rensen-Dice similarity: {:.3}%",
                "\nSame character set: {}",
            ),
            string1, comparison_str, string2,
            lev, dam_lev, optimal_string_align_dist,
            hamming_string, jaro * 100.0, jaro_winkler * 100.0, sorensen_dice * 100.0,
            same_set_str,
        );

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &response,
        ).await;
    }
}