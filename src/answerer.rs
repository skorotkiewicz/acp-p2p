// src/answerer.rs
// Simulated "answering engine" for an agent.
// In a real system this would call an LLM, a knowledge base, etc.

pub struct Answerer {
    pub alias: String,
}

impl Answerer {
    pub fn new(alias: String) -> Self {
        Self { alias }
    }

    /// Try to answer a question. Returns (answer_text, confidence) or None.
    pub fn try_answer(&self, question: &str) -> Option<(String, f32)> {
        let q = question.to_lowercase();

        if q.contains("fibonacci") && q.contains("rust") {
            return Some((
                "In Rust:\nfn fibonacci(n: u64) -> u64 {\n    match n {\n        0 => 0,\n        1 => 1,\n        _ => fibonacci(n-1) + fibonacci(n-2),\n    }\n}".to_string(),
                0.97,
            ));
        }
        if q.contains("fibonacci") && q.contains("python") {
            return Some((
                "In Python:\ndef fib(n):\n    a, b = 0, 1\n    for _ in range(n):\n        a, b = b, a+b\n    return a".to_string(),
                0.95,
            ));
        }
        if q.contains("fibonacci") {
            return Some((
                "Fibonacci: F(0)=0, F(1)=1, F(n)=F(n-1)+F(n-2). Closed form: F(n)=round(φⁿ/√5) where φ=(1+√5)/2.".to_string(),
                0.99,
            ));
        }
        if q.contains("lifetime") || q.contains("borrow") {
            return Some((
                "Rust lifetimes ensure references never outlive the data they point to.".to_string(),
                0.91,
            ));
        }
        if q.contains("tcp") || q.contains("udp") {
            return Some((
                "TCP is reliable and ordered. UDP is fast but unordered. Use TCP for data integrity, UDP for real-time.".to_string(),
                0.92,
            ));
        }
        if q.contains("p2p") || q.contains("peer") {
            return Some((
                "P2P networks remove the central server. Peers discover each other via DHT or mDNS and communicate directly.".to_string(),
                0.89,
            ));
        }

        None
    }
}
