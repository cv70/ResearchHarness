fn main() {
    if let Err(err) = research_harness::cli::run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
