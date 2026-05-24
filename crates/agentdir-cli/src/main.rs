use clap::Parser;

#[derive(Parser)]
#[command(name = "agentdir", version, about = "Virtual filesystem for agent-optimized exploration")]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
    println!("agentdir {}", env!("CARGO_PKG_VERSION"));
}
