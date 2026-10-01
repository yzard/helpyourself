import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

public nonisolated enum APIError: Error, LocalizedError {
    case invalidServer
    case invalidResponse
    case status(Int, String)
    public var errorDescription: String? {
        switch self {
        case .invalidServer: "Enter an HTTPS server address without credentials or a query."
        case .invalidResponse: "The server returned an unreadable response."
        case .status(let code, let message): "\(message) (\(code))"
        }
    }
}

private nonisolated final class RejectRedirects: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask,
                    willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest,
                    completionHandler: @escaping @Sendable (URLRequest?) -> Void) {
        completionHandler(nil)
    }
}

public nonisolated final class APIClient: Sendable {
    public let server: URL
    public let token: String?
    private let session: URLSession

    public init(server: URL, token: String?) {
        self.server = server; self.token = token
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 120
        configuration.httpShouldSetCookies = false
        self.session = URLSession(configuration: configuration, delegate: RejectRedirects(), delegateQueue: nil)
    }

    deinit { session.invalidateAndCancel() }

    public static func serverURL(_ input: String) throws -> URL {
        guard let components = URLComponents(string: input.trimmingCharacters(in: .whitespacesAndNewlines)),
              let host = components.host, !host.isEmpty,
              components.user == nil, components.password == nil,
              components.query == nil, components.fragment == nil,
              components.scheme == "https" || (components.scheme == "http" && ["localhost", "127.0.0.1", "::1", "[::1]"].contains(host)),
              let url = components.url else { throw APIError.invalidServer }
        return url
    }

    public func post(_ path: String, body: JSONValue) async throws -> JSONValue {
        var request = request(path, method: "POST")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(body)
        let (bytes, response) = try await session.data(for: request)
        try check(response, bytes: bytes)
        return try JSONDecoder().decode(JSONValue.self, from: bytes)
    }

    public func upload(_ bytes: Data, draft: UploadDraft, originalBytes: Data?) async throws -> JSONValue {
        guard bytes.count <= 20 * 1024 * 1024 else { throw APIError.status(413, "Choose a file smaller than 20 MiB") }
        let boundary = UUID().uuidString
        let safeName = draft.filename.replacingOccurrences(of: "\"", with: "_").replacingOccurrences(of: "\r", with: "_").replacingOccurrences(of: "\n", with: "_")
        var body = Data("--\(boundary)\r\nContent-Disposition: form-data; name=\"file\"; filename=\"\(safeName)\"\r\nContent-Type: \(draft.contentType)\r\n\r\n".utf8)
        body.append(bytes)
        if let originalBytes {
            guard originalBytes.count <= 20 * 1024 * 1024, let originalFilename = draft.originalFilename, let originalContentType = draft.originalContentType else { throw APIError.invalidResponse }
            let name = originalFilename.replacingOccurrences(of: "\"", with: "_").replacingOccurrences(of: "\r", with: "_").replacingOccurrences(of: "\n", with: "_")
            body.append(Data("\r\n--\(boundary)\r\nContent-Disposition: form-data; name=\"original\"; filename=\"\(name)\"\r\nContent-Type: \(originalContentType)\r\n\r\n".utf8))
            body.append(originalBytes)
        } else if draft.originalFilename != nil || draft.originalContentType != nil { throw APIError.invalidResponse }
        body.append(Data("\r\n--\(boundary)--\r\n".utf8))
        var request = request("files/upload", method: "POST")
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")
        request.setValue(draft.id.uuidString, forHTTPHeaderField: "X-Upload-Id")
        let (responseBytes, response) = try await session.upload(for: request, from: body)
        try check(response, bytes: responseBytes)
        return try JSONDecoder().decode(JSONValue.self, from: responseBytes)
    }

    public func download(_ path: String, destination: URL) async throws {
        let (temporary, response) = try await session.download(for: request(path, method: "GET"))
        try check(response, bytes: Data())
        let manager = FileManager.default
        try manager.createDirectory(at: destination.deletingLastPathComponent(), withIntermediateDirectories: true)
        if manager.fileExists(atPath: destination.path) { try manager.removeItem(at: destination) }
        try manager.moveItem(at: temporary, to: destination)
        #if os(iOS)
        try manager.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: destination.path)
        #endif
    }

    private func request(_ path: String, method: String) -> URLRequest {
        var request = URLRequest(url: server.appendingPathComponent("api/v1").appendingPathComponent(path))
        request.httpMethod = method
        if let token { request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization") }
        return request
    }

    private func check(_ response: URLResponse, bytes: Data) throws {
        guard let response = response as? HTTPURLResponse else { throw APIError.invalidResponse }
        guard (200..<300).contains(response.statusCode) else {
            let error = try? JSONDecoder().decode(JSONValue.self, from: bytes)
            let message = error?["error"]["message"].stringValue ?? "Request failed"
            throw APIError.status(response.statusCode, message.isEmpty ? "Request failed" : message)
        }
    }
}
