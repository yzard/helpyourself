import Foundation
import Security

enum SessionVault {
    private static let service = "helpyourself.session"
    static func save(_ session: StoredSession) throws {
        let encoded = try JSONEncoder().encode(session)
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: "active"]
        let attributes: [String: Any] = [kSecValueData as String: encoded, kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly]
        let updated = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if updated == errSecItemNotFound {
            let inserted = SecItemAdd(query.merging(attributes) { _, new in new } as CFDictionary, nil)
            guard inserted == errSecSuccess else { throw APIError.status(Int(inserted), "Could not save session securely") }
        } else if updated != errSecSuccess { throw APIError.status(Int(updated), "Could not update secure session") }
    }
    static func load() throws -> StoredSession? {
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: "active", kSecReturnData as String: true, kSecMatchLimit as String: kSecMatchLimitOne]
        var output: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &output)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let bytes = output as? Data else { throw APIError.status(Int(status), "Could not read secure session") }
        return try JSONDecoder().decode(StoredSession.self, from: bytes)
    }
    static func clear() throws {
        let status = SecItemDelete([kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: "active"] as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw APIError.status(Int(status), "Could not remove secure session") }
    }
}
