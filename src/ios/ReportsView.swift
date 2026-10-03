import SwiftUI
import PhotosUI
import UniformTypeIdentifiers
import VisionKit
import ImageIO

struct ReportsView: View {
    var model: AppModel
    @State private var importing = false
    @State private var scanning = false
    @State private var choosingPhoto = false
    @State private var photo: PhotosPickerItem?
    var body: some View {
        List {
            if let refresh = model.lastRefresh { Section { Text("Last refreshed \(refresh.formatted())").font(.caption).foregroundStyle(.secondary) } }
            if !model.drafts.isEmpty {
                Section("Waiting to upload") {
                    ForEach(model.drafts) { draft in Label(draft.filename, systemImage: "icloud.and.arrow.up")
                        .swipeActions { Button("Discard", role: .destructive) { model.discardDraft(draft.id) }.disabled(model.isBusy) }
                    }
                    Button("Retry uploads") { Task { await model.sendDrafts() } }.disabled(model.isBusy)
                }
            }
            Section("Your reports") {
                if model.reports.isEmpty { ContentUnavailableView("Add your first report", systemImage: "doc.badge.plus", description: Text("Import a PDF, choose a photo, or scan a paper report.")) }
                ForEach(model.reports.sorted { ($0["created_at"].numberValue ?? 0) > ($1["created_at"].numberValue ?? 0) }, id: \.identifier) { report in
                    NavigationLink { ReportView(model: model, reportID: report["report_id"].stringValue) } label: {
                        VStack(alignment: .leading, spacing: 5) {
                            Text(report["original_name"].stringValue).font(.headline)
                            Text("\(Int(report["page_count"].numberValue ?? 0)) pages · revision \(Int(report["revision"].numberValue ?? 1))").font(.caption).foregroundStyle(.secondary)
                            if let job = model.jobs.first(where: { $0["file_id"] == report["report_id"] }) {
                                Text(job["status"].stringValue == "blocked" ? "Manual review available · OCR is off" : "Extraction: \(job["status"].stringValue)").font(.caption)
                            }
                        }.padding(.vertical, 4)
                    }
                }
            }
        }
        .navigationTitle("Reports")
        .refreshable { await model.refresh() }
        .toolbar { ToolbarItem(placement: .primaryAction) {
            Menu {
                Button("Import PDF or image", systemImage: "folder") { importing = true }
                Button("Choose photo", systemImage: "photo") { choosingPhoto = true }
                if VNDocumentCameraViewController.isSupported { Button("Scan report", systemImage: "doc.viewfinder") { scanning = true } }
            } label: { Image(systemName: "plus") }.disabled(model.isBusy)
        } }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.pdf, .image], allowsMultipleSelection: false) { result in
            Task {
                do {
                    guard let url = try result.get().first else { return }
                    let access = url.startAccessingSecurityScopedResource(); defer { if access { url.stopAccessingSecurityScopedResource() } }
                    let bytes = try Data(contentsOf: url)
                    if url.pathExtension.lowercased() == "pdf" { await model.addDocument(bytes, filename: url.lastPathComponent, contentType: "application/pdf", originalBytes: nil, originalFilename: nil, originalContentType: nil) }
                    else { await addPhoto(bytes, name: url.deletingPathExtension().lastPathComponent) }
                } catch { model.errorMessage = error.localizedDescription }
            }
        }
        .photosPicker(isPresented: $choosingPhoto, selection: $photo, matching: .images)
        .onChange(of: photo) { _, item in Task {
            do { if let bytes = try await item?.loadTransferable(type: Data.self) { await addPhoto(bytes, name: "Report photo") } }
            catch { model.errorMessage = error.localizedDescription }
            photo = nil
        } }
        .sheet(isPresented: $scanning) { DocumentScanner { result in
            scanning = false
            switch result {
            case .success(let images): Task {
                for (index, bytes) in images.enumerated() {
                    await model.addDocument(bytes, filename: "Scanned report page \(index + 1).png", contentType: "image/png", originalBytes: nil, originalFilename: nil, originalContentType: nil)
                }
            }
            case .failure(let error): model.errorMessage = error.localizedDescription
            }
        } }
    }
    private func addPhoto(_ bytes: Data, name: String) async {
        guard let source = CGImageSourceCreateWithData(bytes as CFData, nil), let identifier = CGImageSourceGetType(source),
              let type = UTType(identifier as String), let contentType = type.preferredMIMEType else { model.errorMessage = "Could not decode this image"; return }
        let filename = name + "." + (type.preferredFilenameExtension ?? "image")
        if type == .jpeg || type == .png {
            await model.addDocument(bytes, filename: filename, contentType: contentType, originalBytes: nil, originalFilename: nil, originalContentType: nil)
            return
        }
        guard ["image/heic", "image/heif"].contains(contentType), let image = UIImage(data: bytes), let jpeg = image.jpegData(compressionQuality: 0.92) else {
            model.errorMessage = "Choose a JPEG, PNG, HEIC or HEIF photo"; return
        }
        await model.addDocument(jpeg, filename: name + ".jpg", contentType: "image/jpeg", originalBytes: bytes, originalFilename: filename, originalContentType: contentType)
    }
}

