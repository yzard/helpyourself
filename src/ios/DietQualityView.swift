import SwiftUI

struct DietQualityView: View {
    var model: AppModel
    @State private var date = Date()
    @State private var result: JSONValue = .null
    @State private var failure: String?
    @State private var editing = false
    var body: some View {
        List {
            Section("Confirmed diet assessment") {
                DatePicker("Ending date", selection: $date, displayedComponents: .date)
                Text("HEI-2020 describes diet composition. It requires full-day food-pattern equivalents, not ordinary portion weights or photo estimates.").font(.caption)
                Button("Record full-day equivalents") { editing = true }
                NavigationLink("Edit or delete assessments", destination: DailyLogsView(model: model))
                Link("NCI definitions and scoring", destination: URL(string: "https://epi.grants.cancer.gov/hei/hei-scoring-method.html")!)
                DisclosureGroup("Required totals and units") {
                    Text("energy_kcal; total_fruit_cup_eq; whole_fruit_cup_eq; vegetables_cup_eq; greens_beans_cup_eq; whole_grains_oz_eq; dairy_cup_eq; protein_foods_oz_eq; seafood_plant_oz_eq; unsaturated_fat_g; saturated_fat_g; refined_grains_oz_eq; sodium_mg; added_sugars_tsp_eq")
                    Text("Include legumes in both vegetable and protein totals. Unsaturated fat means monounsaturated plus polyunsaturated fat. Added sugar uses teaspoon equivalents.")
                }
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            ForEach(Array(result["windows"].arrayValue.enumerated()), id: \.offset) { _, window in
                Section("\(Int(window["days"].numberValue ?? 0)) days") {
                    Text("\(Int(window["complete_days"].numberValue ?? 0)) complete days")
                    Text("HEI: " + (window["result"]["total"].numberValue?.formatted() ?? "Insufficient confirmed inputs"))
                    ForEach(Array(window["result"]["components"].arrayValue.enumerated()), id: \.offset) { _, component in
                        LabeledContent(component["component"].stringValue.replacingOccurrences(of: "_", with: " "), value: (component["score"].numberValue?.formatted() ?? "Unknown") + " / " + (component["maximum"].numberValue?.formatted() ?? ""))
                    }
                }
            }
            Section("Method") { ForEach(result["notes"].arrayValue.map(\.stringValue), id: \.self) { Text($0).font(.caption) } }
        }.navigationTitle("Diet quality").task(id: date) { await load() }
            .sheet(isPresented: $editing, onDismiss: { Task { await load() } }) { NavigationStack { LogEditor(model: model, existing: nil, initialKind: "diet_quality", initialValues: [:], initialDate: date) } }
    }
    private func load() async {
        guard let client = model.client, let token = model.session?.token else { return }
        let formatter = DateFormatter(); formatter.dateFormat = "yyyy-MM-dd"; formatter.locale = Locale(identifier: "en_US_POSIX"); formatter.timeZone = .current
        do {
            let response = try await model.request("wellness/diet", body: .object(["date": .string(formatter.string(from: date)), "timezone": .string(TimeZone.current.identifier)]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            result = response; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
}
