use clap::Parser;

#[derive(Parser)]
#[command(name = "contrib", version, about)]
struct Cli {}

fn main() {
    Cli::parse();
}
