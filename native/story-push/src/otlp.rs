//! One OTLP/HTTP JSON exporter for every telemetry signal. Export is off unless an OTLP endpoint
//! variable is set, posts one small payload per command, and never reports a failure: a story
//! command's stdout, stderr, and exit code do not depend on the backend. See docs/telemetry.md.

use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SERVICE_NAME: &str = "story-time";
const EXPORT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
pub enum Signal {
    Logs,
}
impl Signal {
    fn path(self) -> &'static str {
        match self {
            Signal::Logs => "v1/logs",
        }
    }
    fn variable(self) -> &'static str {
        match self {
            Signal::Logs => "LOGS",
        }
    }
}

pub struct Exporter {
    endpoint: String,
    headers: Vec<(String, String)>,
}
impl Exporter {
    /// Reads the standard `OTEL_EXPORTER_OTLP_[<SIGNAL>_]ENDPOINT` and `..._HEADERS` variables.
    /// A signal endpoint is used as given; the generic endpoint gets the signal path appended.
    pub fn from_env(signal: Signal) -> Option<Self> {
        let var = |name: String| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        let specific =
            |suffix: &str| var(format!("OTEL_EXPORTER_OTLP_{}_{suffix}", signal.variable()));
        let generic = |suffix: &str| var(format!("OTEL_EXPORTER_OTLP_{suffix}"));
        let endpoint = specific("ENDPOINT").or_else(|| {
            generic("ENDPOINT")
                .map(|base| format!("{}/{}", base.trim_end_matches('/'), signal.path()))
        })?;
        let headers = specific("HEADERS")
            .or_else(|| generic("HEADERS"))
            .map(|list| parse_headers(&list))
            .unwrap_or_default();
        Some(Self { endpoint, headers })
    }

    /// Posts the payload once within `EXPORT_TIMEOUT`. Every error is dropped on purpose.
    pub fn send(&self, payload: &Value) {
        let Ok(client) = reqwest::blocking::Client::builder()
            .timeout(EXPORT_TIMEOUT)
            .connect_timeout(EXPORT_TIMEOUT)
            .build()
        else {
            return;
        };
        let mut request = client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .body(payload.to_string());
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }
        let _ = request.send();
    }
}

/// `key=value` pairs separated by commas, values percent-decoded, per the OTLP exporter spec.
fn parse_headers(list: &str) -> Vec<(String, String)> {
    list.split(',')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.trim().to_owned(), percent_decode(value.trim())))
        .filter(|(key, _)| !key.is_empty())
        .collect()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The W3C trace id that correlates one command's log event with its later trace and metrics:
/// taken from a valid `TRACEPARENT`, otherwise random.
pub fn correlation_id() -> String {
    std::env::var("TRACEPARENT")
        .ok()
        .and_then(|parent| traceparent_trace_id(&parent))
        .unwrap_or_else(|| {
            let mut bytes = [0u8; 16];
            if SystemRandom::new().fill(&mut bytes).is_err() {
                bytes[15] = 1;
            }
            bytes.iter().map(|b| format!("{b:02x}")).collect()
        })
}

fn traceparent_trace_id(parent: &str) -> Option<String> {
    let parts: Vec<&str> = parent.trim().split('-').collect();
    let lower_hex = |s: &str, len: usize| {
        s.len() == len
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    match parts.as_slice() {
        [version, trace, span, flags]
            if lower_hex(version, 2)
                && *version != "ff"
                && lower_hex(trace, 32)
                && lower_hex(span, 16)
                && lower_hex(flags, 2)
                && trace.bytes().any(|b| b != b'0') =>
        {
            Some((*trace).to_owned())
        }
        _ => None,
    }
}

/// The bounded resource attribute set. Loki indexes `service.name` as the `service_name` label.
fn resource() -> Value {
    json!({"attributes": [
        string_attribute("service.name", SERVICE_NAME),
        string_attribute("service.version", env!("CARGO_PKG_VERSION")),
    ]})
}

fn scope() -> Value {
    json!({"name": SERVICE_NAME, "version": env!("CARGO_PKG_VERSION")})
}

fn string_attribute(key: &str, value: &str) -> Value {
    json!({"key": key, "value": {"stringValue": value}})
}

/// One finished `story` command. It holds only the command name from the dispatch table, never
/// arguments, output, or environment, so credentials and story bodies cannot reach a payload.
pub struct CommandEvent {
    pub operation: &'static str,
    pub exit_code: u8,
    pub duration: Duration,
    pub finished: SystemTime,
    pub correlation_id: String,
}
impl CommandEvent {
    fn outcome(&self) -> &'static str {
        if self.exit_code == 0 {
            "success"
        } else {
            "failure"
        }
    }

    pub fn logs_payload(&self) -> Value {
        let (severity, number) = if self.exit_code == 0 {
            ("INFO", 9)
        } else {
            ("ERROR", 17)
        };
        let nanos = self
            .finished
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        json!({"resourceLogs": [{
            "resource": resource(),
            "scopeLogs": [{
                "scope": scope(),
                "logRecords": [{
                    "timeUnixNano": nanos,
                    "observedTimeUnixNano": nanos,
                    "severityNumber": number,
                    "severityText": severity,
                    "body": {"stringValue": format!("story {} {}", self.operation, self.outcome())},
                    "traceId": self.correlation_id,
                    "attributes": [
                        string_attribute("story.operation", self.operation),
                        string_attribute("story.outcome", self.outcome()),
                        json!({"key": "story.exit_code", "value": {"intValue": self.exit_code.to_string()}}),
                        json!({"key": "story.duration_ms", "value": {"doubleValue": self.duration.as_secs_f64() * 1000.0}}),
                    ],
                }],
            }],
        }]})
    }
}

/// Exports the command's log event when logs export is configured; otherwise does nothing.
pub fn export_command(event: &CommandEvent) {
    if let Some(exporter) = Exporter::from_env(Signal::Logs) {
        exporter.send(&event.logs_payload());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_are_split_and_percent_decoded() {
        assert_eq!(
            parse_headers("Authorization=Bearer%20abc , x-a=b=c,broken,=v"),
            vec![
                ("Authorization".into(), "Bearer abc".into()),
                ("x-a".into(), "b=c".into())
            ]
        );
    }

    #[test]
    fn traceparent_requires_the_w3c_shape() {
        let id = "4bf92f3577b34da6a3ce929d0e0e4736";
        assert_eq!(
            traceparent_trace_id(&format!("00-{id}-00f067aa0ba902b7-01")).as_deref(),
            Some(id)
        );
        for bad in [
            format!("00-{}-00f067aa0ba902b7-01", "0".repeat(32)),
            format!("ff-{id}-00f067aa0ba902b7-01"),
            format!("00-{}-00f067aa0ba902b7-01", id.to_uppercase()),
            format!("00-{id}-00f067aa0ba902b7"),
        ] {
            assert_eq!(traceparent_trace_id(&bad), None, "{bad}");
        }
    }
}
