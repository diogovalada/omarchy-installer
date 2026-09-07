import Darwin
import Foundation

struct BridgeFailure: Error, Sendable {
  let code: String
  let message: String
  init(_ code: String, _ message: String) {
    self.code = code
    self.message = message
  }
}

struct BridgeRequest: Decodable, Sendable {
  struct Parameters: Decodable, Sendable {
    var choiceId: String?
    var confirmation: String?
    var allocationBytes: String?
    var value: Bool?
    var bindingDigest: String?
    var username: String?
    var password: String?
  }
  let version: Int
  let id: String
  let command: String
  let params: Parameters?

  static func decode(_ data: Data) throws -> BridgeRequest {
    guard data.count <= 65_536,
      let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
      Set(object.keys).isSubset(of: ["version", "id", "command", "params"])
    else { throw BridgeFailure("invalid_request", "Invalid request envelope") }
    let request = try JSONDecoder().decode(Self.self, from: data)
    guard request.version == 1,
      (1...128).contains(request.id.utf8.count),
      request.id.utf8.allSatisfy({
        (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0)
          || $0 == 45 || $0 == 95
      })
    else { throw BridgeFailure("invalid_request", "Invalid protocol version or request identifier") }
    let allowed: Set<String>
    switch request.command {
    case "probe", "inspect", "state", "review_plan", "refresh_helper", "cancel": allowed = []
    case "prepare_plan": allowed = ["allocationBytes"]
    case "choose_storage": allowed = ["choiceId", "confirmation", "allocationBytes"]
    case "acknowledge": allowed = ["value"]
    case "approve": allowed = ["bindingDigest"]
    case "execute", "retry_recovery": allowed = ["bindingDigest", "username", "password"]
    default: throw BridgeFailure("unknown_command", "Unknown bridge command")
    }
    if let parameters = object["params"] {
      guard let dictionary = parameters as? [String: Any],
        Set(dictionary.keys).isSubset(of: allowed)
      else { throw BridgeFailure("invalid_request", "Unexpected command parameters") }
    }
    return request
  }
}

/// Reads bounded lines on a worker, keeping credentials out of argv, files and logs.
/// Returning false closes input. The upstream helper has no active-cancel API.
func readBridgeInput(deliver: @escaping @Sendable (Data) async -> Bool) async {
  var pending = Data()
  var buffer = [UInt8](repeating: 0, count: 4096)
  while true {
    let count = buffer.withUnsafeMutableBytes { bytes in
      Darwin.read(STDIN_FILENO, bytes.baseAddress, bytes.count)
    }
    if count < 0 && errno == EINTR { continue }
    if count <= 0 { return }
    for byte in buffer.prefix(count) {
      if byte == 10 {
        guard await deliver(pending) else { return }
        pending.resetBytes(in: 0..<pending.count)
        pending.removeAll(keepingCapacity: true)
      } else {
        guard pending.count < 65_536 else { return }
        pending.append(byte)
      }
    }
    buffer.withUnsafeMutableBytes { bytes in
      if let base = bytes.baseAddress { memset(base, 0, bytes.count) }
    }
  }
}
