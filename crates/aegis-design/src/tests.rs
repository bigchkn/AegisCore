use crate::{BootstrapContext, DesignEngine, TemplateRegistry};
use std::collections::HashMap;
use std::path::Path;
use tempfile::TempDir;

fn write_file(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn simple_vars() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("project_root".into(), "/tmp/project".into());
    m.insert("milestone_id".into(), "20".into());
    m.insert("milestone_name".into(), "Template Engine".into());
    m.insert(
        "lld_path".into(),
        "/tmp/project/.aegis/designs/lld/engine.md".into(),
    );
    m.insert(
        "bastion_agent_id".into(),
        "00000000-0000-0000-0000-000000000001".into(),
    );
    m.insert(
        "task_description".into(),
        "Implement TemplateRegistry".into(),
    );
    m
}

// --- Registry tests ---

#[test]
fn builtin_templates_load() {
    let dir = TempDir::new().unwrap();
    let reg = TemplateRegistry::load(dir.path());
    let names: Vec<_> = reg.list().iter().map(|t| t.name.as_str()).collect();
    assert!(
        names.contains(&"taskflow-bastion"),
        "taskflow-bastion missing"
    );
    assert!(
        names.contains(&"taskflow-splinter"),
        "taskflow-splinter missing"
    );
}

#[test]
fn registry_get_missing_returns_error() {
    let dir = TempDir::new().unwrap();
    let reg = TemplateRegistry::load(dir.path());
    assert!(reg.get("does-not-exist").is_err());
}

#[test]
fn project_layer_shadows_builtin() {
    let dir = TempDir::new().unwrap();
    // Create a project-local template with the same name as a built-in.
    write_file(
        dir.path(),
        ".aegis/templates/taskflow-bastion/template.toml",
        include_str!("builtin/taskflow-bastion/template.toml"),
    );
    write_file(
        dir.path(),
        ".aegis/templates/taskflow-bastion/system_prompt.md",
        "# Project override",
    );

    let reg = TemplateRegistry::load(dir.path());
    let resolved = reg.get("taskflow-bastion").unwrap();
    assert_eq!(resolved.layer, crate::TemplateLayer::Project);
    assert_eq!(resolved.template.system_prompt.trim(), "# Project override");
}

// --- Engine tests ---

#[test]
fn render_substitutes_all_vars() {
    let dir = TempDir::new().unwrap();
    let reg = TemplateRegistry::load(dir.path());
    let t = &reg.get("taskflow-bastion").unwrap().template;
    let vars = simple_vars();
    let rendered = DesignEngine::render(t, &vars).unwrap();
    assert!(!rendered.system_prompt.contains("{{milestone_id}}"));
    assert!(!rendered.system_prompt.contains("{{milestone_name}}"));
    assert!(rendered.system_prompt.contains("20"));
    assert!(rendered.system_prompt.contains("Template Engine"));
}

#[test]
fn render_missing_required_var_is_error() {
    let dir = TempDir::new().unwrap();
    let reg = TemplateRegistry::load(dir.path());
    let t = &reg.get("taskflow-bastion").unwrap().template;
    // Omit milestone_id (required).
    let mut vars = simple_vars();
    vars.remove("milestone_id");
    let err = DesignEngine::render(t, &vars).unwrap_err();
    assert!(matches!(err, crate::DesignError::UnresolvedRequired { .. }));
}

