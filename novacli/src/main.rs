use common::error::NovaError;
use novacore::NovaCore;
use std::process::exit;

fn main() {
    if entry_command().is_none() {
        print_help();
        // TODO: add a repl
    }
}

fn entry_command() -> Option<()> {
    let mut args = std::env::args();
    args.next(); // Skip the file path
    let command = args.next()?;

    let handle_error = |result: Result<(), NovaError>| {
        if let Err(e) = result {
            e.show();
            exit(1);
        }
    };

    let execute_command = |filepath: String, action: fn(NovaCore) -> Result<(), NovaError>| {
        let novacore = compile_file_or_exit(&filepath);
        handle_error(action(novacore));
    };

    match command.as_str() {
        "run" => execute_command(args.next()?, NovaCore::run),
        "dbg" => execute_command(args.next()?, NovaCore::run_debug),
        "dis" => execute_command(args.next()?, NovaCore::dis_file),
        "time" => {
            let filepath = args.next()?;
            let novacore = compile_file_or_exit(&filepath);
            let start_time = std::time::Instant::now();
            let execution_result = novacore.run();
            println!("Execution time: {}ms", start_time.elapsed().as_millis());
            handle_error(execution_result);
        }
        "check" => {
            let filepath = args.next()?;
            let start_time = std::time::Instant::now();
            let novacore = compile_file_or_exit(&filepath);
            handle_error(novacore.check());
            println!("OK | Compile time: {}ms", start_time.elapsed().as_millis());
        }
        _ => print_help(),
    }

    Some(())
}

fn print_help() {
    println!("Nova 0.1.0: by pyrotek45\n");
    println!("HELP MENU");
    println!("\trun   [file]  // runs the file using the nova vm");
    println!("\tdbg   [file]  // debug the file");
    println!("\ttime  [file]  // time the file");
    println!("\tcheck [file]  // check if the file compiles");
    println!("\tdis   [file]  // disassemble the file");
    println!("\thelp          // displays this menu");
}

fn compile_file_or_exit(file: &str) -> NovaCore {
    match novacore::NovaCore::new(file) {
        Ok(novacore) => novacore,
        Err(error) => {
            error.show();
            exit(1);
        }
    }
}
