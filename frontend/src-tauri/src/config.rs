/// Application configuration constants
///
/// Centralized definitions for default models and settings.
/// Used across database initialization, import, and retranscription.

/// Default Whisper model for transcription when no preference is configured.
/// This is the recommended balance of accuracy and speed.
pub const DEFAULT_WHISPER_MODEL: &str = "large-v3-turbo";

/// Default Parakeet model for transcription when no preference is configured.
/// This is the quantized version optimized for speed.
pub const DEFAULT_PARAKEET_MODEL: &str = "parakeet-tdt-0.6b-v3-int8";

/// The ggerganov/whisper.cpp commit that [`WHISPER_MODEL_CATALOG`]'s sizes and
/// SHA-256 hashes were pinned from. Download URLs are built as
/// `resolve/<commit>/<file>`, deliberately not pointed at the mutable
/// `main` branch, so a future upload to the repository can never silently
/// change what's downloaded without also updating these hashes.
pub const WHISPER_MODEL_COMMIT: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";

/// Whisper model catalog with metadata for all supported models.
/// Used by both WhisperEngine::discover_models() and discover_models_standalone().
///
/// Format: (name, filename, size_mb, accuracy, speed, description, size_bytes, sha256)
/// `size_bytes` and `sha256` are pinned from the Hugging Face LFS metadata at
/// [`WHISPER_MODEL_COMMIT`] and are never guessed.
pub const WHISPER_MODEL_CATALOG: &[(&str, &str, u32, &str, &str, &str, u64, &str)] = &[
    // Standard f16 models (full precision)
    ("tiny", "ggml-tiny.bin", 74, "Decent", "Very Fast", "Fastest processing, good for real-time use", 77_691_713, "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21"),
    ("base", "ggml-base.bin", 142, "Good", "Fast", "Good balance of speed and accuracy", 147_951_465, "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe"),
    ("small", "ggml-small.bin", 466, "Good", "Medium", "Better accuracy, moderate speed", 487_601_967, "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b"),
    ("medium", "ggml-medium.bin", 1463, "High", "Slow", "High accuracy for professional use", 1_533_763_059, "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208"),
    ("large-v3-turbo", "ggml-large-v3-turbo.bin", 1549, "High", "Medium", "Best accuracy with improved speed", 1_624_555_275, "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69"),
    ("large-v3", "ggml-large-v3.bin", 2951, "High", "Slow", "Most Accurate, latest large model", 3_095_033_483, "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2"),

    // Q5_1 quantized models (balanced speed/accuracy, slightly better quality than Q5_0)
    ("tiny-q5_1", "ggml-tiny-q5_1.bin", 31, "Decent", "Very Fast", "Quantized tiny model, ~50% faster processing", 32_152_673, "818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7"),
    ("base-q5_1", "ggml-base-q5_1.bin", 57, "Good", "Fast", "Quantized base model, good speed/accuracy balance", 59_707_625, "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898"),
    ("small-q5_1", "ggml-small-q5_1.bin", 181, "Good", "Fast", "Quantized small model, faster than f16 version", 190_085_487, "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb"),

    // Q5_0 quantized models (balanced speed/accuracy)
    ("medium-q5_0", "ggml-medium-q5_0.bin", 514, "High", "Medium", "Quantized medium model, professional quality", 539_212_467, "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f"),
    ("large-v3-turbo-q5_0", "ggml-large-v3-turbo-q5_0.bin", 547, "High", "Medium", "Quantized large model, best balance", 574_041_195, "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2"),
    ("large-v3-q5_0", "ggml-large-v3-q5_0.bin", 1031, "High", "Slow", "Quantized large model, high accuracy", 1_081_140_203, "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1"),
];
