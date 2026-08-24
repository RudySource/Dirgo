fn main() {
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
