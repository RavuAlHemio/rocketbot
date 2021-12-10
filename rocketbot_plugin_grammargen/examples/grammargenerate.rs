use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use clap::Parser;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rocketbot_plugin_grammargen::grammar::{GeneratorState, Metacommand};
use rocketbot_plugin_grammargen::parsing::parse_grammar;


#[derive(Parser)]
struct Opts {
    #[clap(short = 'n', long = "no-verify")]
    pub no_verify: bool,

    #[clap(short = 'g', long = "output-grammar")]
    pub output_grammar: bool,

    #[clap(short = 's', long = "seed")]
    pub seed: Option<u64>,

    pub grammar_path: PathBuf,
}
impl Opts {
    pub fn verify(&self) -> bool { !self.no_verify }
}


fn main() {
    let opts: Opts = Parser::parse();
    /*
    let opts: Opts = Opts {
        no_verify: false,
        output_grammar: false,
        seed: Some(0),
        grammar_path: PathBuf::from(r".\rocketbot_plugin_grammargen\grammars\eqn.grammar"),
    };
    */

    let grammar_name = opts.grammar_path.file_stem()
        .expect("grammar name cannot be derived from file name")
        .to_str()
        .expect("grammar name is not valid Unicode")
        .to_owned();

    let grammar_str = {
        let mut grammar_file = File::open(&opts.grammar_path)
            .expect("failed to open grammar file");

        let mut grammar_string = String::new();
        grammar_file.read_to_string(&mut grammar_string)
            .expect("failed to read grammar file");

        grammar_string
    };

    // parse the string
    let mut rulebook = parse_grammar(&grammar_name, &grammar_str)
        .expect("failed to parse grammar");

    if opts.output_grammar {
        println!("{:#?}", rulebook);
    }

    // add builtins
    let mut nicks = HashSet::new();
    nicks.insert("SampleNick".to_owned());
    rulebook.add_builtins(&nicks, Some("SampleNick"));

    let mut rng = if let Some(seed) = opts.seed {
        StdRng::seed_from_u64(seed)
    } else {
        StdRng::from_entropy()
    };
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

    let start_production = rulebook.rule_definitions
        .get(&grammar_name).unwrap()
        .top_production;
    let mut state = GeneratorState::new_topmost(
        rulebook.clone(),
        start_production,
        conditions,
        rng,
    );

    if opts.verify() {
        if let Err(soundness) = rocketbot_plugin_grammargen::grammar::verify(&mut state) {
            println!("grammar failed soundness check: {}", soundness);
            return;
        }
        state.prepare_again(rulebook.clone(), start_production);
    }

    for _ in 0..100 {
        match rocketbot_plugin_grammargen::grammar::generate(&mut state) {
            Ok(g) => println!("> {}", g),
            Err(e) => println!("! {}", e),
        };
        state.prepare_again(rulebook.clone(), start_production);
    }
}
