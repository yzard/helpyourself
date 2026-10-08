import SwiftUI

struct FoodLookupView: View {
    var model: AppModel
    @State private var barcode = ""
    @State private var result: JSONValue = .null
    @State private var massConfirmed = false
    @State private var loading = false
    @State private var failure: String?
    var body: some View {
        List {
            Section("Open Food Facts") {
                TextField("Barcode", text: $barcode).keyboardType(.numberPad)
                Text("Searching sends this barcode to Open Food Facts. Your health archive stays on your server.").font(.caption)
                Button("Look up product") { Task { await lookup() } }.disabled(loading)
                if loading { ProgressView() }
                if let failure { Text(failure).foregroundStyle(.secondary) }
            }
            if result["candidate"] != .null {
                Section("Review the label") {
                    Text(result["candidate"]["content"]["name"].stringValue).font(.headline)
                    Text(result["candidate"]["content"]["brand"].stringValue)
                    ForEach(result["candidate"]["content"]["nutrients_per_100g"].objectValue.keys.sorted(), id: \.self) { key in
                        LabeledContent(key.replacingOccurrences(of: "_", with: " "), value: result["candidate"]["content"]["nutrients_per_100g"][key].numberValue?.formatted() ?? "Unknown")
                    }
                    Toggle("I confirmed these values are per 100 grams", isOn: $massConfirmed)
                    Text("Do not use values per 100 milliliters without a known density conversion.").font(.caption)
                    if massConfirmed {
                        NavigationLink("Correct fields and save food") { LogEditor(model: model, existing: nil, initialKind: "food", initialValues: values(), initialDate: Date()) }
                    }
                    Text(result["attribution"].stringValue).font(.caption)
                    Link("Open Food Facts database license", destination: URL(string: "https://world.openfoodfacts.org/terms-of-use")!)
                }
            }
        }.navigationTitle("Find food by barcode")
    }
    private func values() -> [String: String] {
        var candidate = result["candidate"]
        candidate["content"]["external_source"]["mass_basis_confirmed"] = .bool(massConfirmed)
        return EntryDraft.decode(candidate)
    }
    private func lookup() async {
        guard model.session != nil, !loading else { return }
        let token = model.session?.token
        loading = true; result = .null; massConfirmed = false; defer { loading = false }
        do {
            let response = try await model.request("wellness/food/lookup", body: .object(["barcode": .string(barcode)]))
            guard !Task.isCancelled, model.session?.token == token else { return }
            result = response; failure = nil
        } catch { if !Task.isCancelled, model.session?.token == token { failure = error.localizedDescription } }
    }
}
