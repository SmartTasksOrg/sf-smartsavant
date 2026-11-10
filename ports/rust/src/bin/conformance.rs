//! Runs the shared vectors (spec/conformance) against the Rust port.
//!
//!     cargo run --quiet --offline --release --bin conformance [DIR]
//!
//! Prints each failure and a last line "rust: N/N vectors passed"; the exit status is 1 on any failure.
fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "../../spec/conformance".into());
    match smartsavant::conformance::run_dir(&dir) {
        Err(e) => {
            println!("{}", e);
            println!("rust: FAILED");
            std::process::exit(1);
        }
        Ok((total, fails)) => {
            for f in &fails {
                println!("{}", f);
            }
            if fails.is_empty() {
                println!("rust: {}/{} vectors passed", total, total);
            } else {
                println!("rust: {}/{} vectors passed ({} failed)", total - fails.len(), total, fails.len());
                std::process::exit(1);
            }
        }
    }
}
