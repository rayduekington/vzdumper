mod backup;
mod gui;
mod api;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("vzdumper v2.5.1");
        println!();
        println!("Usage: vzdumper [OPTIONS]");
        println!();
        println!("Options:");
        println!("  --install      Install as systemd service (creates config, copies binary, enables services)");
        println!("  --update       Update existing installation with this binary");
        println!("  --uninstall    Uninstall vzdumper (remove binary, config, services)");
        println!("  --gui          Run the web GUI on port 8889");
        println!("  --list-disks   List available disks");
        println!("  --list-vms     List available VMs and containers");
        println!("  --add-target   Add backup target (use with --uuid and --label)");
        println!("  --add-profile  Create backup profile (use with --disk, --vms, --retention)");
        println!("  --help, -h     Show this help message");
        println!();
        println!("Default: Run the backup daemon (waits for configured disk and performs backups)");
        println!();
        println!("Config: /root/.config/vzdumper/config.yaml");
        println!("State:  /run/vzdumper/state.json");
        return;
    }
    if args.iter().any(|a| a == "--install") {
        backup::install();
    } else if args.iter().any(|a| a == "--update") {
        backup::update();
    } else if args.iter().any(|a| a == "--uninstall") {
        backup::uninstall();
    } else if args.iter().any(|a| a == "--gui") {
        gui::run();
    } else if args.iter().any(|a| a == "--list-disks") {
        api::list_disks();
    } else if args.iter().any(|a| a == "--list-vms") {
        api::list_vms();
    } else if args.iter().any(|a| a == "--add-target") {
        api::add_target(&args);
    } else if args.iter().any(|a| a == "--add-profile") {
        api::add_profile(&args);
    } else {
        backup::run();
    }
}   