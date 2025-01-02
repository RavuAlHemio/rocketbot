use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::sync::Weak;

use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use rocketbot_interface::send_channel_message;
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::ChannelMessage;
use rocketbot_interface::sync::{Mutex, RwLock};
use serde_json;
use tracing::error;


#[derive(Clone, Debug, Eq, PartialEq)]
struct Config {
    url_safe_characters: HashSet<char>,
    url_safe_characters_before_question_mark: HashSet<char>,
    wrapper_pairs: HashMap<char, char>,
    auto_fix_channels: HashSet<String>,
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct LastFix {
    message_id: String,
    fixed_body: String,
}


// don't escape '/', '%', '?', '&', '=', '+' or '#' by default as parts of the original URL may contain them
const DEFAULT_URL_SAFE_CHARACTERS: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz-_./%?&=+#";
const DEFAULT_URL_SAFE_CHARACTERS_BEFORE_QUESTION_MARK: &str = "~";
const DEFAULT_WRAPPER_PAIRS: &str = "(){}[]";

static URL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(concat!(
    "(?P<scheme>",
        "[A-Za-z]",
        "[A-Za-z0-9+-.]*",
    ")",
    "(?P<cds>://)", // colon-double-slash
    "(?P<rest>\\S+)", // rest of URL (one would assume)
)).expect("failed to parse URL regex"));


fn char_to_escaped(c: char) -> String {
    let mut buf = [0u8; 4];
    let mut ret = String::with_capacity(4*3);
    let utf8 = c.encode_utf8(&mut buf);
    for b in utf8.bytes() {
        write!(&mut ret, "%{:02X}", b).unwrap();
    }
    ret
}


fn find_wrapper_balance(
    balance_me: &str,
    wrapper_pairs: &HashMap<char, char>,
) -> Option<(usize, char)> {
    if balance_me.len() == 0 {
        return None;
    }

    let top_start_char = balance_me.chars().nth(0).unwrap();
    let top_end_char = match wrapper_pairs.get(&top_start_char) {
        Some(c) => *c,
        None => return None,
    };

    let closers: HashSet<char> = wrapper_pairs.values()
        .map(|c| *c)
        .collect();

    let mut wrapper_stack: Vec<(char, char)> = Vec::new();
    wrapper_stack.push((top_start_char, top_end_char));

    for (i, c) in balance_me.char_indices().skip(1) {
        assert!(wrapper_stack.len() > 0);

        if let Some(closer) = wrapper_pairs.get(&c) {
            // the character is an opener
            // place it on the stack
            wrapper_stack.push((c, *closer));
            continue;
        }
        if closers.contains(&c) {
            // the character is a closer
            // does it match the top of the stack?
            let expected_closer = wrapper_stack.last().unwrap().1;
            if c == expected_closer {
                // yes; pop off the stack entry
                wrapper_stack.pop();

                if wrapper_stack.len() == 0 {
                    // we have balanced the outermost brackets
                    // this is it
                    return Some((i, c));
                }

                continue;
            } else {
                // incorrect closer
                // surrender
                return None;
            }
        }

        // neither opener nor closer? not interesting
    }

    None
}


