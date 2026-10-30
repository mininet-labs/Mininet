//! A real loopback TCP server verifies the wire client, not a fake local AI.
//! These tests do not download or run a model, and do not claim inference quality.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use mini_resident::{
    chat, content_hash, validate_provider_config, ContextItem, ProviderConfig, ResidentRequest,
};
use serde_json::{json, Value};

struct Captured {
    headers: String,
    body: Value,
}
fn server(response: Vec<u8>) -> (ProviderConfig, thread::JoinHandle<Captured>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let thread = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut headers = String::new();
        loop {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).unwrap() > 0);
            headers.push_str(&line);
            assert!(headers.len() <= 16 * 1024);
            if line == "\r\n" {
                break;
            }
        }
        let length: usize = headers
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(length <= 2 * 1024 * 1024);
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        // Fragment headers, chunk delimiters and JSON across TCP writes.
        for fragment in response.chunks(7) {
            if stream.write_all(fragment).is_err() {
                break;
            }
        }
        Captured {
            headers,
            body: serde_json::from_slice(&body).unwrap(),
        }
    });
    (
        ProviderConfig {
            endpoint: format!("http://{address}"),
            model: "owner-chosen-model".into(),
        },
        thread,
    )
}
fn response(body: &[u8]) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(body);
    out
}
fn question() -> ResidentRequest {
    ResidentRequest {
        question: "Explain this code".into(),
        context: Vec::new(),
    }
}
fn completion(text: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"model":"local-actual-model", "choices":[{"message":{"role":"assistant", "content":text}}]})).unwrap()
}

