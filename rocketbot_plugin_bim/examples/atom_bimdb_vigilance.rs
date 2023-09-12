use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::Parser;
use regex::Regex;
use reqwest::{self, StatusCode};
use rocketbot_interface::serde::serde_regex;
use serde::{Deserialize, Serialize};
use serde_json;
use sxd_document::dom::Element;
use sxd_document::{self, QName};
use tokio;


/// XML namespace for Atom feeds.
const ATOM_NS: &str = "http://www.w3.org/2005/Atom";


#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct Config {
    pub state_file: String,
    pub urls: Vec<UrlConfig>,
    pub commands: HashMap<String, CommandConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct UrlConfig {
    pub atom_url: String,
    pub newest_last: bool,
    pub matchers: Vec<MatcherConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MatcherConfig {
    #[serde(with = "serde_regex")]
    pub title_regex: Regex,
    pub command_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CommandConfig {
    pub command_args: Vec<String>,
    pub working_dir: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
struct State {
    pub url_to_latest_item: BTreeMap<String, String>,
}

#[derive(Parser)]
struct Opts {
    #[arg(default_value = "atom_bimdb_vigilance.json")]
    pub config_path: PathBuf,
}


fn child_element_text<'p, 'n>(parent_element: &'p Element, name: &'n QName) -> String {
    let children = parent_element.children();
    let first_child_opt = children.iter()
        .filter_map(|c| c.element())
        .filter(|e| &e.name() == name)
        .nth(0);
    let mut s = String::new();
    let Some(first_child) = first_child_opt else { return s };
    for grandchild in first_child.children() {
        let Some(text) = grandchild.text() else { continue };
        s.push_str(text.text());
    }
    s
}


async fn process_url(url: &UrlConfig, config: &Config, state: &mut State) -> bool {
    // fetch URL
    let response = match reqwest::get(&url.atom_url).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("failed to obtain URL {:?}: {}", url.atom_url, e);
            return false;
        },
    };
    if response.status() != StatusCode::OK {
        eprintln!("URL {:?} return exit code {}", url.atom_url, response.status());
        return false;
    }
    let body = match response.bytes().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("failed to obtain URL {:?} bytes: {}", url.atom_url, e);
            return false;
        },
    };
    let body_string = match std::str::from_utf8(&body) {
        Ok(bs) => bs,
        Err(_e) => {
            eprintln!("failed to decode URL {:?} body as UTF-8", url.atom_url);
            return false;
        },
    };

    // parse as XML
    let package = match sxd_document::parser::parse(body_string) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("failed to parse URL {:?} body as XML: {}", url.atom_url, e);
            return false;
        },
    };
    let atom_entry = QName::with_namespace_uri(Some(ATOM_NS), "entry");
    let atom_id = QName::with_namespace_uri(Some(ATOM_NS), "id");
    let atom_title = QName::with_namespace_uri(Some(ATOM_NS), "title");

    // collect entries
    let document = package.as_document();
    let root_element_opt = document.root()
        .children()
        .iter()
        .filter_map(|c| c.element())
        .nth(0);
    let Some(root_element) = root_element_opt else {
        eprintln!("URL {:?} body as XML does not have a root element", url.atom_url);
        return false;
    };
    let root_children = root_element.children();
    let mut entry_elements: Vec<Element> = root_children.iter()
        .filter_map(|c| c.element())
        .filter(|e| e.name() == atom_entry)
        .collect();
    if url.newest_last {
        entry_elements.reverse();
    }

    // for each entry
    let mut run_commands = BTreeSet::new();
    let mut latest_entry_id_opt = None;
    for entry_element in entry_elements {
        let entry_id = child_element_text(&entry_element, &atom_id);
        let entry_title = child_element_text(&entry_element, &atom_title);

        latest_entry_id_opt = Some(entry_id.clone());

        // have we reached this one before?
        let entry_already_known = state.url_to_latest_item
            .get(&url.atom_url)
            .map(|latest_id| latest_id == &entry_id)
            .unwrap_or(false);
        if entry_already_known {
            break;
        }

        // this is a new one; are we interested?
        for matcher in &url.matchers {
            if matcher.title_regex.is_match(&entry_title) {
                run_commands.insert(matcher.command_key.clone());
            }
        }
    }

    // update the newest entry
    if let Some(latest_entry_id) = latest_entry_id_opt {
        state.url_to_latest_item.insert(url.atom_url.clone(), latest_entry_id);
    } else {
        state.url_to_latest_item.remove(&url.atom_url);
    }

    // run commands
    let mut is_good = true;
    for command_name in &run_commands {
        let Some(command_def) = config.commands.get(command_name) else {
            eprintln!("unknown command {:?} specified in {:?}!", command_name, url.atom_url);
            is_good = false;
            continue;
        };

        if command_def.command_args.len() < 1 {
            eprintln!("command {:?} has no args", command_name);
            is_good = false;
            continue;
        }

        let mut cmd = Command::new(&command_def.command_args[0]);
        cmd.args(&command_def.command_args[1..]);
        if let Some(wd) = &command_def.working_dir {
            cmd.current_dir(wd);
        }
        let cmd_status = cmd.status();
        match cmd_status {
            Ok(s) => {
                if !s.success() {
                    eprintln!("command {:?} exited with status {:?}", cmd, s);
                    is_good = false;
                }
            },
            Err(e) => {
                eprintln!("failed to run command {:?}: {}", cmd, e);
                is_good = false;
            },
        }
    }

    is_good
}


async fn run() -> ExitCode {
    let opts = Opts::parse();

    let config: Config = {
        let config_string = std::fs::read_to_string(&opts.config_path)
            .expect("failed to read config file");
        serde_json::from_str(&config_string)
            .expect("failed to parse config file")
    };

    let mut state: State = {
        let path = Path::new(&config.state_file);
        if path.is_file() {
            let state_string = std::fs::read_to_string(&config.state_file)
                .expect("failed to read state file");
            serde_json::from_str(&state_string)
                .expect("failed to parse state file")
        } else {
            State::default()
        }
    };

    let mut is_ok = true;
    for url in &config.urls {
        if !process_url(url, &config, &mut state).await {
            is_ok = false;
            // keep going
        }
    }

    // write out updated state
    let state_json = serde_json::to_string(&state)
        .expect("failed to serialize state");
    std::fs::write(&config.state_file, state_json.as_bytes())
        .expect("failed to write state file");

    if is_ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}


#[tokio::main]
async fn main() -> ExitCode {
    run().await
}
