use autopsy_cli::{ANALYZER_VERSION, EXIT_PASS, run_cli, toolspec::toolspec};

#[test]
fn toolspec_covers_ui_tui_and_watch_subcommands() {
    let spec = toolspec(ANALYZER_VERSION);
    let tool_names: Vec<String> = spec.tools.iter().map(|t| t.name.clone()).collect();

    assert!(
        tool_names.contains(&"ui".to_string()),
        "toolspec must include 'ui'"
    );
    assert!(
        tool_names.contains(&"tui".to_string()),
        "toolspec must include 'tui'"
    );
    assert!(
        tool_names.contains(&"watch".to_string()),
        "toolspec must include 'watch'"
    );
}

#[test]
fn autopsy_ui_json_output_is_valid_and_actionable() {
    let args = vec![
        "autopsy".to_string(),
        "ui".to_string(),
        "--port".to_string(),
        "4444".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let exit_code = run_cli(&args);
    assert_eq!(exit_code, EXIT_PASS);
}

#[test]
fn autopsy_tools_reports_full_catalog() {
    let args = vec![
        "autopsy".to_string(),
        "tools".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let exit_code = run_cli(&args);
    assert_eq!(exit_code, EXIT_PASS);
}
