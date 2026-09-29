//! Spike: pyannote community-1 offline diarization via fluidaudio-rs.
//! Usage: fluidaudio-bench <wav> --out <dir> [--num-speakers N] [--threshold T]
//!
//! fluidaudio-rs (0.14.x) only exposes the clustering threshold; --num-speakers is
//! rejected because the Swift bridge has no way to set OfflineDiarizerConfig.clustering.numSpeakers.
//! Segments come from OfflineDiarizerManager with the default postprocessing
//! (exclusiveSegments = true), i.e. at most one speaker per instant.

use fluidaudio_rs::FluidAudio;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

fn peak_rss_mb() -> f64 {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    // macOS reports ru_maxrss in bytes.
    ru.ru_maxrss as f64 / 1024.0 / 1024.0
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut wav: Option<PathBuf> = None;
    let mut out = PathBuf::from(".");
    let mut threshold = 0.6f64;
    let mut num_speakers: Option<u32> = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => out = it.next().ok_or("--out needs a value")?.into(),
            "--threshold" => threshold = it.next().ok_or("--threshold needs a value")?.parse()?,
            "--num-speakers" => num_speakers = Some(it.next().ok_or("--num-speakers needs a value")?.parse()?),
            _ => wav = Some(a.into()),
        }
    }
    if num_speakers.is_some() {
        return Err("fluidaudio-rs does not expose numSpeakers (only clustering.threshold); use ../fluidaudio-swift".into());
    }
    let wav = wav.ok_or("usage: fluidaudio-bench <wav> --out <dir> [--threshold T]")?;
    let r = hound::WavReader::open(&wav)?;
    let spec = r.spec();
    if spec.sample_rate != 16000 || spec.channels != 1 {
        return Err(format!("expected 16 kHz mono, got {:?}", spec).into());
    }
    let audio_secs = r.duration() as f64 / spec.sample_rate as f64;
    drop(r);
    std::fs::create_dir_all(&out)?;
    let stem = wav.file_stem().unwrap().to_string_lossy().to_string();

    let fa = FluidAudio::new()?;
    let t = Instant::now();
    fa.init_diarization(threshold)?;
    let prep = t.elapsed().as_secs_f64();

    let t = Instant::now();
    let segs = fa.diarize_file(&wav)?;
    let diar = t.elapsed().as_secs_f64();

    let mut rttm = String::new();
    let mut talk: BTreeMap<String, f64> = BTreeMap::new();
    for s in &segs {
        let (st, en) = (s.start_time as f64, s.end_time as f64);
        rttm.push_str(&format!(
            "SPEAKER {stem} 1 {:.3} {:.3} <NA> <NA> {} <NA> <NA>\n",
            st, en - st, s.speaker_id
        ));
        *talk.entry(s.speaker_id.clone()).or_default() += en - st;
    }
    let total: f64 = talk.values().sum();
    let per: serde_json::Map<String, serde_json::Value> = talk
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::json!({"seconds": v, "percent": if total > 0.0 { v / total * 100.0 } else { 0.0 }})))
        .collect();
    let json = serde_json::json!({
        "file": stem, "audio_seconds": audio_secs, "threshold": threshold, "num_speakers_hint": num_speakers,
        "exclusive_segments": true, "segments": segs.len(),
        "model_prep_seconds": prep, "diarization_seconds": diar, "rtf": diar / audio_secs,
        "peak_rss_mb": peak_rss_mb(), "num_speakers": talk.len(), "talk_time": per,
    });
    std::fs::write(out.join(format!("{stem}.fluid.rttm")), rttm)?;
    std::fs::write(out.join(format!("{stem}.fluid.json")), serde_json::to_string_pretty(&json)?)?;
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}
