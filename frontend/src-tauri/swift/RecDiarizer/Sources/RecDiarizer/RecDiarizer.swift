import CoreML
import FluidAudio
import Foundation

// C ABI bridge to FluidAudio's offline (pyannote community-1 / VBx) diarizer.
// Return codes: 0 ok, 1 invalid args, 2 models missing, 3 diarization error.

private let requiredEntries = [
    "Segmentation.mlmodelc", "FBank.mlmodelc", "Embedding.mlmodelc", "PldaRho.mlmodelc",
    "plda-parameters.json",
]

private let cacheLock = NSLock()
nonisolated(unsafe) private var modelCache: [String: OfflineDiarizerModels] = [:]

private struct BridgeError: Error { let message: String }

private func loadModel(_ dir: URL, _ name: String, _ units: MLComputeUnits) throws -> MLModel {
    let config = MLModelConfiguration()
    config.computeUnits = units
    config.allowLowPrecisionAccumulationOnGPU = true
    return try MLModel(contentsOf: dir.appendingPathComponent(name), configuration: config)
}

// Loads the CoreML files straight from `dir` instead of going through FluidAudio's
// downloader, which would fetch from the network (and wipe the folder) on any failure.
private func loadModels(_ dir: URL) throws -> OfflineDiarizerModels {
    let started = Date()
    let data = try Data(contentsOf: dir.appendingPathComponent("plda-parameters.json"))
    guard
        let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
        let tensors = root["tensors"] as? [String: Any],
        let psi = tensors["psi"] as? [String: Any],
        let b64 = psi["data_base64"] as? String,
        let raw = Data(base64Encoded: b64, options: .ignoreUnknownCharacters), raw.count >= 4
    else { throw BridgeError(message: "invalid plda-parameters.json") }
    var floats = [Float](repeating: 0, count: raw.count / MemoryLayout<Float>.size)
    _ = floats.withUnsafeMutableBytes { raw.copyBytes(to: $0) }

    return OfflineDiarizerModels(
        // Measured: compute units don't change the clustering outcome (k-means is seeded in the
        // vendored FluidAudio), and .all is the fastest (~1.6x cpuAndGPU, ~2.3x cpuOnly).
        segmentationModel: try loadModel(dir, "Segmentation.mlmodelc", .all),
        fbankModel: try loadModel(dir, "FBank.mlmodelc", .cpuOnly),
        embeddingModel: try loadModel(dir, "Embedding.mlmodelc", .all),
        pldaRhoModel: try loadModel(dir, "PldaRho.mlmodelc", .all),
        pldaPsi: floats.map { Double($0) },
        compilationDuration: Date().timeIntervalSince(started)
    )
}

private func cachedModels(_ dir: URL) throws -> OfflineDiarizerModels {
    if let hit = cacheLock.withLock({ modelCache[dir.path] }) { return hit }
    let models = try loadModels(dir)
    cacheLock.withLock { modelCache[dir.path] = models }
    return models
}

private func run(samples: [Float], counts: (Int?, Int?, Int?), dir: URL) async throws -> String {
    let models = try cachedModels(dir)

    // The manager's config is immutable, so build one per call around the cached models.
    var config = OfflineDiarizerConfig.default
    config.clustering.numSpeakers = counts.0
    config.clustering.minSpeakers = counts.1
    config.clustering.maxSpeakers = counts.2
    config.postProcessing.exclusiveSegments = true
    let manager = OfflineDiarizerManager(config: config)
    manager.initialize(models: models)

    let result = try await manager.process(audio: samples)
    let segments = result.segments.map {
        ["speaker": $0.speakerId, "start": Double($0.startTimeSeconds), "end": Double($0.endTimeSeconds)]
            as [String: Any]
    }
    let json = try JSONSerialization.data(withJSONObject: ["segments": segments])
    return String(decoding: json, as: UTF8.self)
}

private func emit(_ dict: [String: Any], _ out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?) {
    guard let out = out else { return }
    let data = (try? JSONSerialization.data(withJSONObject: dict)) ?? Data("{\"error\":\"encode\"}".utf8)
    out.pointee = strdup(String(decoding: data, as: UTF8.self))
}

private final class ResultBox: @unchecked Sendable { var value: Result<String, Error>? }

@_cdecl("rec_diarize")
public func recDiarize(
    _ samples: UnsafePointer<Float>?, _ count: Int, _ numSpeakers: Int32, _ minSpeakers: Int32,
    _ maxSpeakers: Int32, _ modelsDir: UnsafePointer<CChar>?,
    _ outJson: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> Int32 {
    outJson?.pointee = nil
    guard let samples = samples, count > 0, let modelsDir = modelsDir else {
        emit(["error": "invalid arguments"], outJson)
        return 1
    }
    let dir = URL(fileURLWithPath: String(cString: modelsDir), isDirectory: true).standardizedFileURL
    let missing = requiredEntries.filter {
        !FileManager.default.fileExists(atPath: dir.appendingPathComponent($0).path)
    }
    guard missing.isEmpty else {
        emit(["error": "missing model files: \(missing.joined(separator: ", "))"], outJson)
        return 2
    }

    let audio = Array(UnsafeBufferPointer(start: samples, count: count))
    let opt = { (v: Int32) -> Int? in v > 0 ? Int(v) : nil }
    let counts = (opt(numSpeakers), opt(minSpeakers), opt(maxSpeakers))

    // The FFI is synchronous but FluidAudio is async: run it on a detached task and block
    // the caller (a Rust worker thread, never the Swift cooperative pool) on a semaphore.
    let box = ResultBox()
    let done = DispatchSemaphore(value: 0)
    Task.detached {
        do { box.value = .success(try await run(samples: audio, counts: counts, dir: dir)) }
        catch { box.value = .failure(error) }
        done.signal()
    }
    done.wait()

    switch box.value! {
    case .success(let json):
        outJson?.pointee = strdup(json)
        return 0
    case .failure(let error):
        let message = (error as? BridgeError)?.message ?? error.localizedDescription
        emit(["error": message], outJson)
        return 3
    }
}

@_cdecl("rec_diarize_free")
public func recDiarizeFree(_ ptr: UnsafeMutablePointer<CChar>?) {
    free(ptr)
}
