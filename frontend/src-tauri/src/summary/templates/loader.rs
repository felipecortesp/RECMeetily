use super::defaults;
use super::types::Template;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use tracing::{debug, info, warn};

// Global storage for the bundled templates directory path
static BUNDLED_TEMPLATES_DIR: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));

/// Maximum length of a template id.
pub const MAX_TEMPLATE_ID_LEN: usize = 64;

/// Where the effective version of a template comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateSource {
    /// User-only template (no built-in with this id)
    Custom,
    /// Bundled/embedded template with no user override
    Builtin,
    /// A user file shadows a built-in id
    Overridden,
}

/// Directories the loader reads from. Explicit so tests can use temp dirs.
struct Dirs {
    custom: Option<PathBuf>,
    bundled: Option<PathBuf>,
}

fn global_dirs() -> Dirs {
    Dirs {
        custom: get_custom_templates_dir(),
        bundled: BUNDLED_TEMPLATES_DIR.read().ok().and_then(|d| d.clone()),
    }
}

/// Set the bundled templates directory path (called once at app startup)
pub fn set_bundled_templates_dir(path: PathBuf) {
    info!("Bundled templates directory set to: {:?}", path);
    if let Ok(mut dir) = BUNDLED_TEMPLATES_DIR.write() {
        *dir = Some(path);
    }
}

/// Get the user's custom templates directory path.
///
/// Portable / self-contained build: templates live inside the program's own
/// install directory (`<exe_dir>/data/templates`) rather than under
/// `%APPDATA%` / `~/Library/Application Support`.
fn get_custom_templates_dir() -> Option<PathBuf> {
    Some(crate::paths::install_data_root().join("templates"))
}

/// Validate a template id: `[a-z0-9_-]`, 1..=64 chars. This also rules out any
/// path separators or `..`, so ids are always safe as file stems.
pub fn validate_template_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("Template id cannot be empty".to_string());
    }
    if id.len() > MAX_TEMPLATE_ID_LEN {
        return Err(format!(
            "Template id is too long (max {} characters)",
            MAX_TEMPLATE_ID_LEN
        ));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(format!(
            "Invalid template id '{}': only a-z, 0-9, '_' and '-' are allowed",
            id
        ));
    }
    Ok(())
}

fn read_template_file(dir: &Option<PathBuf>, template_id: &str) -> Option<String> {
    let path = dir.as_ref()?.join(format!("{}.json", template_id));
    debug!("Checking for template at: {:?}", path);
    std::fs::read_to_string(&path).ok()
}

fn list_json_ids(dir: &Path) -> Vec<String> {
    match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_str()?.to_string();
                name.strip_suffix(".json").map(|s| s.to_string())
            })
            .collect(),
        Err(e) => {
            warn!("Failed to read templates directory {:?}: {}", dir, e);
            Vec::new()
        }
    }
}

fn has_builtin(dirs: &Dirs, id: &str) -> bool {
    defaults::get_builtin_template(id).is_some()
        || dirs
            .bundled
            .as_ref()
            .map(|d| d.join(format!("{}.json", id)).exists())
            .unwrap_or(false)
}

fn has_custom_file(dirs: &Dirs, id: &str) -> bool {
    dirs.custom
        .as_ref()
        .map(|d| d.join(format!("{}.json", id)).exists())
        .unwrap_or(false)
}

fn get_template_in(dirs: &Dirs, template_id: &str) -> Result<Template, String> {
    validate_template_id(template_id)?;

    // Custom first, then bundled, then embedded
    let json_content = if let Some(c) = read_template_file(&dirs.custom, template_id) {
        c
    } else if let Some(c) = read_template_file(&dirs.bundled, template_id) {
        c
    } else if let Some(c) = defaults::get_builtin_template(template_id) {
        c.to_string()
    } else {
        return Err(format!(
            "Template '{}' not found. Available templates: {}",
            template_id,
            list_template_ids_in(dirs).join(", ")
        ));
    };

    validate_and_parse_template(&json_content)
}

fn list_template_ids_in(dirs: &Dirs) -> Vec<String> {
    let mut ids: Vec<String> = defaults::list_builtin_template_ids()
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    for dir in [dirs.bundled.as_ref(), dirs.custom.as_ref()].into_iter().flatten() {
        if dir.exists() {
            for id in list_json_ids(dir) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }

    ids.sort();
    ids
}

fn template_source_in(dirs: &Dirs, id: &str) -> TemplateSource {
    match (has_custom_file(dirs, id), has_builtin(dirs, id)) {
        (true, true) => TemplateSource::Overridden,
        (true, false) => TemplateSource::Custom,
        _ => TemplateSource::Builtin,
    }
}

fn save_custom_template_in(dirs: &Dirs, template_id: &str, json_content: &str) -> Result<String, String> {
    validate_template_id(template_id)?;
    // Validate against the schema first so we never persist invalid templates
    validate_and_parse_template(json_content)?;

    let dir = dirs
        .custom
        .as_ref()
        .ok_or_else(|| "Could not resolve custom templates directory".to_string())?;
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("Failed to create templates directory: {}", e))?;

    let path = dir.join(format!("{}.json", template_id));
    std::fs::write(&path, json_content)
        .map_err(|e| format!("Failed to write template file: {}", e))?;

    info!("Saved custom template '{}' to {:?}", template_id, path);
    Ok(template_id.to_string())
}

