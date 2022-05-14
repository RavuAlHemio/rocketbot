use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::sync::Weak;

use async_trait::async_trait;
use log::error;
use once_cell::sync::Lazy;
use regex::{Captures, Regex, Replacer};
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::RwLock;
use serde_json;


#[derive(Clone, Debug, Eq, PartialEq)]
struct Config {
    url_safe_characters: HashSet<char>,
    wrapper_pairs: HashMap<char, char>,
}

// don't escape '/', '%', '?', '&', '=' or '+' by default as parts of the original URL may contain them
const DEFAULT_URL_SAFE_CHARACTERS: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz-_./%?&=+";
const DEFAULT_WRAPPER_PAIRS: &str = "(){}[]";

static WRAPPED_URL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "(?P<pre_delimiter>",
        "^",
        "|",
        "\\s",
    ")",
    "(?P<pre_wrapper>",
        "\\S*?",
    ")",
    "(?P<scheme>",
        "[A-Za-z]",
        "[A-Za-z0-9+-.]*",
    ")",
    "(?P<cds>://)", // colon-double-slash
    "(?P<rest>\\S+)", // rest of URL (one would assume)
)).expect("failed to parse wrapped URL regex"));


fn char_to_escaped(c: char) -> String {
    let mut buf = [0u8; 4];
    let mut ret = String::with_capacity(4*3);
    let utf8 = c.encode_utf8(&mut buf);
    for b in utf8.bytes() {
        write!(&mut ret, "%{:02X}", b).unwrap();
    }
    ret
}


struct FixUrlReplacer<'a> {
    url_safe_characters: &'a HashSet<char>,
    wrapper_pairs: &'a HashMap<char, char>,
}
impl<'a> Replacer for FixUrlReplacer<'a> {
    fn replace_append(&mut self, caps: &Captures<'_>, dst: &mut String) {
        let pre_delimiter = caps.name("pre_delimiter").expect("pre_delimiter not captured").as_str();
        let pre_wrapper = caps.name("pre_wrapper").expect("pre_wrapper not captured").as_str();
        let scheme = caps.name("scheme").expect("scheme not captured").as_str();
        let cds = caps.name("cds").expect("cds not captured").as_str();
        let rest = caps.name("rest").expect("rest not captured").as_str();

        // be able to detect wrapper characters at the end of the URL
        let wrapper_ends: HashSet<char> = self.wrapper_pairs
            .values()
            .map(|e| *e)
            .collect();

        // collect any wrapper characters in front of the URL
        let mut pre_wrappers: Vec<char> = Vec::with_capacity(pre_wrapper.chars().count());
        for b in pre_wrapper.chars() {
            if let Some(e) = self.wrapper_pairs.get(&b) {
                pre_wrappers.push(*e);
            }
        }

        // go through the URL [rest]
        let mut url_wrappers: Vec<char> = Vec::new();
        let mut escaped_rest = String::with_capacity(rest.len());
        for c in rest.chars() {
            // is this a wrapper character?
            if let Some(e) = self.wrapper_pairs.get(&c) {
                // yes, it's a start character
                url_wrappers.push(*e);
            } else if wrapper_ends.contains(&c) {
                // yes, it's an end character
                if let Some(expected_end) = url_wrappers.pop() {
                    if c == expected_end {
                        // it's a wrapper within the URL; escape it
                        escaped_rest.push_str(&char_to_escaped(c));
                        continue;
                    }

                    // it's not the wrapper we expected; put it back and continue with regular logic
                    url_wrappers.push(expected_end);
                }

                if let Some(expected_end) = pre_wrappers.pop() {
                    if c == expected_end {
                        // it's a wrapper outside the URL; append it unescaped
                        escaped_rest.push(c);
                        continue;
                    }

                    // it's not the wrapper we expected; put it back and continue with regular logic
                    pre_wrappers.push(expected_end);
                }
            }

            // and now, the regular appending logic

            if self.url_safe_characters.contains(&c) {
                escaped_rest.push(c);
            } else {
                escaped_rest.push_str(&char_to_escaped(c));
            }
        }

        // append the replacement string
        // (copy everything verbatim except the rest, where we take the escaped variant)
        dst.push_str(pre_delimiter);
        dst.push_str(pre_wrapper);
        dst.push_str(scheme);
        dst.push_str(cds);
        dst.push_str(&escaped_rest);
    }
}


fn fix_urls(message: &str, url_safe_characters: &HashSet<char>, wrapper_pairs: &HashMap<char, char>) -> String {
    let replacer = FixUrlReplacer {
        url_safe_characters,
        wrapper_pairs,
    };
    WRAPPED_URL_RE.replace_all(message, replacer)
        .into_owned()
}


pub struct UrlPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
}
impl UrlPlugin {
    fn try_get_config(config: serde_json::Value) -> Result<Config, &'static str> {
        let url_safe_characters_val = &config["url_safe_characters"];
        let url_safe_characters_str = if url_safe_characters_val.is_null() {
            DEFAULT_URL_SAFE_CHARACTERS
        } else {
            url_safe_characters_val
                .as_str().ok_or("url_safe_characters neither null nor a string")?
        };
        let url_safe_characters: HashSet<char> = url_safe_characters_str.chars()
            .collect();

        let wrapper_pairs_val = &config["wrapper_pairs"];
        let wrapper_pairs_str = if wrapper_pairs_val.is_null() {
            DEFAULT_WRAPPER_PAIRS
        } else {
            wrapper_pairs_val
                .as_str().ok_or("wrapper_pairs neither null nor a string")?
        };
        let wrapper_pairs_chars: Vec<char> = wrapper_pairs_str.chars().collect();
        if wrapper_pairs_chars.len() % 2 != 0 {
            return Err("wrapper_pairs characters not divisible by 2");
        }

