// Spike: community-1 offline diarization via FluidAudio directly, exposing numSpeakers
// (which fluidaudio-rs does not). Usage: fluidaudio-swift <wav> --out <dir> [--num-speakers N] [--threshold T]
import FluidAudio
import Foundation

var wav: String?
var outDir = "."
var threshold = 0.6
var numSpeakers: Int?
var args = CommandLine.arguments.dropFirst().makeIterator()
while let a = args.next() {
    switch a {
    case "--out": outDir = args.next()!
    case "--threshold": threshold = Double(args.next()!)!
    case "--num-speakers": numSpeakers = Int(args.next()!)!
    default: wav = a
    }
}
guard let wav else { fatalError("usage: fluidaudio-swift <wav> --out <dir>") }
let stem = URL(fileURLWithPath: wav).deletingPathExtension().lastPathComponent

func peakRssMb() -> Double {
    var ru = rusage()
    getrusage(RUSAGE_SELF, &ru)
    return Double(ru.ru_maxrss) / 1024 / 1024  // bytes on macOS
}

var config = OfflineDiarizerConfig()
config.clustering.threshold = threshold
config.clustering.numSpeakers = numSpeakers
let manager = OfflineDiarizerManager(config: config)

var t = Date()
try await manager.prepareModels()
let prep = Date().timeIntervalSince(t)

let samples = try AudioConverter().resampleAudioFile(path: wav)
let audioSecs = Double(samples.count) / 16000
t = Date()
let result = try await manager.process(audio: samples)
let diar = Date().timeIntervalSince(t)

var rttm = ""
var talk: [String: Double] = [:]
for s in result.segments {
    let st = Double(s.startTimeSeconds), en = Double(s.endTimeSeconds)
    rttm += String(format: "SPEAKER %@ 1 %.3f %.3f <NA> <NA> %@ <NA> <NA>\n", stem, st, en - st, s.speakerId)
    talk[s.speakerId, default: 0] += en - st
}
let total = talk.values.reduce(0, +)
var per: [String: Any] = [:]
for (k, v) in talk { per[k] = ["seconds": v, "percent": total > 0 ? v / total * 100 : 0] }
let json: [String: Any] = [
    "file": stem, "audio_seconds": audioSecs, "threshold": threshold,
    "num_speakers_hint": numSpeakers as Any? ?? NSNull(), "exclusive_segments": true,
    "segments": result.segments.count, "model_prep_seconds": prep, "diarization_seconds": diar,
    "rtf": diar / audioSecs, "peak_rss_mb": peakRssMb(), "num_speakers": talk.count, "talk_time": per,
]
try FileManager.default.createDirectory(atPath: outDir, withIntermediateDirectories: true)
try rttm.write(toFile: "\(outDir)/\(stem).fluid.rttm", atomically: true, encoding: .utf8)
let data = try JSONSerialization.data(withJSONObject: json, options: [.prettyPrinted, .sortedKeys])
try data.write(to: URL(fileURLWithPath: "\(outDir)/\(stem).fluid.json"))
print(String(data: data, encoding: .utf8)!)
