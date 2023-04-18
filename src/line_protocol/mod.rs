use aho_corasick::AhoCorasick;

mod data_point;
mod field_value;

pub(crate) use data_point::{DataPoint, DataPointCreateError};

struct Replacer {
    aho_corasick: AhoCorasick,
    to: &'static [&'static str],
}

impl Replacer {
    fn new(from: &[&str], to: &'static [&str]) -> Self {
        let aho_corasick = AhoCorasick::new(from).unwrap();
        Self { aho_corasick, to }
    }

    fn replace_all(&self, haystack: &str) -> String {
        self.aho_corasick.replace_all(haystack, self.to)
    }
}
