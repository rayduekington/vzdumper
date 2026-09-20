mod backup;
mod gui;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--install") {
        backup::install();
    } else if args.iter().any(|a| a == "--gui") {
        gui::run();
    } else {
        backup::run();
    }
}   