//! Search-filter benchmark: current substring filtering vs nucleo.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};
use std::time::{Duration, Instant};

const ITEMS: usize = 5_000;
const QUERIES: &[&str] = &["love", "night", "张学友", "radiohead", "xyz-not-found"];
const SAMPLES: usize = 5;
const ITERS: usize = 10;

fn dataset() -> Vec<String> {
    (0..ITEMS)
        .map(|i| match i % 5 {
            0 => format!("Love in the Night {i} · 张学友"),
            1 => format!("Radiohead — Everything in Its Right Place {i}"),
            2 => format!("夜曲 Nocturne {i} · 周杰伦"),
            3 => format!("A Quiet Night at the Radio {i}"),
            _ => format!("Local Track {i} — voicefox"),
        })
        .collect()
}

fn contains_filter(items: &[String], query: &str) -> usize {
    let query = query.to_lowercase();
    items
        .iter()
        .filter(|item| item.to_lowercase().contains(&query))
        .count()
}

fn nucleo_filter(items: &[String], query: &str, matcher: &mut Matcher) -> usize {
    Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart)
        .match_list(items, matcher)
        .len()
}

fn measure<F: FnMut() -> usize>(mut f: F) -> (Duration, usize) {
    let start = Instant::now();
    let mut count = 0;
    for _ in 0..ITERS {
        count = f();
        std::hint::black_box(count);
    }
    (start.elapsed(), count)
}

fn main() {
    let items = dataset();
    let mut matcher = Matcher::new(Config::DEFAULT);
    println!("voicefox search-filter benchmark");
    println!("items={ITEMS}, samples={SAMPLES}, iterations/sample={ITERS}");
    println!("query | contains ms | nucleo ms | contains matches | nucleo matches");
    for &query in QUERIES {
        let mut contains_total = Duration::ZERO;
        let mut nucleo_total = Duration::ZERO;
        let mut contains_count = 0;
        let mut nucleo_count = 0;
        for _ in 0..SAMPLES {
            let (elapsed, count) = measure(|| contains_filter(&items, query));
            contains_total += elapsed;
            contains_count = count;
            let (elapsed, count) = measure(|| nucleo_filter(&items, query, &mut matcher));
            nucleo_total += elapsed;
            nucleo_count = count;
        }
        let contains_ms = contains_total.as_secs_f64() * 1000.0 / SAMPLES as f64;
        let nucleo_ms = nucleo_total.as_secs_f64() * 1000.0 / SAMPLES as f64;
        println!("{query} | {contains_ms:.2} | {nucleo_ms:.2} | {contains_count} | {nucleo_count}");
    }
}
