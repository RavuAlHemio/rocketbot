use std::env;
use std::io::BufRead;

use rocketbot_spelling::{HunspellEngine, SpellingEngine};
use serde_json;


fn run() -> i32 {
    // set up tracing
    let (stderr_non_blocking, _guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
        .lossy(false)
        .finish(std::io::stderr());
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(stderr_non_blocking)
        .init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        let prog_name = args.get(0)
            .map(|s| s.as_str())
            .unwrap_or("spell_suggest");
        eprintln!("Usage: {} AFFIX DICTIONARY...", prog_name);
        return 1;
    }

    let add_dicts: Vec<String> = args.iter().skip(3).map(|d| d.clone()).collect();
    let config = serde_json::json!({
        "dictionaries": [
            {
                "affix": &args[1],
                "dict": &args[2],
                "additional_dicts": add_dicts,
            },
        ],
    });

    let spelling = HunspellEngine::new(config)
        .expect("failed to initialize Hunspell engine");

    {
        let stdin = std::io::stdin();
        let mut stdin_lock = stdin.lock();
        let mut line = String::new();
        loop {
            line.clear();
            let byte_count = stdin_lock.read_line(&mut line)
                .expect("failed to read");
            if byte_count == 0 {
                break;
            }
            let trimmed_line = line.trim();

            if spelling.is_correct(trimmed_line) {
                println!("!OK");
            } else {
                let suggestions = spelling.suggest(trimmed_line);
                for suggestion in suggestions {
                    println!(">{}", suggestion);
                }
            }
        }
    }

    0
}


fn main() {
    std::process::exit(run());
}
