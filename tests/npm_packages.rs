//! npm package layout and the release workflow that publishes it.

use std::fs;
use std::path::PathBuf;

use trammel::canon;
use trammel::json::{self, Value};

const VERSION: &str = "4.0.1";
const PLATFORMS: &[&str] = &[
    "trammel-linux-x64",
    "trammel-linux-arm64",
    "trammel-darwin-x64",
    "trammel-darwin-arm64",
    "trammel-windows-x64",
    "trammel-windows-arm64",
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_json(path: PathBuf) -> Value {
    let text = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    json::parse(&text).unwrap_or_else(|err| panic!("parse {}: {}", path.display(), err.message))
}

fn str_field(value: &Value, key: &str) -> String {
    canon::get(value, key)
        .and_then(canon::as_str)
        .unwrap_or("")
        .to_string()
}

#[test]
fn wrapper_name_commands_and_lookup() {
    let pkg = read_json(root().join("npm/trammel/package.json"));
    assert_eq!(str_field(&pkg, "name"), "@iron_adamant/trammel");
    assert_eq!(str_field(&pkg, "version"), VERSION);
    assert_eq!(str_field(&pkg, "mcpName"), "io.github.IronAdamant/trammel");
    let bin = canon::get(&pkg, "bin").unwrap();
    assert_eq!(str_field(bin, "trammel"), "bin/trammel.js");
    assert_eq!(str_field(bin, "trammel-mcp"), "bin/trammel-mcp.js");
    let deps = canon::get(&pkg, "optionalDependencies").unwrap();
    let Value::Object(pairs) = deps else {
        panic!("optionalDependencies");
    };
    assert_eq!(pairs.len(), PLATFORMS.len());
    for name in PLATFORMS {
        assert_eq!(str_field(deps, &format!("@iron_adamant/{name}")), VERSION);
    }
    for script in ["trammel.js", "trammel-mcp.js"] {
        let text = fs::read_to_string(root().join("npm/trammel/bin").join(script)).unwrap();
        for name in PLATFORMS {
            assert!(
                text.contains(&format!("@iron_adamant/{name}")),
                "{script} missing {name}"
            );
        }
    }
}

#[test]
fn platform_packages_are_scoped() {
    for name in PLATFORMS {
        let pkg = read_json(root().join("npm").join(name).join("package.json"));
        assert_eq!(str_field(&pkg, "name"), format!("@iron_adamant/{name}"));
        assert_eq!(str_field(&pkg, "version"), VERSION);
        let files = canon::get(&pkg, "files").and_then(canon::as_array).unwrap();
        let names: Vec<&str> = files.iter().filter_map(canon::as_str).collect();
        assert!(names.contains(&"trammel"), "{name}");
        assert!(names.contains(&"trammel-mcp"), "{name}");
    }
}

#[test]
fn publish_is_public_and_registry_id_matches() {
    let workflow = fs::read_to_string(root().join(".github/workflows/release.yml")).unwrap();
    let release = workflow
        .split("  github-release:")
        .nth(1)
        .unwrap()
        .split("  npm:")
        .next()
        .unwrap();
    let npm = workflow.split("  npm:").nth(1).unwrap();
    assert!(!release.contains("actions/checkout"));
    for line in release.lines() {
        let stripped = line.trim();
        if stripped.starts_with('#') || !stripped.contains("gh ") {
            continue;
        }
        assert!(
            stripped.contains("--repo \"$repo\""),
            "{stripped}"
        );
    }
    assert!(npm.contains("scope: \"@iron_adamant\""));
    assert!(npm.contains("registry-url: https://registry.npmjs.org"));
    assert!(npm.contains("NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}"));
    assert!(workflow.contains("workflow_dispatch:"));
    assert!(workflow.contains("required: true"));
    assert!(npm.contains("gh release download \"$tag\" --repo \"$GITHUB_REPOSITORY\""));
    assert!(npm.contains("sha256sum -c SHA256SUMS"));
    assert!(npm.contains("npm publish --access public \"$dir\""));
    assert!(!npm.contains("publish \"npm/"));
    assert!(!npm.contains("publish npm/"));
    let platforms = npm.find("publish \"$dir\"").unwrap();
    let wrapper = npm.find("publish ./npm/trammel").unwrap();
    assert!(platforms < wrapper);
    assert!(npm[..wrapper].contains("./npm/"));
    assert!(npm.contains("npm view \"${name}@${version}\""));
    let server = read_json(root().join("server.json"));
    assert_eq!(
        str_field(&server, "$schema"),
        "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json"
    );
    assert_eq!(str_field(&server, "name"), "io.github.IronAdamant/trammel");
    assert_eq!(str_field(&server, "version"), VERSION);
    let description = str_field(&server, "description");
    assert!(!description.is_empty() && description.chars().count() <= 100, "{description}");
    let packages = canon::get(&server, "packages").and_then(canon::as_array).unwrap();
    assert_eq!(packages.len(), 1);
    assert_eq!(str_field(&packages[0], "registryType"), "npm");
    assert_eq!(str_field(&packages[0], "identifier"), "@iron_adamant/trammel");
    assert_eq!(str_field(&packages[0], "version"), VERSION);
    assert_eq!(str_field(&packages[0], "runtimeHint"), "npx");
    let transport = canon::get(&packages[0], "transport").unwrap();
    assert_eq!(str_field(transport, "type"), "stdio");
    let args = canon::get(&packages[0], "packageArguments").and_then(canon::as_array).unwrap();
    assert_eq!(str_field(&args[0], "type"), "positional");
    assert_eq!(str_field(&args[0], "value"), "mcp");
    let cargo = fs::read_to_string(root().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"4.0.1\""));
}
