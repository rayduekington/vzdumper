mod backup;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--install") {
        backup::install();
    } else {
        backup::run();
    }
}   