        let mut wrapper_pairs: HashMap<char, char> = HashMap::with_capacity(wrapper_pairs_chars.len()/2);
        for i in 0..wrapper_pairs_chars.len()/2 {
            let from_char = wrapper_pairs_chars[2*i+0];
            let to_char = wrapper_pairs_chars[2*i+1];
            wrapper_pairs.insert(from_char, to_char);
        }

        Ok(Config {
            url_safe_characters,
            wrapper_pairs,
        })
    }

    async fn channel_command_fixurls(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let (url_safe_characters, wrapper_pairs) = {
            let config_guard = self.config.read().await;
            (
                config_guard.url_safe_characters.clone(),
                config_guard.wrapper_pairs.clone(),
            )
        };

        if command.rest.trim().len() == 0 {
            return;
        }

        let fixed = fix_urls(&command.rest, &url_safe_characters, &wrapper_pairs);
        if fixed == command.rest {
            return;
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &fixed,
        ).await;
    }
}
#[async_trait]
impl RocketBotPlugin for UrlPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let config_object = Self::try_get_config(config)
            .expect("failed to load config");

        let config_lock = RwLock::new(
            "UrlPlugin::config",
            config_object,
        );

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "fixurls",
                "fixurls",
                "{cpfx}fixurls URL",
                "Re-outputs the given message with its URLs fixed.",
            )
                .build()
        ).await;

        Self {
            interface,
            config: config_lock,
        }
    }

    async fn plugin_name(&self) -> String {
        "url".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "fixurls" {
            self.channel_command_fixurls(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "fixurls" {
            Some(include_str!("../help/fixurls.md").to_owned())
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


#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use super::{DEFAULT_URL_SAFE_CHARACTERS, DEFAULT_WRAPPER_PAIRS, fix_urls};

    fn run_fix_urls_test(unescaped: &str, escaped: &str) {
        let url_safe_characters = DEFAULT_URL_SAFE_CHARACTERS.chars().collect();

        let mut wrapper_pairs = HashMap::new();
        let wpc: Vec<char> = DEFAULT_WRAPPER_PAIRS.chars().collect();
        for i in 0..wpc.len()/2 {
            let f = wpc[2*i+0];
            let t = wpc[2*i+1];
            wrapper_pairs.insert(f, t);
        }

        assert_eq!(
            fix_urls(unescaped, &url_safe_characters, &wrapper_pairs).as_str(),
            escaped,
        );
    }

    #[test]
    fn test_fix_urls_noop() {
        run_fix_urls_test("", "");
        run_fix_urls_test("abc", "abc");
        run_fix_urls_test("abc def ghi", "abc def ghi");
        run_fix_urls_test("look at this site: http://example.com/ or don't", "look at this site: http://example.com/ or don't");
        run_fix_urls_test("look at this site: http://example.com/lol/rofl/ or don't", "look at this site: http://example.com/lol/rofl/ or don't");
    }

    #[test]
    fn test_fix_urls_parens() {
        run_fix_urls_test("look at this site: http://example.com/wiki/A_(disambiguation) or don't", "look at this site: http://example.com/wiki/A_%28disambiguation%29 or don't");
        run_fix_urls_test("look at this site: http://example.com/w/index.php?arg[]=What&arg[]=The or don't", "look at this site: http://example.com/w/index.php?arg%5B%5D=What&arg%5B%5D=The or don't");
    }

    #[test]
    fn test_fix_urls_wrapped() {
        run_fix_urls_test("some of this stuff is weird (http://example.com/wiki/Birdidae) lol", "some of this stuff is weird (http://example.com/wiki/Birdidae) lol");
        run_fix_urls_test("some of this stuff is weird [http://example.com/wiki/Birdidae] lol", "some of this stuff is weird [http://example.com/wiki/Birdidae] lol");
    }

    #[test]
    fn test_fix_urls_wrapped_parens() {
        run_fix_urls_test("some of this stuff is weird (http://example.com/wiki/A_(disambiguation)) lol", "some of this stuff is weird (http://example.com/wiki/A_%28disambiguation%29) lol");
        run_fix_urls_test("some of this stuff is weird [http://example.com/wiki/A_(disambiguation)] lol", "some of this stuff is weird [http://example.com/wiki/A_%28disambiguation%29] lol");
        run_fix_urls_test("some of this stuff is weird (http://example.com/w/index.php?arg[]=What&arg[]=The) lol", "some of this stuff is weird (http://example.com/w/index.php?arg%5B%5D=What&arg%5B%5D=The) lol");
        run_fix_urls_test("some of this stuff is weird [http://example.com/w/index.php?arg[]=What&arg[]=The] lol", "some of this stuff is weird [http://example.com/w/index.php?arg%5B%5D=What&arg%5B%5D=The] lol");
        run_fix_urls_test("some of this stuff is weird ([http://example.com/wiki/A_(disambiguation_[really])]) lol", "some of this stuff is weird ([http://example.com/wiki/A_%28disambiguation_%5Breally%5D%29]) lol");
    }

    #[test]
    fn test_fix_urls_mismatched_wrapper() {
        run_fix_urls_test("some of this stuff is weird (http://example.com/wiki/A_(disambiguation)] lol", "some of this stuff is weird (http://example.com/wiki/A_%28disambiguation%29%5D lol");
    }
}