struct ReportView: View {
    var model: AppModel
    let reportID: String
    @State private var document: JSONValue = .null
    @State private var editor: EditTarget?
    @State private var healthWriter: EditTarget?
    @State private var preview: DocumentLocation?
    @State private var contextVisible = false
    @State private var relationsVisible = false
    @State private var deleting = false
    @Environment(\.dismiss) private var dismiss

    struct EditTarget: Identifiable { let id = UUID(); let observation: JSONValue }
    var body: some View {
        List {
            Section {
                Button("View original report", systemImage: "doc.text.magnifyingglass") { showSource(page: 1) }
                Button("Add result manually", systemImage: "plus") { editor = EditTarget(observation: .null) }
                Button("Collection context", systemImage: "note.text") { contextVisible = true }
                Button("Duplicate or revised report", systemImage: "doc.on.doc") { relationsVisible = true }
                if !document["duplicate_candidates"].arrayValue.isEmpty { Text("An identical file already exists. Review the duplicate relationship before confirming results.").font(.caption).foregroundStyle(.orange) }
                if let job = model.jobs.first(where: { $0["file_id"].stringValue == reportID }), job["status"] == .string("failed") {
                    Button("Retry failed extraction") { Task { await model.perform { if let client = model.client { _ = try await client.post("jobs/retry", body: .object(["job_id": job["job_id"]])) } }; await model.refresh(); await reload() } }
                }
                if !document["relation"]["preferred_report_id"].stringValue.isEmpty {
                    Text("Excluded from trends: \(document["relation"]["kind"].stringValue). Original data is retained.").font(.caption)
                    Button("Treat as independent report") { Task { await unlink() } }
                }
            }
            Section("Review results") {
                if document["observations"].arrayValue.isEmpty { Text("No extracted results yet. You can add results manually while OCR is disabled or processing.").foregroundStyle(.secondary) }
                ForEach(document["observations"].arrayValue, id: \.identifier) { observation in
                    VStack(alignment: .leading, spacing: 7) {
                        Button { editor = EditTarget(observation: observation) } label: {
                            VStack(alignment: .leading, spacing: 5) {
                                HStack { Text(observation["payload"]["raw_name"].stringValue).font(.headline); Spacer(); Text(observation["status"].stringValue.capitalized).font(.caption) }
                                Text("\(observation["payload"]["raw_result"].stringValue) \(observation["payload"]["raw_unit"].stringValue)").foregroundStyle(.primary)
                                Text(LaboratoryPresentation.resultExplanation(observation["interpretation"])).font(.caption)
                                Text(LaboratoryPresentation.originalReference(observation["payload"], reference: observation["interpretation"]["reference"])).font(.caption).foregroundStyle(.secondary)
                                Text(LaboratoryPresentation.standardizedReference(observation["interpretation"]["reference"])).font(.caption).foregroundStyle(.secondary)
                            }
                        }.buttonStyle(.plain)
                        Button("View source · page \(Int(observation["payload"]["source"]["page"].numberValue ?? 1))") { showSource(page: Int(observation["payload"]["source"]["page"].numberValue ?? 1)) }.font(.caption)
                        NavigationLink("Revision history") { ObservationHistory(model: model, observationID: observation["observation_id"].stringValue) }.font(.caption)
                        if observation["status"] == .string("confirmed"), observation["payload"]["metric_id"] == .string("glucose") {
                            Button("Save to Apple Health") { healthWriter = EditTarget(observation: observation) }.font(.caption).disabled(model.isBusy)
                        }
                    }.buttonStyle(.borderless).padding(.vertical, 5)
                }
            }
            if !document["pages"].arrayValue.isEmpty {
                Section("Page processing") {
                    ForEach(Array(document["pages"].arrayValue.enumerated()), id: \.offset) { _, page in
                        DisclosureGroup("Page \(Int(page["page"].numberValue ?? 0)) · \(page["status"].stringValue)") { Text(page["content"].stringValue).font(.caption).textSelection(.enabled) }
                    }
                }
            }
            if !document["extraction_inputs"].arrayValue.isEmpty {
                Section("Document evidence") {
                    ForEach(Array(document["extraction_inputs"].arrayValue.enumerated()), id: \.offset) { _, input in
                        NavigationLink("Page \(Int(input["page"].numberValue ?? 0)) · text layer: \(input["text_status"].stringValue.isEmpty ? "image only" : input["text_status"].stringValue)") {
                            DocumentEvidenceView(model: model, reportID: reportID, input: input)
                        }
                    }
                }
            }
            if !document["extraction_outputs"].arrayValue.isEmpty {
                Section("Original OCR output") {
                    ForEach(Array(document["extraction_outputs"].arrayValue.enumerated()), id: \.offset) { _, output in
                        NavigationLink("Page \(Int(output["page"].numberValue ?? 0)) · \(output["stage"].stringValue)") {
                            ExtractionOutputView(model: model, reportID: reportID, output: output)
                        }
                    }
                }
            }
            Section { Button("Delete report", role: .destructive) { deleting = true } }
        }
        .navigationTitle(document["report"]["original_name"].stringValue.isEmpty ? "Report" : document["report"]["original_name"].stringValue)
        .navigationBarTitleDisplayMode(.inline)
        .task { await reload() }
        .refreshable { await reload() }
        .sheet(item: $preview) { DocumentPreview(location: $0) }
        .sheet(item: $editor) { target in
            ObservationEditor(observation: target.observation, metrics: model.metrics, pageCount: Int(document["report"]["page_count"].numberValue ?? 1)) { change in
                try await save(observations: [change], context: .null)
            }
        }
        .sheet(item: $healthWriter, onDismiss: { Task { await reload() } }) { target in ReviewedGlucoseWriter(model: model, observation: target.observation) }
        .sheet(isPresented: $contextVisible) { ContextEditor(context: document["context"]) { context in try await save(observations: [], context: context) } }
        .sheet(isPresented: $relationsVisible) {
            NavigationStack { List(model.reports.filter { $0["report_id"].stringValue != reportID }, id: \.identifier) { report in
                VStack(alignment: .leading, spacing: 12) {
                    Text(report["original_name"].stringValue).font(.headline)
                    Button("This is a duplicate; prefer selected report") { Task { await relate(to: report["report_id"].stringValue, kind: "duplicate") } }
                    Button("Selected report revises this report") { Task { await relate(to: report["report_id"].stringValue, kind: "superseded") } }
                }.buttonStyle(.borderless)
            }.navigationTitle("Prefer another report").toolbar { Button("Done") { relationsVisible = false } } }
        }
        .confirmationDialog("Delete the original file, results and related analysis?", isPresented: $deleting) {
            Button("Delete report", role: .destructive) { Task { await model.perform {
                guard let client = model.client else { return }
                _ = try await client.post("reports/delete", body: .object(["report_id": .string(reportID), "expected_revision": document["report"]["revision"]]))
                try model.archive?.remove(name: "report-\(reportID).json"); try model.archive?.remove(name: "source-\(reportID)")
                dismiss()
            }; await model.refresh() } }
        } message: { Text("It disappears from your archive immediately. The server then removes stored files. Offline backups you made are separate.") }
    }
    private func reload() async { do { document = try await model.report(reportID) } catch { model.errorMessage = error.localizedDescription } }
    private func save(observations: [JSONValue], context: JSONValue) async throws {
        guard let client = model.client else { throw APIError.invalidResponse }
        document = try await client.post("reports/review", body: .object(["report_id": .string(reportID), "expected_revision": document["report"]["revision"], "context": context, "observations": .array(observations)]))
        try model.archive?.save(document, name: "report-\(reportID).json")
        await model.refresh()
    }
    private func showSource(page: Int) { Task { do { preview = DocumentLocation(url: try await model.source(reportID), page: page) } catch { model.errorMessage = error.localizedDescription } } }
    private func relate(to preferred: String, kind: String) async {
        await model.perform { guard let client = model.client else { return }
            _ = try await client.post("reports/relate", body: .object(["report_id": .string(reportID), "preferred_report_id": .string(preferred), "kind": .string(kind), "expected_revision": document["report"]["revision"]]))
            relationsVisible = false
        }; await reload()
    }
    private func unlink() async {
        await model.perform { guard let client = model.client else { return }
            _ = try await client.post("reports/relate", body: .object(["report_id": .string(reportID), "preferred_report_id": .null, "kind": .string("duplicate"), "expected_revision": document["report"]["revision"]]))
        }; await reload()
    }
}

