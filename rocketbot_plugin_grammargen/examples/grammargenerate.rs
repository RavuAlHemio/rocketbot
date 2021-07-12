use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use num_bigint::BigUint;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rocketbot_interface::sync::Mutex;
use rocketbot_plugin_grammargen::grammar::{
    Alternative, GeneratorState, Production, RuleDefinition, SequenceElement, SequenceElementCount,
    SingleSequenceElement,
};
use rocketbot_plugin_grammargen::parsing::parse_grammar;


#[tokio::main]
async fn main() {
    let args_os: Vec<OsString> = env::args_os().collect();
    let grammar_path = PathBuf::from(&args_os[1]);

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

    // add builtins
    let nick_production = Production::new(
        vec![
            Alternative::new(
                Vec::new(),
                BigUint::from(1u32),
                vec![
                    SequenceElement::new(
                        SingleSequenceElement::new_string("SampleNick".to_owned()),
                        SequenceElementCount::One,
                    )
                ],
            )
        ],
    );
    rulebook.rule_definitions.insert(
        "__IRC_nick".to_owned(),
        RuleDefinition::new(
            "__IRC_nick".to_owned(),
            Vec::new(),
            nick_production.clone(),
        ),
    );
    rulebook.rule_definitions.insert(
        "__IRC_chosen_nick".to_owned(),
        RuleDefinition::new(
            "__IRC_chosen_nick".to_owned(),
            Vec::new(),
            nick_production.clone(),
        ),
    );

    let state = GeneratorState::new(
        rulebook,
        HashSet::new(),
        Arc::new(Mutex::new(
            "GeneratorState",
            StdRng::from_entropy(),
        )),
    );

    if let Err(soundness) = state.verify_soundness().await {
        println!("grammar failed soundness check: {}", soundness);
    } else {
        for _ in 0..100 {
            let generated = state.generate().await;
            if let Some(s) = generated {
                println!("> {}", s);
            } else {
                println!("!");
            }
        }
    }
}
