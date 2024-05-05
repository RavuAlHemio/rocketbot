#[cfg(windows)]
mod windows_version;


use std::borrow::Cow;
use std::fmt::Write;
use std::sync::Weak;

use async_trait::async_trait;
use rocketbot_interface::{send_channel_message, send_private_message};
use rocketbot_interface::commands::{CommandDefinitionBuilder, CommandInstance};
use rocketbot_interface::interfaces::{RocketBotInterface, RocketBotPlugin};
use rocketbot_interface::model::{ChannelMessage, PrivateMessage};
use rustc_version_runtime;
use serde_json;
use tracing::warn;


// should be filled in by CI/CD during a build
const VERSION_STRING: &str = "{{VERSION}}";
const COMMIT_MESSAGE_SHORT: &str = "{{COMMIT_MESSAGE_SHORT}}";


fn bot_revision() -> Cow<'static, str> {
    // use concat! to hide this string from CI/CD, lest it be replaced too
    let unset_version_string = concat!("{{", "VERSION", "}}");

    if VERSION_STRING == unset_version_string {
        warn!("version requested but unknown!");
        Cow::Borrowed("unknown")
    } else {
        Cow::Owned(format!("`{}` _{}_", VERSION_STRING, COMMIT_MESSAGE_SHORT))
    }
}


fn compiler_version() -> String {
    format!("rustc {}", rustc_version_runtime::version())
}


#[cfg(unix)]
fn operating_system_version() -> Option<String> {
    use std::ffi::{c_char, CStr};
    use libc::{uname, utsname};

    fn stringify(buf: &[c_char]) -> Cow<str> {
        unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy()
    }

    let mut buf: utsname = unsafe { std::mem::zeroed() };
    unsafe { uname(&mut buf) };

    Some(format!(
        "{} {} {} {} {}",
        stringify(&buf.sysname),
        stringify(&buf.nodename),
        stringify(&buf.release),
        stringify(&buf.version),
        stringify(&buf.machine),
    ))
}


#[cfg(windows)]
fn operating_system_version() -> Option<String> {
    Some(crate::windows_version::version())
}

#[cfg(not(any(unix, windows)))]
fn operating_system_version() -> Option<String> {
    None
}


fn assemble_version() -> String {
    let mut full_version = format!("rocketbot revision {}", bot_revision());
    if let Some(os_ver) = operating_system_version() {
        write!(full_version, "\nrunning on {}", os_ver).unwrap();
    }
    write!(full_version, "\ncompiled by {}", compiler_version()).unwrap();
    full_version
}


pub struct VersionPlugin {
    interface: Weak<dyn RocketBotInterface>,
}
#[async_trait]
impl RocketBotPlugin for VersionPlugin {
    async fn new(interface: Weak<dyn RocketBotInterface>, _config: serde_json::Value) -> Self {
        let my_interface = match interface.upgrade() {
            None => panic!("interface is gone"),
            Some(i) => i,
        };

        let version_command = CommandDefinitionBuilder::new(
            "version",
            "version",
            "{cpfx}version",
            "Outputs the currently running version of the bot.",
        )
            .build();
        my_interface.register_channel_command(&version_command).await;
        my_interface.register_private_message_command(&version_command).await;

        VersionPlugin {
            interface,
        }
    }

    async fn plugin_name(&self) -> String {
        "version".to_owned()
    }

    async fn channel_command(&self, channel_message: &ChannelMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if command.name != "version" {
            return;
        }

        let version_string = assemble_version();

        send_channel_message!(
            interface,
            &channel_message.channel.name,
            &version_string,
        ).await;
    }

    async fn private_command(&self, private_message: &PrivateMessage, command: &CommandInstance) {
        let interface = match self.interface.upgrade() {
            None => return,
            Some(i) => i,
        };

        if command.name != "version" {
            return;
        }

        let version_string = assemble_version();

        send_private_message!(
            interface,
            &private_message.conversation.id,
            &version_string,
        ).await;
    }

    async fn get_command_help(&self, command_name: &str) -> Option<String> {
        if command_name == "version" {
            Some(include_str!("../help/version.md").to_owned())
        } else {
            None
        }
    }

    async fn configuration_updated(&self, _new_config: serde_json::Value) -> bool {
        // not much to update
        true
    }
}