fn fix_urls(
    escape_me: &str,
    url_safe_characters: &HashSet<char>,
    url_safe_characters_before_question_mark: &HashSet<char>,
    wrapper_pairs: &HashMap<char, char>,
) -> String {
    // choose what to do depending on what appears earlier:
    // 1. a wrapper character
    // 2. a URL

    let mut url_caps_opt = URL_RE.captures(escape_me);
    let mut wrapper_index_char_opt = escape_me.char_indices()
        .filter(|(_i, c)| wrapper_pairs.contains_key(c))
        .nth(0);

    if url_caps_opt.is_some() && wrapper_index_char_opt.is_some() {
        // only keep the earlier option
        let caps_start = url_caps_opt.as_ref().unwrap().get(0).unwrap().start();
        let (wrapper_index, _wrapper_char) = wrapper_index_char_opt.unwrap();
        if caps_start < wrapper_index {
            wrapper_index_char_opt = None;
        } else {
            url_caps_opt = None;
        }
    }

    if let Some(url_caps) = url_caps_opt {
        // assume it's all part of the URL and escape it
        let mut ret = String::with_capacity(escape_me.len());

        let url_full_match = url_caps.get(0).unwrap();

        // add everything before the URL, raw
        ret.push_str(&escape_me[..url_full_match.start()]);

        // add the URL:
        // * scheme raw
        ret.push_str(url_caps.name("scheme").unwrap().as_str());
        // * colon-double-slash raw
        ret.push_str(url_caps.name("cds").unwrap().as_str());
        // * rest escaped
        let mut question_mark_seen = false;
        for c in url_caps.name("rest").unwrap().as_str().chars() {
            if c == '?' {
                question_mark_seen = true;
            }

            if url_safe_characters.contains(&c) {
                ret.push(c);
            } else if !question_mark_seen && url_safe_characters_before_question_mark.contains(&c) {
                ret.push(c);
            } else {
                ret.push_str(&char_to_escaped(c));
            }
        }

        // append the rest, processed by the same algorithm
        let rest = fix_urls(
            &escape_me[url_full_match.end()..],
            url_safe_characters,
            url_safe_characters_before_question_mark,
            wrapper_pairs,
        );
        ret.push_str(&rest);

        ret
    } else if let Some((opening_wrapper_index, opening_wrapper_char)) = wrapper_index_char_opt {
        // try to balance this wrapper
        let closing_wrapper_opt = find_wrapper_balance(
            &escape_me[opening_wrapper_index..],
            wrapper_pairs,
        );
        match closing_wrapper_opt {
            Some((closing_wrapper_relative_index, closing_wrapper_char)) => {
                let closing_wrapper_index = closing_wrapper_relative_index + opening_wrapper_index;

                // succeeded balancing the wrapper
                let mut ret = String::with_capacity(escape_me.len());

                // take everything before the opening wrapper verbatim
                ret.push_str(&escape_me[..opening_wrapper_index]);

                // take the opening wrapper
                ret.push(opening_wrapper_char);

                // take everything within the wrappers, escaped using the same algorithm
                let within_slice = &escape_me[opening_wrapper_index+opening_wrapper_char.len_utf8()..closing_wrapper_index];
                let within = fix_urls(
                    within_slice,
                    url_safe_characters,
                    url_safe_characters_before_question_mark,
                    wrapper_pairs,
                );
                ret.push_str(&within);

                // take the closing wrapper
                ret.push(closing_wrapper_char);

                // take the rest behind the closing wrapper, escaped using the same algorithm
                let rest_slice = &escape_me[closing_wrapper_index+closing_wrapper_char.len_utf8()..];
                let rest = fix_urls(
                    rest_slice,
                    url_safe_characters,
                    url_safe_characters_before_question_mark,
                    wrapper_pairs,
                );
                ret.push_str(&rest);

                return ret;
            },
            None => {
                // failed to balance the wrapper
                // just add it verbatim and escape the rest
                let mut ret = String::with_capacity(escape_me.len());

                // take everything before the opening wrapper verbatim
                ret.push_str(&escape_me[..opening_wrapper_index]);

                // take the opening wrapper
                ret.push(opening_wrapper_char);

                // take the rest behind the opening wrapper, escaped using the same algorithm
                let rest_slice = &escape_me[opening_wrapper_index+opening_wrapper_char.len_utf8()..];
                let rest = fix_urls(
                    rest_slice,
                    url_safe_characters,
                    url_safe_characters_before_question_mark,
                    wrapper_pairs,
                );
                ret.push_str(&rest);

                return ret;
            },
        }
    } else {
        // just spit out the original string
        escape_me.to_string()
    }
}


pub struct UrlPlugin {
    interface: Weak<dyn RocketBotInterface>,
    config: RwLock<Config>,
    channel_id_to_last_fix: Mutex<HashMap<String, LastFix>>,
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

        let url_safe_characters_before_question_mark_val = &config["url_safe_characters_before_question_mark"];
        let url_safe_characters_before_question_mark_str = if url_safe_characters_before_question_mark_val.is_null() {
            DEFAULT_URL_SAFE_CHARACTERS_BEFORE_QUESTION_MARK
        } else {
            url_safe_characters_before_question_mark_val
                .as_str().ok_or("url_safe_characters neither null nor a string")?
        };
        let url_safe_characters_before_question_mark: HashSet<char> = url_safe_characters_before_question_mark_str.chars()
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

        let auto_fix_channels_val = &config["auto_fix_channels"];
        let auto_fix_channels = if auto_fix_channels_val.is_null() {
            HashSet::new()
        } else if let Some(afc_array) = auto_fix_channels_val.as_array() {
            let mut afc_set = HashSet::new();
            for afc_val in afc_array {
                if let Some(afc) = afc_val.as_str() {
                    afc_set.insert(afc.to_owned());
                } else {
                    return Err("auto_fix_channels entry is not string");
                }
            }
            afc_set
        } else {
            return Err("auto_fix_channels is neither null nor an array");
        };

