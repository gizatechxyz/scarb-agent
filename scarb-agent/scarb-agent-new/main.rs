use anyhow::Result;
use camino::Utf8PathBuf;
use clap::Parser;
use colored::*;
use dialoguer::{theme::ColorfulTheme, Confirm};
use new::{new_package, InitOptions, VersionControl};
use scarb::core::{Config, PackageName};
use scarb::ops;

mod fsx;
mod new;
mod new_cairo;
mod new_python;
mod restricted_names;
mod templates;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(value_parser)]
    path: Utf8PathBuf,
    #[clap(long = "name", value_parser)]
    name: Option<PackageName>,
}

pub(crate) struct ProjectConfig {
    preprocess: bool,
    postprocess: bool,
    agent_api: bool,
    oracle: bool,
}

fn run(args: Args, config: &Config) -> Result<()> {
    print_welcome_message();

    let project_config = get_project_config()?;
    let result = new_package(
        InitOptions {
            name: args.name,
            path: args.path,
            vcs: VersionControl::Git,
        },
        config,
        &project_config,
    )?;

    println!("\n{}", "Project created successfully! 🥳".green().bold());
    println!("Project name: {}", result.name.to_string().cyan());
    println!("\n{}", "Next steps:".yellow());
    println!("1. cd into your project directory");
    println!(
        "2. Run {} to generate agent code",
        "`scarb agent-generate`".cyan()
    );
    println!("3. Run {} to build your project", "`scarb build`".cyan());

    Ok(())
}

fn get_project_config() -> Result<ProjectConfig> {
    let theme = ColorfulTheme::default();
    println!("\n{}", "Project Configuration:".yellow().bold());

    let preprocess = Confirm::with_theme(&theme)
        .with_prompt("Do you plan preprocessing in your project?")
        .interact()?;
    let postprocess = Confirm::with_theme(&theme)
        .with_prompt("Do you plan postprocessing in your project?")
        .interact()?;
    let agent_api = Confirm::with_theme(&theme)
        .with_prompt("Are you planning to call a smart contract through the Agent-API?")
        .interact()?;
    let oracle = Confirm::with_theme(&theme)
        .with_prompt("Are you planning to create and interact with an Oracle?")
        .interact()?;

    Ok(ProjectConfig {
        preprocess,
        postprocess,
        agent_api,
        oracle,
    })
}

fn print_welcome_message() {
    let ascii_art = r#"

    _____                _                                _   
    / ____|              | |         /\                   | |  
   | (___   ___ __ _ _ __| |__      /  \   __ _  ___ _ __ | |_ 
    \___ \ / __/ _` | '__| '_ \    / /\ \ / _` |/ _ | '_ \| __|
    ____) | (_| (_| | |  | |_) |  / ____ | (_| |  __| | | | |_ 
   |_____/ \___\__,_|_|  |_.__/  /_/    \_\__, |\___|_| |_|\__|
                                           __/ |               
                                          |___/                
    
    "#;
    println!("{}", ascii_art.bright_cyan());
    println!("{}", "Welcome to Scarb Agent!".green().bold());
    println!(
        "\n{}",
        "Scarb Agent is all you need to build provable agents ready for deployment on the Giza platform."
            .bright_yellow()
    );
    println!(
        "{}",
        "Prove only what you need to prove! Scarb Agent makes it easy to implement Cairo programs that can interact with custom oracles."
            .bright_yellow()
    );
    println!("\n{}", "Let's set up your new project.".bright_yellow());

    println!("{}", "-------------------------------------".bright_blue());
}

fn main() {
    let args: Args = Args::parse();
    let manifest_path = ops::find_manifest_path(None).unwrap();
    let config = Config::builder(manifest_path).build().unwrap();
    if let Err(err) = run(args, &config) {
        eprintln!("{}: {}", "Error".red().bold(), err);
        std::process::exit(1);
    }
}
