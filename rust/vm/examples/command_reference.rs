//! Generate the public command inventory from the Clap command model.

#[allow(dead_code)]
#[path = "../src/cli/mod.rs"]
mod cli;

use clap::{Command, CommandFactory};

fn main() {
    let root = cli::Args::command();
    println!("# VM command inventory\n");
    println!("Generated from the CLI parser. Run `vm help <command>` for options and examples.\n");
    println!("| Command | Purpose |");
    println!("| --- | --- |");
    rows(&root, "vm");
}

fn rows(command: &Command, parent: &str) {
    for subcommand in command
        .get_subcommands()
        .filter(|subcommand| !subcommand.is_hide_set())
    {
        let path = format!("{parent} {}", subcommand.get_name());
        let about = subcommand
            .get_about()
            .map(ToString::to_string)
            .unwrap_or_default()
            .replace('|', "\\|")
            .replace('\n', " ");
        println!("| `{path}` | {about} |");
        rows(subcommand, &path);
    }
}
