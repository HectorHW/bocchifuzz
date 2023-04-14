use configuration::PassStyle;
use execution::{PassViaFile, PassViaStdin, RunTrace};
use fuzzing::{DynEval, Fuzzer};
use mutation::{random, RandomMutator};
use ptracer::disable_aslr;
use std::process;

use crate::configuration::{load_config, ConfigReadError};

mod analysys;
mod configuration;
mod execution;
mod flags;
mod fuzzing;
mod generation;
mod grammar;
mod mutation;

fn report_run(new_code: RunTrace) {
    println!("found new interesting sample");
    println!(
        "    run result: {} functions, exit: {}",
        new_code.trajectory.len(),
        new_code.result
    );
    //println!("    sample: {}", print_input(&sample));

    //println!("trajectory: {:?}", new_code.trajectory)
}

fn main() {
    {
        let parsed = crate::grammar::grammar_parser::grammar(
            &std::fs::read_to_string("input.grammar").unwrap(),
        );

        let generator = crate::generation::Generator::new(parsed.unwrap(), 20);

        for _ in 0..10 {
            let result = generator.generate();
            println!("{}", String::from_utf8_lossy(&result.folded))
        }

        return;
    }

    let config = match load_config("fuzz.toml") {
        Ok(config) => config,
        Err(ConfigReadError::ReadError(e)) => {
            eprintln!("failed to read fuzz.toml: {e}");
            process::exit(exitcode::IOERR)
        }

        Err(ConfigReadError::ParseError(e)) => {
            eprintln!("{e}");
            process::exit(exitcode::CONFIG)
        }
    };

    let path = config.binary.path.clone();

    let mapping = match analysys::analyze_binary(&path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error analyzing binary for trace evaluator");
            eprintln!("error: {e}");
            process::exit(exitcode::DATAERR)
        }
    };

    unsafe {
        disable_aslr();
    }

    let generator = Box::new(RandomMutator {
        generation_size: config.generation.population,
        sample_len_limit: config.generation.sample_limit,
    });

    let evaluator: DynEval<_> = if config.stdin.pass_style == PassStyle::File {
        Box::new(execution::TraceEvaluator::<PassViaFile>::new(mapping))
    } else {
        Box::new(execution::TraceEvaluator::<PassViaStdin>::new(mapping))
    };
    let mut fuzzer = Fuzzer::new(generator, evaluator);

    let mut gen = 0;

    loop {
        gen += 1;
        let exec_status = match fuzzer.run_generation() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error executing : {e:?}");
                process::exit(exitcode::NOPERM);
            }
        };

        println!("running generation {gen}");

        for (new_code, _sample) in exec_status.new_codes {
            report_run(new_code)
        }
    }
}
