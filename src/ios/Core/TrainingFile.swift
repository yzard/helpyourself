import Foundation

public nonisolated struct TrainingFile: Sendable {
    public let filename: String
    public let format: String
    public let contents: String
    public var payloadField: String { format == "fit" ? "data_base64" : "xml" }
    public init(filename: String, data: Data) throws {
        let format = (filename as NSString).pathExtension.lowercased()
        guard ["gpx", "tcx", "fit"].contains(format), !filename.contains("/"), !filename.contains("\\"), filename.utf8.count <= 255 else { throw TrainingFileError.format }
        guard !data.isEmpty, data.count <= 4 * 1024 * 1024 else { throw TrainingFileError.sizeOrEncoding }
        // Round-trip equality rejects repaired invalid UTF-8 and preserves a leading BOM.
        let xml = format == "fit" ? data.base64EncodedString() : String(decoding: data, as: UTF8.self)
        guard format == "fit" || Data(xml.utf8) == data else { throw TrainingFileError.sizeOrEncoding }
        self.filename = filename; self.format = format; self.contents = xml
    }
}
public nonisolated enum TrainingFileError: LocalizedError {
    case format, sizeOrEncoding
    public var errorDescription: String? {
        switch self {
        case .format: "Select a .gpx, .tcx, or .fit file with a valid filename."
        case .sizeOrEncoding: "Select a nonempty file up to 4 MiB. XML files must use UTF-8."
        }
    }
}
