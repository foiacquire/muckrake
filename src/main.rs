use std::env;

use anyhow::Result;
use clap::Parser;

use muckrake::cli::{Cli, Commands, InboxCommands};
use muckrake::context::Scope;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let args: Vec<String> = env::args().collect();
    let (scope, filtered_args) = extract_scope(&args);
    let cli = Cli::parse_from(filtered_args);
    let cwd = env::current_dir()?;

    dispatch(cli.command, &cwd, &scope)
}

fn dispatch(command: Commands, cwd: &std::path::Path, scope: &Scope) -> Result<()> {
    match command {
        Commands::Init {
            workspace,
            inbox,
            no_categories,
            categories,
        } => dispatch_init(cwd, workspace, inbox, no_categories, &categories),
        Commands::Status => muckrake::cli::status::run(cwd),
        Commands::Ingest { paths, category } => {
            muckrake::cli::ingest::run(cwd, &paths, category.as_deref())
        }
        Commands::List { tag } => muckrake::cli::list::run(cwd, scope, tag.as_deref()),
        Commands::View { name } => muckrake::cli::view::run_view(cwd, &name),
        Commands::Edit { name } => muckrake::cli::view::run_edit(cwd, &name),
        Commands::Verify { name } => muckrake::cli::verify::run(cwd, name.as_deref()),
        Commands::Categorize { name, category } => {
            muckrake::cli::categorize::run(cwd, &name, &category)
        }
        Commands::Tag { name, tag } => muckrake::cli::tags::run_tag(cwd, &name, &tag),
        Commands::Untag { name, tag } => muckrake::cli::tags::run_untag(cwd, &name, &tag),
        Commands::Tags { name } => muckrake::cli::tags::run_tags(cwd, name.as_deref()),
        Commands::Inbox { command } => match command {
            Some(InboxCommands::Assign {
                file,
                project,
                category,
            }) => muckrake::cli::inbox::run_assign(cwd, &file, &project, category.as_deref()),
            None => muckrake::cli::inbox::run_list(cwd),
        },
        Commands::Projects => muckrake::cli::projects::run(cwd),
    }
}

fn dispatch_init(
    cwd: &std::path::Path,
    workspace: Option<String>,
    inbox: bool,
    no_categories: bool,
    categories: &[String],
) -> Result<()> {
    workspace.map_or_else(
        || {
            muckrake::cli::init::run_init_project(
                cwd,
                no_categories || !categories.is_empty(),
                categories,
            )
        },
        |projects_dir| {
            muckrake::cli::init::run_init_workspace(cwd, &projects_dir, inbox, no_categories)
        },
    )
}

fn extract_scope(args: &[String]) -> (Scope, Vec<String>) {
    let mut filtered = Vec::new();
    let mut scope = Scope::Current;
    let mut found_scope = false;
    let mut found_command = false;

    for (i, arg) in args.iter().enumerate() {
        if i == 0 {
            filtered.push(arg.clone());
            continue;
        }

        if !found_scope && !found_command && arg.starts_with(':') {
            if let Ok(s) = Scope::parse(arg) {
                scope = s;
                found_scope = true;
                continue;
            }
        }

        if !arg.starts_with('-') {
            found_command = true;
        }

        filtered.push(arg.clone());
    }

    (scope, filtered)
}