private struct ObservationHistory: View {
    var model: AppModel
    let observationID: String
    @State private var history: [JSONValue] = []
    var body: some View {
        List(Array(history.enumerated()), id: \.offset) { _, revision in
            VStack(alignment: .leading) { Text("Revision \(Int(revision["revision"].numberValue ?? 0)) · \(revision["status"].stringValue)").font(.headline)
                Text("\(revision["payload"]["raw_name"].stringValue): \(revision["payload"]["raw_result"].stringValue) \(revision["payload"]["raw_unit"].stringValue)") }
        }.navigationTitle("Revision history").task { do { if let client = model.client { history = try await client.post("observations/history", body: .object(["observation_id": .string(observationID)]))["history"].arrayValue } } catch { model.errorMessage = error.localizedDescription } }
    }
}

private struct ExtractionOutputView: View {
    var model: AppModel
    let reportID: String
    let output: JSONValue
    @State private var original: JSONValue = .null
    var body: some View {
        ScrollView { VStack(alignment: .leading, spacing: 16) {
            Text(output["model"].stringValue).font(.caption)
            Text(original["content"].stringValue).textSelection(.enabled)
            DisclosureGroup("Complete provider response") { Text(original["response_body"].stringValue).font(.caption.monospaced()).textSelection(.enabled) }
        }.padding() }.navigationTitle("Original OCR output").task {
            do {
                guard let client = model.client else { return }
                original = try await client.post("reports/extraction/get", body: .object(["report_id": .string(reportID), "run_id": output["run_id"], "page": output["page"], "stage": output["stage"]]))
            } catch { model.errorMessage = error.localizedDescription }
        }
    }
}

