// Simulated "answering engine" for an agent.
// In a real system this would call an LLM, a knowledge base, etc.
// Here we use keyword matching + canned responses to demo the flow.

use crate::protocol::Capability;

pub struct Answerer {
    pub capabilities: Vec<Capability>,
    #[allow(dead_code)]
    pub alias: String,
}

impl Answerer {
    pub fn new(alias: String, capabilities: Vec<Capability>) -> Self {
        Self {
            alias,
            capabilities,
        }
    }

    /// Try to answer a question. Returns (answer_text, confidence) or None if can't answer.
    pub fn try_answer(&self, question: &str) -> Option<(String, f32)> {
        let q = question.to_lowercase();

        // -- Rust knowledge --
        if self.has_capability("rust") {
            if q.contains("fibonacci") && q.contains("rust") {
                return Some((
                    "In Rust:\n\
                    fn fibonacci(n: u64) -> u64 {\n\
                    \x20   match n {\n\
                    \x20       0 => 0,\n\
                    \x20       1 => 1,\n\
                    \x20       _ => fibonacci(n-1) + fibonacci(n-2),\n\
                    \x20   }\n\
                    }\n\
                    For large n, use an iterative version or memoization."
                        .to_string(),
                    0.97,
                ));
            }
            if q.contains("lifetime") || q.contains("borrow") {
                return Some((
                    "Rust lifetimes ensure references never outlive the data they point to. \
                    The borrow checker enforces this at compile time. Use 'a, 'b etc. to annotate."
                        .to_string(),
                    0.91,
                ));
            }
            if q.contains("async") && q.contains("rust") {
                return Some((
                    "Rust async uses `async fn` + `.await`. You need a runtime like Tokio. \
                    Futures are lazy — they do nothing until polled."
                        .to_string(),
                    0.88,
                ));
            }
            if q.contains("trait") {
                return Some((
                    "Traits in Rust are like interfaces. Define with `trait Foo { fn bar(&self); }` \
                    and implement with `impl Foo for MyType { ... }`."
                    .to_string(),
                    0.93,
                ));
            }
        }

        // -- Python knowledge --
        if self.has_capability("python") {
            if q.contains("fibonacci") && (q.contains("python") || !q.contains("rust")) {
                return Some((
                    "In Python:\ndef fib(n):\n    a, b = 0, 1\n    for _ in range(n):\n        a, b = b, a+b\n    return a"
                    .to_string(),
                    0.95,
                ));
            }
            if q.contains("decorator") {
                return Some((
                    "Python decorators wrap functions. `@my_decorator` above a function is \
                    sugar for `func = my_decorator(func)`. Use `functools.wraps` to preserve metadata."
                    .to_string(),
                    0.90,
                ));
            }
            if q.contains("gil") {
                return Some((
                    "The GIL (Global Interpreter Lock) in CPython prevents true thread parallelism \
                    for CPU-bound tasks. Use `multiprocessing` or async I/O to work around it."
                    .to_string(),
                    0.85,
                ));
            }
        }

        // -- Networking knowledge --
        if self.has_capability("networking") {
            if q.contains("tcp") || q.contains("udp") {
                return Some((
                    "TCP is reliable, ordered, connection-oriented. UDP is fast, unordered, connectionless. \
                    Use TCP for data integrity, UDP for real-time (games, video)."
                    .to_string(),
                    0.92,
                ));
            }
            if q.contains("p2p") || q.contains("peer") {
                return Some((
                    "P2P networks remove the central server. Peers discover each other via DHT, \
                    mDNS, or bootstrap nodes, then communicate directly. libp2p is a great toolkit."
                        .to_string(),
                    0.89,
                ));
            }
        }

        // -- Math knowledge --
        if self.has_capability("math") {
            if q.contains("fibonacci") {
                return Some((
                    "Fibonacci sequence: F(0)=0, F(1)=1, F(n)=F(n-1)+F(n-2). \
                    Closed form: F(n) = round(φⁿ/√5) where φ=(1+√5)/2 (golden ratio)."
                        .to_string(),
                    0.99,
                ));
            }
            if q.contains("prime") {
                return Some((
                    "Primality test: trial division up to √n. For large numbers use \
                    Miller-Rabin probabilistic test. Sieve of Eratosthenes for bulk generation."
                        .to_string(),
                    0.94,
                ));
            }
        }

        None // can't answer this question
    }

    fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|cap| cap == capability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_agent_answers_rust_fibonacci() {
        let answerer = Answerer::new("alice".to_string(), vec!["rust".to_string()]);

        let answer = answerer
            .try_answer("how to write fibonacci function in rust")
            .unwrap();

        assert!(answer.0.contains("fn fibonacci"));
        assert!(answer.1 > 0.9);
    }

    #[test]
    fn python_agent_does_not_answer_rust_question() {
        let answerer = Answerer::new("bob".to_string(), vec!["python".to_string()]);

        assert!(
            answerer
                .try_answer("how to write fibonacci function in rust")
                .is_none()
        );
    }
}
