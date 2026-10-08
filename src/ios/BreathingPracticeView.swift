import SwiftUI

struct BreathingPracticeView: View {
    var model: AppModel
    @Environment(\.scenePhase) private var scenePhase
    @State private var began: ContinuousClock.Instant?
    @State private var startedAt = Date()
    @State private var elapsed = 0.0
    @State private var recordID = UUID()
    @State private var batchID = UUID()
    @State private var saved = false
    @State private var saving = false
    @State private var failure: String?
    var body: some View {
        List {
            Section("Cyclic sighing") {
                Text("Inhale gently through your nose, then take a second small inhale. Exhale slowly through your mouth. Repeat at a comfortable pace. Stop if you feel uncomfortable.")
                Link("Study and method", destination: URL(string: "https://pubmed.ncbi.nlm.nih.gov/36630953/")!)
                Text("The timer records practice duration. It does not measure breathing rate, stress or recovery.").font(.caption)
            }
            Section {
                let remaining = Int(ceil(300 - elapsed))
                Text(String(format: "%d:%02d", remaining / 60, remaining % 60)).font(.largeTitle).monospacedDigit().accessibilityLabel("\(remaining) seconds remaining")
                Button("Start five minutes") {
                    startedAt = Date(); began = ContinuousClock().now; elapsed = 0
                    recordID = UUID(); batchID = UUID(); saved = false; failure = nil
                }.disabled(began != nil || saving)
                Button("Stop practice") { stop() }.disabled(began == nil)
                Text("The timer stops when you leave this page or the app becomes inactive.").font(.caption)
                if began == nil && elapsed >= 0.6 {
                    Text("\((elapsed / 60).formatted()) minutes recorded. Save only if you practiced during this time.")
                    Button(saved ? "Practice saved" : "Save practice log") { Task { await save() } }.disabled(saved || saving)
                }
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
        }.navigationTitle("Breathing practice")
            .task(id: began) {
                while let instant = began {
                    do { try await Task.sleep(for: .milliseconds(250)) } catch { return }
                    guard began == instant else { return }
                    elapsed = seconds(from: instant)
                    if elapsed >= 300 { stop() }
                }
            }
            .onChange(of: scenePhase) { _, phase in if phase != .active { stop() } }
            .onDisappear { stop() }
    }
    private func seconds(from instant: ContinuousClock.Instant) -> Double {
        let parts = instant.duration(to: ContinuousClock().now).components
        return min(300, max(0, Double(parts.seconds) + Double(parts.attoseconds) / 1e18))
    }
    private func stop() {
        guard let instant = began else { return }
        elapsed = seconds(from: instant); began = nil
    }
    private func save() async {
        guard began == nil, elapsed >= 0.6, !saved, !saving, let client = model.client, let token = model.session?.token else { return }
        saving = true
        defer { saving = false }
        do {
            _ = try await model.request("wellness/entries/save", body: .object([
                "record_id": .string(recordID.uuidString), "batch_id": .string(batchID.uuidString), "version": .number(1),
                "at": .number(startedAt.timeIntervalSince1970.rounded(.down)), "timezone": .string(TimeZone.current.identifier),
                "entry": .object(["kind": .string("breathing"), "content": .object([
                    "duration_minutes": .number(elapsed / 60), "breaths_per_minute": .null,
                    "note": .string("Cyclic sighing guide. Foreground practice timer. Breathing rate was not measured.")
                ])])
            ]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            saved = true; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
}
