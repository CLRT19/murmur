/// murmur-transcribe — macOS Speech-to-Text helper for Murmur
///
/// Usage: murmur-transcribe <audio.wav> [--language <locale>]
///
/// Reads a WAV audio file and outputs a JSON result to stdout:
///   {"transcript": "...", "confidence": 0.95}
///
/// Exit codes:
///   0 = success
///   1 = usage error
///   2 = authorization denied
///   3 = recognizer unavailable
///   4 = recognition failed

import Foundation
import Speech

struct TranscribeResult: Codable {
    let transcript: String
    let confidence: Double
}

struct TranscribeError: Codable {
    let error: String
}

func writeJSON<T: Codable>(_ value: T) {
    let encoder = JSONEncoder()
    if let data = try? encoder.encode(value),
       let json = String(data: data, encoding: .utf8) {
        print(json)
    }
}

func fail(_ message: String, code: Int32) -> Never {
    writeJSON(TranscribeError(error: message))
    exit(code)
}

// --- Parse arguments ---

var audioPath: String?
var language = "en-US"
var args = Array(CommandLine.arguments.dropFirst())
var i = 0
while i < args.count {
    if args[i] == "--language" && i + 1 < args.count {
        language = args[i + 1]
        i += 2
    } else if args[i].hasPrefix("--") {
        fail("Unknown flag: \(args[i])", code: 1)
    } else {
        audioPath = args[i]
        i += 1
    }
}

guard let path = audioPath else {
    fail("Usage: murmur-transcribe <audio.wav> [--language <locale>]", code: 1)
}

let audioURL = URL(fileURLWithPath: path)
guard FileManager.default.fileExists(atPath: path) else {
    fail("Audio file not found: \(path)", code: 1)
}

// --- Request authorization ---

let sema = DispatchSemaphore(value: 0)
var authStatus: SFSpeechRecognizerAuthorizationStatus = .notDetermined

SFSpeechRecognizer.requestAuthorization { status in
    authStatus = status
    sema.signal()
}
sema.wait()

switch authStatus {
case .authorized:
    break
case .denied:
    fail("Speech recognition authorization denied. Grant access in System Settings > Privacy > Speech Recognition.", code: 2)
case .restricted:
    fail("Speech recognition is restricted on this device.", code: 2)
case .notDetermined:
    fail("Speech recognition authorization not determined. Ensure Info.plist is embedded.", code: 2)
@unknown default:
    fail("Unknown authorization status.", code: 2)
}

// --- Set up recognizer ---

guard let recognizer = SFSpeechRecognizer(locale: Locale(identifier: language)) else {
    fail("Could not create recognizer for locale: \(language)", code: 3)
}

guard recognizer.isAvailable else {
    fail("Speech recognizer is not available for locale: \(language)", code: 3)
}

// --- Recognize ---

let request = SFSpeechURLRecognitionRequest(url: audioURL)
request.shouldReportPartialResults = false

// Prefer on-device recognition for privacy and no rate limits
if recognizer.supportsOnDeviceRecognition {
    request.requiresOnDeviceRecognition = true
}

let taskSema = DispatchSemaphore(value: 0)

recognizer.recognitionTask(with: request) { result, error in
    if let error = error {
        fail("Recognition failed: \(error.localizedDescription)", code: 4)
    }
    guard let result = result else { return }
    if result.isFinal {
        let transcript = result.bestTranscription.formattedString
        // Compute average confidence from segments
        let segments = result.bestTranscription.segments
        let confidence: Double
        if segments.isEmpty {
            confidence = 0.0
        } else {
            confidence = segments.reduce(0.0) { $0 + Double($1.confidence) } / Double(segments.count)
        }
        writeJSON(TranscribeResult(transcript: transcript, confidence: confidence))
        taskSema.signal()
    }
}

// Wait for recognition to complete (timeout: 60 seconds)
let timeout = DispatchTime.now() + .seconds(60)
if taskSema.wait(timeout: timeout) == .timedOut {
    fail("Recognition timed out after 60 seconds.", code: 4)
}
