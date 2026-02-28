use std::env;
use std::path::Path;

use anyhow::{bail, Result};
use clap::Parser;

use muckrake::cli::scope::extract_scope;
use muckrake::cli::{
    CategoryCommands, Cli, Commands, InboxCommands, PipelineCommands, RuleCommands,
};
use muckrake::context::{discover, resolve_scope, Context};
use muckrake::models::TriggerEvent;

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
        Commands::List { references, .. } => references.is_empty(),
        Commands::Rule { command } => matches!(command, RuleCommands::List),
        Commands::Pipeline { command } => matches!(
            command,
            PipelineCommands::List | PipelineCommands::Add { .. } | PipelineCommands::Remove { .. }
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

#[allow(clippy::too_many_lines)]
fn dispatch(command: Commands, cwd: &Path) -> Result<()> {
    if !matches!(command, Commands::Init { .. }) {
        fire_lifecycle_events(cwd);
    }

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
        Commands::List {
            references,
            no_hash_check,
        } => muckrake::cli::list::run(cwd, &references, no_hash_check),
        Commands::Read {
            references,
            path,
            query,
            raw,
        } => muckrake::cli::read::run(cwd, &references, path, query, raw),
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
        Commands::Tool { name, args } => muckrake::cli::tool::run(cwd, &name, &args),
        Commands::Category { command } => dispatch_category(cwd, command),
        Commands::Rule { command } => dispatch_rule(cwd, command),
        Commands::Pipeline { command } => dispatch_pipeline(cwd, command),
        Commands::Sign {
            reference,
            sign_name,
            pipeline,
            gpg,
        } => muckrake::cli::sign::run_sign(cwd, &reference, &sign_name, pipeline.as_deref(), gpg),
        Commands::Unsign {
            reference,
            sign_name,
            pipeline,
        } => muckrake::cli::sign::run_unsign(cwd, &reference, &sign_name, pipeline.as_deref()),
        Commands::Signs { reference } => muckrake::cli::sign::run_signs(cwd, reference.as_deref()),
        Commands::State {
            reference,
            pipeline,
        } => muckrake::cli::sign::run_state(cwd, reference.as_deref(), pipeline.as_deref()),
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

fn dispatch_category(cwd: &Path, command: Option<CategoryCommands>) -> Result<()> {
    match command {
        None => muckrake::cli::category::run_list(cwd),
        Some(CategoryCommands::Add {
            name,
            pattern,
            category_type,
            protection,
            description,
        }) => {
            let params = muckrake::cli::category::AddCategoryParams {
                name: &name,
                pattern: pattern.as_deref(),
                category_type: &category_type,
                protection: &protection,
                description: description.as_deref(),
            };
            muckrake::cli::category::run_add(cwd, &params)
        }
        Some(CategoryCommands::Update {
            name,
            pattern,
            protection,
        }) => muckrake::cli::category::run_update(
            cwd,
            &name,
            pattern.as_deref(),
            protection.as_deref(),
        ),
        Some(CategoryCommands::Remove { name }) => muckrake::cli::category::run_remove(cwd, &name),
    }
}

fn dispatch_pipeline(cwd: &Path, command: PipelineCommands) -> Result<()> {
    match command {
        PipelineCommands::Add {
            name,
            states,
            transitions,
        } => muckrake::cli::pipeline::run_add(cwd, &name, &states, transitions.as_deref()),
        PipelineCommands::List => muckrake::cli::pipeline::run_list(cwd),
        PipelineCommands::Remove { name } => muckrake::cli::pipeline::run_remove(cwd, &name),
        PipelineCommands::Attach {
            pipeline,
            category,
            tag,
        } => {
            muckrake::cli::pipeline::run_attach(cwd, &pipeline, category.as_deref(), tag.as_deref())
        }
        PipelineCommands::Detach {
            pipeline,
            category,
            tag,
        } => {
            muckrake::cli::pipeline::run_detach(cwd, &pipeline, category.as_deref(), tag.as_deref())
        }
    }
}

fn dispatch_rule(cwd: &Path, command: RuleCommands) -> Result<()> {
    match command {
        RuleCommands::Add {
            name,
            on,
            action,
            tool,
            tag,
            category,
            mime_type,
            file_type,
            trigger_tag,
            trigger_pipeline,
            trigger_sign,
            trigger_state,
            pipeline,
            sign_name,
            priority,
        } => {
            let params = muckrake::cli::rule::AddRuleParams {
                name: &name,
                on: &on,
                action: &action,
                tool: tool.as_deref(),
                tag: tag.as_deref(),
                category: category.as_deref(),
                mime_type: mime_type.as_deref(),
                file_type: file_type.as_deref(),
                trigger_tag: trigger_tag.as_deref(),
                trigger_pipeline: trigger_pipeline.as_deref(),
                trigger_sign: trigger_sign.as_deref(),
                trigger_state: trigger_state.as_deref(),
                pipeline: pipeline.as_deref(),
                sign_name: sign_name.as_deref(),
                priority,
            };
            muckrake::cli::rule::run_add(cwd, &params)
        }
        RuleCommands::List => muckrake::cli::rule::run_list(cwd),
        RuleCommands::Remove { name } => muckrake::cli::rule::run_remove(cwd, &name),
        RuleCommands::Enable { name } => muckrake::cli::rule::run_enable(cwd, &name),
        RuleCommands::Disable { name } => muckrake::cli::rule::run_disable(cwd, &name),
    }
}

fn fire_lifecycle_events(cwd: &Path) {
    let Ok(ctx) = discover(cwd) else {
        return;
    };
    if ctx.require_project().is_err() {
        return;
    }
    if let Context::Project {
        workspace: Some(_), ..
    } = &ctx
    {
        muckrake::rules::fire_lifecycle_rules(&ctx, TriggerEvent::WorkspaceEnter);
    }
    muckrake::rules::fire_lifecycle_rules(&ctx, TriggerEvent::ProjectEnter);
}