private struct ReviewedGlucoseWriter: View {
    var model: AppModel
    let observation: JSONValue
    @State private var sampledAt = Date()
    @State private var verifiedTime = false
    @State private var error: String?
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        NavigationStack { Form {
            Text("\(observation["payload"]["raw_result"].stringValue) \(observation["payload"]["raw_unit"].stringValue)")
            Text("Report collection date: \(observation["payload"]["sampled_at"].stringValue)").font(.caption)
            DatePicker("Collection date and time", selection: $sampledAt)
            Toggle("I verified the collection date and time", isOn: $verifiedTime)
            Text("Apple Health requires a timestamp. Check the original report before saving. The complete report and its revisions remain on your server.").font(.caption)
            if let error { Text(error).foregroundStyle(.red) }
            Button("Save reviewed glucose") { Task {
                guard let session = model.session, !model.isBusy else { return }
                var saved = false
                await model.perform {
                    do {
                        guard let client = model.client else { throw APIError.invalidResponse }
                        let timestamp = ISO8601DateFormatter().string(from: sampledAt)
                        var desired = observation["payload"]; desired["sampled_at"] = .string(timestamp)
                        var report = try await model.report(observation["report_id"].stringValue)
                        guard var current = report["observations"].arrayValue.first(where: { $0["observation_id"] == observation["observation_id"] }) else { throw APIError.invalidResponse }
                        if current["payload"] != desired {
                            guard current["revision"] == observation["revision"] else { throw APIError.status(409, "This result changed. Reload it before saving to Apple Health") }
                            report = try await client.post("reports/review", body: .object(["report_id": observation["report_id"], "expected_revision": report["report"]["revision"], "context": .null,
                                "observations": .array([.object(["observation_id": observation["observation_id"], "expected_revision": current["revision"], "status": .string("confirmed"), "payload": desired])])]))
                            guard let updated = report["observations"].arrayValue.first(where: { $0["observation_id"] == observation["observation_id"] }) else { throw APIError.invalidResponse }
                            current = updated
                        }
                        try model.archive?.save(report, name: "report-\(observation["report_id"].stringValue).json")
                        try await model.health.writeReviewedGlucose(current, server: session.server, userID: session.userID, sampledAt: sampledAt)
                        saved = true
                    }
                    catch { self.error = error.localizedDescription }
                }
                if saved { dismiss(); await model.synchronizeHealth(requestAccess: false) }
            } }.disabled(!verifiedTime || model.isBusy)
        }.navigationTitle("Save to Apple Health").toolbar { Button("Cancel") { dismiss() }.disabled(model.isBusy) } }
    }
}

private struct DocumentEvidenceView: View {
    var model: AppModel
    let reportID: String
    let input: JSONValue
    @State private var evidence: JSONValue = .null
    @State private var error: String?
    var body: some View {
        List {
            if let error { Text(error).foregroundStyle(.red) }
            if evidence == .null && error == nil { ProgressView() }
            else {
                Text("Text layer: \(evidence["text_layer"]["status"].stringValue.isEmpty ? "not applicable to image" : evidence["text_layer"]["status"].stringValue)")
                if !evidence["error_code"].stringValue.isEmpty { Text("Text extraction issue: \(evidence["error_code"].stringValue)").foregroundStyle(.orange) }
                Text("The page image is always used for recognition. PDF text may be incomplete or disagree with the image.").font(.caption)
                Text(evidence["text_layer"]["text"].stringValue).textSelection(.enabled)
                Text("Positioned words: \(evidence["text_layer"]["words"].arrayValue.count)").font(.caption)
            }
        }.navigationTitle("Document evidence").task {
            do {
                guard let client = model.client else { return }
                evidence = try await client.post("reports/input/get", body: .object(["report_id": .string(reportID), "run_id": input["run_id"], "page": input["page"]]))
            } catch { self.error = error.localizedDescription }
        }
    }
}
