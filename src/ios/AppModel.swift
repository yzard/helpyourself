import Foundation
import Observation
import CryptoKit

@MainActor @Observable
final class AppModel {
    private(set) var session: StoredSession?
    private(set) var archive: LocalArchive?
    var reports: [JSONValue] = []
    var jobs: [JSONValue] = []
    var drafts: [UploadDraft] = []
    var metrics: [JSONValue] = []
    var capabilities: JSONValue = .null
    var errorMessage: String?
    var isBusy = false
    var lastRefresh: Date?
    let health = HealthBridge()
    var client: APIClient? { session.map { APIClient(server: $0.server, token: $0.token) } }

    init() {
        do { if let saved = try SessionVault.load() { try activate(saved) } }
        catch { errorMessage = error.localizedDescription }
    }

    private func activate(_ saved: StoredSession) throws {
        let key = SHA256.hash(data: Data("\(saved.server.absoluteString)|\(saved.userID)".utf8)).map { String(format: "%02x", $0) }.joined()
        let root = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let cache = try LocalArchive(directory: root.appendingPathComponent(key))
        reports = try cache.load([JSONValue].self, name: "reports.json") ?? []
        drafts = try cache.load([UploadDraft].self, name: "drafts.json") ?? []
        metrics = try cache.load([JSONValue].self, name: "metrics.json") ?? []
        lastRefresh = try cache.load(Date.self, name: "refresh.json")
        session = saved; archive = cache
    }

    func login(server: String, username: String, password: String) async {
        await perform {
            let address = try APIClient.serverURL(server)
            let response = try await APIClient(server: address, token: nil).post("session/login", body: .object(["username": .string(username), "password": .string(password)]))
            let saved = StoredSession(server: address, userID: response["user"]["user_id"].stringValue, username: response["user"]["username"].stringValue, token: response["token"].stringValue)
            guard !saved.userID.isEmpty, !saved.token.isEmpty else { throw APIError.invalidResponse }
            try SessionVault.save(saved); try activate(saved)
            UserDefaults.standard.set(address.absoluteString, forKey: "lastServer")
        }
        if session != nil { await refresh() }
    }

    func refresh() async {
        guard let client, let archive, let identity = session?.userID, !isBusy else { return }
        await perform {
            let status = try await client.post("server/status", body: .object([:]))
            var loaded: [JSONValue] = []; var cursor: JSONValue = .null
            repeat {
                let page = try await client.post("reports/list", body: .object(["limit": .number(100), "after_id": cursor]))["reports"].arrayValue
                loaded.append(contentsOf: page)
                guard let last = page.last else { break }
                cursor = last["report_id"]
                if page.count < 100 { break }
            } while true
            let definitions = try await client.post("metrics/list", body: .object([:]))["metrics"].arrayValue
            let tasks = try await client.post("jobs/list", body: .object(["limit": .number(100)]))["jobs"].arrayValue
            guard session?.userID == identity else { return }
            let retained = Set(loaded.map { $0["report_id"].stringValue })
            for previous in reports where !retained.contains(previous["report_id"].stringValue) {
                let id = previous["report_id"].stringValue
                try archive.remove(name: "report-\(id).json"); try archive.remove(name: "source-\(id)")
            }
            capabilities = status["capabilities"]; reports = loaded; jobs = tasks; metrics = definitions; lastRefresh = Date()
            try archive.save(reports, name: "reports.json"); try archive.save(metrics, name: "metrics.json"); try archive.save(lastRefresh, name: "refresh.json")
        }
    }

