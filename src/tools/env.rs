use std::collections::HashMap;

use console::style;

const DEFAULT_PROXY: &str = "socks5h://127.0.0.1:9050";

pub fn build_tool_env(
    env_overrides: Option<&str>,
    command_name: &str,
) -> HashMap<String, Option<String>> {
    let mut env: HashMap<String, Option<String>> = HashMap::new();
    env.insert("ALL_PROXY".to_string(), Some(DEFAULT_PROXY.to_string()));
    env.insert("HTTPS_PROXY".to_string(), Some(DEFAULT_PROXY.to_string()));
    env.insert("HTTP_PROXY".to_string(), Some(DEFAULT_PROXY.to_string()));

    let mut privacy_removed = false;

    if let Some(overrides_json) = env_overrides {
        if let Ok(overrides) =
            serde_json::from_str::<HashMap<String, serde_json::Value>>(overrides_json)
        {
            for (key, value) in overrides {
                if value.is_null() {
                    env.insert(key.clone(), None);
                    if is_proxy_var(&key) {
                        privacy_removed = true;
                    }
                } else if let Some(s) = value.as_str() {
                    if is_proxy_var(&key) && s != DEFAULT_PROXY {
                        privacy_removed = true;
                    }
                    env.insert(key, Some(s.to_string()));
                }
            }
        }
    }

    print_privacy_notice(command_name, privacy_removed);
    env
}

fn is_proxy_var(key: &str) -> bool {
    matches!(
        key.to_uppercase().as_str(),
        "ALL_PROXY" | "HTTPS_PROXY" | "HTTP_PROXY"
    )
}

fn print_privacy_notice(command_name: &str, privacy_removed: bool) {
    if privacy_removed {
        eprintln!(
            "{} Running \"{}\" without privacy protections (by request)",
            style("!").red().bold(),
            command_name
        );
    } else {
        eprintln!(
            "{} Running \"{}\" with proxy environment ({})",
            style("→").cyan(),
            command_name,
            DEFAULT_PROXY
        );
    }
    eprintln!(
        "{} mkrk cannot guarantee that \"{}\" respects proxy settings.",
        style("⚠").yellow(),
        command_name
    );
    eprintln!("  Verify this tool does not leak identifying information.");
}

pub fn confirm_privacy_removal(command_name: &str, env_json: &str) -> anyhow::Result<()> {
    let overrides: HashMap<String, serde_json::Value> =
        serde_json::from_str(env_json).map_err(|e| anyhow::anyhow!("invalid env JSON: {e}"))?;

    let removes_proxy = overrides
        .iter()
        .any(|(key, value)| is_proxy_var(key) && value.is_null());

    if !removes_proxy {
        return Ok(());
    }

    eprintln!(
        "{} This configuration removes proxy environment variables for \"{}\".",
        style("!").red().bold(),
        command_name
    );
    eprintln!("  The tool will run WITHOUT Tor or any proxy, exposing your IP address.");
    eprintln!();
    eprint!("  Type \"I understand the risk\" to continue: ");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    if input.trim() != "I understand the risk" {
        anyhow::bail!("privacy removal not confirmed");
    }

    Ok(())
}

pub fn apply_env(cmd: &mut std::process::Command, env: &HashMap<String, Option<String>>) {
    for (key, value) in env {
        match value {
            Some(v) => {
                cmd.env(key, v);
            }
            None => {
                cmd.env_remove(key);
            }
        }
    }
}