fn remove_custom_file_in(dirs: &Dirs, template_id: &str) -> Result<(), String> {
    validate_template_id(template_id)?;
    let dir = dirs
        .custom
        .as_ref()
        .ok_or_else(|| "Could not resolve custom templates directory".to_string())?;
    let path = dir.join(format!("{}.json", template_id));
    if !path.exists() {
        return Err(format!("Custom template '{}' not found", template_id));
    }
    std::fs::remove_file(&path).map_err(|e| format!("Failed to delete template file: {}", e))
}

fn delete_custom_template_in(dirs: &Dirs, template_id: &str) -> Result<(), String> {
    remove_custom_file_in(dirs, template_id)?;
    info!("Deleted custom template '{}'", template_id);
    Ok(())
}

fn restore_template_default_in(dirs: &Dirs, template_id: &str) -> Result<(), String> {
    validate_template_id(template_id)?;
    match template_source_in(dirs, template_id) {
        TemplateSource::Overridden => {
            remove_custom_file_in(dirs, template_id)?;
            info!("Restored default for template '{}'", template_id);
            Ok(())
        }
        TemplateSource::Custom => Err(format!(
            "Template '{}' is a custom template and has no default to restore",
            template_id
        )),
        TemplateSource::Builtin => Err(format!(
            "Template '{}' has not been modified; nothing to restore",
            template_id
        )),
    }
}

fn list_templates_detailed_in(dirs: &Dirs) -> Vec<(String, Template, TemplateSource)> {
    let mut out = Vec::new();
    for id in list_template_ids_in(dirs) {
        match get_template_in(dirs, &id) {
            Ok(t) => {
                let source = template_source_in(dirs, &id);
                out.push((id, t, source));
            }
            Err(e) => warn!("Failed to load template '{}': {}", id, e),
        }
    }
    out
}

/// Load and parse a template by identifier
///
/// Fallback strategy:
/// 1. User's custom templates directory
/// 2. Bundled resources directory (app templates)
/// 3. Built-in embedded templates
pub fn get_template(template_id: &str) -> Result<Template, String> {
    info!("Loading template: {}", template_id);
    get_template_in(&global_dirs(), template_id)
}

/// Validate and parse template JSON
pub fn validate_and_parse_template(json_content: &str) -> Result<Template, String> {
    let template: Template = serde_json::from_str(json_content)
        .map_err(|e| format!("Failed to parse template JSON: {}", e))?;

    template.validate()?;

    Ok(template)
}

/// List all available template identifiers (built-in, bundled and custom)
pub fn list_template_ids() -> Vec<String> {
    list_template_ids_in(&global_dirs())
}

/// Save (create or overwrite) a custom template in the user's templates directory.
///
/// Validates the id and the JSON against the Template schema before writing.
/// Saving over a built-in id creates a user override. Returns the id written.
pub fn save_custom_template(template_id: &str, json_content: &str) -> Result<String, String> {
    save_custom_template_in(&global_dirs(), template_id, json_content)
}

/// Delete a custom template file from the user's templates directory.
pub fn delete_custom_template(template_id: &str) -> Result<(), String> {
    delete_custom_template_in(&global_dirs(), template_id)
}

/// Delete the user override of a built-in template so the built-in applies again.
/// Errors for ids that are not currently overridden.
pub fn restore_template_default(template_id: &str) -> Result<(), String> {
    restore_template_default_in(&global_dirs(), template_id)
}

/// Returns true if the given template id has a user file on disk.
pub fn is_custom_template(template_id: &str) -> bool {
    validate_template_id(template_id).is_ok() && has_custom_file(&global_dirs(), template_id)
}

/// Source of the effective version of a template.
pub fn template_source(template_id: &str) -> TemplateSource {
    template_source_in(&global_dirs(), template_id)
}

/// List all available templates with their metadata
///
/// Returns a list of (id, name, description) tuples
pub fn list_templates() -> Vec<(String, String, String)> {
    list_templates_detailed()
        .into_iter()
        .map(|(id, t, _)| (id, t.name, t.description))
        .collect()
}

