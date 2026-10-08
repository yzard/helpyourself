import SwiftUI
import Charts

struct SleepSessionsView: View {
    var model: AppModel
    @State private var result: JSONValue = .null
    @State private var regularity: JSONValue = .null
    @State private var correction: SleepCorrectionSelection?
    private let stages = ["In bed", "Asleep, unspecified", "Awake", "Core", "Deep", "REM"]
    var body: some View {
        List {
            Section {
                Text("Last 30 days. Gaps do not establish wakefulness. Confirm main sleep or naps without replacing source observations.").font(.caption)
            }
            ForEach(Array(result["sources"].arrayValue.enumerated()), id: \.offset) { _, source in
                Section(source["source"].stringValue) {
                    ForEach(Array(source["sessions"].arrayValue.enumerated()), id: \.offset) { _, session in
                        DisclosureGroup(Date(timeIntervalSince1970: session["end_at"].numberValue ?? 0).formatted()) {
                            if let seconds = session["asleep_seconds"].numberValue {
                                Text("\((seconds / 3600).formatted()) hours asleep").font(.headline)
                            } else { Text(session["state"].stringValue.replacingOccurrences(of: "_", with: " ")) }
                            if let efficiency = session["efficiency"].numberValue {
                                Text("Observed sleep efficiency: \((efficiency * 100).formatted())%")
                            } else { Text("Efficiency needs complete in-bed evidence.").font(.caption) }
                            Text(session["classification"].stringValue.replacingOccurrences(of: "_", with: " "))
                            Text(session["correction_state"].stringValue.replacingOccurrences(of: "_", with: " ")).font(.caption)
                            if let seconds = session["user_asleep_seconds"].numberValue { Text("User estimate: \((seconds / 3600).formatted()) hours") }
                            if session["corrections"].arrayValue.count <= 1 {
                                Button("Confirm or correct sleep") { correction = SleepCorrectionSelection(source: source["source"].stringValue, session: session) }
                            } else { Text("Delete duplicate corrections in Daily logs before confirming again.").font(.caption) }
                            Chart {
                                ForEach(Array(session["timeline"].arrayValue.enumerated()), id: \.offset) { _, record in
                                    BarMark(xStart: .value("Start", Date(timeIntervalSince1970: record["start_at"].numberValue ?? 0)), xEnd: .value("End", Date(timeIntervalSince1970: record["end_at"].numberValue ?? 0)), y: .value("Stage", stage(record)))
                                        .foregroundStyle(by: .value("Stage", stage(record)))
                                }
                            }.frame(height: 220).accessibilityLabel("Observed sleep stage intervals")
                            ForEach(Array(session["timeline"].arrayValue.enumerated()), id: \.offset) { _, record in
                                VStack(alignment: .leading) {
                                    Text(stage(record))
                                    Text("\(Date(timeIntervalSince1970: record["start_at"].numberValue ?? 0).formatted()) to \(Date(timeIntervalSince1970: record["end_at"].numberValue ?? 0).formatted())").font(.caption)
                                }
                            }
                        }
                    }
                }
            }
            if result != .null && result["sources"].arrayValue.isEmpty { Text("No supported sleep records in this range.") }
            Section("Seven-day sleep regularity") {
                Text("Every minute needs an explicit sleep or wake state. Missing daytime intervals are unknown.").font(.caption)
                ForEach(Array(regularity["sources"].arrayValue.enumerated()), id: \.offset) { _, source in
                    VStack(alignment: .leading) {
                        Text(source["source"].stringValue)
                        Text(source["value"].numberValue.map { "\($0.formatted()) SRI" } ?? "Not available").font(.headline)
                        Text("\(Int(source["known_minutes"].numberValue ?? 0))/10080 known minute states").font(.caption)
                        Text(source["state"].stringValue.replacingOccurrences(of: "_", with: " ")).font(.caption)
                    }
                }
                ForEach(regularity["notes"].arrayValue.map(\.stringValue), id: \.self) { Text($0).font(.caption) }
            }
            Section("Method") {
                Text("Sessions group intervals separated by at most 90 minutes. This engineering threshold does not classify main sleep or naps.").font(.caption)
                Text(result["algorithm_version"].stringValue).font(.caption)
            }
        }.navigationTitle("Sleep").task { await reload() }.refreshable { await reload() }
            .sheet(item: $correction, onDismiss: { Task { await reload() } }) { selection in
                NavigationStack { SleepCorrectionEditor(model: model, selection: selection) }
            }
    }
    private func stage(_ record: JSONValue) -> String {
        let index = Int(record["category"].numberValue ?? -1)
        return stages.indices.contains(index) ? stages[index] : "Unknown"
    }
    private func reload() async {
        guard let client = model.client, let token = model.session?.token else { return }
        do {
            let now = floor(Date().timeIntervalSince1970)
            let response = try await model.request("wellness/sleep", body: .object(["start_at": .number(now - 30 * 86400), "end_at": .number(now), "timezone": .string(TimeZone.current.identifier)]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            result = response
            regularity = .null
            let end = floor(now / 60) * 60
            let index = try await model.request("wellness/sleep/regularity", body: .object(["start_at": .number(end - 7 * 86400), "end_at": .number(end), "timezone": .string(TimeZone.current.identifier)]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            regularity = index
        } catch { if !Task.isCancelled, model.session?.token == token { model.errorMessage = error.localizedDescription } }
    }
}


private struct SleepCorrectionSelection: Identifiable {
    let id = UUID()
    let source: String
    let session: JSONValue
}

private struct SleepCorrectionEditor: View {
    var model: AppModel
    let selection: SleepCorrectionSelection
    @Environment(\.dismiss) private var dismiss
    @State private var classification = "unclassified_session"
    @State private var minutes = ""
    @State private var note = ""
    @State private var recordID = UUID().uuidString
    @State private var failure: String?
    @State private var saving = false
    var body: some View {
        Form {
            Text("Source intervals remain unchanged. Your estimate does not create sleep stages or change SRI.").font(.caption)
            Picker("Classification", selection: $classification) {
                Text("Unclassified").tag("unclassified_session")
                Text("Main sleep").tag("main_sleep")
                Text("Nap").tag("nap")
            }
            TextField("User estimated sleep minutes (optional)", text: $minutes).keyboardType(.decimalPad)
            TextField("Reason or context", text: $note, axis: .vertical)
            if let failure { Text(failure).foregroundStyle(.secondary) }
            Button("Save sleep confirmation") { Task { await save() } }.disabled(saving || note.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }.navigationTitle("Sleep confirmation")
            .toolbar { Button("Cancel") { dismiss() } }
            .onAppear {
                classification = selection.session["classification"].stringValue
                if let existing = selection.session["corrections"].arrayValue.first {
                    recordID = existing["record_id"].stringValue
                    minutes = existing["entry"]["content"]["corrected_asleep_minutes"].numberValue.map { String($0) } ?? ""
                    note = existing["entry"]["content"]["note"].stringValue
                }
            }
    }
    private func save() async {
        guard !saving, let client = model.client, let token = model.session?.token else { return }
        saving = true
        defer { saving = false }
        do {
            var estimate: JSONValue = .null
            if !minutes.isEmpty {
                guard let value = Double(minutes), value.isFinite else { throw DraftError.message("Enter valid sleep minutes.") }
                estimate = .number(value)
            }
            var basis: [String: JSONValue] = [:]
            for stage in selection.session["timeline"].arrayValue { basis[stage["record_id"].stringValue] = stage["version"] }
            let content: JSONValue = .object(["source": .string(selection.source), "session_start": selection.session["start_at"], "session_end": selection.session["end_at"], "classification": .string(classification), "corrected_asleep_minutes": estimate, "basis_revisions": .object(basis), "note": .string(note)])
            let version = (selection.session["corrections"].arrayValue.first?["version"].numberValue ?? 0) + 1
            _ = try await model.request("wellness/entries/save", body: .object(["record_id": .string(recordID), "version": .number(version), "batch_id": .string(UUID().uuidString), "at": selection.session["start_at"], "timezone": .string(TimeZone.current.identifier), "entry": .object(["kind": .string("sleep_correction"), "content": content])]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            dismiss()
        } catch {
            guard !Task.isCancelled, model.session?.token == token else { return }
            failure = error.localizedDescription
        }
    }
}
