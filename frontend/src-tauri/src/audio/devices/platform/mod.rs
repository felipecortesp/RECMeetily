// Platform-specific audio device implementations

pub mod macos;

// Re-export platform-specific functions
pub use macos::configure_macos_audio;
