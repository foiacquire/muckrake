use std::env;

use anyhow::Result;
use clap::Parser;

use muckrake::cli::{Cli, Commands, InboxCommands, ToolCommands};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let cwd = env::current_dir()?;

    dispatch(cli.command, &cwd)
}

fn dispatch(command: Commands, cwd: &std::path::Path) -> Result<()> {
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
        Commands::List { references } => muckrake::cli::list::run(cwd, &references),
        Commands::View { reference } => muckrake::cli::view::run_view(cwd, &reference),
        Commands::Edit { reference } => muckrake::cli::view::run_edit(cwd, &reference),
        Commands::Verify { reference } => muckrake::cli::verify::run(cwd, reference.as_deref()),
        Commands::Categorize {
            reference,
            category,
        } => muckrake::cli::categorize::run(cwd, &reference, &category),
        Commands::Tag { reference, tag } => muckrake::cli::tags::run_tag(cwd, &reference, &tag),
        Commands::Untag { reference, tag } => muckrake::cli::tags::run_untag(cwd, &reference, &tag),
        Commands::Tags {
            reference,
            no_hash_check,
        } => muckrake::cli::tags::run_tags(cwd, reference.as_deref(), no_hash_check),
        Commands::Inbox { command } => match command {
            Some(InboxCommands::Assign {
                file,
                project,
                category,
            }) => muckrake::cli::inbox::run_assign(cwd, &file, &project, category.as_deref()),
            None => muckrake::cli::inbox::run_list(cwd),
        },
        Commands::Projects => muckrake::cli::projects::run(cwd),
        Commands::Tool { command } => dispatch_tool(cwd, command),
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

fn dispatch_tool(cwd: &std::path::Path, command: ToolCommands) -> Result<()> {
    match command {
        ToolCommands::Add {
            name,
            command,
            scope,
            file_type,
            tag,
            env,
            verbose,
        } => {
            let params = muckrake::cli::tool::AddToolParams {
                name: &name,
                command: &command,
                scope: scope.as_deref(),
                file_type: &file_type,
                tag: tag.as_deref(),
                env: env.as_deref(),
                verbose,
            };
            muckrake::cli::tool::run_add(cwd, &params)
        }
        ToolCommands::List => muckrake::cli::tool::run_list(cwd),
        ToolCommands::Remove {
            name,
            scope,
            file_type,
            tag,
        } => muckrake::cli::tool::run_remove(
            cwd,
            &name,
            scope.as_deref(),
            file_type.as_deref(),
            tag.as_deref(),
        ),
        ToolCommands::Run(args) => muckrake::cli::tool::run(cwd, &args),
    }
}
