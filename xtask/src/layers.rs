//! Layer check: each workspace crate may only depend on the Leon crates its
//! layer allows (`spec/architecture.md`), and only `leon-ui` may use GPUI.

use std::process::Command;

use serde_json::Value;

/// Crate → Leon crates it may depend on. `*` means any.
const RULES: &[(&str, &[&str])] = &[
    ("leon-domain", &[]),
    ("leon-core", &["leon-domain"]),
    ("leon-config", &["leon-domain", "leon-core"]),
    ("leon-store", &["leon-domain", "leon-core"]),
    ("leon-backends", &["leon-domain", "leon-core"]),
    ("leon-catalog", &["leon-domain", "leon-core"]),
    ("leon-git", &["leon-domain", "leon-core"]),
    ("leon-github", &["leon-domain", "leon-core"]),
    ("leon-ui", &["leon-domain", "leon-core"]),
    ("leon", &["*"]),
    ("xtask", &[]),
];

/// Packages only `leon-ui` may depend on (P1).
const UI_ONLY: &[&str] = &["gpui-pre", "gpui-pre-platform", "gpui-component"];

/// Prints every violation; returns `true` when there is none.
pub fn check() -> bool {
    println!("$ layer check");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = match Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
    {
        Ok(output) if output.status.success() => output.stdout,
        Ok(output) => {
            eprintln!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return false;
        }
        Err(error) => {
            eprintln!("could not run cargo metadata: {error}");
            return false;
        }
    };
    let metadata: Value = match serde_json::from_slice(&output) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("cargo metadata output is not JSON: {error}");
            return false;
        }
    };
    let violations = violations(&metadata);
    for violation in &violations {
        eprintln!("layers: {violation}");
    }
    violations.is_empty()
}

fn violations(metadata: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let packages = metadata["packages"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    for package in packages {
        let name = package["name"].as_str().unwrap_or_default();
        let Some((_, allowed)) = RULES.iter().find(|(crate_name, _)| *crate_name == name) else {
            out.push(format!("{name} is not in the layer table of xtask/src/layers.rs; add it there and to spec/architecture.md"));
            continue;
        };
        let deps = package["dependencies"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        for dep in deps
            .iter()
            .filter(|dep| dep["kind"].as_str() != Some("dev"))
        {
            let dep_name = dep["name"].as_str().unwrap_or_default();
            if dep_name.starts_with("leon")
                && !allowed.contains(&"*")
                && !allowed.contains(&dep_name)
            {
                out.push(format!("{name} must not depend on {dep_name}"));
            }
            if UI_ONLY.contains(&dep_name) && name != "leon-ui" {
                out.push(format!(
                    "{name} must not depend on {dep_name}: only leon-ui uses the UI framework"
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::violations;

    fn meta(name: &str, deps: &[(&str, Option<&str>)]) -> serde_json::Value {
        let deps: Vec<_> = deps
            .iter()
            .map(|(n, kind)| json!({ "name": n, "kind": kind }))
            .collect();
        json!({ "packages": [{ "name": name, "dependencies": deps }] })
    }

    #[test]
    fn allowed_dependencies_pass() {
        assert_eq!(
            violations(&meta(
                "leon-store",
                &[("leon-core", None), ("rusqlite", None)]
            )),
            Vec::<String>::new()
        );
    }

    #[test]
    fn wrong_way_dependency_fails() {
        assert_eq!(
            violations(&meta("leon-core", &[("leon-store", None)])).len(),
            1
        );
    }

    #[test]
    fn adapters_cannot_depend_on_each_other() {
        assert_eq!(
            violations(&meta("leon-store", &[("leon-config", None)])).len(),
            1
        );
    }

    #[test]
    fn gpui_outside_the_ui_fails() {
        assert_eq!(
            violations(&meta("leon-core", &[("gpui-pre", None)])).len(),
            1
        );
    }

    #[test]
    fn dev_dependencies_are_ignored() {
        assert_eq!(
            violations(&meta("leon-core", &[("leon-store", Some("dev"))])),
            Vec::<String>::new()
        );
    }

    #[test]
    fn unknown_crate_fails() {
        assert_eq!(violations(&meta("leon-new", &[])).len(), 1);
    }
}
