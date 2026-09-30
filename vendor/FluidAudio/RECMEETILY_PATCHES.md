# RECMeetily patches to vendored FluidAudio

- Upstream: https://github.com/FluidInference/FluidAudio
- Version: tag `v0.14.8`, commit `56607d90b97de7d95a731200563bf49aa5beef20`
- License: Apache-2.0 (`LICENSE` and `ThirdPartyLicenses/` are kept unchanged; all upstream source headers/notices are retained).

## What was removed

Only these were copied from upstream: `Package.swift`, `LICENSE`, `ThirdPartyLicenses/`,
`Sources/FluidAudio`, `Sources/FastClusterWrapper`, `Sources/MachTaskSelfWrapper`.
Not copied: `Sources/FluidAudioCLI`, `Tests`, docs, images, etc.
`Package.swift` was edited only to drop the `fluidaudiocli` executable product and the
`FluidAudioCLI` / `FluidAudioTests` targets.

## Why

With `clustering.numSpeakers` set, the offline diarizer uses k-means seeded with
`UInt64.random` (KMeansClustering.swift, caller in VBxClustering.swift never passes a seed) and a
single initialisation. The same audio gave different clusterings run to run (one outlier at 51 % DER).
The app needs deterministic, robust results.

## Patch (KMeansClustering.swift)

Runs 10 initialisations with seeds `base..base+9` (fixed default base, no randomness), keeps the
lowest-inertia solution (ties -> lowest seed).

```diff
@@ -35,13 +35,63 @@
         ).clusters
     }
 
+    // RECMeetily patch: deterministic default seed and number of restarts.
+    static let defaultBaseSeed: UInt64 = 0x5245_434D_4545_5449
+    static let restartCount = 10
+
     /// Clusters embeddings and returns both assignments and centroids.
+    // RECMeetily patch: run `restartCount` k-means initialisations with seeds
+    // base..<base+restartCount and keep the lowest-inertia solution (ties -> lowest seed).
     static func clusterWithCentroids(
         embeddings: [[Double]],
         numClusters: Int,
         maxIterations: Int = 300,
         seed: UInt64? = nil
     ) -> (clusters: [Int], centroids: [[Double]]) {
+        let base = seed ?? defaultBaseSeed
+        var best: (clusters: [Int], centroids: [[Double]])?
+        var bestInertia = Double.greatestFiniteMagnitude
+        for offset in 0..<UInt64(restartCount) {
+            let run = clusterOnce(
+                embeddings: embeddings,
+                numClusters: numClusters,
+                maxIterations: maxIterations,
+                seed: base &+ offset
+            )
+            let inertia = inertiaOf(embeddings: embeddings, run: run)
+            if best == nil || inertia < bestInertia {
+                best = run
+                bestInertia = inertia
+            }
+            // Degenerate inputs return early without using the seed; no point in repeating.
+            if run.centroids.count != min(numClusters, embeddings.count) || embeddings.count <= numClusters {
+                break
+            }
+        }
+        return best ?? ([], [])
+    }
+
+    // RECMeetily patch: sum of squared distances to the assigned centroid (on normalised vectors,
+    // matching what the single run optimises).
+    private static func inertiaOf(
+        embeddings: [[Double]],
+        run: (clusters: [Int], centroids: [[Double]])
+    ) -> Double {
+        let normalized = normalizeEmbeddings(embeddings)
+        var total = 0.0
+        for (idx, cluster) in run.clusters.enumerated() where cluster < run.centroids.count {
+            total += euclideanDistanceSquared(normalized[idx], run.centroids[cluster])
+        }
+        return total
+    }
+
+    // RECMeetily patch: body of the upstream clusterWithCentroids, unchanged except `seed` is required.
+    private static func clusterOnce(
+        embeddings: [[Double]],
+        numClusters: Int,
+        maxIterations: Int,
+        seed: UInt64
+    ) -> (clusters: [Int], centroids: [[Double]]) {
         let kmeansState = signposter.beginInterval("KMeans Clustering")
         defer { signposter.endInterval("KMeans Clustering", kmeansState) }
 
@@ -61,7 +111,7 @@
             return (Array(0..<count), embeddings)
         }
 
-        var rng = SeededRNG(seed: seed ?? UInt64.random(in: 0...UInt64.max))
+        var rng = SeededRNG(seed: seed)
         let normalized = normalizeEmbeddings(embeddings)
         var centroids = initializeCentroids(from: normalized, k: k, rng: &rng)
         var assignments = [Int](repeating: 0, count: count)
```
