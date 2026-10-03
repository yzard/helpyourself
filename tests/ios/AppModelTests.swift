import Foundation
import Testing
@testable import Helpyourself

@MainActor @Suite(.serialized) struct AppModelTests {
    @Test func sessionUploadRetryCacheAndLogout() async throws {
        try SessionVault.clear()
        let control = APIClient(server: URL(string: "http://localhost:18765")!, token: nil)
        _ = try await control.post("test/reset", body: .object([:]))
        let model = AppModel()
        await model.login(server: "http://localhost:18765", username: "simulator", password: "synthetic-test-password")
        #expect(model.errorMessage == nil)
        #expect(model.session?.userID == "simulator-only")
        #expect(try SessionVault.load()?.token == "synthetic-token")
        await model.addDocument(Data(count: 20 * 1024 * 1024 + 1), filename: "oversized.png", contentType: "image/png", originalBytes: nil, originalFilename: nil, originalContentType: nil)
        #expect(model.errorMessage != nil && model.drafts.isEmpty)
        model.errorMessage = nil
        await model.addDocument(Data(), filename: "invalid-pair.jpg", contentType: "image/jpeg", originalBytes: Data(), originalFilename: nil, originalContentType: nil)
        #expect(model.errorMessage != nil && model.drafts.isEmpty)
        model.errorMessage = nil
        _ = try await control.post("test/fail-upload", body: .object([:]))
        await model.addDocument(Data("synthetic report".utf8), filename: "fixture.png", contentType: "image/png", originalBytes: nil, originalFilename: nil, originalContentType: nil)
        #expect(model.drafts.count == 1)
        let draftID = try #require(model.drafts.first?.id)
        let restored = AppModel()
        #expect(restored.drafts.first?.id == draftID)
        await restored.sendDrafts()
        #expect(restored.errorMessage == nil)
        #expect(restored.drafts.isEmpty)
        let state = try await control.post("test/state", body: .object([:]))
        #expect(state["upload_ids"].arrayValue == [.string(draftID.uuidString), .string(draftID.uuidString)])
        #expect(restored.reports.count == 1)
        let report = try await restored.report("fixture-report")
        #expect(report["report"]["report_id"] == .string("fixture-report"))
        let source = try await restored.source("fixture-report")
        #expect(FileManager.default.fileExists(atPath: source.path))
        _ = try await control.post("test/delete-report", body: .object([:]))
        do { _ = try await restored.report("fixture-report"); Issue.record("Deleted report returned from cache") }
        catch { #expect(!FileManager.default.fileExists(atPath: source.path)) }
        await restored.refresh()
        #expect(restored.reports.isEmpty)
        let cache = try #require(restored.archive)
        await restored.logout()
        #expect(restored.session == nil && restored.archive == nil)
        #expect(try SessionVault.load() == nil)
        #expect(try cache.load([UploadDraft].self, name: "drafts.json") == nil)
    }
}
