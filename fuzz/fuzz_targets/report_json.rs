#![no_main]

use libfuzzer_sys::fuzz_target;
use treesync_verify::{explain_report, ComparisonReport};

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    if let Ok(report) = serde_json::from_str::<ComparisonReport>(&input) {
        let _ = explain_report(&report);
    }
});
