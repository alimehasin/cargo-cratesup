use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[clap(version)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Cratesup {
        #[arg(short, long)]
        update: bool,
    },
}
