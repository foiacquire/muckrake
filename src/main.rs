use std::env;
use std::path::Path;

use anyhow::{bail, Result};
use clap::Parser;

use muckrake::cli::scope::extract_scope;
use muckrake::cli::{CategoryCommands, Cli, Commands, InboxCommands, ToolCommands};
use muckrake::context::{discover, resolve_scope, Context};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let raw_args: Vec<String> = env::args().collect();
    let (scope, mut filtered_args) = extract_scope(raw_args);

    if scope.is_some() && filtered_args.len() == 1 {
        bail!(
            "scope prefix requires a subcommand (e.g., mkrk :{} list)",
            scope.as_deref().unwrap_or("")
        );
    }

    rewrite_category_shorthand(&mut filtered_args);
    let cli = Cli::parse_from(&filtered_args);
    let real_cwd = env::current_dir()?;

    if let Some(ref scope_name) = scope {
        if matches!(cli.command, Commands::Init { .. }) {
            bail!("scope prefix cannot be used with 'init'");
        }
        let effective_cwd = resolve_scope(&real_cwd, scope_name)?;
        return dispatch(cli.command, &effective_cwd);
    }

    if should_iterate_projects(&cli.command) {
        if let Some(result) = try_dispatch_workspace(&cli.command, &real_cwd)? {
            return result;
        }
    }

    dispatch(cli.command, &real_cwd)
}

/// When in workspace context, iterate all projects and dispatch the command
/// for each one. Returns `Some(result)` if workspace iteration was performed,
/// `None` if we're not in workspace context.
fn try_dispatch_workspace(command: &Commands, cwd: &Path) -> Result<Option<Result<()>>> {
    let ctx = discover(cwd)?;
    let Context::Workspace {
        workspace_root,
        workspace_db,
    } = ctx
    else {
        return Ok(None);
    };

    let projects = workspace_db.list_projects()?;
    if projects.is_empty() {
        bail!("no projects in workspace");
    }

    let mut attempted = 0;
    let mut succeeded = 0;
    for proj in &projects {
        let proj_root = workspace_root.join(&proj.path);
        if !proj_root.join(".mkrk").exists() {
            continue;
        }
        attempted += 1;
        eprintln!("{}:", proj.name);
        match dispatch(command.clone(), &proj_root) {
            Ok(()) => succeeded += 1,
            Err(e) => eprintln!("  {e}"),
        }
    }
    if attempted > 0 && succeeded == 0 {
        return Ok(Some(Err(anyhow::anyhow!(
            "command failed for all projects"
        ))));
    }
    Ok(Some(Ok(())))
}

/// Whether this command should automatically iterate over all workspace
/// projects when run in workspace context without a scope prefix.
const fn should_iterate_projects(command: &Commands) -> bool {
    match command {
        Commands::Ingest { .. }
        | Commands::Category { .. }
        | Commands::Verify { reference: None }
        | Commands::Tags {
            reference: None, ..
        } => true,
        Commands::List { references } => references.is_empty(),
        Commands::Tool { command } => matches!(
            command,
            ToolCommands::Add { .. } | ToolCommands::Remove { .. }
        ),
        _ => false,
    }
}

/// Rewrite `mkrk category <pattern> ...` → `mkrk category update <pattern> ...`
/// when <pattern> isn't a known subcommand. Allows the user to skip typing `update`.
fn rewrite_category_shorthand(args: &mut Vec<String>) {
    let Some(cat_pos) = args.iter().position(|a| a == "category" || a == "cat") else {
        return;
    };
    let next_pos = cat_pos + 1;
    if next_pos >= args.len() {
        return;
    }
    let next = &args[next_pos];
    if matches!(
        next.as_str(),
        "add" | "update" | "remove" | "help" | "--help" | "-h"
    ) {
        return;
    }
    args.insert(next_pos, "update".to_string());
}

fn dispatch(command: Commands, cwd: &Path) -> Result<()> {
    match command {
        Commands::Init {
            name,
            workspace,
            inbox,
            no_categories,
            categories,
        } => dispatch_init(
            cwd,
            name.as_deref(),
            workspace,
            inbox,
            no_categories,
            &categories,
        ),
        Commands::Status => muckrake::cli::status::run(cwd),
        Commands::Ingest { scope } => muckrake::cli::ingest::run(cwd, scope.as_deref()),
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
        Commands::Category { command } => dispatch_category(cwd, command),
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch_init(
    cwd: &Path,
    name: Option<&str>,
    workspace: Option<String>,
    inbox: bool,
    no_categories: bool,
    categories: &[String],
) -> Result<()> {
    if name.is_some() && workspace.is_some() {
        bail!("cannot specify project name with --workspace");
    }
    if !categories.is_empty() && workspace.is_some() {
        bail!("--category is not supported with --workspace");
    }
    workspace.map_or_else(
        || {
            muckrake::cli::init::run_init_project(
                cwd,
                name,
                no_categories || !categories.is_empty(),
                categories,
            )
        },
        |projects_dir| {
            muckrake::cli::init::run_init_workspace(cwd, &projects_dir, inbox, no_categories)
        },
    )
}

fn dispatch_tool(cwd: &Path, command: ToolCommands) -> Result<()> {
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

fn dispatch_category(cwd: &Path, command: Option<CategoryCommands>) -> Result<()> {
    match command {
        None => muckrake::cli::category::run_list(cwd),
        Some(CategoryCommands::Add {
            pattern,
            category_type,
            protection,
            description,
        }) => muckrake::cli::category::run_add(
            cwd,
            &pattern,
            &category_type,
            &protection,
            description.as_deref(),
        ),
        Some(CategoryCommands::Update {
            current,
            pattern,
            protection,
        }) => muckrake::cli::category::run_update(
            cwd,
            &current,
            pattern.as_deref(),
            protection.as_deref(),
        ),
        Some(CategoryCommands::Remove { pattern }) => {
            muckrake::cli::category::run_remove(cwd, &pattern)
        }
    }
}