#[test]
fn provider_sends_only_explicit_context_and_preserves_advisory_model_text() {
    let temporary = tempfile::tempdir().unwrap();
    let marker = temporary.path().join("never-executed");
    let model_text = format!(
        "Suggestion: code_change\n```sh\ntouch {}\n```\nHuman approval is required.\n",
        marker.display()
    );
    let (config, worker) = server(response(&completion(&model_text)));
    let request = ResidentRequest {
        question: "Explain only this granted file".into(),
        context: vec![ContextItem {
            path: "src/main.rs".into(),
            content: "fn main() {}\n".into(),
            hash: content_hash(b"fn main() {}\n"),
        }],
    };
    let reply = chat(&config, &request).unwrap();
    assert_eq!(reply.text, model_text);
    assert_eq!(reply.model, "local-actual-model");
    assert!(reply.provenance.advisory && !reply.provenance.human_approval);
    assert!(reply.provenance.label.contains("AI-generated"));
    assert!(
        !marker.exists(),
        "model output is data, never shell execution"
    );
    let captured = worker.join().unwrap();
    assert!(captured
        .headers
        .starts_with("POST /v1/chat/completions HTTP/1.1\r\n"));
    assert!(!captured
        .headers
        .to_ascii_lowercase()
        .contains("authorization"));
    assert!(!captured.headers.to_ascii_lowercase().contains("proxy-"));
    assert_eq!(captured.body["model"], "owner-chosen-model");
    assert_eq!(captured.body["stream"], false);
    assert_eq!(captured.body["n"], 1);
    assert_eq!(captured.body.as_object().unwrap().len(), 5);
    let messages = captured.body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "system");
    assert!(messages[0]["content"]
        .as_str()
        .unwrap()
        .contains("untrusted reference data"));
    assert!(messages[0]["content"]
        .as_str()
        .unwrap()
        .contains("not a human signer"));
    let sent: ResidentRequest =
        serde_json::from_str(messages[1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(
        sent, request,
        "no environment, keys, archive or implicit workspace content may be added"
    );
}

#[test]
fn provider_supports_chunked_and_connection_close_json_responses() {
    let body = completion("Local answer ✓");
    let mut chunked =
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n"
            .to_vec();
    for chunk in body.chunks(11) {
        chunked.extend_from_slice(format!("{:x};part=1\r\n", chunk.len()).as_bytes());
        chunked.extend_from_slice(chunk);
        chunked.extend_from_slice(b"\r\n");
    }
    chunked.extend_from_slice(b"0\r\n\r\n");
    let mut close = b"HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n".to_vec();
    close.extend_from_slice(&body);
    for wire in [chunked, close] {
        let (config, worker) = server(wire);
        assert_eq!(chat(&config, &question()).unwrap().text, "Local answer ✓");
        worker.join().unwrap();
    }
}

#[test]
fn provider_rejects_http_errors_ambiguous_framing_and_invalid_completions() {
    let body = completion("hello");
    let mut trailing = body.clone();
    trailing.extend_from_slice(b" unrelated trailing data");
    let mut oversized_headers = b"HTTP/1.1 200 OK\r\nX-Large: ".to_vec();
    oversized_headers.extend_from_slice(&vec![b'x'; 17 * 1024]);
    oversized_headers.extend_from_slice(b"\r\n\r\n");
    let cases = vec![
        b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n".to_vec(),
        b"HTTP/1.1 302 Found\r\nLocation: https://external.invalid/\r\nContent-Length: 0\r\n\r\n"
            .to_vec(),
        response(b"{}"),
        response(b"{\"choices\":[]}"),
        response(b"{\"choices\":[{\"message\":{\"content\":null}}]}"),
        response(b"{broken"),
        response(&trailing),
        response(&completion(" ")),
        response(&completion(&"x".repeat(mini_resident::MAX_REPLY_BYTES + 1))),
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n{}".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nTransfer-Encoding: chunked\r\n\r\n{}".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 1048577\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 99\r\n\r\n{}".to_vec(),
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nZZ\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 2\r\n\r\n{}".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n".to_vec(),
        oversized_headers,
    ];
    for (index, wire) in cases.into_iter().enumerate() {
        let (config, worker) = server(wire);
        assert!(
            chat(&config, &question()).is_err(),
            "case {index} must fail closed"
        );
        worker.join().unwrap();
    }
}

#[test]
fn provider_config_is_numeric_loopback_only_and_bad_context_never_connects() {
    for endpoint in [
        "http://localhost:8080",
        "https://127.0.0.1:8080",
        "http://192.0.2.1:8080",
        "http://0.0.0.0:8080",
        "http://127.0.0.1:0",
        "http://user@127.0.0.1:8080",
        "http://127.0.0.1:8080/v1/chat/completions?key=secret",
        "http://127.0.0.1:8080/\r\nInjected: yes",
        "http://127.0.0.1",
    ] {
        assert!(
            validate_provider_config(&ProviderConfig {
                endpoint: endpoint.into(),
                model: "local".into()
            })
            .is_err(),
            "{endpoint}"
        );
    }
    for endpoint in [
        "http://127.0.0.1:8080",
        "http://127.0.0.1:8080/",
        "http://[::1]:8080/v1",
        "http://127.0.0.1:8080/v1/chat/completions",
    ] {
        validate_provider_config(&ProviderConfig {
            endpoint: endpoint.into(),
            model: "local".into(),
        })
        .unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let config = ProviderConfig {
        endpoint: format!("http://{}", listener.local_addr().unwrap()),
        model: "local".into(),
    };
    let invalid = ResidentRequest {
        question: "explain".into(),
        context: vec![ContextItem {
            path: "src/main.rs".into(),
            content: "not granted under this hash".into(),
            hash: content_hash(b"different"),
        }],
    };
    assert!(chat(&config, &invalid).is_err());
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn provider_rejects_cut_off_code_and_tool_calls_in_text_completions() {
    let cases = [
        json!({"choices":[{"finish_reason":"length", "message":{"content":"fn unfinished("}}]}),
        json!({"choices":[{"finish_reason":"tool_calls", "message":{"content":"done"}}]}),
        json!({"choices":[{"finish_reason":"stop", "message":{"content":"done", "tool_calls":[{"function":{"name":"execute"}}]}}]}),
        json!({"choices":[{"message":{"content":"done", "function_call":{"name":"execute"}}}]}),
    ];
    for body in cases {
        let (config, worker) = server(response(&serde_json::to_vec(&body).unwrap()));
        assert!(chat(&config, &question()).is_err());
        worker.join().unwrap();
    }
    let valid = json!({"choices":[{"finish_reason":"stop", "message":{"content":"Complete advisory text", "tool_calls":null, "function_call":null}}]});
    let (config, worker) = server(response(&serde_json::to_vec(&valid).unwrap()));
    assert_eq!(
        chat(&config, &question()).unwrap().text,
        "Complete advisory text"
    );
    worker.join().unwrap();
}
