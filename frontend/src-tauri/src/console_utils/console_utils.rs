use std::process::Command;

#[tauri::command]
pub fn show_console() -> Result<String, String> {
    // On macOS, we'll open Terminal.app with our app's logs
    // First, get the app name from the bundle
    match Command::new("osascript")
        .arg("-e")
        .arg(r#"
            tell application "Terminal"
                activate
                do script "log stream --process meetily --level info --style compact"
            end tell
        "#)
        .spawn()
    {
        Ok(_) => Ok("Console opened in Terminal".to_string()),
        Err(e) => Err(format!("Failed to open console: {}", e)),
    }
}

#[tauri::command]
pub fn hide_console() -> Result<String, String> {
    // On macOS, we'll close the Terminal window that's showing our logs
    match Command::new("osascript")
        .arg("-e")
        .arg(r#"
            tell application "Terminal"
                set windowList to windows
                repeat with aWindow in windowList
                    if contents of selected tab of aWindow contains "log stream --process meetily" then
                        close aWindow
                    end if
                end repeat
            end tell
        "#)
        .spawn()
    {
        Ok(_) => Ok("Console closed".to_string()),
        Err(e) => Err(format!("Failed to close console: {}", e)),
    }
}

#[tauri::command]
pub fn toggle_console() -> Result<String, String> {
    // On macOS, check if Terminal is running with our log stream
    let check_result = Command::new("osascript")
        .arg("-e")
        .arg(r#"
            tell application "Terminal"
                set windowList to windows
                repeat with aWindow in windowList
                    if contents of selected tab of aWindow contains "log stream --process meetily" then
                        return "found"
                    end if
                end repeat
                return "not found"
            end tell
        "#)
        .output();

    match check_result {
        Ok(output) => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if output_str.trim() == "found" {
                hide_console()
            } else {
                show_console()
            }
        }
        Err(_) => show_console()
    }
}
