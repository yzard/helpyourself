import Foundation

public nonisolated enum JSONValue: Codable, Sendable, Equatable {
    case object([String: JSONValue])
    case array([JSONValue])
    case string(String)
    case number(Double)
    case bool(Bool)
    case null

    public init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() { self = .null }
        else if let value = try? container.decode(Bool.self) { self = .bool(value) }
        else if let value = try? container.decode(String.self) { self = .string(value) }
        else if let value = try? container.decode(Double.self) { self = .number(value) }
        else if let value = try? container.decode([JSONValue].self) { self = .array(value) }
        else { self = .object(try container.decode([String: JSONValue].self)) }
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .object(let value): try container.encode(value)
        case .array(let value): try container.encode(value)
        case .string(let value): try container.encode(value)
        case .number(let value): try container.encode(value)
        case .bool(let value): try container.encode(value)
        case .null: try container.encodeNil()
        }
    }

    public subscript(key: String) -> JSONValue {
        get { if case .object(let fields) = self { return fields[key] ?? .null }; return .null }
        set { var fields = objectValue; fields[key] = newValue; self = .object(fields) }
    }
    public var objectValue: [String: JSONValue] { if case .object(let fields) = self { return fields }; return [:] }
    public var arrayValue: [JSONValue] { if case .array(let values) = self { return values }; return [] }
    public var stringValue: String { if case .string(let value) = self { return value }; return "" }
    public var numberValue: Double? {
        if case .number(let value) = self { return value }
        if case .string(let value) = self { return Double(value) }
        return nil
    }
    public var boolValue: Bool { if case .bool(let value) = self { return value }; return false }
    public static func optional(_ text: String) -> JSONValue { text.isEmpty ? .null : .string(text) }
    public var identifier: String {
        for key in ["feedback_id", "observation_id", "run_id", "export_id", "job_id", "report_id", "metric_id"] {
            let candidate = self[key].stringValue
            if !candidate.isEmpty { return candidate }
        }
        return stringValue
    }
}

public nonisolated struct StoredSession: Codable, Sendable {
    public let server: URL
    public let userID: String
    public let username: String
    public let token: String
    public init(server: URL, userID: String, username: String, token: String) {
        self.server = server; self.userID = userID; self.username = username; self.token = token
    }
}

public nonisolated struct UploadDraft: Codable, Identifiable, Sendable {
    public let id: UUID
    public let filename: String
    public let contentType: String
    public let createdAt: Date
    public let originalFilename: String?
    public let originalContentType: String?
    public init(id: UUID, filename: String, contentType: String, createdAt: Date, originalFilename: String?, originalContentType: String?) {
        self.id = id; self.filename = filename; self.contentType = contentType; self.createdAt = createdAt; self.originalFilename = originalFilename; self.originalContentType = originalContentType
    }
}

public nonisolated struct SyncCheckpoint: Codable, Sendable {
    public let requests: [JSONValue]
    public let nextAnchor: Data
    public init(requests: [JSONValue], nextAnchor: Data) { self.requests = requests; self.nextAnchor = nextAnchor }
}