#[test]
fn render_optional_var_absent_becomes_empty() {
    let dir = TempDir::new().unwrap();
    // Minimal template with one optional var.
    write_file(
        dir.path(),
        ".aegis/templates/test-optional/template.toml",
        r#"
[template]
name = "test-optional"
description = "test"
kind = "bastion"
version = "1"

[agent]
role = "tester"
cli_provider = "claude-code"

[variables]
required = ["project_root"]
optional = ["extra_note"]
"#,
    );
    write_file(
        dir.path(),
        ".aegis/templates/test-optional/system_prompt.md",
        "Root: {{project_root}} Note: {{extra_note}}",
    );

    let reg = TemplateRegistry::load(dir.path());
    let t = &reg.get("test-optional").unwrap().template;
    let mut vars = HashMap::new();
    vars.insert("project_root".into(), "/tmp".into());
    // extra_note not provided — should resolve to empty string.
    let rendered = DesignEngine::render(t, &vars).unwrap();
    assert_eq!(rendered.system_prompt, "Root: /tmp Note: ");
}

#[test]
fn render_unresolved_placeholder_in_prose_is_error() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        ".aegis/templates/test-unknown/template.toml",
        r#"
[template]
name = "test-unknown"
description = "test"
kind = "bastion"
version = "1"

[agent]
role = "tester"
cli_provider = "claude-code"

[variables]
required = ["project_root"]
"#,
    );
    // system_prompt contains a var not declared anywhere.
    write_file(
        dir.path(),
        ".aegis/templates/test-unknown/system_prompt.md",
        "Hello {{undeclared_var}}",
    );

    let reg = TemplateRegistry::load(dir.path());
    let t = &reg.get("test-unknown").unwrap().template;
    let mut vars = HashMap::new();
    vars.insert("project_root".into(), "/tmp".into());
    let err = DesignEngine::render(t, &vars).unwrap_err();
    assert!(matches!(
        err,
        crate::DesignError::UnresolvedPlaceholders { .. }
    ));
}

// --- BootstrapContext tests ---

#[test]
fn bootstrap_reads_current_milestone_from_index() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        ".aegis/designs/roadmap/index.toml",
        r#"
[project]
name = "TestProject"
current_milestone = 7

[milestones.M7]
path = "milestones/M7.toml"
status = "in-progress"
"#,
    );
    write_file(
        dir.path(),
        ".aegis/designs/roadmap/milestones/M7.toml",
        r#"
id = 7
name = "My Milestone"
status = "in-progress"
lld = "lld/my-lld.md"
"#,
    );

    let reg = TemplateRegistry::load(dir.path());
    let t = &reg.get("taskflow-bastion").unwrap().template;
    let vars = BootstrapContext::build(t, dir.path(), &HashMap::new(), None).unwrap();

    assert_eq!(vars.get("milestone_id").unwrap(), "7");
    assert_eq!(vars.get("milestone_name").unwrap(), "My Milestone");
    assert!(vars.get("lld_path").unwrap().ends_with("my-lld.md"));
}

#[test]
fn bootstrap_cli_vars_override_taskflow() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        ".aegis/designs/roadmap/index.toml",
        r#"
[project]
name = "TestProject"
current_milestone = 5

[milestones.M5]
path = "milestones/M5.toml"
status = "in-progress"
"#,
    );
    write_file(
        dir.path(),
        ".aegis/designs/roadmap/milestones/M5.toml",
        r#"id = 5; name = "Five"; status = "in-progress""#,
    );

    let reg = TemplateRegistry::load(dir.path());
    let t = &reg.get("taskflow-bastion").unwrap().template;

    let mut cli_vars = HashMap::new();
    cli_vars.insert("milestone_id".into(), "99".into());
    let vars = BootstrapContext::build(t, dir.path(), &cli_vars, None).unwrap();

    assert_eq!(vars.get("milestone_id").unwrap(), "99");
}

#[test]
fn bootstrap_injects_bastion_agent_id() {
    let dir = TempDir::new().unwrap();
    let reg = TemplateRegistry::load(dir.path());
    let t = &reg.get("taskflow-splinter").unwrap().template;
    let vars = BootstrapContext::build(
        t,
        dir.path(),
        &HashMap::new(),
        Some("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"),
    )
    .unwrap();

    assert_eq!(
        vars.get("bastion_agent_id").unwrap(),
        "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
    );
}
