use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_plugin_grammargen::grammar::{GeneratorState, Metacommand};
use rocketbot_plugin_grammargen::parsing::parse_grammar;


#[tokio::main]
async fn main() {
    let args_os: Vec<OsString> = env::args_os().collect();
    let mut verify = true;
    let mut output_grammar = false;
    let mut path_index: usize = 1;
    loop {
        if args_os.len() < path_index {
            eprintln!("Usage: grammargenerate [--no-verify|--output-grammar]... GRAMMAR");
            return;
        }

        if args_os[path_index] == "--no-verify" {
            verify = false;
            path_index += 1;
            continue;
        } else if args_os[path_index] == "--output-grammar" {
            output_grammar = true;
            path_index += 1;
            continue;
        }

        // probably an actual file name
        break;
    }
    let grammar_path = PathBuf::from(&args_os[path_index]);

    let grammar_name = grammar_path.file_stem()
        .expect("grammar name cannot be derived from file name")
        .to_str()
        .expect("grammar name is not valid Unicode")
        .to_owned();

    let grammar_str = {
        let mut grammar_file = File::open(&grammar_path)
            .expect("failed to open grammar file");

        let mut grammar_string = String::new();
        grammar_file.read_to_string(&mut grammar_string)
            .expect("failed to read grammar file");

        grammar_string
    };

    // parse the string
    let mut rulebook = parse_grammar(&grammar_name, &grammar_str)
        .expect("failed to parse grammar");

    if output_grammar {
        println!("{:#?}", rulebook);
    }

    // add builtins
    let mut nicks = HashSet::new();
    nicks.insert("SampleNick".to_owned());
    rulebook.add_builtins(&nicks, Some("SampleNick"));

    let mut rng = StdRng::from_entropy();
    let mut conditions = HashSet::new();

    // process metacommands
    for mcmd in &rulebook.metacommands {
        match mcmd {
            Metacommand::RandomizeCondition(flag) => {
                let activate_flag: bool = rng.gen();
                if activate_flag {
                    conditions.insert(flag.clone());
                }
            },
        }
    }

    let mut state = GeneratorState::new_topmost(
        rulebook,
        conditions,
        rng,
    );

    if verify {
        if let Err(soundness) = state.verify_soundness() {
            println!("grammar failed soundness check: {}", soundness);
            return;
        }
    }

    for _ in 0..100 {
        match state.generate() {
            Ok(s) => println!("> {}", s),
            Err(e) => println!("! {}", e),
        }
    }
}
