//! ONNX Runtime availability for VAD, Parakeet, and diarization.
//! On macOS the system/bundled ONNX Runtime is used directly by the `ort`
//! crate; there is no packaged DLL to preflight, so this is a no-op that
//! exists to keep a single call site for callers that need to report a
//! consistent initialization error.

pub const START_ERROR_CODE: &str = "TRANSCRIPTION_RUNTIME_INITIALIZATION_FAILED";

#[derive(Debug, thiserror::Error)]
#[error("Speech recognition could not initialize: {0}")]
pub struct InitializationError(#[source] pub anyhow::Error);

pub fn ensure_available() -> anyhow::Result<()> {
    Ok(())
}

#[tauri::command]
pub async fn check_transcription_runtime() -> Result<(), String> {
    ensure_available().map_err(|error| format!("{START_ERROR_CODE}: {error}"))
}
