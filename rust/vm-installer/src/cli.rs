use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Clean all build artifacts before building
    #[arg(long)]
    pub clean: bool,
}
