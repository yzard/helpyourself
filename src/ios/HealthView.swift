import SwiftUI

struct HealthView: View {
    var model: AppModel
    @State private var coverage: [JSONValue] = []
    @State private var days: [JSONValue] = []
    @State private var kind = "resting_heart_rate"
    var body: some View {
        List {
            Section {
                Button("Connect Apple Health") { Task { await model.synchronizeHealth(requestAccess: true); await reload() } }.disabled(model.isBusy)
                Button("Select prescriptions and medications") { Task { await model.synchronizeHealth(requestAccess: true, requestSelectedAccess: true); await reload() } }.disabled(model.isBusy)
                Text("Vision prescriptions and medications require individual selection. Use this button each time you want to archive those records.").font(.caption).foregroundStyle(.secondary)
                Button("Sync now") { Task { await model.synchronizeHealth(requestAccess: false); await reload() } }.disabled(model.isBusy)
                Text(model.health.status).font(.caption)
                Text("Only data visible to this iPhone can sync. An empty result does not tell us whether access was denied or data is absent.").font(.caption).foregroundStyle(.secondary)
            }
            Section("Last 30 days · \(TimeZone.current.identifier)") {
                Picker("Metric", selection: $kind) {
                    Text("Resting heart rate").tag("resting_heart_rate"); Text("Heart rate").tag("heart_rate"); Text("HRV SDNN").tag("hrv_sdnn")
                    Text("Steps").tag("steps"); Text("Sleep").tag("sleep"); Text("Workout").tag("workout")
                }
                Text("Sources remain separate. Missing days are not zero; sample averages are not daily clinical measurements.").font(.caption)
                ForEach(Array(days.enumerated()), id: \.offset) { _, day in
                    VStack(alignment: .leading) {
                        Text(day["date"].stringValue)
                        Text(day["value"] == .null ? "No visible samples" : "\(day["value"].numberValue?.formatted() ?? "") \(day["unit"].stringValue)")
                        Text(day["source"].stringValue).font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            Section { Text("The app archives the HealthKit types available on this device, including sample metadata, ECG and other series, routes, clinical FHIR records, characteristics and activity summaries. Clinical records need separate permission and device support. Read coverage is shown below; hidden data cannot be inferred.").font(.caption).foregroundStyle(.secondary) }
            Section("Sync coverage") { ForEach(Array(coverage.enumerated()), id: \.offset) { _, entry in
                VStack(alignment: .leading) { Text(entry["record_type"].stringValue); Text(entry["status"].stringValue).font(.caption)
                    if let last = entry["last_success_at"].numberValue { Text("Last successful query: \(Date(timeIntervalSince1970: last).formatted())").font(.caption) }
                    if let first = entry["visible_start_at"].numberValue, let last = entry["visible_end_at"].numberValue { Text("Archived platform range: \(Date(timeIntervalSince1970: first).formatted(date: .abbreviated, time: .omitted)) – \(Date(timeIntervalSince1970: last).formatted(date: .abbreviated, time: .omitted))").font(.caption) } }
            } }
        }.navigationTitle("Health").task(id: kind) { await reload() }.refreshable { await reload() }
    }
    private func reload() async { do { if let client = model.client {
        coverage = try await model.request("health/coverage", body: .object([:]))["coverage"].arrayValue
        var scope = dateScope(days: 30); scope["record_type"] = .string(kind)
        days = try await model.request("health/aggregate", body: scope)["days"].arrayValue
    } } catch { model.errorMessage = error.localizedDescription } }
}

func dateScope(days: Int) -> JSONValue {
    let calendar = Calendar.current
    let end = Date(); let start = calendar.date(byAdding: .day, value: -days, to: end) ?? end
    let format = DateFormatter(); format.calendar = Calendar(identifier: .gregorian); format.locale = Locale(identifier: "en_US_POSIX"); format.timeZone = .current; format.dateFormat = "yyyy-MM-dd"
    return .object(["start_date": .string(format.string(from: start)), "end_date": .string(format.string(from: end)), "timezone": .string(TimeZone.current.identifier)])
}
