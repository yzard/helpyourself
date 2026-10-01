import SwiftUI

struct AnalysisView: View {
    var model: AppModel
    @State private var runs: [JSONValue] = []
    var body: some View {
        List {
            Section {
                Text("Personal research preview").font(.headline)
                Text("Lipid-related hypotheses are unverified. Review the referenced data and discuss questions with your clinician. New confirmed reports can also start a review when the server's analysis provider is enabled.")
                Button("Review lipid history and recent health data") { Task { await model.perform {
                    guard let client = model.client else { return }
                    _ = try await client.post("analysis/create", body: dateScope(days: 90))
                }; await reload() } }.disabled(model.isBusy || !model.capabilities["analysis"].boolValue)
            }
            Section("Reviews") {
                ForEach(runs, id: \.identifier) { run in
                    NavigationLink { AnalysisDetail(model: model, runID: run["run_id"].stringValue) } label: {
                        VStack(alignment: .leading) {
                            Text(run["status"].stringValue.capitalized)
                            Text(Date(timeIntervalSince1970: run["created_at"].numberValue ?? 0), format: .dateTime).font(.caption)
                        }
                    }
                }
            }
        }.navigationTitle("Analysis").task { await reload() }.refreshable { await reload() }
    }
    private func reload() async { do { if let client = model.client { runs = try await client.post("analysis/list", body: .object([:]))["runs"].arrayValue } } catch { model.errorMessage = error.localizedDescription } }
}

private struct AnalysisDetail: View {
    var model: AppModel
    let runID: String
    @State private var run: JSONValue = .null
    @State private var note = ""
    var body: some View {
        List {
            Section {
                Text("Unverified · \(run["status"].stringValue)").font(.headline)
                if run["status"] == .string("stale") { Text("Source data changed. This review has been invalidated; create a new review.") }
                if run["status"] == .string("failed") { Button("Retry") { Task { await action("analysis/retry", body: .object(["run_id": .string(runID)])) } } }
                Text(run["output"]["review"]["summary"].stringValue)
            }
            ForEach(Array(run["output"]["review"]["findings"].arrayValue.enumerated()), id: \.offset) { _, finding in
                Section(finding["title"].stringValue) {
                    Text(finding["hypothesis"].stringValue)
                    lines("Other explanations", finding["other_explanations"])
                    lines("Missing information", finding["missing_information"])
                    lines("Questions for your clinician", finding["questions_for_clinician"])
                    ForEach(finding["observation_ids"].arrayValue, id: \.stringValue) { id in
                        if let observation = run["input"]["observations"].arrayValue.first(where: { $0["observation_id"] == id }) {
                            NavigationLink("Source: \(observation["payload"]["raw_name"].stringValue) · \(observation["payload"]["raw_result"].stringValue)") { ReportView(model: model, reportID: observation["report_id"].stringValue) }
                        }
                    }
                }
            }
            Section("References") { ForEach(run["output"]["evidence"].arrayValue, id: \.selfDescription) { source in
                if let url = URL(string: source["url"].stringValue) { Link(source["title"].stringValue, destination: url) }
            } }
            if run["status"] == .string("ready") {
                Section("Your feedback") {
                    TextField("Incorrect, helpful, or discussed with your clinician", text: $note, axis: .vertical)
                    Button("Save feedback") { Task { await action("analysis/feedback", body: .object(["run_id": .string(runID), "note": .string(note)])); note = "" } }.disabled(note.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.isBusy)
                    ForEach(run["feedback"].arrayValue, id: \.identifier) { Text($0["note"].stringValue) }
                }
            }
        }.navigationTitle("Lipid review").task { await reload() }.refreshable { await reload() }
    }
    private func lines(_ title: String, _ values: JSONValue) -> some View { VStack(alignment: .leading, spacing: 5) { Text(title).font(.headline); ForEach(Array(values.arrayValue.enumerated()), id: \.offset) { _, value in Text(value.stringValue) } } }
    private func reload() async { do { if let client = model.client { run = try await client.post("analysis/get", body: .object(["run_id": .string(runID)])) } } catch { model.errorMessage = error.localizedDescription } }
    private func action(_ path: String, body: JSONValue) async { await model.perform { if let client = model.client { _ = try await client.post(path, body: body) } }; await reload() }
}

private extension JSONValue { var selfDescription: String { self["source_id"].stringValue } }
