mod backup;
mod gui;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 1
        || args.iter().any(|a| a == "--help" || a == "-h")
    {
        println!("vzdumper v2.0.0");
        println!();
        println!("Usage: vzdumper [OPTIONS]");
        println!();
        println!("Options:");
        println!("  --install    Install as systemd service (creates config, copies binary, enables services)");
        println!("  --gui        Run the web GUI on port 8889");
        println!("  --help, -h   Show this help message");
        println!();
        println!("Default: Run the backup daemon (waits for configured disk and performs backups)");
        println!();
        println!("Config: /root/.config/vzdumper/config.yaml");
        println!("State:  /run/vzdumper/state.json");
        return;
    }
    if args.iter().any(|a| a == "--install") {
        backup::install();
    } else if args.iter().any(|a| a == "--gui") {
        gui::run();
    } else {
        backup::run();
    }
}   