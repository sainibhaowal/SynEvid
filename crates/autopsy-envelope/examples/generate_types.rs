use autopsy_envelope::ts_types::{write_typescript_definitions, write_envelope_json_schema};
use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get workspace root from CARGO_MANIFEST_DIR
    let workspace_root = env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    
    println!("Workspace root: {:?}", workspace_root);
    
    // Generate for web app
    let web_types_dir = workspace_root.join("apps/web/src/types");
    std::fs::create_dir_all(&web_types_dir)?;
    write_typescript_definitions(&web_types_dir)?;
    write_envelope_json_schema(&web_types_dir)?;
    println!("Generated web types at {:?}", web_types_dir);
    
    // Generate for electron app
    let electron_types_dir = workspace_root.join("apps/desktop-electron/src/types");
    std::fs::create_dir_all(&electron_types_dir)?;
    write_typescript_definitions(&electron_types_dir)?;
    write_envelope_json_schema(&electron_types_dir)?;
    println!("Generated electron types at {:?}", electron_types_dir);
    
    // Generate for schemas directory
    let schemas_dir = workspace_root.join("schemas");
    std::fs::create_dir_all(&schemas_dir)?;
    write_envelope_json_schema(&schemas_dir)?;
    println!("Generated schema at {:?}", schemas_dir);
    
    Ok(())
}