        Ok(Config {
            url_safe_characters,
            url_safe_characters_before_question_mark,
            wrapper_pairs,
            auto_fix_channels,
        })
    }

    async fn channel_command_fixurls(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        let (
            url_safe_characters,
            url_safe_characters_before_question_mark,
            wrapper_pairs,
        ) = {
            let config_guard = self.config.read().await;
            (
                config_guard.url_safe_characters.clone(),
                config_guard.url_safe_characters_before_question_mark.clone(),
                config_guard.wrapper_pairs.clone(),
            )
        };

        let command_body = command.rest.trim();
        if command_body.len() == 0 {
            // get the last message with a fixable URL
            let last_fix_guard = self.channel_id_to_last_fix.lock().await;
            if let Some(channel_last_fix) = last_fix_guard.get(&channel_message.channel.id) {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &channel_last_fix.fixed_body,
                ).await;
            }
            return;
        }

        let fixed = fix_urls(
            &command.rest,
            &url_safe_characters,
            &url_safe_characters_before_question_mark,
            &wrapper_pairs,
        );
        if fixed == command.rest {
            return;
        }

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &fixed,
        ).await;
    }

    async fn store_fixed_message(&self, channel_message: &ChannelMessage) -> Option<(String, HashSet<String>)> {
        let message_body = match &channel_message.message.raw {
            None => return None,
            Some(mb) => mb,
        };

        let (
            url_safe_characters,
            url_safe_characters_before_question_mark,
            wrapper_pairs,
            auto_fix_channels,
        ) = {
            let config_guard = self.config.read().await;
            (
                config_guard.url_safe_characters.clone(),
                config_guard.url_safe_characters_before_question_mark.clone(),
                config_guard.wrapper_pairs.clone(),
                config_guard.auto_fix_channels.clone(),
            )
        };

        let fixed = fix_urls(
            message_body,
            &url_safe_characters,
            &url_safe_characters_before_question_mark,
            &wrapper_pairs,
        );
        if &fixed == message_body {
            return None;
        }

        {
            let mut fix_guard = self.channel_id_to_last_fix.lock().await;
            fix_guard.insert(
                channel_message.channel.id.clone(),
                LastFix {
                    message_id: channel_message.message.id.clone(),
                    fixed_body: fixed.clone(),
                },
            );
        }

        Some((fixed, auto_fix_channels))
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

        let channel_id_to_last_fix = Mutex::new(
            "UrlPlugin::channel_id_to_last_fix",
            HashMap::new(),
        );

        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "fixurls",
                "url",
                "{cpfx}fixurls MESSAGE",
                "Re-outputs the given message with its URLs fixed.",
            )
                .build()
        ).await;
        my_interface.register_channel_command(
            &CommandDefinitionBuilder::new(
                "fixurl",
                "url",
                "{cpfx}fixurl MESSAGE",
                "Re-outputs the given message with its URLs fixed.",
            )
                .build()
        ).await;

        Self {
            interface,
            config: config_lock,
            channel_id_to_last_fix,
        }
    }

    async fn plugin_name(&self) -> String {
        "url".to_owned()
    }

    async fn channel_message(&self, channel_message: &ChannelMessage) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if let Some((fixed, auto_fix_channels)) = self.store_fixed_message(channel_message).await {
            if auto_fix_channels.contains(&channel_message.channel.name) {
                send_channel_message!(
                    interface,
                    &channel_message.channel.name,
                    &fixed,
                ).await;
            }
        }
    }

    async fn channel_message_edited(&self, channel_message: &ChannelMessage) {
        // is this the most recent fixed message for this channel?
        let last_fix_id = {
            let citlf_guard = self.channel_id_to_last_fix.lock().await;
            match citlf_guard.get(&channel_message.channel.id) {
                Some(lf) => lf.message_id.clone(),
                None => return,
            }
        };
        if last_fix_id != channel_message.message.id {
            // no, there has been a different message in between
            return;
        }

        // yes; update it
        self.store_fixed_message(channel_message).await;
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        if command.name == "fixurls" || command.name == "fixurl" {
            self.channel_command_fixurls(channel_message, command).await
        }
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "fixurls" || command_name == "fixurl" {
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
    use super::{
        DEFAULT_URL_SAFE_CHARACTERS, DEFAULT_URL_SAFE_CHARACTERS_BEFORE_QUESTION_MARK,
        DEFAULT_WRAPPER_PAIRS, fix_urls,
    };

    fn run_fix_urls_test(unescaped: &str, escaped: &str) {
        let url_safe_characters = DEFAULT_URL_SAFE_CHARACTERS.chars().collect();
        let url_safe_characters_before_question_mark = DEFAULT_URL_SAFE_CHARACTERS_BEFORE_QUESTION_MARK.chars().collect();

        let mut wrapper_pairs = HashMap::new();
        let wpc: Vec<char> = DEFAULT_WRAPPER_PAIRS.chars().collect();
        for i in 0..wpc.len()/2 {
            let f = wpc[2*i+0];
            let t = wpc[2*i+1];
            wrapper_pairs.insert(f, t);
        }

        assert_eq!(
            fix_urls(unescaped, &url_safe_characters, &url_safe_characters_before_question_mark, &wrapper_pairs).as_str(),
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
        run_fix_urls_test("tr/[a-zA-Z]/[n-za-mN-ZA-M]/", "tr/[a-zA-Z]/[n-za-mN-ZA-M]/");
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

    #[test]
    fn test_fix_urls_outer_wrapper() {
        run_fix_urls_test("(borrowed from [The Grauniad](https://www.theguardian.com/))", "(borrowed from [The Grauniad](https://www.theguardian.com/))");
    }

    #[test]
    fn test_tilde() {
        run_fix_urls_test("https://example.com/~wavy/page?arg=~wavy~", "https://example.com/~wavy/page?arg=%7Ewavy%7E");
    }
}
