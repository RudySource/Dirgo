fn main() {
    if version_requested() {
        match dirgo::update::print_version() {
            Ok(code) => std::process::exit(code),
            Err(error) => {
                eprintln!("Dirgo: {}", dirgo::terminal::safe_text(&error.to_string()));
                std::process::exit(1);
            }
        }
    }
    match dirgo::app::run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            let message = error.to_string();
            eprintln!("Dirgo: {}", dirgo::terminal::safe_text(&message));
            if std::env::var_os("DGO_LOG").is_some() {
                eprintln!("\nDebug: {error:?}");
            }
            std::process::exit(1);
        }
    }
}

fn version_requested() -> bool {
    std::env::args_os()
        .skip(1)
        .take_while(|argument| argument != "--")
        .any(|argument| argument == "--version" || argument == "-V")
}