/// List all templates with metadata and source.
pub fn list_templates_detailed() -> Vec<(String, Template, TemplateSource)> {
    list_templates_detailed_in(&global_dirs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_builtin_template() {
        let template = get_template("daily_standup");
        assert!(template.is_ok());

        let template = template.unwrap();
        assert_eq!(template.name, "Daily Standup");
        assert!(!template.sections.is_empty());
    }

    #[test]
    fn test_get_nonexistent_template() {
        let result = get_template("nonexistent_template");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_template_ids() {
        let ids = list_template_ids();
        assert!(ids.contains(&"daily_standup".to_string()));
        assert!(ids.contains(&"standard_meeting".to_string()));
    }

    fn tmp_dirs(tag: &str) -> Dirs {
        let base = std::env::temp_dir().join(format!(
            "recmeetily_tpl_{}_{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let bundled = base.join("bundled");
        std::fs::create_dir_all(&bundled).unwrap();
        std::fs::write(bundled.join("bundled_one.json"), TPL).unwrap();
        Dirs {
            custom: Some(base.join("custom")),
            bundled: Some(bundled),
        }
    }

    const TPL: &str = r#"{"name":"T","description":"D","sections":[{"title":"S","instruction":"I","format":"list"}]}"#;

    fn source_of(dirs: &Dirs, id: &str) -> TemplateSource {
        list_templates_detailed_in(dirs)
            .into_iter()
            .find(|(i, _, _)| i == id)
            .unwrap_or_else(|| panic!("{} not listed", id))
            .2
    }

    #[test]
    fn test_get_template_builtin_bundled_and_custom() {
        let dirs = tmp_dirs("get");
        assert_eq!(get_template_in(&dirs, "daily_standup").unwrap().name, "Daily Standup");
        assert_eq!(get_template_in(&dirs, "bundled_one").unwrap().name, "T");
        save_custom_template_in(&dirs, "mine", TPL).unwrap();
        let t = get_template_in(&dirs, "mine").unwrap();
        assert_eq!(t.sections[0].title, "S");
        assert_eq!(source_of(&dirs, "mine"), TemplateSource::Custom);
        assert_eq!(source_of(&dirs, "daily_standup"), TemplateSource::Builtin);
        assert_eq!(source_of(&dirs, "bundled_one"), TemplateSource::Builtin);
    }

    #[test]
    fn test_override_then_restore() {
        let dirs = tmp_dirs("override");
        for id in ["daily_standup", "bundled_one"] {
            save_custom_template_in(&dirs, id, TPL).unwrap();
            assert_eq!(source_of(&dirs, id), TemplateSource::Overridden);
            assert_eq!(get_template_in(&dirs, id).unwrap().name, "T");
            // Saving again updates in place
            let updated = TPL.replace("\"T\"", "\"T2\"");
            save_custom_template_in(&dirs, id, &updated).unwrap();
            assert_eq!(get_template_in(&dirs, id).unwrap().name, "T2");

            restore_template_default_in(&dirs, id).unwrap();
            assert_eq!(source_of(&dirs, id), TemplateSource::Builtin);
        }
        assert_eq!(get_template_in(&dirs, "daily_standup").unwrap().name, "Daily Standup");
    }

    #[test]
    fn test_restore_errors_for_custom_and_builtin() {
        let dirs = tmp_dirs("restore_err");
        save_custom_template_in(&dirs, "mine", TPL).unwrap();
        assert!(restore_template_default_in(&dirs, "mine").is_err());
        assert!(dirs.custom.as_ref().unwrap().join("mine.json").exists());
        assert!(restore_template_default_in(&dirs, "daily_standup").is_err());
        assert!(restore_template_default_in(&dirs, "does_not_exist").is_err());
    }

    #[test]
    fn test_invalid_ids_rejected() {
        let dirs = tmp_dirs("ids");
        let long = "a".repeat(65);
        for bad in ["", "../evil", "a/b", "a\\b", "Upper", "sp ace", "dot.json", "..", long.as_str()] {
            assert!(validate_template_id(bad).is_err(), "{:?} should be invalid", bad);
            assert!(save_custom_template_in(&dirs, bad, TPL).is_err());
            assert!(get_template_in(&dirs, bad).is_err());
            assert!(delete_custom_template_in(&dirs, bad).is_err());
            assert!(restore_template_default_in(&dirs, bad).is_err());
        }
        assert!(validate_template_id(&"a".repeat(64)).is_ok());
        assert!(validate_template_id("my-tpl_2").is_ok());
        // Invalid JSON is still rejected
        assert!(save_custom_template_in(&dirs, "ok", "{}").is_err());
    }

    #[test]
    fn test_validate_invalid_json() {
        let result = validate_and_parse_template("invalid json");
        assert!(result.is_err());
    }
}