    func addDocument(_ bytes: Data, filename: String, contentType: String, originalBytes: Data?, originalFilename: String?, originalContentType: String?) async {
        guard let archive else { return }
        do {
            guard bytes.count <= 20 * 1024 * 1024 else { throw APIError.status(413, "The document exceeds 20 MiB") }
            guard (originalBytes == nil) == (originalFilename == nil), (originalBytes == nil) == (originalContentType == nil) else { throw APIError.invalidResponse }
            if let originalBytes, originalBytes.count > 20 * 1024 * 1024 { throw APIError.status(413, "The original photo exceeds 20 MiB") }
            let draft = UploadDraft(id: UUID(), filename: filename, contentType: contentType, createdAt: Date(), originalFilename: originalFilename, originalContentType: originalContentType)
            if let originalBytes { try archive.saveBytes(originalBytes, name: "draft-original-\(draft.id.uuidString)") }
            try archive.saveBytes(bytes, name: "draft-\(draft.id.uuidString)")
            drafts.append(draft); try archive.save(drafts, name: "drafts.json")
        } catch { errorMessage = error.localizedDescription; return }
        await sendDrafts()
    }

    func sendDrafts() async {
        guard let client, let archive, let identity = session?.userID, !isBusy else { return }
        await perform {
            for draft in drafts {
                _ = try await client.upload(archive.bytes(name: "draft-\(draft.id.uuidString)"), draft: draft, originalBytes: draft.originalFilename == nil ? nil : archive.bytes(name: "draft-original-\(draft.id.uuidString)"))
                guard session?.userID == identity else { return }
                // Persist queue removal only after the server acknowledges this stable upload ID.
                drafts.removeAll { $0.id == draft.id }; try archive.save(drafts, name: "drafts.json")
                try archive.remove(name: "draft-\(draft.id.uuidString)"); try archive.remove(name: "draft-original-\(draft.id.uuidString)")
            }
        }
        await refresh()
    }

    func report(_ id: String) async throws -> JSONValue {
        guard let client, let archive, let currentSession = session else { throw APIError.invalidResponse }
        do {
            let report = try await client.post("reports/get", body: .object(["report_id": .string(id)]))
            guard session?.token == currentSession.token else { throw APIError.invalidResponse }
            try archive.save(report, name: "report-\(id).json"); return report
        } catch {
            guard session?.token == currentSession.token else { throw APIError.invalidResponse }
            if case APIError.status(404, _) = error { try archive.remove(name: "report-\(id).json"); try archive.remove(name: "source-\(id)"); throw error }
            if let cached = try archive.load(JSONValue.self, name: "report-\(id).json") { return cached }
            throw error
        }
    }

    func source(_ id: String) async throws -> URL {
        guard let client, let archive, let currentSession = session else { throw APIError.invalidResponse }
        let location = try archive.location("source-\(id)")
        if FileManager.default.fileExists(atPath: location.path) { return location }
        try await client.download("files/\(id)/download", destination: location)
        guard session?.token == currentSession.token else { try? archive.remove(name: "source-\(id)"); throw APIError.invalidResponse }
        return location
    }

    func synchronizeHealth(requestAccess: Bool, requestSelectedAccess: Bool = false) async {
        guard let client, let archive, !isBusy else { return }
        await perform { try await health.synchronize(client: client, archive: archive, requestAccess: requestAccess, requestSelectedAccess: requestSelectedAccess) }
    }

    func logout() async {
        guard !isBusy else { return }
        let previous = client
        do { try clearLocalSession() }
        catch { errorMessage = error.localizedDescription; return }
        // Local removal does not wait for a network timeout. Server revocation is best effort.
        if let previous { Task { _ = try? await previous.post("session/logout", body: .object([:])) } }
    }

    func discardDraft(_ id: UUID) {
        guard let archive, !isBusy else { return }
        do {
            drafts.removeAll { $0.id == id }
            try archive.save(drafts, name: "drafts.json")
            try archive.remove(name: "draft-\(id.uuidString)"); try archive.remove(name: "draft-original-\(id.uuidString)")
        } catch { errorMessage = error.localizedDescription }
    }

    func clearLocalSession() throws {
        try SessionVault.clear(); try archive?.clear()
        session = nil; archive = nil; reports = []; drafts = []; jobs = []; metrics = []; lastRefresh = nil; capabilities = .null; health.status = "Not connected"
    }

    func perform(_ operation: () async throws -> Void) async {
        guard !isBusy else { return }
        isBusy = true; defer { isBusy = false }
        do { try await operation() }
        catch { errorMessage = error.localizedDescription }
    }
}
